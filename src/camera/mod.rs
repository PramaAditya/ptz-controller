pub mod directshow;
pub mod ks_types;
pub mod preview;
pub mod worker;

use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CameraDeviceInfo {
    pub index: usize,
    pub friendly_name: String,
    pub device_path: String,
    pub is_logi_group: bool,
    pub is_ptz: bool,
}

impl CameraDeviceInfo {
    /// Stable identifier: uses `device_path` when present, falling back to `friendly_name`
    /// for virtual cameras that omit a USB device path.
    pub fn id(&self) -> &str {
        if self.device_path.is_empty() {
            &self.friendly_name
        } else {
            &self.device_path
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlRange {
    pub min: i32,
    pub max: i32,
    pub step: i32,
    pub default: i32,
    pub supports_auto: bool,
    pub supports_manual: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlValue {
    pub value: i32,
    pub auto: bool,
    pub range: ControlRange,
}

#[derive(Clone, Debug, Default)]
pub struct CameraState {
    pub devices: Vec<CameraDeviceInfo>,
    pub selected_device_path: Option<String>,
    pub connected_name: Option<String>,
    pub connected: bool,
    pub last_error: Option<String>,
    pub supports_pan_relative: bool,
    pub supports_tilt_relative: bool,
    pub supports_pan_absolute: bool,
    pub supports_tilt_absolute: bool,
    pub supports_zoom_relative: bool,
    pub active_pan: i32,
    pub active_tilt: i32,
    pub zoom: Option<ControlValue>,
    pub focus: Option<ControlValue>,
    pub exposure: Option<ControlValue>,
}

impl CameraState {
    pub fn supports_pan(&self) -> bool {
        self.supports_pan_relative || self.supports_pan_absolute
    }

    pub fn supports_tilt(&self) -> bool {
        self.supports_tilt_relative || self.supports_tilt_absolute
    }
}

#[derive(Clone, Debug)]
pub enum CameraCommand {
    SelectDevice(Option<String>),
    Reconnect,
    HoldMotion {
        pan: i32,
        tilt: i32,
    },
    Heartbeat,
    Nudge {
        pan: i32,
        tilt: i32,
        duration_ms: u64,
    },
    StopAll,
    SetZoom(i32),
    HoldZoomRelative(i32),
    SetFocus {
        auto: bool,
        value: i32,
    },
    SetExposure {
        auto: bool,
        value: i32,
    },
    Shutdown,
}

#[derive(Debug, Error)]
pub enum CameraError {
    #[error("No PTZ camera detected")]
    NoDevicesFound,
    #[error("Requested camera device not found: {0}")]
    DeviceNotFound(String),
    #[error("Camera interface not supported: {0}")]
    InterfaceUnavailable(&'static str),
    #[error("Windows COM/DirectShow error: {0}")]
    Windows(#[from] windows::core::Error),
}

pub struct CameraHandle {
    tx: Sender<CameraCommand>,
    state: Arc<Mutex<CameraState>>,
    repaint: Arc<Mutex<Option<egui::Context>>>,
    worker: Option<JoinHandle<()>>,
}

impl CameraHandle {
    pub fn spawn(initial_device_path: Option<String>) -> Self {
        let state = Arc::new(Mutex::new(CameraState::default()));
        let repaint = Arc::new(Mutex::new(None));
        let (tx, rx) = std::sync::mpsc::channel();

        let state_clone = Arc::clone(&state);
        let repaint_clone = Arc::clone(&repaint);
        let worker = std::thread::Builder::new()
            .name("camera-com-sta".to_owned())
            .spawn(move || {
                worker::run_worker(rx, state_clone, repaint_clone, initial_device_path);
            })
            .expect("failed to spawn camera COM worker thread");

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

    pub fn send(&self, cmd: CameraCommand) {
        if let Err(err) = self.tx.send(cmd) {
            log::warn!("failed to send camera command: {err}");
        }
    }

    pub fn snapshot(&self) -> CameraState {
        self.state
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }
}

impl Drop for CameraHandle {
    fn drop(&mut self) {
        let _ = self.tx.send(CameraCommand::StopAll);
        let _ = self.tx.send(CameraCommand::Shutdown);
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
    }
}
