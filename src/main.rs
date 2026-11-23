#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod camera;
mod config;
mod icons;
mod theme;
mod ui;

use std::process::ExitCode;

const APP_ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

fn main() -> ExitCode {
    let _ = fastframe_log::Logging::new("ptz-controller", env!("CARGO_PKG_VERSION"))
        .filter("warn,ptz-controller=info,ptz_controller=info")
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = handle_cli_args(&args) {
        return code;
    }

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("PTZ Controller")
        .with_inner_size([380.0, 820.0])
        .with_min_inner_size([320.0, 460.0]);

    if let Some(icon) = load_app_icon() {
        viewport = viewport.with_icon(icon);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    match eframe::run_native(
        "PTZ Controller",
        native_options,
        Box::new(|cc| Ok(Box::new(app::PtzControllerApp::new(cc)))),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Failed to launch PTZ Controller: {err}");
            ExitCode::FAILURE
        }
    }
}

fn load_app_icon() -> Option<egui::IconData> {
    let img = image::load_from_memory_with_format(APP_ICON_PNG, image::ImageFormat::Png).ok()?;
    let rgba = img.into_rgba8();
    let (width, height) = rgba.dimensions();
    Some(egui::IconData {
        rgba: rgba.into_raw(),
        width,
        height,
    })
}

fn handle_cli_args(args: &[String]) -> Option<ExitCode> {
    if args.is_empty() {
        return None;
    }

    let cmd = args[0].as_str();
    let parse_ms = |idx: usize| -> u64 {
        args.get(idx)
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(200)
    };

    match cmd {
        "--probe" => match camera::directshow::run_probe_cli() {
            Ok(()) => Some(ExitCode::SUCCESS),
            Err(err) => {
                eprintln!("Error: {err}");
                Some(ExitCode::FAILURE)
            }
        },
        "--probe-preview" => match camera::preview::run_probe_preview_cli() {
            Ok(()) => Some(ExitCode::SUCCESS),
            Err(err) => {
                eprintln!("Error: {err}");
                Some(ExitCode::FAILURE)
            }
        },
        "--nudge-right" | "--nudge-left" | "--nudge-up" | "--nudge-down" => {
            let (pan, tilt) = match cmd {
                "--nudge-right" => (1, 0),
                "--nudge-left" => (-1, 0),
                "--nudge-up" => (0, 1),
                "--nudge-down" => (0, -1),
                _ => (0, 0),
            };
            match camera::directshow::run_nudge_cli(pan, tilt, parse_ms(1)) {
                Ok(()) => Some(ExitCode::SUCCESS),
                Err(err) => {
                    eprintln!("Error: {err}");
                    Some(ExitCode::FAILURE)
                }
            }
        }
        "--help" | "-h" => {
            println!("PTZ Controller");
            println!("Usage:");
            println!("  ptz-controller                     Launch GUI remote");
            println!("  ptz-controller --probe             List DirectShow cameras & PTZ capabilities");
            println!("  ptz-controller --probe-preview     Verify SharedReadOnly live preview + OBS coexistence");
            println!("  ptz-controller --nudge-right [ms]  Nudge pan right (default 200ms)");
            println!("  ptz-controller --nudge-left  [ms]  Nudge pan left");
            println!("  ptz-controller --nudge-up    [ms]  Nudge tilt up");
            println!("  ptz-controller --nudge-down  [ms]  Nudge tilt down");
            Some(ExitCode::SUCCESS)
        }
        other => {
            eprintln!("Unknown argument: {other}. Run with --help for usage.");
            Some(ExitCode::FAILURE)
        }
    }
}
