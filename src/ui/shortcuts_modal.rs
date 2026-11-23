use egui::{Align2, CornerRadius, Id, Margin, Order, Sense, Stroke, Vec2};
use fastframe_fonts::Weight;

use crate::icons::AppIcon;
use crate::theme::AppPalette;

struct ShortcutRow {
    keys: &'static [&'static str],
    action: &'static str,
}

const PTZ_ROWS: &[ShortcutRow] = &[
    ShortcutRow {
        keys: &["W", "A", "S", "D"],
        action: "Pan & Tilt (follows Hold / Nudge mode)",
    },
    ShortcutRow {
        keys: &["↑", "←", "↓", "→"],
        action: "Pan & Tilt (arrow keys)",
    },
    ShortcutRow {
        keys: &["Space", "Esc"],
        action: "Emergency Stop motor immediately",
    },
    ShortcutRow {
        keys: &["M"],
        action: "Switch Hold / Nudge mode",
    },
];

const ZOOM_ROWS: &[ShortcutRow] = &[
    ShortcutRow {
        keys: &["+", "-"],
        action: "Hold for smooth zoom (Shift = fine/slow)",
    },
    ShortcutRow {
        keys: &["Scroll"],
        action: "Scroll on preview for smooth zoom",
    },
    ShortcutRow {
        keys: &["1", "2", "5", "0"],
        action: "Quick Zoom (1x / 2x / 5x / 10x)",
    },
];

const OPTICS_ROWS: &[ShortcutRow] = &[
    ShortcutRow {
        keys: &["[", "]"],
        action: "Manual Focus Near / Far",
    },
    ShortcutRow {
        keys: &["F"],
        action: "Toggle Auto Focus",
    },
    ShortcutRow {
        keys: &[",", "."],
        action: "Manual Exposure Darker / Brighter",
    },
    ShortcutRow {
        keys: &["E"],
        action: "Toggle Auto Exposure",
    },
];

const APP_ROWS: &[ShortcutRow] = &[
    ShortcutRow {
        keys: &["P"],
        action: "Toggle Live Preview (OBS-safe)",
    },
    ShortcutRow {
        keys: &["T"],
        action: "Toggle Always-on-Top",
    },
    ShortcutRow {
        keys: &["R"],
        action: "Reconnect / Refresh camera",
    },
    ShortcutRow {
        keys: &["?", "F1"],
        action: "Open / Close this cheat sheet",
    },
];

/// Renders a centered modal cheat sheet with dimmed backdrop. Returns `true` if it should remain open.
pub fn show(ctx: &egui::Context, open: &mut bool, palette: &AppPalette) {
    if !*open {
        return;
    }

    let screen_rect = ctx.content_rect();

    // Dimmed backdrop — click anywhere outside modal to close
    egui::Area::new(Id::new("shortcuts_modal_backdrop"))
        .order(Order::Middle)
        .fixed_pos(screen_rect.min)
        .show(ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(screen_rect.size(), Sense::click());
            ui.painter().rect_filled(rect, 0.0, palette.overlay);
            if resp.clicked() {
                *open = false;
            }
        });

    let modal_w = (screen_rect.width() - 28.0).clamp(260.0, 340.0);

    egui::Area::new(Id::new("shortcuts_modal_dialog"))
        .order(Order::Foreground)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(palette.panel)
                .stroke(Stroke::new(1.0, palette.outline))
                .corner_radius(CornerRadius::same(14))
                .shadow(egui::Shadow {
                    offset: [0, 8],
                    blur: 24,
                    spread: 0,
                    color: palette.shadow,
                })
                .inner_margin(Margin::same(14))
                .show(ui, |ui| {
                    ui.set_width(modal_w);

                    // Header
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Keyboard Controls")
                                .font(Weight::SemiBold.font_id(15.0))
                                .color(palette.text),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let (rect, resp) =
                                ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
                            let bg = if resp.hovered() {
                                palette.surface_hover
                            } else {
                                palette.surface
                            };
                            ui.painter().rect(
                                rect,
                                CornerRadius::same(6),
                                bg,
                                Stroke::new(1.0, palette.outline),
                                egui::StrokeKind::Inside,
                            );
                            let icon_rect =
                                egui::Rect::from_center_size(rect.center(), Vec2::splat(13.0));
                            AppIcon::Close
                                .image(palette.text, 13.0)
                                .paint_at(ui, icon_rect);
                            if resp.on_hover_text("Close (Esc)").clicked() {
                                *open = false;
                            }
                        });
                    });

                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(
                            "Works whenever PTZ Controller is focused. Motor stops automatically if window loses focus.",
                        )
                        .font(Weight::Regular.font_id(11.0))
                        .color(palette.secondary),
                    );
                    ui.add_space(8.0);
                    ui.separator();

                    egui::ScrollArea::vertical()
                        .max_height((screen_rect.height() - 130.0).max(200.0))
                        .show(ui, |ui| {
                            ui.add_space(6.0);
                            show_section(ui, "PAN & TILT", PTZ_ROWS, palette);
                            ui.add_space(8.0);
                            show_section(ui, "ZOOM", ZOOM_ROWS, palette);
                            ui.add_space(8.0);
                            show_section(ui, "FOCUS & EXPOSURE", OPTICS_ROWS, palette);
                            ui.add_space(8.0);
                            show_section(ui, "APP & PREVIEW", APP_ROWS, palette);
                        });
                });
        });
}

fn show_section(ui: &mut egui::Ui, title: &str, rows: &[ShortcutRow], palette: &AppPalette) {
    ui.label(
        egui::RichText::new(title)
            .font(Weight::SemiBold.font_id(10.5))
            .color(palette.dim),
    );
    ui.add_space(4.0);

    for row in rows {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for key in row.keys {
                draw_keycap(ui, key, palette);
            }
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(row.action)
                    .font(Weight::Regular.font_id(11.5))
                    .color(palette.text),
            );
        });
        ui.add_space(3.0);
    }
}

fn draw_keycap(ui: &mut egui::Ui, key: &str, palette: &AppPalette) {
    egui::Frame::new()
        .fill(palette.surface)
        .stroke(Stroke::new(1.0, palette.outline))
        .corner_radius(CornerRadius::same(5))
        .inner_margin(Margin::symmetric(5, 2))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(key)
                    .font(Weight::SemiBold.font_id(10.5))
                    .color(palette.text),
            );
        });
}
