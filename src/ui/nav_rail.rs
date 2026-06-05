use crate::app::{AppTab, BackupApp};
use crate::ui::theme::*;
use eframe::egui::{self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Stroke};

pub(crate) fn render_nav_rail(ctx: &egui::Context, app: &mut BackupApp) {
    egui::SidePanel::left("nav_rail")
        .resizable(false)
        .exact_width(92.0)
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(BG_LAYER)
                .stroke(Stroke::new(1.0, BORDER_CARD))
                .inner_margin(Margin::symmetric(8, 10)),
        )
        .show(ctx, |ui| {
            ui.set_min_height(ui.available_height());

            ui.vertical_centered(|ui| {
                ui.label(RichText::new("ADB").size(15.0).strong().color(TEXT_PRIMARY));
                ui.label(RichText::new("Backup").size(10.0).color(TEXT_TERTIARY));
            });
            ui.add_space(16.0);

            nav_item(ui, app, AppTab::Dashboard, "Home", false);
            nav_item(ui, app, AppTab::Backup, "Backup", false);
            nav_item(ui, app, AppTab::Cleanup, "Cleanup", false);
            nav_item(ui, app, AppTab::Devices, "Device", true);

            ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                nav_item(ui, app, AppTab::Settings, "Settings", false);
                ui.add_space(8.0);
                nerd_toggle(ui, app);
            });
        });
}

fn nav_item(ui: &mut egui::Ui, app: &mut BackupApp, tab: AppTab, label: &str, coming_soon: bool) {
    let is_active = app.active_tab == tab;
    let text_color = if coming_soon {
        TEXT_TERTIARY
    } else if is_active {
        ACCENT
    } else {
        TEXT_PRIMARY
    };
    let fill = if is_active {
        ACCENT.gamma_multiply(0.08)
    } else {
        Color32::TRANSPARENT
    };

    let inner = Frame::new()
        .fill(fill)
        .stroke(Stroke::new(
            if is_active { 1.0 } else { 0.0 },
            if is_active {
                ACCENT
            } else {
                Color32::TRANSPARENT
            },
        ))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(8, 9))
        .show(ui, |ui| {
            ui.set_min_width(58.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(label).size(11.0).strong().color(text_color));
                if coming_soon {
                    ui.label(RichText::new("soon").size(9.0).color(TEXT_TERTIARY));
                }
            });
        });

    let response = ui.interact(
        inner.response.rect,
        ui.make_persistent_id(("nav_item", label)),
        egui::Sense::click(),
    );
    if response.hovered() && !coming_soon {
        ui.output_mut(|output| output.cursor_icon = egui::CursorIcon::PointingHand);
    }
    if response.clicked() && !coming_soon {
        app.active_tab = tab;
    }

    ui.add_space(6.0);
}

fn nerd_toggle(ui: &mut egui::Ui, app: &mut BackupApp) {
    let label = if app.nerd_mode { "Logs on" } else { "Logs" };
    let color = if app.nerd_mode {
        ACCENT
    } else {
        TEXT_SECONDARY
    };
    let fill = if app.nerd_mode {
        ACCENT.gamma_multiply(0.08)
    } else {
        Color32::TRANSPARENT
    };

    let inner = Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(8, 8))
        .show(ui, |ui| {
            ui.set_min_width(58.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(label).size(10.5).strong().color(color));
            });
        });

    let response = ui.interact(
        inner.response.rect,
        ui.make_persistent_id("nerd_toggle"),
        egui::Sense::click(),
    );
    if response.hovered() {
        ui.output_mut(|output| output.cursor_icon = egui::CursorIcon::PointingHand);
    }
    if response.clicked() {
        app.nerd_mode = !app.nerd_mode;
    }
}
