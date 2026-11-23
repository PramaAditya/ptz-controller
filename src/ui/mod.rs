pub mod dpad;
pub mod preview;
pub mod shortcuts_modal;
pub mod sliders;
pub mod status_bar;

use egui::{CornerRadius, Margin, Sense, Stroke};
use fastframe_fonts::Weight;

use crate::theme::AppPalette;

/// Styled surface card used to group remote controls cleanly.
pub fn card(ui: &mut egui::Ui, palette: &AppPalette, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(palette.panel)
        .stroke(Stroke::new(1.0, palette.outline))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui);
        });
}

/// Bottom clickable keyboard-shortcut hint + author credit bar. Returns `true` if shortcuts clicked.
pub fn shortcut_footer(ui: &mut egui::Ui, palette: &AppPalette) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let shortcuts_resp = ui
            .add(
                egui::Label::new(
                    egui::RichText::new("Press ? for shortcuts")
                        .font(Weight::Medium.font_id(10.5))
                        .color(palette.secondary),
                )
                .sense(Sense::click()),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Click to open keyboard controls cheat sheet");
        if shortcuts_resp.clicked() {
            clicked = true;
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            let prama_resp = ui
                .add(
                    egui::Label::new(
                        egui::RichText::new("Prama")
                            .font(Weight::SemiBold.font_id(10.5))
                            .color(palette.accent),
                    )
                    .sense(Sense::click()),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text("https://github.com/PramaAditya");
            if prama_resp.clicked() {
                ui.ctx()
                    .open_url(egui::OpenUrl::new_tab("https://github.com/PramaAditya"));
            }

            ui.label(
                egui::RichText::new("Made by")
                    .font(Weight::Regular.font_id(10.5))
                    .color(palette.dim),
            );
        });
    });
    clicked
}
