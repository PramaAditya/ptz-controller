use std::time::{Duration, Instant};

use egui::{CornerRadius, Sense, Stroke, Vec2};
use fastframe_fonts::Weight;

use crate::camera::{CameraCommand, CameraHandle, CameraState};
use crate::icons::AppIcon;
use crate::theme::AppPalette;

const SLIDER_SEND_INTERVAL: Duration = Duration::from_millis(28);

#[derive(Default)]
pub struct SliderUiState {
    pub zoom_draft: Option<i32>,
    pub focus_draft: Option<i32>,
    pub exposure_draft: Option<i32>,
    last_zoom_sent: Option<Instant>,
    last_focus_sent: Option<Instant>,
    last_exposure_sent: Option<Instant>,
}

pub struct SliderOutput {
    /// `-1` while holding Zoom Out button, `+1` while holding Zoom In button, `0` otherwise.
    pub zoom_button_hold: i32,
}

const ZOOM_PRESETS: [(&str, i32); 4] = [("1x", 100), ("2x", 200), ("5x", 500), ("10x", 1000)];

impl SliderUiState {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        state: &CameraState,
        camera: &CameraHandle,
        palette: &AppPalette,
    ) -> SliderOutput {
        let zoom_button_hold = self.show_zoom(ui, state, camera, palette);
        ui.add_space(3.0);
        ui.separator();
        ui.add_space(3.0);
        self.show_focus(ui, state, camera, palette);
        ui.add_space(3.0);
        ui.separator();
        ui.add_space(3.0);
        self.show_exposure(ui, state, camera, palette);

        SliderOutput { zoom_button_hold }
    }

    pub fn send_zoom_throttled(&mut self, value: i32, camera: &CameraHandle) {
        self.zoom_draft = Some(value);
        if should_send_throttled(&mut self.last_zoom_sent) {
            camera.send(CameraCommand::SetZoom(value));
        }
    }

    pub fn send_zoom_immediate(&mut self, value: i32, camera: &CameraHandle) {
        self.zoom_draft = Some(value);
        self.last_zoom_sent = Some(Instant::now());
        camera.send(CameraCommand::SetZoom(value));
    }

    fn show_zoom(
        &mut self,
        ui: &mut egui::Ui,
        state: &CameraState,
        camera: &CameraHandle,
        palette: &AppPalette,
    ) -> i32 {
        let Some(zoom) = state.zoom else {
            if state.supports_zoom_relative {
                let mut hold = 0i32;
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Zoom (Relative)")
                            .font(Weight::SemiBold.font_id(13.0))
                            .color(palette.text),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let plus =
                            small_hold_icon_btn(ui, AppIcon::Plus, "Hold to zoom in (+)", palette);
                        if plus.is_pointer_button_down_on() && plus.hovered() {
                            hold = 1;
                        }
                        let minus = small_hold_icon_btn(
                            ui,
                            AppIcon::Minus,
                            "Hold to zoom out (-)",
                            palette,
                        );
                        if minus.is_pointer_button_down_on() && minus.hovered() {
                            hold = -1;
                        }
                    });
                });
                return hold;
            }
            show_unavailable_row(ui, "Zoom", palette);
            return 0;
        };

        let mut val = self.zoom_draft.unwrap_or(zoom.value);
        let range = zoom.range;
        let mut hold_dir = 0i32;

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Zoom")
                    .font(Weight::SemiBold.font_id(13.0))
                    .color(palette.text),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (label, target) in ZOOM_PRESETS.iter().rev() {
                    let clamped = (*target).clamp(range.min, range.max);
                    let active = (val - clamped).abs() <= 4;
                    if preset_button(ui, label, active, palette).clicked() {
                        val = clamped;
                        self.zoom_draft = None;
                        self.last_zoom_sent = Some(Instant::now());
                        camera.send(CameraCommand::SetZoom(clamped));
                    }
                }
            });
        });

        ui.add_space(2.0);

        ui.horizontal(|ui| {
            let minus_resp = small_hold_icon_btn(
                ui,
                AppIcon::Minus,
                "Hold to zoom out smoothly (- · Shift = slow)",
                palette,
            );
            if minus_resp.is_pointer_button_down_on() && minus_resp.hovered() {
                hold_dir = -1;
            }

            let slider_w = (ui.available_width() - 104.0).max(86.0);
            ui.spacing_mut().slider_width = slider_w;
            let resp = ui.add(
                egui::Slider::new(&mut val, range.min..=range.max)
                    .show_value(false)
                    .step_by(range.step as f64),
            );

            let plus_resp = small_hold_icon_btn(
                ui,
                AppIcon::Plus,
                "Hold to zoom in smoothly (+ · Shift = slow)",
                palette,
            );
            if plus_resp.is_pointer_button_down_on() && plus_resp.hovered() {
                hold_dir = 1;
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("{:.2}x", val as f32 / 100.0))
                        .font(Weight::Medium.font_id(12.0))
                        .color(palette.secondary),
                )
                .on_hover_text(format!("Raw value: {val} / {}", range.max));
            });

            if resp.changed() {
                self.send_zoom_throttled(val, camera);
            }
            if resp.drag_stopped() {
                self.zoom_draft = None;
                self.last_zoom_sent = Some(Instant::now());
                camera.send(CameraCommand::SetZoom(val));
            }
        });

        hold_dir
    }

    fn show_focus(
        &mut self,
        ui: &mut egui::Ui,
        state: &CameraState,
        camera: &CameraHandle,
        palette: &AppPalette,
    ) {
        let Some(focus) = state.focus else {
            show_unavailable_row(ui, "Focus", palette);
            return;
        };

        let mut auto = focus.auto;
        let mut val = self.focus_draft.unwrap_or(focus.value);
        let range = focus.range;

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Focus")
                    .font(Weight::SemiBold.font_id(13.0))
                    .color(palette.text),
            );
            if range.supports_auto
                && ui
                    .checkbox(
                        &mut auto,
                        egui::RichText::new("Auto")
                            .font(Weight::Medium.font_id(12.0))
                            .color(palette.secondary),
                    )
                    .changed()
            {
                self.focus_draft = None;
                camera.send(CameraCommand::SetFocus { auto, value: val });
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(if auto {
                        format!("Auto ({val})")
                    } else {
                        format!("{val} / {}", range.max)
                    })
                    .font(Weight::Medium.font_id(12.0))
                    .color(palette.secondary),
                );
            });
        });

        ui.add_space(2.0);

        ui.add_enabled_ui(!auto || range.supports_manual, |ui| {
            let slider_w = (ui.available_width() - 4.0).max(120.0);
            ui.spacing_mut().slider_width = slider_w;
            let resp = ui.add(
                egui::Slider::new(&mut val, range.min..=range.max)
                    .show_value(false)
                    .step_by(range.step as f64),
            );
            if resp.changed() {
                self.focus_draft = Some(val);
                if should_send_throttled(&mut self.last_focus_sent) {
                    camera.send(CameraCommand::SetFocus {
                        auto: false,
                        value: val,
                    });
                }
            }
            if resp.drag_stopped() {
                self.focus_draft = None;
                self.last_focus_sent = Some(Instant::now());
                camera.send(CameraCommand::SetFocus {
                    auto: false,
                    value: val,
                });
            }
        });
    }

    fn show_exposure(
        &mut self,
        ui: &mut egui::Ui,
        state: &CameraState,
        camera: &CameraHandle,
        palette: &AppPalette,
    ) {
        let Some(exposure) = state.exposure else {
            show_unavailable_row(ui, "Exposure", palette);
            return;
        };

        let mut auto = exposure.auto;
        let mut val = self.exposure_draft.unwrap_or(exposure.value);
        let range = exposure.range;

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Exposure")
                    .font(Weight::SemiBold.font_id(13.0))
                    .color(palette.text),
            );
            if range.supports_auto
                && ui
                    .checkbox(
                        &mut auto,
                        egui::RichText::new("Auto")
                            .font(Weight::Medium.font_id(12.0))
                            .color(palette.secondary),
                    )
                    .changed()
            {
                self.exposure_draft = None;
                camera.send(CameraCommand::SetExposure { auto, value: val });
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(if auto {
                        format!("Auto ({val})")
                    } else {
                        format!("{val} ({}..{})", range.min, range.max)
                    })
                    .font(Weight::Medium.font_id(12.0))
                    .color(palette.secondary),
                );
            });
        });

        ui.add_space(2.0);

        ui.add_enabled_ui(!auto || range.supports_manual, |ui| {
            let slider_w = (ui.available_width() - 4.0).max(120.0);
            ui.spacing_mut().slider_width = slider_w;
            let resp = ui.add(
                egui::Slider::new(&mut val, range.min..=range.max)
                    .show_value(false)
                    .step_by(range.step as f64),
            );
            if resp.changed() {
                self.exposure_draft = Some(val);
                if should_send_throttled(&mut self.last_exposure_sent) {
                    camera.send(CameraCommand::SetExposure {
                        auto: false,
                        value: val,
                    });
                }
            }
            if resp.drag_stopped() {
                self.exposure_draft = None;
                self.last_exposure_sent = Some(Instant::now());
                camera.send(CameraCommand::SetExposure {
                    auto: false,
                    value: val,
                });
            }
        });
    }
}

fn should_send_throttled(last: &mut Option<Instant>) -> bool {
    let now = Instant::now();
    let ok = last.is_none_or(|prev| now.duration_since(prev) >= SLIDER_SEND_INTERVAL);
    if ok {
        *last = Some(now);
    }
    ok
}

fn show_unavailable_row(ui: &mut egui::Ui, label: &str, palette: &AppPalette) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(label)
                .font(Weight::SemiBold.font_id(13.0))
                .color(palette.dim),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new("Unavailable")
                    .font(Weight::Regular.font_id(11.5))
                    .color(palette.dim),
            );
        });
    });
}

fn preset_button(
    ui: &mut egui::Ui,
    label: &str,
    active: bool,
    palette: &AppPalette,
) -> egui::Response {
    let fill = if active {
        palette.accent
    } else {
        palette.surface
    };
    let fg = if active {
        palette.on_accent
    } else {
        palette.secondary
    };
    let stroke = if active {
        Stroke::new(1.0, palette.accent_hover)
    } else {
        Stroke::new(1.0, palette.outline)
    };
    ui.add(
        egui::Button::new(
            egui::RichText::new(label)
                .font(Weight::Medium.font_id(11.0))
                .color(fg),
        )
        .fill(fill)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(6))
        .min_size(Vec2::new(32.0, 21.0)),
    )
}

fn small_hold_icon_btn(
    ui: &mut egui::Ui,
    icon: AppIcon,
    tooltip: &str,
    palette: &AppPalette,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::splat(24.0), Sense::click_and_drag());
    let pressed = response.is_pointer_button_down_on() && response.hovered();
    let fill = if pressed {
        palette.accent
    } else if response.hovered() {
        palette.surface_hover
    } else {
        palette.surface
    };
    let stroke = if pressed {
        palette.accent_hover
    } else if response.hovered() {
        palette.accent
    } else {
        palette.outline
    };
    let tint = if pressed {
        palette.on_accent
    } else {
        palette.text
    };
    ui.painter().rect(
        rect,
        CornerRadius::same(6),
        fill,
        Stroke::new(1.0, stroke),
        egui::StrokeKind::Inside,
    );
    let icon_rect = egui::Rect::from_center_size(rect.center(), Vec2::splat(13.0));
    icon.image(tint, 13.0).paint_at(ui, icon_rect);
    response.on_hover_text(tooltip)
}
