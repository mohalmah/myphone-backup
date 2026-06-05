use crate::app::{AppTab, BackupApp};
use crate::ui::theme::*;
use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, RichText, Stroke};

pub(crate) fn render_nav_rail(ctx: &egui::Context, app: &mut BackupApp) {
    egui::SidePanel::left("nav_rail")
        .resizable(false)
        .exact_width(116.0)
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(Color32::from_rgb(252, 252, 252))
                .stroke(Stroke::new(1.0, BORDER_CARD))
                .inner_margin(Margin::symmetric(12, 14)),
        )
        .show(ctx, |ui| {
            brand(ui);
            ui.add_space(18.0);

            nav_item(ui, app, AppTab::Dashboard, "Home", "Start here", false);
            nav_item(ui, app, AppTab::Backup, "Backup", "Copy safely", false);
            nav_item(ui, app, AppTab::Cleanup, "Cleanup", "Delete safely", false);
            nav_item(ui, app, AppTab::Devices, "Device", "Soon", true);

            ui.add_space(14.0);
            ui.separator();
            ui.add_space(10.0);

            log_toggle(ui, app);
            nav_item(ui, app, AppTab::Settings, "Settings", "Controls", false);
        });
}

fn brand(ui: &mut egui::Ui) {
    Frame::new()
        .fill(ACCENT.gamma_multiply(0.08))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(10, 10))
        .show(ui, |ui| {
            ui.set_min_width(72.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("ADB").size(16.0).strong().color(ACCENT));
                ui.label(
                    RichText::new("Smart Backup")
                        .size(9.5)
                        .color(TEXT_SECONDARY),
                );
            });
        });
}

fn nav_item(
    ui: &mut egui::Ui,
    app: &mut BackupApp,
    tab: AppTab,
    label: &str,
    hint: &str,
    disabled: bool,
) {
    let active = app.active_tab == tab;
    let fill = if active {
        Color32::from_rgb(232, 243, 255)
    } else {
        Color32::TRANSPARENT
    };
    let stroke = if active {
        Stroke::new(1.0, ACCENT)
    } else {
        Stroke::new(1.0, Color32::TRANSPARENT)
    };
    let title_color = if disabled {
        TEXT_TERTIARY
    } else if active {
        ACCENT
    } else {
        TEXT_PRIMARY
    };

    let inner = Frame::new()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_min_size(egui::vec2(78.0, 42.0));
            ui.vertical(|ui| {
                ui.label(RichText::new(label).size(12.0).strong().color(title_color));
                ui.label(RichText::new(hint).size(9.0).color(TEXT_TERTIARY));
            });
        });

    let response = ui.interact(
        inner.response.rect,
        ui.make_persistent_id(("nav_item", label)),
        egui::Sense::click(),
    );
    if response.hovered() && !disabled {
        ui.output_mut(|output| output.cursor_icon = egui::CursorIcon::PointingHand);
    }
    if response.clicked() && !disabled {
        app.active_tab = tab;
    }

    ui.add_space(6.0);
}

fn log_toggle(ui: &mut egui::Ui, app: &mut BackupApp) {
    let active = app.nerd_mode;
    let fill = if active {
        Color32::from_rgb(232, 243, 255)
    } else {
        Color32::TRANSPARENT
    };
    let stroke = if active {
        Stroke::new(1.0, ACCENT)
    } else {
        Stroke::new(1.0, Color32::TRANSPARENT)
    };

    let inner = Frame::new()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_min_size(egui::vec2(78.0, 42.0));
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(if active { "Logs on" } else { "Logs" })
                        .size(12.0)
                        .strong()
                        .color(if active { ACCENT } else { TEXT_PRIMARY }),
                );
                ui.label(RichText::new("ADB output").size(9.0).color(TEXT_TERTIARY));
            });
        });

    let response = ui.interact(
        inner.response.rect,
        ui.make_persistent_id("nav_logs_toggle"),
        egui::Sense::click(),
    );
    if response.hovered() {
        ui.output_mut(|output| output.cursor_icon = egui::CursorIcon::PointingHand);
    }
    if response.clicked() {
        app.nerd_mode = !app.nerd_mode;
    }

    ui.add_space(6.0);
}
