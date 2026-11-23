use std::time::{Duration, Instant};

use egui::{Key, Margin, ViewportCommand, WindowLevel};
use fastframe_fonts::FontSetup;
use fastframe_text::TextRendering;
use fastframe_theme::{Base, Palette};

use crate::camera::preview::PreviewHandle;
use crate::camera::{CameraCommand, CameraHandle, CameraState};
use crate::config::AppConfig;
use crate::icons::AppIcon;
use crate::theme::AppPalette;
use crate::ui::preview::PreviewUiState;
use crate::ui::sliders::SliderUiState;

const HEARTBEAT_INTERVAL: Duration = Duration::from_millis(120);
/// Base smooth zoom speed (units/sec out of 100..=1000) when holding +/-.
const ZOOM_SPEED_START: f32 = 170.0;
const ZOOM_SPEED_MAX: f32 = 380.0;
const ZOOM_SPEED_FINE: f32 = 55.0;

pub struct PtzControllerApp {
    camera: CameraHandle,
    preview: PreviewHandle,
    preview_ui: PreviewUiState,
    last_preview_target: (Option<String>, Option<String>),
    config: AppConfig,
    palette: AppPalette,
    text_rendering: TextRendering,
    sliders: SliderUiState,
    shortcuts_modal_open: bool,
    active_hold: Option<(i32, i32)>,
    last_heartbeat: Option<Instant>,
    zoom_hold_started: Option<Instant>,
    zoom_hold_last_tick: Option<Instant>,
    zoom_smooth_pos: Option<f32>,
    was_focused: bool,
}

impl PtzControllerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let config = AppConfig::load();
        let text_rendering = fastframe_text::detect();

        // Install Inter fonts + desktop hinting
        let mut fonts = FontSetup::default().definitions();
        text_rendering.apply_to(&mut fonts);
        cc.egui_ctx.set_fonts(fonts);

        // Install SVG image loaders + fastframe-icons Lucide bundle
        egui_extras::install_image_loaders(&cc.egui_ctx);
        fastframe_icons::install::<AppIcon>(&cc.egui_ctx);

        let palette = AppPalette::base(Base::from_dark(config.dark_mode));
        palette.apply(&cc.egui_ctx, &text_rendering);

        if config.always_on_top {
            cc.egui_ctx
                .send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop));
        }

        let camera = CameraHandle::spawn(config.last_device_path.clone());
        camera.set_repaint_ctx(cc.egui_ctx.clone());

        let preview = PreviewHandle::spawn(config.show_preview);
        preview.set_repaint_ctx(cc.egui_ctx.clone());

        Self {
            camera,
            preview,
            preview_ui: PreviewUiState::default(),
            last_preview_target: (None, None),
            config,
            palette,
            text_rendering,
            sliders: SliderUiState::default(),
            shortcuts_modal_open: false,
            active_hold: None,
            last_heartbeat: None,
            zoom_hold_started: None,
            zoom_hold_last_tick: None,
            zoom_smooth_pos: None,
            was_focused: true,
        }
    }

    fn toggle_preview(&mut self) -> bool {
        self.config.show_preview = !self.config.show_preview;
        self.preview.set_enabled(self.config.show_preview);
        if !self.config.show_preview {
            self.preview_ui.clear_texture();
        }
        true
    }

    fn toggle_always_on_top(&mut self, ctx: &egui::Context) -> bool {
        self.config.always_on_top = !self.config.always_on_top;
        let level = if self.config.always_on_top {
            WindowLevel::AlwaysOnTop
        } else {
            WindowLevel::Normal
        };
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(level));
        true
    }

    fn handle_keyboard(
        &mut self,
        ctx: &egui::Context,
        state: &CameraState,
    ) -> (Option<(i32, i32)>, i32, bool) {
        let mut config_dirty = false;
        let focused = ctx.input(|i| i.focused);
        if !focused {
            if self.was_focused {
                self.was_focused = false;
                if self.active_hold.take().is_some()
                    || state.active_pan != 0
                    || state.active_tilt != 0
                {
                    log::debug!("Window lost focus; stopping PTZ motors");
                    self.camera.send(CameraCommand::StopAll);
                }
                self.stop_zoom_hold(state);
            }
            return (None, 0, false);
        }
        self.was_focused = true;

        // Help / Shortcuts modal toggle (? or F1)
        let help_toggled = ctx.input(|i| {
            i.key_pressed(Key::F1)
                || i.key_pressed(Key::Questionmark)
                || (i.modifiers.shift && i.key_pressed(Key::Slash))
        });
        if help_toggled {
            self.shortcuts_modal_open = !self.shortcuts_modal_open;
        }

        // Escape closes modal if open AND always emergency-stops motor
        let esc_pressed = ctx.input(|i| i.key_pressed(Key::Escape));
        let space_pressed = ctx.input(|i| i.key_pressed(Key::Space));
        if esc_pressed {
            if self.shortcuts_modal_open {
                self.shortcuts_modal_open = false;
            }
            self.active_hold = None;
            self.stop_zoom_hold(state);
            self.camera.send(CameraCommand::StopAll);
            return (None, 0, false);
        }
        if space_pressed {
            self.active_hold = None;
            self.stop_zoom_hold(state);
            self.camera.send(CameraCommand::StopAll);
            return (None, 0, false);
        }

        // Ignore movement/control hotkeys while cheat-sheet modal is open
        if self.shortcuts_modal_open {
            self.stop_zoom_hold(state);
            return (None, 0, false);
        }

        // App-level hotkeys (P = preview, M = mode, T = always-on-top, R = reconnect)
        let (p_key, m_key, t_key, r_key, f_key, e_key) = ctx.input(|i| {
            let no_ctrl = !i.modifiers.ctrl && !i.modifiers.alt && !i.modifiers.command;
            (
                no_ctrl && i.key_pressed(Key::P),
                no_ctrl && i.key_pressed(Key::M),
                no_ctrl && i.key_pressed(Key::T),
                no_ctrl && i.key_pressed(Key::R),
                no_ctrl && i.key_pressed(Key::F),
                no_ctrl && i.key_pressed(Key::E),
            )
        });

        if p_key {
            config_dirty |= self.toggle_preview();
        }
        if m_key {
            self.config.nudge_mode = !self.config.nudge_mode;
            config_dirty = true;
        }
        if t_key {
            config_dirty |= self.toggle_always_on_top(ctx);
        }
        if r_key {
            self.camera.send(CameraCommand::Reconnect);
            self.preview.restart();
        }
        if f_key
            && let Some(focus) = state.focus
            && focus.range.supports_auto
        {
            self.sliders.focus_draft = None;
            self.camera.send(CameraCommand::SetFocus {
                auto: !focus.auto,
                value: focus.value,
            });
        }
        if e_key
            && let Some(exposure) = state.exposure
            && exposure.range.supports_auto
        {
            self.sliders.exposure_draft = None;
            self.camera.send(CameraCommand::SetExposure {
                auto: !exposure.auto,
                value: exposure.value,
            });
        }

        // Quick zoom preset shortcuts (1 / 2 / 5 / 0) + smooth hold (+/-)
        let mut key_zoom_hold = 0i32;
        if state.zoom.is_some() || state.supports_zoom_relative {
            let mut target_zoom = None;
            ctx.input(|i| {
                if let Some(zoom) = state.zoom {
                    let range = zoom.range;
                    if i.key_pressed(Key::Num1) {
                        target_zoom = Some(100.clamp(range.min, range.max));
                    } else if i.key_pressed(Key::Num2) {
                        target_zoom = Some(200.clamp(range.min, range.max));
                    } else if i.key_pressed(Key::Num5) {
                        target_zoom = Some(500.clamp(range.min, range.max));
                    } else if i.key_pressed(Key::Num0) {
                        target_zoom = Some(1000.clamp(range.min, range.max));
                    }
                }

                if i.key_down(Key::Plus) || i.key_down(Key::Equals) {
                    key_zoom_hold += 1;
                }
                if i.key_down(Key::Minus) {
                    key_zoom_hold -= 1;
                }
            });
            if let Some(z) = target_zoom {
                self.stop_zoom_hold(state);
                self.sliders.zoom_draft = None;
                self.camera.send(CameraCommand::SetZoom(z));
            }
        }

        // Manual Focus shortcuts ([ and ])
        if let Some(focus) = state.focus {
            let range = focus.range;
            let step = ((range.max - range.min) / 25).max(range.step);
            let mut delta = 0i32;
            ctx.input(|i| {
                if i.key_pressed(Key::OpenBracket) {
                    delta -= step;
                }
                if i.key_pressed(Key::CloseBracket) {
                    delta += step;
                }
            });
            if delta != 0 {
                let next = (focus.value + delta).clamp(range.min, range.max);
                self.sliders.focus_draft = None;
                self.camera.send(CameraCommand::SetFocus {
                    auto: false,
                    value: next,
                });
            }
        }

        // Manual Exposure shortcuts (, and .)
        if let Some(exposure) = state.exposure {
            let range = exposure.range;
            let step = range.step.max(1);
            let mut delta = 0i32;
            ctx.input(|i| {
                if i.key_pressed(Key::Comma) {
                    delta -= step;
                }
                if i.key_pressed(Key::Period) {
                    delta += step;
                }
            });
            if delta != 0 {
                let next = (exposure.value + delta).clamp(range.min, range.max);
                self.sliders.exposure_draft = None;
                self.camera.send(CameraCommand::SetExposure {
                    auto: false,
                    value: next,
                });
            }
        }

        // Pan/Tilt keys (Arrows / WASD) — respects Hold vs Nudge mode!
        if self.config.nudge_mode {
            let mut pan = 0i32;
            let mut tilt = 0i32;
            ctx.input(|i| {
                if i.key_pressed(Key::ArrowLeft) || i.key_pressed(Key::A) {
                    pan -= 1;
                }
                if i.key_pressed(Key::ArrowRight) || i.key_pressed(Key::D) {
                    pan += 1;
                }
                if i.key_pressed(Key::ArrowUp) || i.key_pressed(Key::W) {
                    tilt += 1;
                }
                if i.key_pressed(Key::ArrowDown) || i.key_pressed(Key::S) {
                    tilt -= 1;
                }
            });
            if pan != 0 || tilt != 0 {
                self.camera.send(CameraCommand::Nudge {
                    pan,
                    tilt,
                    duration_ms: self.config.nudge_ms,
                });
            }
            return (None, key_zoom_hold, config_dirty);
        }

        let mut pan = 0i32;
        let mut tilt = 0i32;
        ctx.input(|i| {
            if i.key_down(Key::ArrowLeft) || i.key_down(Key::A) {
                pan -= 1;
            }
            if i.key_down(Key::ArrowRight) || i.key_down(Key::D) {
                pan += 1;
            }
            if i.key_down(Key::ArrowUp) || i.key_down(Key::W) {
                tilt += 1;
            }
            if i.key_down(Key::ArrowDown) || i.key_down(Key::S) {
                tilt -= 1;
            }
        });

        let hold = (pan != 0 || tilt != 0).then_some((pan, tilt));
        (hold, key_zoom_hold, config_dirty)
    }

    fn update_zoom_hold(&mut self, ctx: &egui::Context, state: &CameraState, dir: i32) {
        let Some(zoom) = state.zoom else {
            if state.supports_zoom_relative {
                self.camera.send(CameraCommand::HoldZoomRelative(dir));
                if dir != 0 {
                    ctx.request_repaint_after(Duration::from_millis(80));
                }
            }
            self.zoom_hold_started = None;
            self.zoom_hold_last_tick = None;
            self.zoom_smooth_pos = None;
            return;
        };

        if dir == 0 {
            self.stop_zoom_hold(state);
            return;
        }

        let now = Instant::now();
        let fine = ctx.input(|i| i.modifiers.shift);
        let range = zoom.range;

        if self.zoom_hold_started.is_none() {
            // Immediate subtle nudge on initial press so a quick tap feels responsive
            let base = self.sliders.zoom_draft.unwrap_or(zoom.value) as f32;
            let initial_step = if fine { 3.0 } else { 8.0 } * dir as f32;
            let next = (base + initial_step).clamp(range.min as f32, range.max as f32);
            self.zoom_hold_started = Some(now);
            self.zoom_hold_last_tick = Some(now);
            self.zoom_smooth_pos = Some(next);
            self.sliders.send_zoom_immediate(next.round() as i32, &self.camera);
        } else {
            let dt = self
                .zoom_hold_last_tick
                .map(|t| now.duration_since(t).as_secs_f32())
                .unwrap_or(0.0)
                .min(0.1);
            self.zoom_hold_last_tick = Some(now);

            let held_secs = self
                .zoom_hold_started
                .map(|t| now.duration_since(t).as_secs_f32())
                .unwrap_or(0.0);

            // Smooth acceleration ramp while held (or constant slow crawl with Shift)
            let speed = if fine {
                ZOOM_SPEED_FINE
            } else {
                let ramp = (held_secs / 1.2).clamp(0.0, 1.0);
                ZOOM_SPEED_START + (ZOOM_SPEED_MAX - ZOOM_SPEED_START) * ramp
            };

            let current = self
                .zoom_smooth_pos
                .unwrap_or_else(|| self.sliders.zoom_draft.unwrap_or(zoom.value) as f32);
            let next = (current + speed * dt * dir as f32).clamp(range.min as f32, range.max as f32);
            self.zoom_smooth_pos = Some(next);

            let rounded = next.round() as i32;
            if self.sliders.zoom_draft != Some(rounded) {
                self.sliders.send_zoom_throttled(rounded, &self.camera);
            }
        }

        ctx.request_repaint_after(Duration::from_millis(16));
    }

    fn stop_zoom_hold(&mut self, state: &CameraState) {
        if let Some(pos) = self.zoom_smooth_pos.take() {
            if let Some(zoom) = state.zoom {
                let final_val = (pos.round() as i32).clamp(zoom.range.min, zoom.range.max);
                self.camera.send(CameraCommand::SetZoom(final_val));
            }
            self.sliders.zoom_draft = None;
        }
        self.zoom_hold_started = None;
        self.zoom_hold_last_tick = None;
    }

    fn apply_preview_scroll_zoom(
        &mut self,
        ctx: &egui::Context,
        state: &CameraState,
        scroll_y: f32,
    ) {
        if scroll_y.abs() <= 0.1 {
            return;
        }
        let Some(zoom) = state.zoom else {
            return;
        };
        let fine = ctx.input(|i| i.modifiers.shift);
        // smooth_scroll_delta.y is roughly +/-30..50 per mouse wheel notch
        let sensitivity = if fine { 0.15 } else { 0.55 };
        let delta = (scroll_y * sensitivity).round() as i32;
        let delta = if delta == 0 {
            if scroll_y > 0.0 { 1 } else { -1 }
        } else {
            delta
        };
        let current = self.sliders.zoom_draft.unwrap_or(zoom.value);
        let next = (current + delta).clamp(zoom.range.min, zoom.range.max);
        if next != current {
            self.sliders.send_zoom_immediate(next, &self.camera);
            self.sliders.zoom_draft = None;
        }
    }

    fn update_hold_state(&mut self, ctx: &egui::Context, desired: Option<(i32, i32)>) {
        match (self.active_hold, desired) {
            (prev, Some((pan, tilt))) => {
                if prev != Some((pan, tilt)) {
                    self.active_hold = Some((pan, tilt));
                    self.last_heartbeat = Some(Instant::now());
                    self.camera.send(CameraCommand::HoldMotion { pan, tilt });
                } else {
                    let now = Instant::now();
                    if self
                        .last_heartbeat
                        .is_none_or(|t| now.duration_since(t) >= HEARTBEAT_INTERVAL)
                    {
                        self.last_heartbeat = Some(now);
                        self.camera.send(CameraCommand::Heartbeat);
                    }
                }
                ctx.request_repaint_after(Duration::from_millis(80));
            }
            (Some(_), None) => {
                self.active_hold = None;
                self.last_heartbeat = None;
                self.camera.send(CameraCommand::StopAll);
            }
            (None, None) => {}
        }
    }

    fn sync_preview_target(&mut self, state: &CameraState) {
        let target = (
            state.selected_device_path.clone(),
            state.connected_name.clone(),
        );
        if target != self.last_preview_target {
            self.last_preview_target = target.clone();
            self.preview.set_target(target.0, target.1);
        }
    }
}

impl eframe::App for PtzControllerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let state = self.camera.snapshot();
        self.sync_preview_target(&state);
        let preview_state = self.preview.snapshot();

        let (key_hold, key_zoom_hold, mut config_dirty) = self.handle_keyboard(&ctx, &state);

        let mut pointer_hold = None;
        let mut stop_clicked = false;
        let mut button_zoom_hold = 0i32;
        let mut preview_scroll_y = 0.0f32;

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(self.palette.window)
                    .inner_margin(Margin::same(10)),
            )
            .show(ui, |ui| {
                let status_events = crate::ui::status_bar::show(
                    ui,
                    &state,
                    &self.camera,
                    &self.config,
                    self.shortcuts_modal_open,
                    &self.palette,
                );

                if status_events.toggle_always_on_top {
                    config_dirty |= self.toggle_always_on_top(&ctx);
                }

                if status_events.toggle_theme {
                    self.config.dark_mode = !self.config.dark_mode;
                    self.palette = AppPalette::base(Base::from_dark(self.config.dark_mode));
                    self.palette.apply(&ctx, &self.text_rendering);
                    config_dirty = true;
                }

                if status_events.toggle_preview {
                    config_dirty |= self.toggle_preview();
                }

                if status_events.toggle_shortcuts_modal {
                    self.shortcuts_modal_open = !self.shortcuts_modal_open;
                }

                if status_events.reconnect_clicked {
                    self.preview.restart();
                }

                if let Some(path) = status_events.device_selected {
                    self.config.last_device_path = Some(path);
                    config_dirty = true;
                }

                ui.add_space(8.0);

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.config.show_preview {
                            preview_scroll_y = self.preview_ui.show(
                                ui,
                                &preview_state,
                                &self.preview,
                                &state,
                                &self.palette,
                            );
                            ui.add_space(8.0);
                        }

                        crate::ui::card(ui, &self.palette, |ui| {
                            let dpad_out = crate::ui::dpad::show(
                                ui,
                                &state,
                                &self.camera,
                                &mut self.config,
                                &self.palette,
                            );
                            pointer_hold = dpad_out.pointer_hold;
                            stop_clicked = dpad_out.stop_clicked;
                            if dpad_out.config_changed {
                                config_dirty = true;
                            }
                        });

                        ui.add_space(8.0);

                        crate::ui::card(ui, &self.palette, |ui| {
                            let slider_out =
                                self.sliders.show(ui, &state, &self.camera, &self.palette);
                            button_zoom_hold = slider_out.zoom_button_hold;
                        });

                        ui.add_space(6.0);
                        if crate::ui::shortcut_footer(ui, &self.palette) {
                            self.shortcuts_modal_open = true;
                        }
                    });
            });

        // Keyboard controls cheat sheet modal overlay
        crate::ui::shortcuts_modal::show(&ctx, &mut self.shortcuts_modal_open, &self.palette);

        if config_dirty {
            self.config.save();
        }

        if stop_clicked || self.shortcuts_modal_open {
            if self.active_hold.take().is_some() {
                self.camera.send(CameraCommand::StopAll);
            }
            self.last_heartbeat = None;
            self.stop_zoom_hold(&state);
        } else {
            let desired_hold = pointer_hold.or(key_hold);
            self.update_hold_state(&ctx, desired_hold);

            let desired_zoom_hold = if button_zoom_hold != 0 {
                button_zoom_hold
            } else {
                key_zoom_hold
            };
            self.update_zoom_hold(&ctx, &state, desired_zoom_hold);
            self.apply_preview_scroll_zoom(&ctx, &state, preview_scroll_y);
        }
    }
}
