<p align="center">
  <img src="assets/icon.png" width="92" alt="PTZ Controller icon">
</p>

<h1 align="center">PTZ Controller</h1>

<p align="center">
  <b>A clean, lightweight Windows remote for USB PTZ cameras that works alongside OBS, Zoom, Google Meet, and Teams without locking your video feed.</b>
  <br>
  Pan, tilt, smooth zoom, focus, exposure, and live preview in one distraction-free floating window.
</p>

<p align="center">
  <a href="https://github.com/PramaAditya/ptz-controller/releases/latest">
    <img src="https://img.shields.io/github/v/release/PramaAditya/ptz-controller?style=flat-square&label=download&color=2563EB&sort=semver" alt="Latest Release">
  </a>
  <img src="https://img.shields.io/badge/platform-Windows%2010%2F11%20(64--bit)-0078D4?style=flat-square" alt="Windows 10/11 64-bit">
  <img src="https://img.shields.io/badge/OBS%20%2F%20Zoom%20%2F%20Meet-No%20Camera%20Lock-22C55E?style=flat-square" alt="No Camera Lock">
  <img src="https://img.shields.io/badge/license-MIT-gray?style=flat-square" alt="MIT License">
</p>

<p align="center">
  <b><a href="https://github.com/PramaAditya/ptz-controller/releases/latest">Download Windows Installer (.exe)</a></b> ·
  <a href="#controls">Controls</a> ·
  <a href="#compatible-cameras">Compatible Cameras</a> ·
  <a href="#build-from-source">Build from Source</a>
</p>

<br>

---

## Install in 10 Seconds

1. Open the **[Latest Release](https://github.com/PramaAditya/ptz-controller/releases/latest)** page.
2. Download **`PTZ-Controller-Setup-v*-windows-x64.exe`** *(or the Portable `.zip` if you prefer not to install)*.
3. Run the installer — **no administrator permission required**.
4. Open **PTZ Controller** from your Start Menu. Your USB PTZ camera is detected automatically.

> **Using OBS, Zoom, Teams, or Meet at the same time?**
> Open them in any order. PTZ Controller uses Windows Shared Read-Only camera access, so it never steals or blacks out your video feed.

<br>

---

## Why PTZ Controller?

Standard webcam apps often lock your camera (causing a black screen in OBS), hide controls inside clunky menus, or only work with a single brand.

| What you need | PTZ Controller | OBS Camera Properties | Vendor Apps |
| :--- | :---: | :---: | :---: |
| Works live alongside OBS, Zoom, Meet & Teams | ✓ | ✗ | ⚠️ varies |
| Built-in live preview without locking OBS | ✓ | ✓ | ✗ |
| Works across multiple USB PTZ camera brands | ✓ | ⚠️ partial | ✗ |
| Smooth hold-to-zoom, scroll-wheel zoom & hotkeys | ✓ | ✗ | ✗ |
| Always-on-top compact remote + auto-stop motor safety | ✓ | ✗ | ✗ |

## Highlights

- **OBS-safe live preview** — frame your shot directly inside the remote while streaming or presenting, with aspect-ratio lock on resize and 1-click toggle (`P`).
- **Auto-detects PTZ cameras only** — automatically skips laptop webcams and virtual cameras (OBS Virtual Camera, VTubeStudio, etc.) so your dropdown stays clean.
- **Hold or Nudge movement** — hold D-Pad buttons or keys for continuous pan/tilt, or switch to **Nudge** (`80ms` / `150ms` / `300ms`) for tiny framing adjustments.
- **Smooth zoom** — hold `+`/`-`, scroll your mouse wheel directly over the preview, drag the slider, or jump straight to `1x` / `2x` / `5x` / `10x`.
- **No stuck motors** — motors stop automatically if you release the button, switch windows (`Alt+Tab`), press `Space`/`Esc`, or close the app.
- **Always-on-top & Dark/Light mode** — pin the compact remote above OBS or your meeting window; your preferences save automatically.

## Controls

Press **`?`** or **`F1`** anytime inside the app to pop up the built-in keyboard cheat sheet.

| Input | Action |
| :--- | :--- |
| `W A S D` or `↑ ← ↓ →` | Pan & Tilt (follows **Hold** or **Nudge** mode) |
| `Space` or `Esc` | Stop camera movement immediately |
| Hold `+` / `-` | Smooth continuous zoom (hold `Shift` for slow precision) |
| Mouse `Scroll` on preview | Smooth zoom in / out (hold `Shift` for fine adjustment) |
| `1` `2` `5` `0` | Quick zoom presets (`1x`, `2x`, `5x`, `10x`) |
| `[` / `]` · `F` | Manual focus near / far · Toggle Auto Focus |
| `,` / `.` · `E` | Manual exposure darker / brighter · Toggle Auto Exposure |
| `P` · `M` · `T` · `R` | Toggle Preview · Hold/Nudge Mode · Always-on-Top · Reconnect |

## Compatible Cameras

Works with standard **USB UVC PTZ cameras** on Windows 10 and Windows 11 (both Relative and Absolute UVC Pan/Tilt):

- **Logitech**: GROUP (verified), Rally, PTZ Pro / PTZ Pro 2, Brio / C920 (digital PTZ)
- **Generic USB UVC PTZ**: OBSBOT Tiny, Insta360 Link, PTZOptics USB, AVer USB PTZ, and other UVC-compliant USB PTZ cameras

*(Note: Network/IP PTZ cameras using VISCA-over-IP or ONVIF are not supported yet.)*

<br>

---

## Build from Source

<details>
<summary>For developers — Rust toolchain, architecture, and CLI diagnostics</summary>

<br>

Requires **Windows 10/11** and **Rust 1.98+** (pinned in [rust-toolchain.toml](rust-toolchain.toml)).

```powershell
git clone https://github.com/PramaAditya/ptz-controller.git
cd ptz-controller
cargo run --release
```

**CLI diagnostic flags:**

```powershell
cargo run -- --probe              # List DirectShow cameras and detected PTZ capabilities
cargo run -- --probe-preview      # Verify SharedReadOnly preview + simultaneous OBS coexistence
cargo run -- --nudge-right 150    # Pulse pan/tilt directly (--nudge-left, --nudge-up, --nudge-down)
```

**Thread architecture:**

```text
egui UI Thread (eframe + fastframe)
  ├── mpsc commands ──→ DirectShow STA Thread
  │                      ├── IKsPropertySet (40-byte KSPROPERTY_CAMERACONTROL_S) ──→ Relative Pan/Tilt/Zoom
  │                      ├── IAMCameraControl (no capture graph = zero lock)    ──→ Absolute PTZ / Zoom / Focus / Exposure
  │                      └── 400ms Watchdog                                     ──→ Auto-stop on blur / drop / timeout
  └── target sync   ──→ WinRT MTA Preview Thread
                         └── MediaCapture (SharedReadOnly) + MediaFrameReader   ──→ Aspect-locked GPU texture
```

Built with [egui](https://github.com/emilk/egui), [fastframe](https://github.com/crmne/fastframe), and [windows-rs](https://github.com/microsoft/windows-rs). Settings persist to `%LOCALAPPDATA%\ptz-controller\config.json`.

</details>

<br>

---

<p align="center">
  <a href="https://github.com/PramaAditya/ptz-controller/releases">Releases</a> ·
  <a href="PLAN.md">Architecture Plan</a> ·
  <a href="LICENSE">MIT License</a>
</p>

<p align="center">
  <sub>Made by <a href="https://github.com/PramaAditya">Prama</a> · MIT License</sub>
</p>
