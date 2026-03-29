use eframe::egui::{self, Color32, Context, FontData, FontDefinitions, FontFamily, Stroke};

pub(crate) fn apply_theme(ctx: &Context) {
    install_text_fonts(ctx);

    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = Color32::TRANSPARENT;
    visuals.extreme_bg_color = Color32::from_rgb(255, 252, 246);
    visuals.faint_bg_color = Color32::from_rgb(247, 241, 230);
    visuals.override_text_color = Some(Color32::from_rgb(51, 43, 35));
    visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(248, 243, 236);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(221, 211, 190));
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(244, 238, 227);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(234, 225, 207);
    visuals.widgets.active.bg_fill = Color32::from_rgb(225, 213, 188);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(66, 56, 45));
    visuals.window_fill = Color32::from_rgb(247, 241, 230);
    visuals.selection.bg_fill = Color32::from_rgb(198, 106, 44);
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.interact_size = egui::vec2(44.0, 28.0);
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
