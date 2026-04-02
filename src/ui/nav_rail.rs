use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Stroke,
};
use crate::app::{AppTab, BackupApp};
use crate::ui::theme::*;

pub(crate) fn render_nav_rail(ctx: &egui::Context, app: &mut BackupApp) {
    egui::SidePanel::left("nav_rail")
        .resizable(false)
        .exact_width(88.0)
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(BG_LAYER)
                .stroke(Stroke::new(1.0, BORDER_CARD))
                .inner_margin(Margin::same(0)),
        )
        .show(ctx, |ui| {
            ui.set_min_height(ui.available_height());

            // Hamburger (decorative — always expanded)
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.label(RichText::new("☰").size(18.0).color(TEXT_PRIMARY));
            });
            ui.add_space(8.0);

            // Primary nav items
            nav_item(ui, app, AppTab::Dashboard, "⊞", "Dashboard", false);
            nav_item(ui, app, AppTab::Backup, "📦", "Backup", false);
            nav_item(ui, app, AppTab::Cleanup, "🧹", "Cleanup", false);
            nav_item(ui, app, AppTab::Devices, "📱", "Devices", true);

            // Bottom-pinned items
            ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                // Nerd mode toggle
                ui.add_space(6.0);
                let nerd_color = if app.nerd_mode { ACCENT } else { TEXT_TERTIARY };
                let inner = Frame::new()
                    .fill(Color32::TRANSPARENT)
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::symmetric(4, 6))
                    .show(ui, |ui| {
                        ui.set_min_width(72.0);
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new("⌨").size(14.0).color(nerd_color));
                            ui.label(
                                RichText::new(if app.nerd_mode { "Nerd ✓" } else { "Nerd" })
                                    .size(9.0)
                                    .color(nerd_color),
                            );
                        });
                    });
                let resp = ui.interact(
                    inner.response.rect,
                    ui.make_persistent_id("nerd_toggle"),
                    egui::Sense::click(),
                );
                if resp.hovered() {
                    ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    app.nerd_mode = !app.nerd_mode;
                }

                // Settings nav item (bottom-pinned)
                nav_item(ui, app, AppTab::Settings, "⚙", "Settings", false);
            });
        });
}

fn nav_item(
    ui: &mut egui::Ui,
    app: &mut BackupApp,
    tab: AppTab,
    icon: &str,
    label: &str,
    coming_soon: bool,
) {
    let is_active = app.active_tab == tab;
    let text_color = if coming_soon {
        TEXT_TERTIARY
    } else if is_active {
        ACCENT
    } else {
        TEXT_PRIMARY
    };
    let fill = if is_active { BG_CARD } else { Color32::TRANSPARENT };

    let inner = Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(4, 8))
        .show(ui, |ui| {
            ui.set_min_width(72.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(icon).size(16.0).color(text_color));
                ui.label(RichText::new(label).size(9.5).color(text_color));
            });
        });

    // Accent left-edge indicator bar for active item
    if is_active {
        let rect = inner.response.rect;
        ui.painter().rect_filled(
            egui::Rect::from_min_size(rect.left_top(), egui::vec2(3.0, rect.height())),
            0.0,
            ACCENT,
        );
    }

    let response = ui.interact(
        inner.response.rect,
        ui.make_persistent_id(("nav_item", label)),
        egui::Sense::click(),
    );
    if response.hovered() && !coming_soon {
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
    }
    if response.clicked() && !coming_soon {
        app.active_tab = tab;
    }

    ui.add_space(2.0);
}
