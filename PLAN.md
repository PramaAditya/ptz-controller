# LogiCam Controller — Implementation Plan

Fast, lightweight native Windows GUI to control a **Logitech GROUP Camera** (`VID_046D&PID_086E`) — Pan, Tilt, Zoom, Focus, and Exposure — built with **Rust**, **egui/eframe**, and [**crmne/fastframe**](https://github.com/crmne/fastframe).

Designed to run **alongside Zoom, Google Meet, Microsoft Teams, or OBS** without locking the video stream.

---

## 1. Verified Hardware Capabilities (This Machine)

DirectShow probing against the connected `Logi Group Camera` (`\\?\usb#vid_046d&pid_086e&mi_00#...`) confirmed:

| Control | Interface | Property ID | Mode | Range / Values |
| --- | --- | --- | --- | --- |
| **Pan** | `IKsPropertySet` (`PROPSETID_VIDCAP_CAMERACONTROL`) | `10` (`PAN_RELATIVE`) | **Relative** (Get+Set) | `-1` (left), `0` (stop), `+1` (right) |
| **Tilt** | `IKsPropertySet` (`PROPSETID_VIDCAP_CAMERACONTROL`) | `11` (`TILT_RELATIVE`) | **Relative** (Get+Set) | `-1` (down), `0` (stop), `+1` (up) |
| **Zoom** | `IAMCameraControl` / `IKsPropertySet` | `3` (`ZOOM`) | Absolute (Get+Set) | `100..=1000`, step `1`, default `100` |
| **Focus** | `IAMCameraControl` / `IKsPropertySet` | `6` (`FOCUS`) | Auto + Manual | `0..=255`, step `1`, default `0` |
| **Exposure** | `IAMCameraControl` / `IKsPropertySet` | `4` (`EXPOSURE`) | Auto + Manual | `-11..=-2`, step `1`, default `-5` |
| **Image (`VideoProcAmp`)** | `IAMVideoProcAmp` / `IKsPropertySet` | `0, 1, 3, 4, 7, 8, 9, 13` | Optional (Phase 4) | Brightness, Contrast, Saturation, Sharpness, WB, Backlight, Gain, Powerline |

### Critical Windows DirectShow Notes
1. **Pan & Tilt are relative, not absolute**: Standard `IAMCameraControl::Set(CameraControl_Pan, ...)` is not supported by this camera. Pan/Tilt must be sent via `IKsPropertySet` with `PROPSETID_VIDCAP_CAMERACONTROL` (`{C6E13370-30AC-11D0-A18C-00A0C9118956}`), property IDs `10` (`PAN_RELATIVE`) and `11` (`TILT_RELATIVE`), and flag `KSPROPERTY_CAMERACONTROL_FLAGS_RELATIVE` (`0x0010`).
2. **40-byte struct layout (`0x8007007A` pitfall)**: `KSPROPERTY_CAMERACONTROL_S` must be **40 bytes** (`#[repr(C, align(8))]`) because `KSIDENTIFIER` has 8-byte alignment on x64 (`24` bytes `KSPROPERTY` + `4` bytes `value` + `4` bytes `flags` + `4` bytes `capabilities` + `4` bytes trailing padding). Passing 36 bytes fails with `ERROR_INSUFFICIENT_BUFFER` (`0x8007007A`).
3. **No stream locking**: Opening only the capture filter's control interfaces (`IBaseFilter` → `IAMCameraControl` / `IKsPropertySet`) without building a capture graph allows PTZ adjustments **while another app is actively streaming video**.

---

## 2. Prerequisites & Toolchain

> **Action required before `cargo build`:** Local `rustc` is currently `1.89.0`, while `fastframe v0.17.1` declares `edition = "2024"` and `rust-version = "1.98"`.

```powershell
rustup update stable
rustc --version   # confirm >= 1.98
```

---

## 3. Tech Stack & Dependencies

`fastframe` is not published on crates.io — pin all `fastframe-*` crates to the same Git tag (`v0.17.1`), which pairs with `egui` / `eframe` `0.36.2`.

### Target `Cargo.toml`
```toml
[package]
name = "logicam-controller"
version = "0.1.0"
edition = "2024"
rust-version = "1.98"

[dependencies]
# UI core (matched to fastframe v0.17.1)
egui = "0.36.2"
eframe = { version = "0.36.2", default-features = false, features = ["glow"] }

# fastframe MVP crates (pin all to identical tag)
fastframe-fonts = { git = "https://github.com/crmne/fastframe", tag = "v0.17.1" }
fastframe-text  = { git = "https://github.com/crmne/fastframe", tag = "v0.17.1" }
fastframe-icons = { git = "https://github.com/crmne/fastframe", tag = "v0.17.1" }
fastframe-theme = { git = "https://github.com/crmne/fastframe", tag = "v0.17.1" }
fastframe-log   = { git = "https://github.com/crmne/fastframe", tag = "v0.17.1" }

# fastframe Phase 3 desktop-polish crates (add when ready)
# fastframe-shell    = { git = "https://github.com/crmne/fastframe", tag = "v0.17.1" }
# fastframe-tray     = { git = "https://github.com/crmne/fastframe", tag = "v0.17.1" }
# fastframe-dirs     = { git = "https://github.com/crmne/fastframe", tag = "v0.17.1" }
# fastframe-file     = { git = "https://github.com/crmne/fastframe", tag = "v0.17.1" }
# fastframe-instance = { git = "https://github.com/crmne/fastframe", tag = "v0.17.1" }

# Windows COM / DirectShow
[target.'cfg(windows)'.dependencies]
windows = { version = "0.60", features = [
    "Win32_Foundation",
    "Win32_Media_DirectShow",
    "Win32_Media_KernelStreaming",
    "Win32_System_Com",
    "Win32_System_Com_StructuredStorage",
    "Win32_System_Variant",
] }

serde = { version = "1", features = ["derive"] }
serde_json = "1"
log = "0.4"
thiserror = "2"
```

*Note:* `fastframe` works out-of-the-box with stock `egui`/`eframe` `0.36.2`. If multi-monitor DPI resize bugs appear on Windows, optionally add `fastframe`'s `[patch.crates-io]` block from `docs/_guide/getting-started.md`.

---

## 4. Architecture & Module Structure

Keep all unsafe Windows COM / DirectShow calls isolated on a **single dedicated STA worker thread**, completely decoupled from the `egui` render loop.

```text
D:\Developer\logicam-controller\
├── Cargo.toml
├── PLAN.md
└── src/
    ├── main.rs              # Fastframe init (log, fonts, text, theme) + eframe entrypoint
    ├── app.rs               # egui::App state, keyboard/pointer handling, command dispatch
    ├── camera/
    │   ├── mod.rs           # Public CameraHandle, CameraCommand, CameraState, CameraError
    │   ├── ks_types.rs      # #[repr(C, align(8))] KSPROPERTY_CAMERACONTROL_S (40 bytes) & GUIDs
    │   ├── directshow.rs    # Device enumeration (VID_046D&PID_086E / "Logi Group"), IKsPropertySet & IAMCameraControl
    │   └── worker.rs        # Dedicated STA COM thread + command queue + motor auto-stop watchdog
    ├── ui/
    │   ├── mod.rs           # Compact layout orchestrator (~320x460 default window)
    │   ├── dpad.rs          # Pan/Tilt hold-to-move & nudge D-Pad widget
    │   ├── sliders.rs       # Zoom (100–1000), Focus (Auto/Manual 0–255), Exposure (-11..-2)
    │   └── status_bar.rs    # Connection status, device selector, reconnect & always-on-top toggle
    └── config.rs            # Persistent preferences (nudge ms, always-on-top, last camera)
```

### Threading & Safety Model
- **UI Thread (`egui`)**: Never calls COM directly. Sends non-blocking `CameraCommand` messages over `std::sync::mpsc::Sender` and reads a shared `CameraState` snapshot (`Arc<Mutex<CameraState>>` or watch channel).
- **Camera Worker Thread**:
  1. Calls `CoInitializeEx(COINIT_APARTMENTTHREADED)`.
  2. Enumerates `CLSID_VideoInputDeviceCategory`, prioritizes `VID_046D&PID_086E` / `"Logi Group Camera"`, and binds `IBaseFilter` + `IAMCameraControl` + `IKsPropertySet`.
  3. Processes commands (`Pan(i32)`, `Tilt(i32)`, `StopAll`, `Nudge { pan, tilt, duration_ms }`, `SetZoom(i32)`, `SetFocus { auto, value }`, `SetExposure { auto, value }`, `Reconnect`).
  4. **Dead-man's switch (critical for relative PTZ)**:
     - While a pan/tilt button or key is held, the UI sends a lightweight keepalive / hold state; if no release/heartbeat arrives within e.g. `400ms`, or when the window loses focus (`!ctx.input(|i| i.focused)`), or on `Drop`, the worker automatically sends `PAN_RELATIVE = 0` and `TILT_RELATIVE = 0`.
     - Prevents the camera motor from ever getting stuck running if a mouse-up event is missed outside the window.

---

## 5. UI Design (Compact Remote Layout)

Target a compact, distraction-free floating remote window (`~320 × 460 px`, resizable with sensible min-size, optional Always-on-Top):

```text
┌────────────────────────────────────┐
│ ● Logi Group Camera     [📌] [↻]   │  ← Status + Always-on-Top + Reconnect
├────────────────────────────────────┤
│            [  ▲  ]                 │
│      [ ◀ ] [ ■ ] [ ▶ ]             │  ← Hold to move (or toggle Nudge mode)
│            [  ▼  ]                 │     Center ■ = Emergency Stop
│   Mode: (●) Hold   ( ) Nudge 150ms │
├────────────────────────────────────┤
│ Zoom   [1x] [2x] [5x] [10x]        │
│ ━━━●━━━━━━━━━━━━━━━  250 / 1000    │
├────────────────────────────────────┤
│ Focus  [✓ Auto]                    │
│ ━━━━━●━━━━━━━━━━━━━   80 / 255     │
├────────────────────────────────────┤
│ Exposure  [✓ Auto]                 │
│ ━━━━━━━●━━━━━━━━━━━   -5 (-11..-2) │
├────────────────────────────────────┤
│ Keys: Arrows/WASD · +/- Zoom · Spc │
└────────────────────────────────────┘
```

### Keyboard Shortcuts (when focused)
- `Arrow keys` / `WASD`: Hold to Pan / Tilt (auto-stop on key release)
- `+` / `-` (or `=` / `-`): Step Zoom in / out
- `1` / `2` / `5` / `0`: Quick Zoom (`100`, `200`, `500`, `1000`)
- `Space` / `Esc`: Immediate `StopAll`

---

## 6. Phased Roadmap

### Phase 0 — Toolchain & Project Bootstrap
- [ ] Run `rustup update stable` to reach Rust `>= 1.98`.
- [ ] Initialize `D:\Developer\logicam-controller` (`cargo init`), add `Cargo.toml` dependencies pinned to `fastframe` tag `v0.17.1`.
- [ ] Wire up minimal `eframe::run_native` window with `fastframe-fonts`, `fastframe-text`, `fastframe-theme`, and `fastframe-log` to verify clean compilation on Windows.

### Phase 1 — DirectShow PTZ Backend & Hardware Verification
- [ ] Implement `src/camera/ks_types.rs` with 40-byte `#[repr(C, align(8))] KsPropertyCameraControlS` and exact GUIDs.
- [ ] Implement `src/camera/directshow.rs`:
  - Enumerate video input devices, auto-select `Logi Group Camera` (`VID_046D&PID_086E`).
  - Read initial Zoom / Focus / Exposure ranges & current values via `IAMCameraControl`.
  - Send `PAN_RELATIVE` (`-1`, `0`, `+1`) and `TILT_RELATIVE` (`-1`, `0`, `+1`) via `IKsPropertySet::Set`.
- [ ] Implement `src/camera/worker.rs` (STA thread, `mpsc` loop, auto-stop watchdog, clean `StopAll` on `Drop`).
- [ ] Add a quick CLI smoke-test flag (e.g., `cargo run -- --probe` and `--nudge-right 200`) to confirm real physical movement and verify it works while Windows Camera / Meet is open.

### Phase 2 — MVP GUI Controller
- [ ] Build `src/ui/dpad.rs`:
  - Custom `egui` hold-to-move buttons (`response.is_pointer_button_down_on()`), stopping immediately on release or pointer leave.
  - Mode switch between **Hold** (continuous) and **Nudge** (configurable pulse, default `150ms`).
  - Keyboard support (`WASD` / Arrows + `Space` stop) with window-blur safety stop.
- [ ] Build `src/ui/sliders.rs`:
  - Throttled slider updates (avoid flooding COM on every micro-drag; send on value change at ~30–60Hz).
  - Zoom slider + quick-zoom presets (`1x`, `2x`, `5x`, `10x`).
  - Focus & Exposure Auto/Manual toggles + sliders.
- [ ] Build `src/ui/status_bar.rs` with Lucide icons (`fastframe-icons`), Always-on-Top (`ViewportCommand::WindowLevel`), and Reconnect button.

### Phase 3 — Desktop Polish (`fastframe` Essentials)
- [ ] Add `fastframe-tray` + `fastframe-shell`: minimize to system tray, quick click to pop up remote during meetings, persist window position & size across restarts.
- [ ] Add `fastframe-instance`: single-instance guard so relaunching focuses the existing window.
- [ ] Add `fastframe-dirs` + `fastframe-file`: atomically persist user settings (`always_on_top`, `nudge_ms`, theme preference, last selected camera).
- [ ] Build release binary (`cargo build --release`) with Windows subsystem flag (`#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`) so no console window flashes on launch.

### Phase 4 — Optional Enhancements (Post-MVP)
- [ ] **Hardware Presets (Home / Preset 1–5)**: Investigate Logitech GROUP's UVC Extension Unit / HID interface (`VID_046D&PID_086E&MI_02`) to trigger the camera's built-in preset slots (just like the physical remote).
- [ ] **Image Tuning Drawer**: Collapsible section for `IAMVideoProcAmp` (Brightness, Contrast, White Balance, Backlight Comp).
- [ ] **Global Hotkeys**: Optional background hotkeys to pan/zoom without leaving Zoom/Meet/OBS.
- [ ] **Toggleable Live Preview**: Optional on-demand preview (explicitly off by default so it never competes with conferencing apps).

---

## 7. Verification Checklist

1. **Coexistence test**: Open Windows Camera or Google Meet using `Logi Group Camera`, then launch `logicam-controller` and verify Pan, Tilt, Zoom, and Focus all respond with zero stream interruption.
2. **Stuck-motor safety test**: Hold Pan Right, then `Alt+Tab` away or drag pointer off the button — camera must stop immediately.
3. **Hot-unplug / replug test**: Disconnect and reconnect the Logitech GROUP USB cable, click Reconnect (or auto-retry) — app recovers cleanly without crashing.
4. **Clean shutdown**: Close the app mid-movement — `Drop` sends `StopAll` and releases COM interfaces cleanly.
