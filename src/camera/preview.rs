use std::ptr;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use egui::ColorImage;
use windows::Graphics::Imaging::{
    BitmapAlphaMode, BitmapBufferAccessMode, BitmapPixelFormat, SoftwareBitmap,
};
use windows::Media::Capture::Frames::{
    MediaFrameReader, MediaFrameReaderAcquisitionMode, MediaFrameReaderStartStatus,
    MediaFrameSourceGroup, MediaFrameSourceKind,
};
use windows::Media::Capture::{
    MediaCapture, MediaCaptureInitializationSettings, MediaCaptureMemoryPreference,
    MediaCaptureSharingMode, MediaStreamType, StreamingCaptureMode,
};
use windows::Media::MediaProperties::MediaEncodingSubtypes;
use windows::Win32::System::WinRT::{
    IMemoryBufferByteAccess, RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize,
};
use windows::core::Interface;

/// Target max preview width in pixels (keeps GPU texture uploads lightweight).
const MAX_PREVIEW_WIDTH: usize = 512;
/// Poll interval while streaming (~30 fps cap).
const FRAME_POLL_INTERVAL: Duration = Duration::from_millis(33);
/// Backoff between reconnect retries if camera is busy/reconfiguring.
const RETRY_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Clone)]
pub struct PreviewFrame {
    pub seq: u64,
    pub image: ColorImage,
    pub src_width: u32,
    pub src_height: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PreviewStatus {
    Off,
    Starting,
    Streaming {
        width: u32,
        height: u32,
        fps: f32,
    },
    WaitingForStream,
    Error(String),
}

#[derive(Clone)]
pub struct PreviewState {
    pub enabled: bool,
    pub status: PreviewStatus,
    pub latest_frame: Option<PreviewFrame>,
}

impl Default for PreviewState {
    fn default() -> Self {
        Self {
            enabled: false,
            status: PreviewStatus::Off,
            latest_frame: None,
        }
    }
}

#[derive(Clone, Debug)]
enum PreviewCommand {
    SetEnabled(bool),
    SetTarget {
        device_path: Option<String>,
        friendly_name: Option<String>,
    },
    Restart,
    Shutdown,
}

pub struct PreviewHandle {
    tx: Sender<PreviewCommand>,
    state: Arc<Mutex<PreviewState>>,
    repaint: Arc<Mutex<Option<egui::Context>>>,
    worker: Option<JoinHandle<()>>,
}

impl PreviewHandle {
    pub fn spawn(initial_enabled: bool) -> Self {
        let state = Arc::new(Mutex::new(PreviewState {
            enabled: initial_enabled,
            status: if initial_enabled {
                PreviewStatus::Starting
            } else {
                PreviewStatus::Off
            },
            latest_frame: None,
        }));
        let repaint = Arc::new(Mutex::new(None));
        let (tx, rx) = std::sync::mpsc::channel();

        let state_clone = Arc::clone(&state);
        let repaint_clone = Arc::clone(&repaint);
        let worker = std::thread::Builder::new()
            .name("camera-preview-winrt".to_owned())
            .spawn(move || {
                run_preview_worker(rx, state_clone, repaint_clone, initial_enabled);
            })
            .expect("failed to spawn camera preview thread");

        Self {
            tx,
            state,
            repaint,
            worker: Some(worker),
        }
    }

    pub fn set_repaint_ctx(&self, ctx: egui::Context) {
        if let Ok(mut guard) = self.repaint.lock() {
            *guard = Some(ctx);
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        let _ = self.tx.send(PreviewCommand::SetEnabled(enabled));
    }

    pub fn set_target(&self, device_path: Option<String>, friendly_name: Option<String>) {
        let _ = self.tx.send(PreviewCommand::SetTarget {
            device_path,
            friendly_name,
        });
    }

    pub fn restart(&self) {
        let _ = self.tx.send(PreviewCommand::Restart);
    }

    pub fn snapshot(&self) -> PreviewState {
        self.state
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }
}

impl Drop for PreviewHandle {
    fn drop(&mut self) {
        let _ = self.tx.send(PreviewCommand::Shutdown);
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
    }
}

struct RoInitGuard;

impl RoInitGuard {
    fn init() -> windows::core::Result<Self> {
        unsafe { RoInitialize(RO_INIT_MULTITHREADED)? };
        Ok(Self)
    }
}

impl Drop for RoInitGuard {
    fn drop(&mut self) {
        unsafe { RoUninitialize() };
    }
}

struct SharedCaptureSession {
    capture: MediaCapture,
    reader: MediaFrameReader,
    group_id_lower: String,
    group_name_lower: String,
}

impl SharedCaptureSession {
    /// Opens the camera in Windows FrameServer `SharedReadOnly` mode so OBS/Zoom/Meet
    /// retain full exclusive control and are NEVER locked out.
    fn open(
        device_path: Option<&str>,
        friendly_name: Option<&str>,
    ) -> Result<Self, String> {
        let groups: Vec<MediaFrameSourceGroup> = MediaFrameSourceGroup::FindAllAsync()
            .and_then(|op| op.get())
            .map_err(|e| format!("Failed to enumerate camera groups: {e}"))?
            .into_iter()
            .collect();

        let (group, source_id) = pick_matching_group(&groups, device_path, friendly_name)
            .ok_or_else(|| "Camera not available via Windows FrameServer".to_owned())?;

        let group_id_lower = group
            .Id()
            .map(|s| s.to_string().to_ascii_lowercase())
            .unwrap_or_default();
        let group_name_lower = group
            .DisplayName()
            .map(|s| s.to_string().to_ascii_lowercase())
            .unwrap_or_default();

        let settings = MediaCaptureInitializationSettings::new()
            .map_err(|e| format!("Failed to create capture settings: {e}"))?;
        settings
            .SetSourceGroup(&group)
            .map_err(|e| format!("SetSourceGroup failed: {e}"))?;
        settings
            .SetSharingMode(MediaCaptureSharingMode::SharedReadOnly)
            .map_err(|e| format!("SetSharingMode(SharedReadOnly) failed: {e}"))?;
        settings
            .SetStreamingCaptureMode(StreamingCaptureMode::Video)
            .map_err(|e| format!("SetStreamingCaptureMode failed: {e}"))?;
        settings
            .SetMemoryPreference(MediaCaptureMemoryPreference::Cpu)
            .map_err(|e| format!("SetMemoryPreference failed: {e}"))?;

        let capture = MediaCapture::new()
            .map_err(|e| format!("MediaCapture::new failed: {e}"))?;
        capture
            .InitializeWithSettingsAsync(&settings)
            .and_then(|op| op.get())
            .map_err(|e| format!("MediaCapture init (SharedReadOnly) failed: {e}"))?;

        let frame_sources = capture
            .FrameSources()
            .map_err(|e| format!("FrameSources failed: {e}"))?;
        let source = frame_sources
            .Lookup(&source_id)
            .map_err(|e| format!("FrameSource lookup failed: {e}"))?;

        // Ask FrameServer for BGRA8 directly when supported; fall back to native subtype.
        let reader = match MediaEncodingSubtypes::Bgra8().and_then(|bgra8| {
            capture
                .CreateFrameReaderWithSubtypeAsync(&source, &bgra8)
                .and_then(|op| op.get())
        }) {
            Ok(r) => r,
            Err(_) => capture
                .CreateFrameReaderAsync(&source)
                .and_then(|op| op.get())
                .map_err(|e| format!("CreateFrameReaderAsync failed: {e}"))?,
        };

        let _ = reader.SetAcquisitionMode(MediaFrameReaderAcquisitionMode::Realtime);

        let start_status = reader
            .StartAsync()
            .and_then(|op| op.get())
            .map_err(|e| format!("FrameReader StartAsync failed: {e}"))?;

        if start_status != MediaFrameReaderStartStatus::Success {
            let _ = capture.Close();
            return Err(format!("FrameReader did not start ({start_status:?})"));
        }

        Ok(Self {
            capture,
            reader,
            group_id_lower,
            group_name_lower,
        })
    }

    fn matches_target(&self, device_path: Option<&str>, friendly_name: Option<&str>) -> bool {
        if let Some(path) = device_path {
            let p = path.to_ascii_lowercase();
            if !p.is_empty()
                && (self.group_id_lower.contains(&p)
                    || extract_vid_pid(&p).is_some_and(|vp| self.group_id_lower.contains(vp)))
            {
                return true;
            }
        }
        if let Some(name) = friendly_name {
            let n = name.to_ascii_lowercase();
            if !n.is_empty() && self.group_name_lower == n {
                return true;
            }
        }
        false
    }

    fn try_read_frame(&self) -> Option<(ColorImage, u32, u32)> {
        let frame_ref = self.reader.TryAcquireLatestFrame().ok()?;
        let result = (|| {
            let video_frame = frame_ref.VideoMediaFrame().ok()?;
            let bitmap = video_frame.SoftwareBitmap().ok()?;
            let out = software_bitmap_to_color_image(&bitmap);
            let _ = bitmap.Close();
            out
        })();
        let _ = frame_ref.Close();
        result
    }
}

impl Drop for SharedCaptureSession {
    fn drop(&mut self) {
        if let Ok(op) = self.reader.StopAsync() {
            let _ = op.get();
        }
        let _ = self.reader.Close();
        let _ = self.capture.Close();
    }
}

fn pick_matching_group(
    groups: &[MediaFrameSourceGroup],
    device_path: Option<&str>,
    friendly_name: Option<&str>,
) -> Option<(MediaFrameSourceGroup, windows::core::HSTRING)> {
    let target_path = device_path.map(|s| s.to_ascii_lowercase());
    let target_name = friendly_name.map(|s| s.to_ascii_lowercase());

    let mut fallback_logi: Option<(MediaFrameSourceGroup, windows::core::HSTRING)> = None;
    let mut fallback_any: Option<(MediaFrameSourceGroup, windows::core::HSTRING)> = None;

    for group in groups {
        let Ok(source_infos) = group.SourceInfos() else {
            continue;
        };
        let group_name = group
            .DisplayName()
            .map(|s| s.to_string().to_ascii_lowercase())
            .unwrap_or_default();
        let group_id = group
            .Id()
            .map(|s| s.to_string().to_ascii_lowercase())
            .unwrap_or_default();

        // Find best Color stream in this group (prefer VideoPreview, then VideoRecord)
        let mut chosen_source_id: Option<windows::core::HSTRING> = None;
        let mut dev_id_lower = String::new();
        let info_count = source_infos.Size().unwrap_or(0);
        for j in 0..info_count {
            let Ok(info) = source_infos.GetAt(j) else {
                continue;
            };
            if info.SourceKind().ok() != Some(MediaFrameSourceKind::Color) {
                continue;
            }
            let stream_type = info.MediaStreamType().ok();
            let is_preview = stream_type == Some(MediaStreamType::VideoPreview);
            let is_record = stream_type == Some(MediaStreamType::VideoRecord);
            if !is_preview && !is_record {
                continue;
            }
            if let Ok(di) = info.DeviceInformation()
                && let Ok(id) = di.Id()
            {
                dev_id_lower = id.to_string().to_ascii_lowercase();
            }
            if let Ok(src_id) = info.Id() {
                if is_preview || chosen_source_id.is_none() {
                    chosen_source_id = Some(src_id);
                    if is_preview {
                        break;
                    }
                }
            }
        }

        let Some(source_id) = chosen_source_id else {
            continue;
        };

        // Match against selected camera's device_path (extract VID/PID or full match)
        if let Some(ref want_path) = target_path
            && !want_path.is_empty()
            && (dev_id_lower == *want_path
                || group_id.contains(want_path)
                || extract_vid_pid(want_path)
                    .is_some_and(|vp| dev_id_lower.contains(vp) || group_id.contains(vp)))
        {
            return Some((group.clone(), source_id));
        }

        if let Some(ref want_name) = target_name
            && !want_name.is_empty()
            && group_name == *want_name
        {
            return Some((group.clone(), source_id));
        }

        if fallback_logi.is_none()
            && (dev_id_lower.contains("vid_046d&pid_086e")
                || group_id.contains("vid_046d&pid_086e")
                || group_name.contains("logi group"))
        {
            fallback_logi = Some((group.clone(), source_id.clone()));
        }

        if fallback_any.is_none() {
            fallback_any = Some((group.clone(), source_id));
        }
    }

    fallback_logi.or(fallback_any)
}

fn extract_vid_pid(path: &str) -> Option<&str> {
    let start = path.find("vid_")?;
    let rest = &path[start..];
    let end = rest.find('#').unwrap_or(rest.len());
    let slice = &rest[..end];
    if slice.contains("pid_") {
        Some(slice)
    } else {
        None
    }
}

fn software_bitmap_to_color_image(bitmap: &SoftwareBitmap) -> Option<(ColorImage, u32, u32)> {
    let pixel_format = bitmap.BitmapPixelFormat().ok()?;
    let mut converted: Option<SoftwareBitmap> = None;

    let (source_bmp, is_bgra) = match pixel_format {
        BitmapPixelFormat::Bgra8 => (bitmap, true),
        BitmapPixelFormat::Rgba8 => (bitmap, false),
        _ => {
            let conv = SoftwareBitmap::ConvertWithAlpha(
                bitmap,
                BitmapPixelFormat::Rgba8,
                BitmapAlphaMode::Ignore,
            )
            .ok()?;
            converted = Some(conv);
            (converted.as_ref().unwrap(), false)
        }
    };

    let result = read_and_downscale_bitmap(source_bmp, is_bgra);
    if let Some(conv) = converted {
        let _ = conv.Close();
    }
    result
}

fn read_and_downscale_bitmap(
    bmp: &SoftwareBitmap,
    is_bgra: bool,
) -> Option<(ColorImage, u32, u32)> {
    let buffer = bmp.LockBuffer(BitmapBufferAccessMode::Read).ok()?;
    let desc = buffer.GetPlaneDescription(0).ok()?;
    let reference = buffer.CreateReference().ok()?;
    let byte_access: IMemoryBufferByteAccess = reference.cast().ok()?;

    let src_w = desc.Width.max(0) as usize;
    let src_h = desc.Height.max(0) as usize;
    let stride = desc.Stride.max(0) as usize;
    let start = desc.StartIndex.max(0) as usize;

    if src_w == 0 || src_h == 0 || stride < src_w * 4 {
        let _ = reference.Close();
        let _ = buffer.Close();
        return None;
    }

    let mut data_ptr: *mut u8 = ptr::null_mut();
    let mut capacity: u32 = 0;
    unsafe {
        if byte_access.GetBuffer(&mut data_ptr, &mut capacity).is_err() || data_ptr.is_null() {
            let _ = reference.Close();
            let _ = buffer.Close();
            return None;
        }
    }

    let total_len = capacity as usize;
    let needed = start.saturating_add(stride.saturating_mul(src_h));
    if needed > total_len {
        let _ = reference.Close();
        let _ = buffer.Close();
        return None;
    }

    let raw = unsafe { std::slice::from_raw_parts(data_ptr, total_len) };

    // Downscale proportionally if wider than MAX_PREVIEW_WIDTH for ultra-fast GPU upload.
    let (dst_w, dst_h) = if src_w > MAX_PREVIEW_WIDTH {
        let scale = MAX_PREVIEW_WIDTH as f32 / src_w as f32;
        (MAX_PREVIEW_WIDTH, ((src_h as f32 * scale).round() as usize).max(1))
    } else {
        (src_w, src_h)
    };

    let mut rgba = vec![255u8; dst_w * dst_h * 4];
    for y in 0..dst_h {
        let sy = (y * src_h) / dst_h;
        let row_offset = start + sy * stride;
        let dst_row = y * dst_w * 4;
        for x in 0..dst_w {
            let sx = (x * src_w) / dst_w;
            let si = row_offset + sx * 4;
            let di = dst_row + x * 4;
            if is_bgra {
                rgba[di] = raw[si + 2];
                rgba[di + 1] = raw[si + 1];
                rgba[di + 2] = raw[si];
                rgba[di + 3] = 255;
            } else {
                rgba[di] = raw[si];
                rgba[di + 1] = raw[si + 1];
                rgba[di + 2] = raw[si + 2];
                rgba[di + 3] = 255;
            }
        }
    }

    drop(byte_access);
    let _ = reference.Close();
    let _ = buffer.Close();

    let image = ColorImage::from_rgba_unmultiplied([dst_w, dst_h], &rgba);
    Some((image, src_w as u32, src_h as u32))
}

fn run_preview_worker(
    rx: Receiver<PreviewCommand>,
    state: Arc<Mutex<PreviewState>>,
    repaint: Arc<Mutex<Option<egui::Context>>>,
    initial_enabled: bool,
) {
    let _ro = match RoInitGuard::init() {
        Ok(g) => g,
        Err(err) => {
            update_preview(&state, &repaint, |s| {
                s.status = PreviewStatus::Error(format!("WinRT init failed: {err}"));
            });
            return;
        }
    };

    let mut enabled = initial_enabled;
    let mut target_path: Option<String> = None;
    let mut target_name: Option<String> = None;
    let mut session: Option<SharedCaptureSession> = None;
    let mut next_retry_at: Option<Instant> = None;
    let mut seq: u64 = 0;

    // FPS tracking
    let mut fps_window_start = Instant::now();
    let mut fps_frames: u32 = 0;
    let mut current_fps: f32 = 0.0;
    let mut last_frame_at: Option<Instant> = None;

    loop {
        // Open session if enabled and not yet open
        if enabled
            && session.is_none()
            && next_retry_at.is_none_or(|t| Instant::now() >= t)
        {
            update_preview(&state, &repaint, |s| {
                s.enabled = true;
                s.status = PreviewStatus::Starting;
            });

            match SharedCaptureSession::open(target_path.as_deref(), target_name.as_deref()) {
                Ok(new_session) => {
                    log::info!("Live preview started in SharedReadOnly mode (OBS-safe)");
                    session = Some(new_session);
                    next_retry_at = None;
                    fps_window_start = Instant::now();
                    fps_frames = 0;
                    last_frame_at = None;
                    update_preview(&state, &repaint, |s| {
                        s.status = PreviewStatus::WaitingForStream;
                    });
                }
                Err(err) => {
                    log::warn!("Preview open failed: {err}");
                    next_retry_at = Some(Instant::now() + RETRY_INTERVAL);
                    update_preview(&state, &repaint, |s| {
                        s.status = PreviewStatus::Error(err);
                    });
                }
            }
        }

        // Pull latest frame if streaming
        if let Some(active) = session.as_ref() {
            if let Some((image, src_w, src_h)) = active.try_read_frame() {
                seq += 1;
                fps_frames += 1;
                last_frame_at = Some(Instant::now());

                let elapsed = fps_window_start.elapsed().as_secs_f32();
                if elapsed >= 1.0 {
                    current_fps = fps_frames as f32 / elapsed;
                    fps_frames = 0;
                    fps_window_start = Instant::now();
                }

                update_preview(&state, &repaint, |s| {
                    s.status = PreviewStatus::Streaming {
                        width: src_w,
                        height: src_h,
                        fps: if current_fps > 0.0 {
                            current_fps
                        } else {
                            fps_frames as f32
                        },
                    };
                    s.latest_frame = Some(PreviewFrame {
                        seq,
                        image,
                        src_width: src_w,
                        src_height: src_h,
                    });
                });
            } else if let Some(last) = last_frame_at
                && last.elapsed() > Duration::from_secs(3)
            {
                // Stream stalled (e.g. OBS changed resolution or camera unplugged) -> gently reopen
                log::info!("Preview stream stalled for 3s; reconnecting SharedReadOnly session");
                session = None;
                last_frame_at = None;
                next_retry_at = Some(Instant::now() + Duration::from_millis(500));
            }
        }

        let wait = if enabled && session.is_some() {
            FRAME_POLL_INTERVAL
        } else if enabled {
            Duration::from_millis(200)
        } else {
            Duration::from_millis(250)
        };

        match rx.recv_timeout(wait) {
            Ok(PreviewCommand::Shutdown) => break,
            Ok(PreviewCommand::SetEnabled(new_enabled)) => {
                if new_enabled != enabled {
                    enabled = new_enabled;
                    if !enabled {
                        session = None;
                        next_retry_at = None;
                        last_frame_at = None;
                        update_preview(&state, &repaint, |s| {
                            s.enabled = false;
                            s.status = PreviewStatus::Off;
                            s.latest_frame = None;
                        });
                    } else {
                        next_retry_at = None;
                        update_preview(&state, &repaint, |s| {
                            s.enabled = true;
                            s.status = PreviewStatus::Starting;
                        });
                    }
                }
            }
            Ok(PreviewCommand::SetTarget {
                device_path,
                friendly_name,
            }) => {
                if target_path != device_path || target_name != friendly_name {
                    target_path = device_path;
                    target_name = friendly_name;
                    let already_matches = session.as_ref().is_some_and(|s| {
                        s.matches_target(target_path.as_deref(), target_name.as_deref())
                    });
                    if !already_matches {
                        session = None;
                        next_retry_at = None;
                        last_frame_at = None;
                        if enabled {
                            update_preview(&state, &repaint, |s| {
                                s.status = PreviewStatus::Starting;
                                s.latest_frame = None;
                            });
                        }
                    }
                }
            }
            Ok(PreviewCommand::Restart) => {
                session = None;
                next_retry_at = None;
                last_frame_at = None;
                if enabled {
                    update_preview(&state, &repaint, |s| {
                        s.status = PreviewStatus::Starting;
                    });
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    drop(session);
}

fn update_preview(
    state: &Arc<Mutex<PreviewState>>,
    repaint: &Arc<Mutex<Option<egui::Context>>>,
    f: impl FnOnce(&mut PreviewState),
) {
    if let Ok(mut guard) = state.lock() {
        f(&mut guard);
    }
    if let Ok(guard) = repaint.lock()
        && let Some(ctx) = guard.as_ref()
    {
        ctx.request_repaint();
    }
}

/// CLI `--probe-preview`: verifies `SharedReadOnly` frame capture AND tests that a second
/// exclusive capture session (simulating OBS) can open the camera simultaneously!
pub fn run_probe_preview_cli() -> Result<(), String> {
    let _ro = RoInitGuard::init().map_err(|e| format!("RoInitialize failed: {e}"))?;
    println!("Opening Logi Group Camera in SharedReadOnly mode...");
    let session = SharedCaptureSession::open(Some("vid_046d&pid_086e"), Some("Logi Group Camera"))?;
    println!("SharedReadOnly session opened! Waiting up to 3s for frames...");

    let start = Instant::now();
    let mut got_frame = false;
    while start.elapsed() < Duration::from_secs(3) {
        if let Some((img, w, h)) = session.try_read_frame() {
            println!(
                "Received live frame! Source={}x{}, Preview={}x{}",
                w, h, img.size[0], img.size[1]
            );
            got_frame = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    if !got_frame {
        println!("Note: SharedReadOnly opened cleanly, but no frame arrived within 3s (waiting for active stream).");
    }

    // Now simulate OBS opening the camera in ExclusiveControl while our SharedReadOnly preview is open!
    println!("Testing OBS coexistence: opening a second ExclusiveControl MediaCapture while preview is open...");
    let groups: Vec<MediaFrameSourceGroup> = MediaFrameSourceGroup::FindAllAsync()
        .and_then(|op| op.get())
        .map_err(|e| format!("FindAllAsync failed: {e}"))?
        .into_iter()
        .collect();
    let (group, source_id) =
        pick_matching_group(&groups, Some("vid_046d&pid_086e"), Some("Logi Group Camera"))
            .ok_or_else(|| "Group not found".to_owned())?;
    let settings = MediaCaptureInitializationSettings::new().map_err(|e| e.to_string())?;
    settings.SetSourceGroup(&group).map_err(|e| e.to_string())?;
    settings
        .SetSharingMode(MediaCaptureSharingMode::ExclusiveControl)
        .map_err(|e| e.to_string())?;
    settings
        .SetStreamingCaptureMode(StreamingCaptureMode::Video)
        .map_err(|e| e.to_string())?;
    settings
        .SetMemoryPreference(MediaCaptureMemoryPreference::Cpu)
        .map_err(|e| e.to_string())?;

    let obs_sim = MediaCapture::new().map_err(|e| e.to_string())?;
    obs_sim
        .InitializeWithSettingsAsync(&settings)
        .and_then(|op| op.get())
        .map_err(|e| format!("OBS ExclusiveControl simulation failed: {e}"))?;
    let obs_src = obs_sim
        .FrameSources()
        .and_then(|s| s.Lookup(&source_id))
        .map_err(|e| e.to_string())?;
    let obs_reader = obs_sim
        .CreateFrameReaderAsync(&obs_src)
        .and_then(|op| op.get())
        .map_err(|e| e.to_string())?;
    let obs_status = obs_reader
        .StartAsync()
        .and_then(|op| op.get())
        .map_err(|e| e.to_string())?;
    println!("OBS ExclusiveControl simulation StartStatus = {obs_status:?}");

    // Check if BOTH receive frames simultaneously!
    let start2 = Instant::now();
    let mut preview_ok = false;
    let mut obs_ok = false;
    while start2.elapsed() < Duration::from_secs(3) && (!preview_ok || !obs_ok) {
        if !preview_ok && let Some((_, w, h)) = session.try_read_frame() {
            println!("SharedReadOnly preview frame while OBS active: {w}x{h}");
            preview_ok = true;
        }
        if !obs_ok && let Ok(fr) = obs_reader.TryAcquireLatestFrame() {
            let _ = fr.Close();
            println!("OBS ExclusiveControl received live frame simultaneously!");
            obs_ok = true;
        }
        std::thread::sleep(Duration::from_millis(30));
    }

    if let Ok(op) = obs_reader.StopAsync() {
        let _ = op.get();
    }
    let _ = obs_reader.Close();
    let _ = obs_sim.Close();
    drop(session);

    println!("Result: preview_ok={preview_ok}, obs_coexistence_ok={obs_ok}");
    Ok(())
}
