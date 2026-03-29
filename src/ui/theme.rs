#![allow(dead_code)]
use eframe::egui::{self, Color32, Context, FontData, FontDefinitions, FontFamily, Stroke};

// ── Fluent Design Light Mode Color Tokens ──
pub const BG_BASE: Color32 = Color32::from_rgb(243, 243, 243);       // Mica base
pub const BG_LAYER: Color32 = Color32::from_rgb(249, 249, 249);      // Layer/surface
pub const BG_CARD: Color32 = Color32::from_rgb(255, 255, 255);       // Card
pub const BG_CARD_HOVER: Color32 = Color32::from_rgb(246, 246, 246); // Card hover
pub const ACCENT: Color32 = Color32::from_rgb(0, 120, 212);          // Windows blue
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0, 99, 177);
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(28, 28, 28);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(96, 96, 96);
pub const TEXT_TERTIARY: Color32 = Color32::from_rgb(140, 140, 140);
pub const BORDER_CARD: Color32 = Color32::from_rgb(229, 229, 229);
pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(220, 220, 220);
pub const SUCCESS: Color32 = Color32::from_rgb(16, 124, 16);
pub const ERROR: Color32 = Color32::from_rgb(196, 43, 28);
pub const WARNING: Color32 = Color32::from_rgb(157, 93, 0);

pub(crate) fn apply_theme(ctx: &Context) {
    install_text_fonts(ctx);

    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = BG_BASE;
    visuals.extreme_bg_color = BG_CARD;
    visuals.faint_bg_color = BG_LAYER;
    visuals.override_text_color = Some(TEXT_PRIMARY);
    visuals.widgets.noninteractive.bg_fill = BG_LAYER;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER_CARD);
    visuals.widgets.inactive.bg_fill = BG_CARD;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER_CARD);
    visuals.widgets.hovered.bg_fill = BG_CARD_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.active.bg_fill = Color32::from_rgb(230, 230, 230);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    visuals.window_fill = BG_LAYER;
    visuals.selection.bg_fill = ACCENT;
    visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(11.0, 5.0);
    style.spacing.interact_size = egui::vec2(20.0, 32.0);
    style.interaction.resize_grab_radius_side = 3.0;
    ctx.set_style(style);
}

pub(crate) fn install_text_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    let fallback_fonts = [
        ("windows_tahoma", "C:\\Windows\\Fonts\\tahoma.ttf"),
        ("windows_arial", "C:\\Windows\\Fonts\\arial.ttf"),
        ("windows_segoe_ui", "C:\\Windows\\Fonts\\segoeui.ttf"),
    ];

    for (font_name, path) in fallback_fonts.into_iter().rev() {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert(font_name.to_string(), FontData::from_owned(bytes).into());
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .insert(0, font_name.to_string());
            fonts
                .families
                .entry(FontFamily::Monospace)
                .or_default()
                .insert(0, font_name.to_string());
        }
    }

    ctx.set_fonts(fonts);
}
