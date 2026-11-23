use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::camera::directshow::{ComStaGuard, DirectShowCamera, enumerate_video_devices};
use crate::camera::{CameraCommand, CameraState};

/// Dead-man's switch timeout: if no hold/heartbeat arrives within 400ms, stop PTZ motors.
const HOLD_WATCHDOG_TIMEOUT: Duration = Duration::from_millis(400);
/// Step interval when emulating continuous Pan/Tilt hold on absolute-only UVC cameras.
const ABS_PTZ_STEP_INTERVAL: Duration = Duration::from_millis(45);
const IDLE_POLL_TIMEOUT: Duration = Duration::from_millis(250);

pub fn run_worker(
    rx: Receiver<CameraCommand>,
    state: Arc<Mutex<CameraState>>,
    repaint: Arc<Mutex<Option<egui::Context>>>,
    initial_device_path: Option<String>,
) {
    let _com = match ComStaGuard::init() {
        Ok(guard) => guard,
        Err(err) => {
            log::error!("COM STA initialization failed: {err}");
            update_state(&state, &repaint, |s| {
                s.connected = false;
                s.last_error = Some(err.to_string());
            });
            return;
        }
    };

    let mut preferred_path = initial_device_path;
    let mut camera: Option<DirectShowCamera> = None;
    let mut active_pan = 0i32;
    let mut active_tilt = 0i32;
    let mut active_zoom_rel = 0i32;
    let mut hold_deadline: Option<Instant> = None;
    let mut nudge_deadline: Option<Instant> = None;
    let mut abs_step_deadline: Option<Instant> = None;

    connect_camera(&mut camera, preferred_path.as_deref(), &state, &repaint);

    loop {
        let now = Instant::now();

        // Check nudge expiry first
        if let Some(deadline) = nudge_deadline
            && now >= deadline
        {
            nudge_deadline = None;
            stop_motion(
                camera.as_ref(),
                &mut active_pan,
                &mut active_tilt,
                &mut active_zoom_rel,
                &mut hold_deadline,
                &mut abs_step_deadline,
                &state,
                &repaint,
            );
        }

        // Check dead-man's switch watchdog
        if let Some(deadline) = hold_deadline
            && now >= deadline
        {
            log::debug!("Hold watchdog expired; stopping PTZ motors");
            stop_motion(
                camera.as_ref(),
                &mut active_pan,
                &mut active_tilt,
                &mut active_zoom_rel,
                &mut hold_deadline,
                &mut abs_step_deadline,
                &state,
                &repaint,
            );
        }

        // Step absolute-only Pan/Tilt cameras continuously while held
        if let Some(step_at) = abs_step_deadline
            && now >= step_at
        {
            if let Some(cam) = camera.as_ref() {
                if active_pan != 0 && !cam.supports_pan_relative && cam.pan_abs.is_some()
                    && let Err(err) = cam.step_pan_absolute(active_pan, 1)
                {
                    record_error(&state, &repaint, err.to_string());
                }
                if active_tilt != 0 && !cam.supports_tilt_relative && cam.tilt_abs.is_some()
                    && let Err(err) = cam.step_tilt_absolute(active_tilt, 1)
                {
                    record_error(&state, &repaint, err.to_string());
                }
                abs_step_deadline = Some(now + ABS_PTZ_STEP_INTERVAL);
            } else {
                abs_step_deadline = None;
            }
        }

        let wait_duration = next_timeout(now, hold_deadline, nudge_deadline, abs_step_deadline);
        let cmd = match rx.recv_timeout(wait_duration) {
            Ok(cmd) => cmd,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        };

        match cmd {
            CameraCommand::Shutdown => {
                stop_motion(
                    camera.as_ref(),
                    &mut active_pan,
                    &mut active_tilt,
                    &mut active_zoom_rel,
                    &mut hold_deadline,
                    &mut abs_step_deadline,
                    &state,
                    &repaint,
                );
                break;
            }
            CameraCommand::SelectDevice(path) => {
                stop_motion(
                    camera.as_ref(),
                    &mut active_pan,
                    &mut active_tilt,
                    &mut active_zoom_rel,
                    &mut hold_deadline,
                    &mut abs_step_deadline,
                    &state,
                    &repaint,
                );
                nudge_deadline = None;
                preferred_path = path;
                connect_camera(&mut camera, preferred_path.as_deref(), &state, &repaint);
            }
            CameraCommand::Reconnect => {
                stop_motion(
                    camera.as_ref(),
                    &mut active_pan,
                    &mut active_tilt,
                    &mut active_zoom_rel,
                    &mut hold_deadline,
                    &mut abs_step_deadline,
                    &state,
                    &repaint,
                );
                nudge_deadline = None;
                connect_camera(&mut camera, preferred_path.as_deref(), &state, &repaint);
            }
            CameraCommand::StopAll => {
                nudge_deadline = None;
                stop_motion(
                    camera.as_ref(),
                    &mut active_pan,
                    &mut active_tilt,
                    &mut active_zoom_rel,
                    &mut hold_deadline,
                    &mut abs_step_deadline,
                    &state,
                    &repaint,
                );
            }
            CameraCommand::Heartbeat => {
                if active_pan != 0 || active_tilt != 0 || active_zoom_rel != 0 {
                    hold_deadline = Some(Instant::now() + HOLD_WATCHDOG_TIMEOUT);
                }
            }
            CameraCommand::HoldMotion { pan, tilt } => {
                nudge_deadline = None;
                let pan = pan.clamp(-1, 1);
                let tilt = tilt.clamp(-1, 1);
                if pan == 0 && tilt == 0 {
                    stop_motion(
                        camera.as_ref(),
                        &mut active_pan,
                        &mut active_tilt,
                        &mut active_zoom_rel,
                        &mut hold_deadline,
                        &mut abs_step_deadline,
                        &state,
                        &repaint,
                    );
                } else {
                    apply_motion(
                        camera.as_ref(),
                        pan,
                        tilt,
                        &mut active_pan,
                        &mut active_tilt,
                        &mut abs_step_deadline,
                        &state,
                        &repaint,
                    );
                    hold_deadline = Some(Instant::now() + HOLD_WATCHDOG_TIMEOUT);
                }
            }
            CameraCommand::Nudge {
                pan,
                tilt,
                duration_ms,
            } => {
                hold_deadline = None;
                abs_step_deadline = None;
                let pan = pan.clamp(-1, 1);
                let tilt = tilt.clamp(-1, 1);
                if pan == 0 && tilt == 0 {
                    nudge_deadline = None;
                    stop_motion(
                        camera.as_ref(),
                        &mut active_pan,
                        &mut active_tilt,
                        &mut active_zoom_rel,
                        &mut hold_deadline,
                        &mut abs_step_deadline,
                        &state,
                        &repaint,
                    );
                } else if let Some(cam) = camera.as_ref() {
                    let needs_relative_stop =
                        (pan != 0 && cam.supports_pan_relative)
                            || (tilt != 0 && cam.supports_tilt_relative);

                    if pan != 0 && !cam.supports_pan_relative && cam.pan_abs.is_some() {
                        let mult = ((duration_ms / 80).clamp(1, 5)) as i32;
                        if let Err(err) = cam.step_pan_absolute(pan, mult) {
                            record_error(&state, &repaint, err.to_string());
                        }
                    }
                    if tilt != 0 && !cam.supports_tilt_relative && cam.tilt_abs.is_some() {
                        let mult = ((duration_ms / 80).clamp(1, 5)) as i32;
                        if let Err(err) = cam.step_tilt_absolute(tilt, mult) {
                            record_error(&state, &repaint, err.to_string());
                        }
                    }

                    if needs_relative_stop {
                        apply_motion(
                            Some(cam),
                            if cam.supports_pan_relative { pan } else { 0 },
                            if cam.supports_tilt_relative { tilt } else { 0 },
                            &mut active_pan,
                            &mut active_tilt,
                            &mut abs_step_deadline,
                            &state,
                            &repaint,
                        );
                        nudge_deadline = Some(
                            Instant::now() + Duration::from_millis(duration_ms.clamp(20, 2000)),
                        );
                    }
                }
            }
            CameraCommand::SetZoom(value) => {
                if let Some(cam) = camera.as_ref() {
                    match cam.set_zoom(value) {
                        Ok(()) => {
                            update_state(&state, &repaint, |s| {
                                if let Some(z) = s.zoom.as_mut() {
                                    z.value = value.clamp(z.range.min, z.range.max);
                                    z.auto = false;
                                }
                                s.last_error = None;
                            });
                        }
                        Err(err) => record_error(&state, &repaint, err.to_string()),
                    }
                }
            }
            CameraCommand::HoldZoomRelative(dir) => {
                let dir = dir.clamp(-1, 1);
                if let Some(cam) = camera.as_ref()
                    && cam.supports_zoom_relative
                {
                    if dir != active_zoom_rel {
                        match cam.set_zoom_relative(dir) {
                            Ok(()) => active_zoom_rel = dir,
                            Err(err) => record_error(&state, &repaint, err.to_string()),
                        }
                    }
                    if dir != 0 {
                        hold_deadline = Some(Instant::now() + HOLD_WATCHDOG_TIMEOUT);
                    } else if active_pan == 0 && active_tilt == 0 {
                        hold_deadline = None;
                    }
                }
            }
            CameraCommand::SetFocus { auto, value } => {
                if let Some(cam) = camera.as_ref() {
                    match cam.set_focus(auto, value) {
                        Ok(()) => {
                            let fresh = cam.read_focus();
                            update_state(&state, &repaint, |s| {
                                if let Some(f) = fresh {
                                    s.focus = Some(f);
                                } else if let Some(f) = s.focus.as_mut() {
                                    f.auto = auto;
                                    f.value = value.clamp(f.range.min, f.range.max);
                                }
                                s.last_error = None;
                            });
                        }
                        Err(err) => record_error(&state, &repaint, err.to_string()),
                    }
                }
            }
            CameraCommand::SetExposure { auto, value } => {
                if let Some(cam) = camera.as_ref() {
                    match cam.set_exposure(auto, value) {
                        Ok(()) => {
                            let fresh = cam.read_exposure();
                            update_state(&state, &repaint, |s| {
                                if let Some(e) = fresh {
                                    s.exposure = Some(e);
                                } else if let Some(e) = s.exposure.as_mut() {
                                    e.auto = auto;
                                    e.value = value.clamp(e.range.min, e.range.max);
                                }
                                s.last_error = None;
                            });
                        }
                        Err(err) => record_error(&state, &repaint, err.to_string()),
                    }
                }
            }
        }
    }

    if let Some(cam) = camera.take() {
        cam.stop_ptz();
    }
}

fn next_timeout(
    now: Instant,
    hold_deadline: Option<Instant>,
    nudge_deadline: Option<Instant>,
    abs_step_deadline: Option<Instant>,
) -> Duration {
    let earliest = [hold_deadline, nudge_deadline, abs_step_deadline]
        .into_iter()
        .flatten()
        .min();
    match earliest {
        Some(deadline) => deadline.saturating_duration_since(now),
        None => IDLE_POLL_TIMEOUT,
    }
}

fn connect_camera(
    slot: &mut Option<DirectShowCamera>,
    preferred_path: Option<&str>,
    state: &Arc<Mutex<CameraState>>,
    repaint: &Arc<Mutex<Option<egui::Context>>>,
) {
    if let Some(old) = slot.take() {
        old.stop_ptz();
    }

    match DirectShowCamera::open(preferred_path) {
        Ok((cam, devices)) => {
            let zoom = cam.read_zoom();
            let focus = cam.read_focus();
            let exposure = cam.read_exposure();
            let name = cam.device.friendly_name.clone();
            let id = cam.device.id().to_owned();
            let pan_rel = cam.supports_pan_relative;
            let tilt_rel = cam.supports_tilt_relative;
            let pan_abs = cam.pan_abs.is_some();
            let tilt_abs = cam.tilt_abs.is_some();
            let zoom_rel = cam.supports_zoom_relative;

            *slot = Some(cam);
            update_state(state, repaint, |s| {
                s.devices = devices;
                s.selected_device_path = Some(id);
                s.connected_name = Some(name);
                s.connected = true;
                s.last_error = None;
                s.supports_pan_relative = pan_rel;
                s.supports_tilt_relative = tilt_rel;
                s.supports_pan_absolute = pan_abs;
                s.supports_tilt_absolute = tilt_abs;
                s.supports_zoom_relative = zoom_rel;
                s.active_pan = 0;
                s.active_tilt = 0;
                s.zoom = zoom;
                s.focus = focus;
                s.exposure = exposure;
            });
        }
        Err(err) => {
            log::warn!("Failed to open camera: {err}");
            let devices = enumerate_video_devices().unwrap_or_default();
            update_state(state, repaint, |s| {
                s.devices = devices;
                s.connected = false;
                s.connected_name = None;
                s.supports_pan_relative = false;
                s.supports_tilt_relative = false;
                s.supports_pan_absolute = false;
                s.supports_tilt_absolute = false;
                s.supports_zoom_relative = false;
                s.active_pan = 0;
                s.active_tilt = 0;
                s.zoom = None;
                s.focus = None;
                s.exposure = None;
                s.last_error = Some(err.to_string());
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_motion(
    camera: Option<&DirectShowCamera>,
    pan: i32,
    tilt: i32,
    active_pan: &mut i32,
    active_tilt: &mut i32,
    abs_step_deadline: &mut Option<Instant>,
    state: &Arc<Mutex<CameraState>>,
    repaint: &Arc<Mutex<Option<egui::Context>>>,
) {
    let Some(cam) = camera else {
        return;
    };

    let mut error_msg: Option<String> = None;
    let mut needs_abs_loop = false;

    if pan != *active_pan {
        if cam.supports_pan_relative {
            match cam.set_pan_relative(pan) {
                Ok(()) => *active_pan = pan,
                Err(err) => {
                    log::warn!("set_pan_relative({pan}) failed: {err}");
                    error_msg = Some(err.to_string());
                }
            }
        } else if cam.pan_abs.is_some() {
            *active_pan = pan;
            if pan != 0 {
                if let Err(err) = cam.step_pan_absolute(pan, 1) {
                    error_msg = Some(err.to_string());
                }
                needs_abs_loop = true;
            }
        }
    } else if pan != 0 && !cam.supports_pan_relative && cam.pan_abs.is_some() {
        needs_abs_loop = true;
    }

    if tilt != *active_tilt {
        if cam.supports_tilt_relative {
            match cam.set_tilt_relative(tilt) {
                Ok(()) => *active_tilt = tilt,
                Err(err) => {
                    log::warn!("set_tilt_relative({tilt}) failed: {err}");
                    error_msg = Some(err.to_string());
                }
            }
        } else if cam.tilt_abs.is_some() {
            *active_tilt = tilt;
            if tilt != 0 {
                if let Err(err) = cam.step_tilt_absolute(tilt, 1) {
                    error_msg = Some(err.to_string());
                }
                needs_abs_loop = true;
            }
        }
    } else if tilt != 0 && !cam.supports_tilt_relative && cam.tilt_abs.is_some() {
        needs_abs_loop = true;
    }

    *abs_step_deadline = if needs_abs_loop {
        Some(Instant::now() + ABS_PTZ_STEP_INTERVAL)
    } else {
        None
    };

    let final_pan = *active_pan;
    let final_tilt = *active_tilt;
    update_state(state, repaint, |s| {
        s.active_pan = final_pan;
        s.active_tilt = final_tilt;
        if let Some(err) = error_msg {
            s.last_error = Some(err);
        } else {
            s.last_error = None;
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn stop_motion(
    camera: Option<&DirectShowCamera>,
    active_pan: &mut i32,
    active_tilt: &mut i32,
    active_zoom_rel: &mut i32,
    hold_deadline: &mut Option<Instant>,
    abs_step_deadline: &mut Option<Instant>,
    state: &Arc<Mutex<CameraState>>,
    repaint: &Arc<Mutex<Option<egui::Context>>>,
) {
    *hold_deadline = None;
    *abs_step_deadline = None;
    let was_moving = *active_pan != 0 || *active_tilt != 0 || *active_zoom_rel != 0;
    if let Some(cam) = camera {
        cam.stop_ptz();
    }
    *active_pan = 0;
    *active_tilt = 0;
    *active_zoom_rel = 0;
    if was_moving {
        update_state(state, repaint, |s| {
            s.active_pan = 0;
            s.active_tilt = 0;
        });
    }
}

fn record_error(
    state: &Arc<Mutex<CameraState>>,
    repaint: &Arc<Mutex<Option<egui::Context>>>,
    msg: String,
) {
    log::warn!("Camera control error: {msg}");
    update_state(state, repaint, |s| {
        s.last_error = Some(msg);
    });
}

fn update_state(
    state: &Arc<Mutex<CameraState>>,
    repaint: &Arc<Mutex<Option<egui::Context>>>,
    f: impl FnOnce(&mut CameraState),
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
