use egui::{Color32, CornerRadius, Stroke, Visuals};
use fastframe_text::TextRendering;
use fastframe_theme::{Base, Palette, PaletteBase};

#[derive(Clone, Debug, PartialEq)]
pub struct AppPalette {
    pub base: Base,
    pub window: Color32,
    pub panel: Color32,
    pub surface: Color32,
    pub surface_hover: Color32,
    pub surface_active: Color32,
    pub outline: Color32,
    pub text: Color32,
    pub secondary: Color32,
    pub dim: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub on_accent: Color32,
    pub danger: Color32,
    pub warning: Color32,
    pub overlay: Color32,
    pub shadow: Color32,
    pub success: Color32,
}

impl Palette for AppPalette {
    fn base(base: Base) -> Self {
        match base {
            Base::Dark => Self {
                base,
                window: Color32::from_rgb(0x11, 0x13, 0x18),
                panel: Color32::from_rgb(0x18, 0x1B, 0x22),
                surface: Color32::from_rgb(0x21, 0x25, 0x30),
                surface_hover: Color32::from_rgb(0x2A, 0x30, 0x3E),
                surface_active: Color32::from_rgb(0x34, 0x3C, 0x4E),
                outline: Color32::from_rgb(0x2E, 0x34, 0x44),
                text: Color32::from_rgb(0xF1, 0xF5, 0xF9),
                secondary: Color32::from_rgb(0x94, 0xA3, 0xB8),
                dim: Color32::from_rgb(0x64, 0x74, 0x8B),
                accent: Color32::from_rgb(0x3B, 0x82, 0xF6),
                accent_hover: Color32::from_rgb(0x60, 0xA5, 0xFA),
                on_accent: Color32::WHITE,
                danger: Color32::from_rgb(0xEF, 0x44, 0x44),
                warning: Color32::from_rgb(0xF5, 0x9E, 0x0B),
                overlay: Color32::from_black_alpha(140),
                shadow: Color32::from_black_alpha(96),
                success: Color32::from_rgb(0x22, 0xC5, 0x5E),
            },
            Base::Light => Self {
                base,
                window: Color32::from_rgb(0xF4, 0xF6, 0xFA),
                panel: Color32::from_rgb(0xFF, 0xFF, 0xFF),
                surface: Color32::from_rgb(0xEE, 0xF1, 0xF7),
                surface_hover: Color32::from_rgb(0xE2, 0xE8, 0xF0),
                surface_active: Color32::from_rgb(0xCB, 0xD5, 0xE1),
                outline: Color32::from_rgb(0xD8, 0xE0, 0xEC),
                text: Color32::from_rgb(0x0F, 0x17, 0x2A),
                secondary: Color32::from_rgb(0x47, 0x55, 0x69),
                dim: Color32::from_rgb(0x64, 0x74, 0x8B),
                accent: Color32::from_rgb(0x25, 0x63, 0xEB),
                accent_hover: Color32::from_rgb(0x1D, 0x4E, 0xD8),
                on_accent: Color32::WHITE,
                danger: Color32::from_rgb(0xDC, 0x26, 0x26),
                warning: Color32::from_rgb(0xD9, 0x77, 0x06),
                overlay: Color32::from_black_alpha(80),
                shadow: Color32::from_black_alpha(32),
                success: Color32::from_rgb(0x16, 0xA3, 0x4A),
            },
        }
    }

    fn set(&mut self, name: &str, color: Color32) -> bool {
        match name {
            "window" => self.window = color,
            "panel" => self.panel = color,
            "surface" => self.surface = color,
            "surface_hover" => self.surface_hover = color,
            "surface_active" => self.surface_active = color,
            "outline" => self.outline = color,
            "text" => self.text = color,
            "secondary" => self.secondary = color,
            "dim" => self.dim = color,
            "accent" => self.accent = color,
            "accent_hover" => self.accent_hover = color,
            "on_accent" => self.on_accent = color,
            "danger" => self.danger = color,
            "warning" => self.warning = color,
            "overlay" => self.overlay = color,
            "shadow" => self.shadow = color,
            _ => return false,
        }
        true
    }
}

impl PaletteBase for AppPalette {
    fn palette_base(&self) -> Base {
        self.base
    }
}

impl AppPalette {
    pub fn apply(&self, ctx: &egui::Context, rendering: &TextRendering) {
        let mut visuals = match self.base {
            Base::Dark => Visuals::dark(),
            Base::Light => Visuals::light(),
        };

        visuals.panel_fill = self.window;
        visuals.window_fill = self.panel;
        visuals.extreme_bg_color = self.surface;
        visuals.faint_bg_color = self.panel;
        visuals.override_text_color = Some(self.text);

        visuals.selection.bg_fill = self.accent;
        visuals.selection.stroke = Stroke::new(1.0, self.on_accent);

        visuals.widgets.noninteractive.bg_fill = self.panel;
        visuals.widgets.noninteractive.weak_bg_fill = self.panel;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, self.outline);
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, self.secondary);
        visuals.widgets.noninteractive.corner_radius = CornerRadius::same(8);

        visuals.widgets.inactive.bg_fill = self.surface;
        visuals.widgets.inactive.weak_bg_fill = self.surface;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, self.outline);
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, self.text);
        visuals.widgets.inactive.corner_radius = CornerRadius::same(8);

        visuals.widgets.hovered.bg_fill = self.surface_hover;
        visuals.widgets.hovered.weak_bg_fill = self.surface_hover;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, self.accent);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, self.text);
        visuals.widgets.hovered.corner_radius = CornerRadius::same(8);

        visuals.widgets.active.bg_fill = self.surface_active;
        visuals.widgets.active.weak_bg_fill = self.surface_active;
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, self.accent_hover);
        visuals.widgets.active.fg_stroke = Stroke::new(1.0, self.text);
        visuals.widgets.active.corner_radius = CornerRadius::same(8);

        rendering.apply_to_visuals(&mut visuals);

        let (theme_pref, theme) = match self.base {
            Base::Dark => (egui::ThemePreference::Dark, egui::Theme::Dark),
            Base::Light => (egui::ThemePreference::Light, egui::Theme::Light),
        };
        ctx.set_theme(theme_pref);
        ctx.set_visuals_of(theme, visuals);
        ctx.all_styles_mut(|style| {
            style.spacing.item_spacing = egui::vec2(8.0, 8.0);
            style.spacing.button_padding = egui::vec2(8.0, 5.0);
            style.spacing.slider_width = 160.0;
        });
    }
}
