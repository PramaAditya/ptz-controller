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

/// Bottom clickable keyboard-shortcut hint bar. Returns `true` if clicked to open cheat sheet.
pub fn shortcut_footer(ui: &mut egui::Ui, palette: &AppPalette) -> bool {
    let mut clicked = false;
    ui.vertical_centered(|ui| {
        let resp = ui
            .add(
                egui::Label::new(
                    egui::RichText::new(
                        "WASD/Arrows · +/- Zoom · P Preview · Press ? for all shortcuts",
                    )
                    .font(Weight::Medium.font_id(10.5))
                    .color(palette.secondary),
                )
                .sense(Sense::click()),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Click to open keyboard controls cheat sheet");
        if resp.clicked() {
            clicked = true;
        }
    });
    clicked
}
