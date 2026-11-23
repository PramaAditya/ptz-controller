use egui::{CornerRadius, Pos2, Rect, Sense, Stroke, TextureHandle, TextureOptions, Vec2};
use fastframe_fonts::Weight;

use crate::camera::CameraState;
use crate::camera::preview::{PreviewHandle, PreviewState, PreviewStatus};
use crate::theme::AppPalette;

pub struct PreviewUiState {
    texture: Option<TextureHandle>,
    last_seq: u64,
}

impl Default for PreviewUiState {
    fn default() -> Self {
        Self {
            texture: None,
            last_seq: 0,
        }
    }
}

impl PreviewUiState {
    pub fn clear_texture(&mut self) {
        self.texture = None;
        self.last_seq = 0;
    }

    /// Returns scroll zoom delta (`>0` zoom in, `<0` zoom out) if user scrolled over the preview.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        preview: &PreviewState,
        preview_handle: &PreviewHandle,
        cam_state: &CameraState,
        palette: &AppPalette,
    ) -> f32 {
        // Upload new frame into GPU texture only when sequence number changes
        if let Some(frame) = &preview.latest_frame
            && frame.seq != self.last_seq
        {
            self.last_seq = frame.seq;
            match self.texture.as_mut() {
                Some(tex) => tex.set(frame.image.clone(), TextureOptions::LINEAR),
                None => {
                    self.texture = Some(ui.ctx().load_texture(
                        "ptz-live-preview",
                        frame.image.clone(),
                        TextureOptions::LINEAR,
                    ));
                }
            }
        }

        // Preserve exact native aspect ratio regardless of window width/height
        let avail_w = ui.available_width().max(220.0);
        let native_aspect = preview
            .latest_frame
            .as_ref()
            .filter(|f| f.src_width > 0 && f.src_height > 0)
            .map(|f| f.src_width as f32 / f.src_height as f32)
            .unwrap_or(16.0 / 9.0)
            .clamp(1.0, 2.4);

        let win_h = ui.ctx().content_rect().height();
        let max_h = (win_h * 0.29).clamp(160.0, 460.0);
        let box_h = (avail_w / native_aspect).clamp(140.0, max_h);
        let (rect, response) = ui.allocate_exact_size(Vec2::new(avail_w, box_h), Sense::hover());

        // Dark cinema backdrop + border
        let bg = egui::Color32::from_rgb(0x0B, 0x0D, 0x12);
        ui.painter().rect(
            rect,
            CornerRadius::same(10),
            bg,
            Stroke::new(1.0, palette.outline),
            egui::StrokeKind::Inside,
        );

        let inner = rect.shrink(1.0);

        if let Some(tex) = &self.texture
            && matches!(preview.status, PreviewStatus::Streaming { .. })
        {
            // Strict contain-fit so the video NEVER stretches or flattens on resize
            let inner_aspect = inner.width() / inner.height().max(1.0);
            let img_size = if inner_aspect > native_aspect {
                Vec2::new(inner.height() * native_aspect, inner.height())
            } else {
                Vec2::new(inner.width(), inner.width() / native_aspect)
            };
            let img_rect = Rect::from_center_size(inner.center(), img_size);

            let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
            ui.painter()
                .image(tex.id(), img_rect, uv, egui::Color32::WHITE);

            // Top-left live status badge pinned to image corner
            if let PreviewStatus::Streaming { width, height, fps } = preview.status {
                let badge = format!("{width}×{height} · {:.0} fps · Shared", fps.max(1.0));
                draw_overlay_pill(
                    ui,
                    img_rect.left_top() + Vec2::new(8.0, 8.0),
                    &badge,
                    palette.success,
                );
            }

            // Bottom-left active PTZ badge while moving
            if cam_state.active_pan != 0 || cam_state.active_tilt != 0 {
                let dir = match (cam_state.active_pan, cam_state.active_tilt) {
                    (-1, 0) => "◀ PAN LEFT",
                    (1, 0) => "▶ PAN RIGHT",
                    (0, 1) => "▲ TILT UP",
                    (0, -1) => "▼ TILT DOWN",
                    _ => "● MOVING",
                };
                draw_overlay_pill(
                    ui,
                    Pos2::new(img_rect.left() + 8.0, img_rect.bottom() - 26.0),
                    dir,
                    palette.accent,
                );
            }
        } else {
            // Loading / Waiting / Error placeholder
            let (headline, subtext, show_retry) = match &preview.status {
                PreviewStatus::Starting => (
                    "Starting shared preview…",
                    "Opening in OBS-safe SharedReadOnly mode",
                    false,
                ),
                PreviewStatus::WaitingForStream => (
                    "Waiting for camera frames…",
                    "OBS-safe SharedReadOnly mode active",
                    false,
                ),
                PreviewStatus::Error(err) => (
                    "Preview unavailable",
                    err.as_str(),
                    true,
                ),
                PreviewStatus::Off => ("Preview paused", "Press P to resume", false),
                PreviewStatus::Streaming { .. } => ("Loading frame…", "", false),
            };

            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
            child.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(box_h * 0.28);
                    ui.label(
                        egui::RichText::new(headline)
                            .font(Weight::SemiBold.font_id(12.5))
                            .color(egui::Color32::from_rgb(0xE2, 0xE8, 0xF0)),
                    );
                    if !subtext.is_empty() {
                        ui.add_space(3.0);
                        ui.label(
                            egui::RichText::new(subtext)
                                .font(Weight::Regular.font_id(11.0))
                                .color(egui::Color32::from_rgb(0x94, 0xA3, 0xB8)),
                        );
                    }
                    if show_retry {
                        ui.add_space(8.0);
                        if ui.small_button("Retry Preview").clicked() {
                            preview_handle.restart();
                        }
                    }
                });
            });
        }

        let scroll_zoom = if response.hovered() {
            let delta = ui.input(|i| i.smooth_scroll_delta.y);
            if delta.abs() > 0.1 {
                // Consume vertical scroll so the parent ScrollArea doesn't jump while zooming
                ui.input_mut(|i| i.smooth_scroll_delta.y = 0.0);
                delta
            } else {
                0.0
            }
        } else {
            0.0
        };

        let _ = response.on_hover_text(
            "Windows FrameServer SharedReadOnly preview (OBS-safe) · Scroll wheel to zoom (Shift = fine)",
        );

        scroll_zoom
    }
}

fn draw_overlay_pill(ui: &egui::Ui, pos: Pos2, text: &str, dot_color: egui::Color32) {
    let font = Weight::Medium.font_id(10.5);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font, egui::Color32::WHITE);
    let pad_x = 7.0;
    let dot_space = 10.0;
    let pill_size = Vec2::new(
        galley.size().x + pad_x * 2.0 + dot_space,
        galley.size().y + 6.0,
    );
    let pill_rect = Rect::from_min_size(pos, pill_size);

    ui.painter().rect_filled(
        pill_rect,
        CornerRadius::same(6),
        egui::Color32::from_black_alpha(165),
    );
    ui.painter().circle_filled(
        Pos2::new(pill_rect.left() + pad_x + 2.5, pill_rect.center().y),
        3.0,
        dot_color,
    );
    ui.painter().galley(
        Pos2::new(
            pill_rect.left() + pad_x + dot_space,
            pill_rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        egui::Color32::WHITE,
    );
}
