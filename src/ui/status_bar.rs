use egui::{CornerRadius, Sense, Stroke, Vec2};
use fastframe_fonts::Weight;

use crate::camera::{CameraCommand, CameraHandle, CameraState};
use crate::config::AppConfig;
use crate::icons::AppIcon;
use crate::theme::AppPalette;

pub struct StatusBarEvents {
    pub toggle_always_on_top: bool,
    pub toggle_theme: bool,
    pub toggle_preview: bool,
    pub toggle_shortcuts_modal: bool,
    pub reconnect_clicked: bool,
    pub device_selected: Option<String>,
}

pub fn show(
    ui: &mut egui::Ui,
    state: &CameraState,
    camera: &CameraHandle,
    config: &AppConfig,
    shortcuts_open: bool,
    palette: &AppPalette,
) -> StatusBarEvents {
    let mut events = StatusBarEvents {
        toggle_always_on_top: false,
        toggle_theme: false,
        toggle_preview: false,
        toggle_shortcuts_modal: false,
        reconnect_clicked: false,
        device_selected: None,
    };

    ui.horizontal(|ui| {
        // Status LED dot
        let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
        let dot_color = if state.connected {
            if state.active_pan != 0 || state.active_tilt != 0 {
                palette.accent
            } else {
                palette.success
            }
        } else {
            palette.danger
        };
        ui.painter()
            .circle_filled(dot_rect.center(), 4.5, dot_color);

        // Camera selector / title (reserve space for 5 icon buttons on the right)
        let current_label = state
            .connected_name
            .as_deref()
            .unwrap_or("No PTZ Camera Found");
        let buttons_reserve = 154.0;

        if !state.devices.is_empty() {
            let combo_width = (ui.available_width() - buttons_reserve).max(95.0);
            egui::ComboBox::from_id_salt("camera_device_selector")
                .width(combo_width)
                .selected_text(
                    egui::RichText::new(current_label)
                        .font(Weight::SemiBold.font_id(13.0))
                        .color(palette.text),
                )
                .show_ui(ui, |ui| {
                    for dev in &state.devices {
                        let dev_id = dev.id();
                        let is_selected = state
                            .selected_device_path
                            .as_deref()
                            .is_some_and(|p| p.eq_ignore_ascii_case(dev_id));
                        let label = if dev.is_logi_group {
                            format!("{} ★", dev.friendly_name)
                        } else {
                            dev.friendly_name.clone()
                        };
                        if ui.selectable_label(is_selected, label).clicked() && !is_selected {
                            events.device_selected = Some(dev_id.to_owned());
                            camera.send(CameraCommand::SelectDevice(Some(dev_id.to_owned())));
                        }
                    }
                });
        } else {
            let text_width = (ui.available_width() - buttons_reserve).max(90.0);
            ui.add_sized(
                [text_width, 24.0],
                egui::Label::new(
                    egui::RichText::new(current_label)
                        .font(Weight::SemiBold.font_id(13.5))
                        .color(palette.text),
                )
                .truncate(),
            );
        }

        // Right-aligned utility icon buttons
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;

            if icon_button(
                ui,
                AppIcon::Refresh,
                false,
                "Reconnect camera & refresh values (R)",
                palette,
            )
            .clicked()
            {
                camera.send(CameraCommand::Reconnect);
                events.reconnect_clicked = true;
            }

            if icon_button(
                ui,
                AppIcon::Info,
                shortcuts_open,
                "Keyboard controls cheat sheet (? / F1)",
                palette,
            )
            .clicked()
            {
                events.toggle_shortcuts_modal = true;
            }

            let theme_icon = if config.dark_mode {
                AppIcon::Sun
            } else {
                AppIcon::Moon
            };
            let theme_tip = if config.dark_mode {
                "Switch to light mode"
            } else {
                "Switch to dark mode"
            };
            if icon_button(ui, theme_icon, false, theme_tip, palette).clicked() {
                events.toggle_theme = true;
            }

            let pin_icon = if config.always_on_top {
                AppIcon::Pin
            } else {
                AppIcon::PinOff
            };
            let pin_tip = if config.always_on_top {
                "Always on top: ON (T)"
            } else {
                "Always on top: OFF (T)"
            };
            if icon_button(ui, pin_icon, config.always_on_top, pin_tip, palette).clicked() {
                events.toggle_always_on_top = true;
            }

            let preview_icon = if config.show_preview {
                AppIcon::Eye
            } else {
                AppIcon::EyeOff
            };
            let preview_tip = if config.show_preview {
                "Live Preview: ON (P) · OBS-safe Shared mode"
            } else {
                "Live Preview: OFF (P) · Click to enable"
            };
            if icon_button(ui, preview_icon, config.show_preview, preview_tip, palette).clicked() {
                events.toggle_preview = true;
            }
        });
    });

    if let Some(err) = &state.last_error {
        ui.add_space(4.0);
        egui::Frame::new()
            .fill(palette.danger.gamma_multiply(0.14))
            .stroke(Stroke::new(1.0, palette.danger.gamma_multiply(0.45)))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(8, 5))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(err)
                        .font(Weight::Medium.font_id(11.5))
                        .color(palette.danger),
                );
            });
    }

    events
}

fn icon_button(
    ui: &mut egui::Ui,
    icon: AppIcon,
    active: bool,
    tooltip: &str,
    palette: &AppPalette,
) -> egui::Response {
    let size = Vec2::splat(26.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    let fill = if active {
        palette.accent
    } else if response.is_pointer_button_down_on() {
        palette.surface_active
    } else if response.hovered() {
        palette.surface_hover
    } else {
        palette.surface
    };

    let stroke_color = if active {
        palette.accent_hover
    } else if response.hovered() {
        palette.accent
    } else {
        palette.outline
    };

    let tint = if active {
        palette.on_accent
    } else if response.hovered() {
        palette.text
    } else {
        palette.secondary
    };

    ui.painter().rect(
        rect,
        CornerRadius::same(6),
        fill,
        Stroke::new(1.0, stroke_color),
        egui::StrokeKind::Inside,
    );

    let icon_rect = egui::Rect::from_center_size(rect.center(), Vec2::splat(14.0));
    icon.image(tint, 14.0).paint_at(ui, icon_rect);

    response.on_hover_text(tooltip)
}
