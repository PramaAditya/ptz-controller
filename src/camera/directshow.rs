use std::ffi::c_void;
use std::ptr;

use windows::Win32::Foundation::S_OK;
use windows::Win32::Media::DirectShow::{
    CameraControl_Exposure, CameraControl_Flags_Auto, CameraControl_Flags_Manual,
    CameraControl_Focus, CameraControl_Pan, CameraControl_Tilt, CameraControl_Zoom,
    IAMCameraControl, IBaseFilter, ICreateDevEnum,
};
use windows::Win32::Media::KernelStreaming::IKsPropertySet;
use windows::Win32::System::Com::StructuredStorage::IPropertyBag;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize, IEnumMoniker, IMoniker,
};
use windows::Win32::System::Variant::{VARIANT, VT_BSTR, VariantClear};
use windows::core::{Interface, PCWSTR, w};

use crate::camera::ks_types::{
    CLSID_SYSTEM_DEVICE_ENUM, CLSID_VIDEO_INPUT_DEVICE_CATEGORY,
    KSPROPERTY_CAMERACONTROL_PAN_RELATIVE, KSPROPERTY_CAMERACONTROL_TILT_RELATIVE,
    KSPROPERTY_CAMERACONTROL_ZOOM_RELATIVE, KsPropertyCameraControlS,
    PROPSETID_VIDCAP_CAMERACONTROL,
};
use crate::camera::{CameraDeviceInfo, CameraError, ControlRange, ControlValue};

const KSPROPERTY_SUPPORT_SET: u32 = 0x0000_0002;

/// RAII guard for COM apartment initialization on the current thread.
pub struct ComStaGuard {
    should_uninit: bool,
}

impl ComStaGuard {
    pub fn init() -> Result<Self, CameraError> {
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if hr.is_ok() {
            Ok(Self {
                should_uninit: true,
            })
        } else {
            Err(CameraError::Windows(windows::core::Error::from(hr)))
        }
    }
}

impl Drop for ComStaGuard {
    fn drop(&mut self) {
        if self.should_uninit {
            unsafe { CoUninitialize() };
        }
    }
}

pub struct DirectShowCamera {
    pub device: CameraDeviceInfo,
    _filter: IBaseFilter,
    cam_ctrl: Option<IAMCameraControl>,
    ks_prop: Option<IKsPropertySet>,
    pub supports_pan_relative: bool,
    pub supports_tilt_relative: bool,
    pub supports_zoom_relative: bool,
    pub pan_abs: Option<ControlRange>,
    pub tilt_abs: Option<ControlRange>,
}

impl DirectShowCamera {
    pub fn open(
        preferred_device_path: Option<&str>,
    ) -> Result<(Self, Vec<CameraDeviceInfo>), CameraError> {
        let entries = enumerate_monikers()?;
        if entries.is_empty() {
            return Err(CameraError::NoDevicesFound);
        }

        let ptz_entries: Vec<&(CameraDeviceInfo, IMoniker)> =
            entries.iter().filter(|(info, _)| info.is_ptz).collect();

        if ptz_entries.is_empty() {
            return Err(CameraError::NoDevicesFound);
        }

        let devices: Vec<CameraDeviceInfo> =
            ptz_entries.iter().map(|(info, _)| info.clone()).collect();

        let selected_idx = if let Some(target_id) = preferred_device_path {
            ptz_entries
                .iter()
                .position(|(info, _)| info.id().eq_ignore_ascii_case(target_id))
                .or_else(|| pick_default_index(&devices))
                .ok_or_else(|| CameraError::DeviceNotFound(target_id.to_owned()))?
        } else {
            pick_default_index(&devices).ok_or(CameraError::NoDevicesFound)?
        };

        let (device, moniker) = ptz_entries[selected_idx];
        let filter: IBaseFilter = unsafe { moniker.BindToObject(None, None)? };
        let cam_ctrl: Option<IAMCameraControl> = filter.cast().ok();
        let ks_prop: Option<IKsPropertySet> = filter.cast().ok();

        let supports_pan_relative = ks_prop
            .as_ref()
            .is_some_and(|ks| query_ks_set_supported(ks, KSPROPERTY_CAMERACONTROL_PAN_RELATIVE));
        let supports_tilt_relative = ks_prop
            .as_ref()
            .is_some_and(|ks| query_ks_set_supported(ks, KSPROPERTY_CAMERACONTROL_TILT_RELATIVE));
        let supports_zoom_relative = ks_prop
            .as_ref()
            .is_some_and(|ks| query_ks_set_supported(ks, KSPROPERTY_CAMERACONTROL_ZOOM_RELATIVE));

        let mut cam = Self {
            device: device.clone(),
            _filter: filter,
            cam_ctrl,
            ks_prop,
            supports_pan_relative,
            supports_tilt_relative,
            supports_zoom_relative,
            pan_abs: None,
            tilt_abs: None,
        };

        cam.pan_abs = cam.read_control(CameraControl_Pan.0).map(|v| v.range);
        cam.tilt_abs = cam.read_control(CameraControl_Tilt.0).map(|v| v.range);

        log::info!(
            "Opened camera '{}' (pan_rel={}, tilt_rel={}, pan_abs={}, tilt_abs={}, zoom_rel={}, iam={})",
            device.friendly_name,
            cam.supports_pan_relative,
            cam.supports_tilt_relative,
            cam.pan_abs.is_some(),
            cam.tilt_abs.is_some(),
            cam.supports_zoom_relative,
            cam.cam_ctrl.is_some()
        );

        Ok((cam, devices))
    }

    pub fn set_pan_relative(&self, direction: i32) -> Result<(), CameraError> {
        let dir = direction.clamp(-1, 1);
        self.set_ks_relative(KSPROPERTY_CAMERACONTROL_PAN_RELATIVE, dir)
    }

    pub fn set_tilt_relative(&self, direction: i32) -> Result<(), CameraError> {
        let dir = direction.clamp(-1, 1);
        self.set_ks_relative(KSPROPERTY_CAMERACONTROL_TILT_RELATIVE, dir)
    }

    pub fn set_zoom_relative(&self, direction: i32) -> Result<(), CameraError> {
        let dir = direction.clamp(-1, 1);
        self.set_ks_relative(KSPROPERTY_CAMERACONTROL_ZOOM_RELATIVE, dir)
    }

    pub fn step_pan_absolute(&self, direction: i32, multiplier: i32) -> Result<(), CameraError> {
        let Some(range) = self.pan_abs else {
            return Err(CameraError::InterfaceUnavailable("CameraControl_Pan"));
        };
        let current = self
            .read_control(CameraControl_Pan.0)
            .map(|c| c.value)
            .unwrap_or(range.default);
        let base_step = ((range.max - range.min) / 45).max(range.step).max(1);
        let next = (current + direction.clamp(-1, 1) * base_step * multiplier.max(1))
            .clamp(range.min, range.max);
        let ctrl = self
            .cam_ctrl
            .as_ref()
            .ok_or(CameraError::InterfaceUnavailable("IAMCameraControl"))?;
        unsafe {
            ctrl.Set(CameraControl_Pan.0, next, CameraControl_Flags_Manual.0)?;
        }
        Ok(())
    }

    pub fn step_tilt_absolute(&self, direction: i32, multiplier: i32) -> Result<(), CameraError> {
        let Some(range) = self.tilt_abs else {
            return Err(CameraError::InterfaceUnavailable("CameraControl_Tilt"));
        };
        let current = self
            .read_control(CameraControl_Tilt.0)
            .map(|c| c.value)
            .unwrap_or(range.default);
        let base_step = ((range.max - range.min) / 45).max(range.step).max(1);
        let next = (current + direction.clamp(-1, 1) * base_step * multiplier.max(1))
            .clamp(range.min, range.max);
        let ctrl = self
            .cam_ctrl
            .as_ref()
            .ok_or(CameraError::InterfaceUnavailable("IAMCameraControl"))?;
        unsafe {
            ctrl.Set(CameraControl_Tilt.0, next, CameraControl_Flags_Manual.0)?;
        }
        Ok(())
    }

    pub fn stop_ptz(&self) {
        if self.supports_pan_relative
            && let Err(err) = self.set_pan_relative(0)
        {
            log::warn!("stop_ptz pan=0 failed: {err}");
        }
        if self.supports_tilt_relative
            && let Err(err) = self.set_tilt_relative(0)
        {
            log::warn!("stop_ptz tilt=0 failed: {err}");
        }
        if self.supports_zoom_relative
            && let Err(err) = self.set_zoom_relative(0)
        {
            log::warn!("stop_ptz zoom=0 failed: {err}");
        }
    }

    pub fn read_zoom(&self) -> Option<ControlValue> {
        self.read_control(CameraControl_Zoom.0)
    }

    pub fn read_focus(&self) -> Option<ControlValue> {
        self.read_control(CameraControl_Focus.0)
    }

    pub fn read_exposure(&self) -> Option<ControlValue> {
        self.read_control(CameraControl_Exposure.0)
    }

    pub fn set_zoom(&self, value: i32) -> Result<(), CameraError> {
        let ctrl = self
            .cam_ctrl
            .as_ref()
            .ok_or(CameraError::InterfaceUnavailable("IAMCameraControl"))?;
        unsafe {
            ctrl.Set(
                CameraControl_Zoom.0,
                value,
                CameraControl_Flags_Manual.0,
            )?;
        }
        Ok(())
    }

    pub fn set_focus(&self, auto: bool, value: i32) -> Result<(), CameraError> {
        let ctrl = self
            .cam_ctrl
            .as_ref()
            .ok_or(CameraError::InterfaceUnavailable("IAMCameraControl"))?;
        let flags = if auto {
            CameraControl_Flags_Auto.0
        } else {
            CameraControl_Flags_Manual.0
        };
        unsafe {
            ctrl.Set(CameraControl_Focus.0, value, flags)?;
        }
        Ok(())
    }

    pub fn set_exposure(&self, auto: bool, value: i32) -> Result<(), CameraError> {
        let ctrl = self
            .cam_ctrl
            .as_ref()
            .ok_or(CameraError::InterfaceUnavailable("IAMCameraControl"))?;
        let flags = if auto {
            CameraControl_Flags_Auto.0
        } else {
            CameraControl_Flags_Manual.0
        };
        unsafe {
            ctrl.Set(CameraControl_Exposure.0, value, flags)?;
        }
        Ok(())
    }

    fn set_ks_relative(&self, prop_id: u32, value: i32) -> Result<(), CameraError> {
        let ks = self
            .ks_prop
            .as_ref()
            .ok_or(CameraError::InterfaceUnavailable("IKsPropertySet"))?;
        let data = KsPropertyCameraControlS::new_relative_set(prop_id, value);
        unsafe {
            ks.Set(
                &PROPSETID_VIDCAP_CAMERACONTROL,
                prop_id,
                ptr::null(),
                0,
                (&raw const data).cast::<c_void>(),
                std::mem::size_of::<KsPropertyCameraControlS>() as u32,
            )?;
        }
        Ok(())
    }

    fn read_control(&self, property: i32) -> Option<ControlValue> {
        let ctrl = self.cam_ctrl.as_ref()?;
        let mut min = 0;
        let mut max = 0;
        let mut step = 1;
        let mut default = 0;
        let mut caps = 0;
        let mut value = 0;
        let mut flags = 0;

        unsafe {
            ctrl.GetRange(
                property,
                &mut min,
                &mut max,
                &mut step,
                &mut default,
                &mut caps,
            )
            .ok()?;
            ctrl.Get(property, &mut value, &mut flags).ok()?;
        }

        Some(ControlValue {
            value,
            auto: (flags & CameraControl_Flags_Auto.0) != 0,
            range: ControlRange {
                min,
                max,
                step: step.max(1),
                default,
                supports_auto: (caps & CameraControl_Flags_Auto.0) != 0,
                supports_manual: (caps & CameraControl_Flags_Manual.0) != 0,
            },
        })
    }
}

pub fn enumerate_video_devices() -> Result<Vec<CameraDeviceInfo>, CameraError> {
    let entries = enumerate_monikers()?;
    Ok(entries
        .into_iter()
        .filter(|(info, _)| info.is_ptz)
        .map(|(info, _)| info)
        .collect())
}

fn pick_default_index(devices: &[CameraDeviceInfo]) -> Option<usize> {
    devices
        .iter()
        .position(|d| d.is_logi_group)
        .or_else(|| devices.iter().position(|d| d.is_ptz))
        .or_else(|| (!devices.is_empty()).then_some(0))
}

fn enumerate_monikers() -> Result<Vec<(CameraDeviceInfo, IMoniker)>, CameraError> {
    let dev_enum: ICreateDevEnum =
        unsafe { CoCreateInstance(&CLSID_SYSTEM_DEVICE_ENUM, None, CLSCTX_INPROC_SERVER)? };

    let mut enum_moniker: Option<IEnumMoniker> = None;
    let hr = unsafe {
        dev_enum.CreateClassEnumerator(&CLSID_VIDEO_INPUT_DEVICE_CATEGORY, &mut enum_moniker, 0)
    };
    if hr.is_err() {
        return Ok(Vec::new());
    }
    let Some(enum_moniker) = enum_moniker else {
        return Ok(Vec::new());
    };

    let mut result = Vec::new();
    let mut index = 0usize;
    loop {
        let mut batch: [Option<IMoniker>; 1] = [None];
        let mut fetched = 0u32;
        let hr = unsafe { enum_moniker.Next(&mut batch, Some(&mut fetched)) };
        if hr != S_OK || fetched == 0 {
            break;
        }
        let Some(moniker) = batch[0].take() else {
            break;
        };

        let (friendly_name, device_path) = read_moniker_props(&moniker);
        let path_lower = device_path.to_ascii_lowercase();
        let name_lower = friendly_name.to_ascii_lowercase();
        let is_logi_group =
            path_lower.contains("vid_046d&pid_086e") || name_lower.contains("logi group");
        let is_ptz = probe_moniker_is_ptz(&moniker, &device_path);

        result.push((
            CameraDeviceInfo {
                index,
                friendly_name,
                device_path,
                is_logi_group,
                is_ptz,
            },
            moniker,
        ));
        index += 1;
    }

    Ok(result)
}

/// Checks whether a DirectShow video device actually exposes Pan or Tilt (relative or absolute)
/// without building a capture graph or locking the stream. Skips virtual cameras with no hardware path.
fn probe_moniker_is_ptz(moniker: &IMoniker, device_path: &str) -> bool {
    if device_path.is_empty() || !device_path.starts_with(r"\\?\") {
        return false;
    }
    let Ok(filter): Result<IBaseFilter, _> = (unsafe { moniker.BindToObject(None, None) }) else {
        return false;
    };

    if let Ok(ks) = filter.cast::<IKsPropertySet>()
        && (query_ks_set_supported(&ks, KSPROPERTY_CAMERACONTROL_PAN_RELATIVE)
            || query_ks_set_supported(&ks, KSPROPERTY_CAMERACONTROL_TILT_RELATIVE))
    {
        return true;
    }

    if let Ok(ctrl) = filter.cast::<IAMCameraControl>() {
        let mut min = 0;
        let mut max = 0;
        let mut step = 0;
        let mut def = 0;
        let mut caps = 0;
        let has_pan = unsafe {
            ctrl.GetRange(
                CameraControl_Pan.0,
                &mut min,
                &mut max,
                &mut step,
                &mut def,
                &mut caps,
            )
            .is_ok()
                && max > min
        };
        if has_pan {
            return true;
        }
        let has_tilt = unsafe {
            ctrl.GetRange(
                CameraControl_Tilt.0,
                &mut min,
                &mut max,
                &mut step,
                &mut def,
                &mut caps,
            )
            .is_ok()
                && max > min
        };
        if has_tilt {
            return true;
        }
    }

    false
}

fn read_moniker_props(moniker: &IMoniker) -> (String, String) {
    let bag: Result<IPropertyBag, _> = unsafe { moniker.BindToStorage(None, None) };
    let Ok(bag) = bag else {
        return ("Unknown Camera".to_owned(), String::new());
    };

    let friendly = read_bag_string(&bag, w!("FriendlyName"))
        .unwrap_or_else(|| "Unknown Camera".to_owned());
    let path = read_bag_string(&bag, w!("DevicePath")).unwrap_or_default();
    (friendly, path)
}

fn read_bag_string(bag: &IPropertyBag, name: PCWSTR) -> Option<String> {
    let mut var = VARIANT::default();
    unsafe {
        if bag.Read(name, &mut var, None).is_err() {
            return None;
        }
        let result = if var.Anonymous.Anonymous.vt == VT_BSTR {
            Some((*var.Anonymous.Anonymous.Anonymous.bstrVal).to_string())
        } else {
            None
        };
        let _ = VariantClear(&mut var);
        result
    }
}

fn query_ks_set_supported(ks: &IKsPropertySet, prop_id: u32) -> bool {
    unsafe {
        ks.QuerySupported(&PROPSETID_VIDCAP_CAMERACONTROL, prop_id)
            .map(|flags| (flags & KSPROPERTY_SUPPORT_SET) != 0)
            .unwrap_or(false)
    }
}

/// CLI `--probe` diagnostic: lists all DirectShow video devices and prints PTZ capabilities.
pub fn run_probe_cli() -> Result<(), CameraError> {
    let _com = ComStaGuard::init()?;
    let all_entries = enumerate_monikers()?;
    println!(
        "Found {} DirectShow video input device(s) ({} PTZ):",
        all_entries.len(),
        all_entries.iter().filter(|(d, _)| d.is_ptz).count()
    );
    for (dev, _) in &all_entries {
        println!(
            "  [{}] {}{}{} ({})",
            dev.index,
            dev.friendly_name,
            if dev.is_ptz { " [PTZ]" } else { "" },
            if dev.is_logi_group {
                " [LOGI GROUP]"
            } else {
                ""
            },
            dev.device_path
        );
    }

    if all_entries.is_empty() {
        return Ok(());
    }

    let (cam, _) = DirectShowCamera::open(None)?;
    println!("\nSelected: {}", cam.device.friendly_name);
    println!("  PAN_RELATIVE (ID 10):  {}", cam.supports_pan_relative);
    println!("  TILT_RELATIVE (ID 11): {}", cam.supports_tilt_relative);
    if let Some(z) = cam.read_zoom() {
        println!(
            "  Zoom:     {} (range {}..={}, step {}, default {})",
            z.value, z.range.min, z.range.max, z.range.step, z.range.default
        );
    }
    if let Some(f) = cam.read_focus() {
        println!(
            "  Focus:    {} [{}] (range {}..={}, step {}, default {})",
            f.value,
            if f.auto { "Auto" } else { "Manual" },
            f.range.min,
            f.range.max,
            f.range.step,
            f.range.default
        );
    }
    if let Some(e) = cam.read_exposure() {
        println!(
            "  Exposure: {} [{}] (range {}..={}, step {}, default {})",
            e.value,
            if e.auto { "Auto" } else { "Manual" },
            e.range.min,
            e.range.max,
            e.range.step,
            e.range.default
        );
    }
    Ok(())
}

/// CLI `--nudge-*` smoke test: sends a timed relative pan/tilt pulse and stops cleanly.
pub fn run_nudge_cli(pan: i32, tilt: i32, duration_ms: u64) -> Result<(), CameraError> {
    let _com = ComStaGuard::init()?;
    let (cam, _) = DirectShowCamera::open(None)?;
    println!(
        "Nudging '{}' pan={} tilt={} for {}ms...",
        cam.device.friendly_name, pan, tilt, duration_ms
    );
    if pan != 0 {
        cam.set_pan_relative(pan)?;
    }
    if tilt != 0 {
        cam.set_tilt_relative(tilt)?;
    }
    std::thread::sleep(std::time::Duration::from_millis(duration_ms.clamp(20, 3000)));
    cam.stop_ptz();
    println!("Stopped cleanly.");
    Ok(())
}

#[cfg(test)]
mod hw_tests {
    use super::*;

    #[test]
    #[ignore]
    fn probe_zoom_granularity_and_relative() {
        let _com = ComStaGuard::init().unwrap();
        let (cam, _) = DirectShowCamera::open(None).unwrap();
        let ks = cam.ks_prop.as_ref().unwrap();

        let zoom_rel = query_ks_set_supported(ks, 13);
        let focus_rel = query_ks_set_supported(ks, 16);
        let pantilt_rel = query_ks_set_supported(ks, 17);
        println!("KSPROPERTY support on '{}':", cam.device.friendly_name);
        println!("  ZOOM_RELATIVE (13):    {zoom_rel}");
        println!("  FOCUS_RELATIVE (16):   {focus_rel}");
        println!("  PANTILT_RELATIVE (17): {pantilt_rel}");

        let orig = cam.read_zoom().unwrap();
        println!(
            "Original Zoom: {} (range {}..={}, step={}, default={})",
            orig.value, orig.range.min, orig.range.max, orig.range.step, orig.range.default
        );

        for target in [100, 101, 105, 110, 125, 137, 150, 175, 200] {
            let set_res = cam.set_zoom(target);
            std::thread::sleep(std::time::Duration::from_millis(120));
            let readback = cam.read_zoom().map(|z| z.value);
            println!("  Set({target:4}) -> {set_res:?}, Readback = {readback:?}");
        }

        // Restore original zoom
        let _ = cam.set_zoom(orig.value);
    }
}
