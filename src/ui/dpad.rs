use egui::{CornerRadius, Sense, Stroke, Vec2};
use fastframe_fonts::Weight;

use crate::camera::{CameraCommand, CameraHandle, CameraState};
use crate::config::AppConfig;
use crate::icons::AppIcon;
use crate::theme::AppPalette;

pub struct DpadOutput {
    /// Active hold direction from pointer interaction (Hold mode only).
    pub pointer_hold: Option<(i32, i32)>,
    /// True if user clicked emergency Stop.
    pub stop_clicked: bool,
    /// True if config (mode / nudge_ms) changed.
    pub config_changed: bool,
}

const NUDGE_PRESETS: [u64; 3] = [80, 150, 300];

pub fn show(
    ui: &mut egui::Ui,
    state: &CameraState,
    camera: &CameraHandle,
    config: &mut AppConfig,
    palette: &AppPalette,
) -> DpadOutput {
    let mut output = DpadOutput {
        pointer_hold: None,
        stop_clicked: false,
        config_changed: false,
    };

    let ptz_enabled = state.connected && (state.supports_pan() || state.supports_tilt());

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("PAN / TILT")
                .font(Weight::SemiBold.font_id(11.0))
                .color(palette.dim),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let motion_label = match (state.active_pan, state.active_tilt) {
                (0, 0) => {
                    if !ptz_enabled {
                        "Not Supported"
                    } else if !state.supports_pan_relative && state.supports_pan_absolute {
                        "Idle · Absolute"
                    } else {
                        "Idle"
                    }
                }
                (-1, 0) => "Panning Left",
                (1, 0) => "Panning Right",
                (0, 1) => "Tilting Up",
                (0, -1) => "Tilting Down",
                _ => "Moving",
            };
            let color = if state.active_pan != 0 || state.active_tilt != 0 {
                palette.accent
            } else {
                palette.dim
            };
            ui.label(
                egui::RichText::new(motion_label)
                    .font(Weight::Medium.font_id(11.5))
                    .color(color),
            );
        });
    });

    ui.add_space(4.0);

    // Centered 3x3 tactile D-Pad (compact height so entire remote fits without scrolling)
    let btn_size = Vec2::new(56.0, 32.0);
    let gap = 5.0;
    let total_w = btn_size.x * 3.0 + gap * 2.0;

    ui.horizontal(|ui| {
        let pad = ((ui.available_width() - total_w) * 0.5).max(0.0);
        ui.add_space(pad);

        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(gap);

            // Top row: Tilt Up
            ui.horizontal(|ui| {
                ui.add_space(btn_size.x + gap);
                handle_dir_button(
                    ui,
                    btn_size,
                    AppIcon::ChevronUp,
                    "Tilt Up (W / ↑)",
                    ptz_enabled && state.supports_tilt(),
                    state.active_tilt > 0,
                    (0, 1),
                    config,
                    camera,
                    palette,
                    &mut output,
                );
            });

            // Middle row: Pan Left, Emergency Stop, Pan Right
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;

                handle_dir_button(
                    ui,
                    btn_size,
                    AppIcon::ChevronLeft,
                    "Pan Left (A / ←)",
                    ptz_enabled && state.supports_pan(),
                    state.active_pan < 0,
                    (-1, 0),
                    config,
                    camera,
                    palette,
                    &mut output,
                );

                // Center emergency Stop button
                let stop_resp = draw_dpad_button(
                    ui,
                    btn_size,
                    AppIcon::Stop,
                    state.connected,
                    false,
                    true,
                    palette,
                )
                .on_hover_text("Stop motor immediately (Space / Esc)");
                if stop_resp.clicked() {
                    output.stop_clicked = true;
                    camera.send(CameraCommand::StopAll);
                }

                handle_dir_button(
                    ui,
                    btn_size,
                    AppIcon::ChevronRight,
                    "Pan Right (D / →)",
                    ptz_enabled && state.supports_pan(),
                    state.active_pan > 0,
                    (1, 0),
                    config,
                    camera,
                    palette,
                    &mut output,
                );
            });

            // Bottom row: Tilt Down
            ui.horizontal(|ui| {
                ui.add_space(btn_size.x + gap);
                handle_dir_button(
                    ui,
                    btn_size,
                    AppIcon::ChevronDown,
                    "Tilt Down (S / ↓)",
                    ptz_enabled && state.supports_tilt(),
                    state.active_tilt < 0,
                    (0, -1),
                    config,
                    camera,
                    palette,
                    &mut output,
                );
            });
        });
    });

    ui.add_space(7.0);

    // Hold vs Nudge mode bar
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(
            egui::RichText::new("Mode")
                .font(Weight::Medium.font_id(12.0))
                .color(palette.secondary),
        );

        if pill_toggle(ui, "Hold", !config.nudge_mode, palette).clicked() && config.nudge_mode {
            config.nudge_mode = false;
            output.config_changed = true;
        }
        if pill_toggle(ui, "Nudge", config.nudge_mode, palette).clicked() && !config.nudge_mode {
            config.nudge_mode = true;
            output.config_changed = true;
        }

        if config.nudge_mode {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for ms in NUDGE_PRESETS.iter().rev() {
                    let label = format!("{ms}ms");
                    if pill_toggle(ui, &label, config.nudge_ms == *ms, palette).clicked()
                        && config.nudge_ms != *ms
                    {
                        config.nudge_ms = *ms;
                        output.config_changed = true;
                    }
                }
            });
        }
    });

    output
}

#[allow(clippy::too_many_arguments)]
fn handle_dir_button(
    ui: &mut egui::Ui,
    size: Vec2,
    icon: AppIcon,
    tooltip: &str,
    enabled: bool,
    motor_active: bool,
    dir: (i32, i32),
    config: &AppConfig,
    camera: &CameraHandle,
    palette: &AppPalette,
    output: &mut DpadOutput,
) {
    let response =
        draw_dpad_button(ui, size, icon, enabled, motor_active, false, palette)
            .on_hover_text(tooltip);

    if !enabled {
        return;
    }

    if config.nudge_mode {
        if response.clicked() {
            camera.send(CameraCommand::Nudge {
                pan: dir.0,
                tilt: dir.1,
                duration_ms: config.nudge_ms,
            });
        }
    } else if response.is_pointer_button_down_on() && response.hovered() {
        output.pointer_hold = Some(dir);
    }
}

fn draw_dpad_button(
    ui: &mut egui::Ui,
    size: Vec2,
    icon: AppIcon,
    enabled: bool,
    active: bool,
    is_stop: bool,
    palette: &AppPalette,
) -> egui::Response {
    let sense = if enabled {
        Sense::click_and_drag()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let pressed = enabled && response.is_pointer_button_down_on() && response.hovered();

    let (fill, stroke, tint) = if !enabled {
        (
            palette.surface.gamma_multiply(0.55),
            palette.outline.gamma_multiply(0.5),
            palette.dim.gamma_multiply(0.5),
        )
    } else if is_stop {
        if pressed {
            (palette.danger, palette.danger, palette.on_accent)
        } else if response.hovered() {
            (
                palette.danger.gamma_multiply(0.22),
                palette.danger,
                palette.danger,
            )
        } else {
            (
                palette.surface,
                palette.outline,
                palette.danger.gamma_multiply(0.85),
            )
        }
    } else if pressed || active {
        (palette.accent, palette.accent_hover, palette.on_accent)
    } else if response.hovered() {
        (palette.surface_hover, palette.accent, palette.text)
    } else {
        (palette.surface, palette.outline, palette.text)
    };

    ui.painter().rect(
        rect,
        CornerRadius::same(10),
        fill,
        Stroke::new(1.0, stroke),
        egui::StrokeKind::Inside,
    );

    let icon_size = if is_stop { 15.0 } else { 20.0 };
    let icon_rect = egui::Rect::from_center_size(rect.center(), Vec2::splat(icon_size));
    icon.image(tint, icon_size).paint_at(ui, icon_rect);

    response
}

fn pill_toggle(
    ui: &mut egui::Ui,
    label: &str,
    selected: bool,
    palette: &AppPalette,
) -> egui::Response {
    let fill = if selected {
        palette.accent
    } else {
        palette.surface
    };
    let text_color = if selected {
        palette.on_accent
    } else {
        palette.secondary
    };
    let stroke = if selected {
        Stroke::new(1.0, palette.accent_hover)
    } else {
        Stroke::new(1.0, palette.outline)
    };

    ui.add(
        egui::Button::new(
            egui::RichText::new(label)
                .font(Weight::Medium.font_id(11.5))
                .color(text_color),
        )
        .fill(fill)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(6))
        .min_size(Vec2::new(38.0, 22.0)),
    )
}
