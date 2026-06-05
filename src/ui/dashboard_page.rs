use crate::app::{AppTab, BackupApp};
use crate::ui::theme::*;
use crate::ui::widgets::*;
use eframe::egui::{
    self, Align, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea, Stroke,
};

pub(crate) fn render_dashboard_page(ctx: &egui::Context, app: &mut BackupApp) {
    egui::CentralPanel::default()
        .frame(Frame::new().fill(BG_BASE).inner_margin(Margin::same(22)))
        .show(ctx, |ui| {
            ScrollArea::vertical()
                .id_salt("dashboard_scroll")
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    header(ui, app);
                    ui.add_space(16.0);

                    if ui.available_width() >= 900.0 {
                        ui.horizontal_top(|ui| {
                            ui.vertical(|ui| {
                                ui.set_width((ui.available_width() * 0.56).max(480.0));
                                quick_start_card(ui, app);
                            });
                            ui.add_space(14.0);
                            ui.vertical(|ui| {
                                device_card(ui, app);
                                storage_card(ui, app);
                            });
                        });
                    } else {
                        quick_start_card(ui, app);
                        device_card(ui, app);
                        storage_card(ui, app);
                    }

                    ui.add_space(12.0);
                    recent_log_card(ui, app);
                });
        });
}

fn header(ui: &mut egui::Ui, app: &BackupApp) {
    ui.horizontal_wrapped(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new("ADB Smart Backup")
                    .size(25.0)
                    .strong()
                    .color(TEXT_PRIMARY),
            );
            ui.label(
                RichText::new("A safer way to move phone media, verify it, then clean up.")
                    .size(12.0)
                    .color(TEXT_SECONDARY),
            );
        });

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            status_pill(
                ui,
                app.device_info.state.label(),
                app.device_info.state.color(),
            );
            status_pill(
                ui,
                if app.settings.dry_run {
                    "SIMULATION"
                } else {
                    "LIVE"
                },
                if app.settings.dry_run {
                    WARNING
                } else {
                    SUCCESS
                },
            );
        });
    });
}

fn quick_start_card(ui: &mut egui::Ui, app: &mut BackupApp) {
    card(ui, |ui| {
        ui.label(
            RichText::new("Choose a workflow")
                .size(17.0)
                .strong()
                .color(TEXT_PRIMARY),
        );
        ui.label(
            RichText::new("Most users only need these two doors.")
                .size(12.0)
                .color(TEXT_SECONDARY),
        );
        ui.add_space(14.0);

        if big_action(
            ui,
            "Backup phone files",
            "Copy, verify, and optionally delete originals",
        )
        .clicked()
        {
            app.active_tab = AppTab::Backup;
        }
        ui.add_space(8.0);
        if big_action(
            ui,
            "Review cleanup",
            "Inspect a phone folder before deleting anything",
        )
        .clicked()
        {
            app.active_tab = AppTab::Cleanup;
        }

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(10.0);

        let last = app
            .last_backup_time
            .as_deref()
            .unwrap_or("No completed backup this session");
        ui.label(
            RichText::new("Last backup")
                .size(11.0)
                .color(TEXT_SECONDARY),
        );
        ui.label(RichText::new(last).size(12.0).color(TEXT_PRIMARY));

        if let Some(error) = &app.error_banner {
            ui.add_space(8.0);
            ui.colored_label(ERROR, error);
        }
        if !app.status_banner.is_empty() {
            ui.add_space(8.0);
            ui.label(
                RichText::new(&app.status_banner)
                    .size(12.0)
                    .color(TEXT_SECONDARY),
            );
        }
    });
}

fn big_action(ui: &mut egui::Ui, title: &str, subtitle: &str) -> egui::Response {
    let inner = Frame::new()
        .fill(BG_LAYER)
        .stroke(Stroke::new(1.0, BORDER_CARD))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).size(15.0).strong().color(TEXT_PRIMARY));
                    ui.label(RichText::new(subtitle).size(12.0).color(TEXT_SECONDARY));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new("Open").size(12.0).strong().color(ACCENT));
                });
            });
        });

    let response = ui.interact(
        inner.response.rect,
        ui.make_persistent_id(("dashboard_action", title)),
        egui::Sense::click(),
    );
    if response.hovered() {
        ui.output_mut(|output| output.cursor_icon = egui::CursorIcon::PointingHand);
    }
    response
}

fn device_card(ui: &mut egui::Ui, app: &mut BackupApp) {
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Device")
                    .size(15.0)
                    .strong()
                    .color(TEXT_PRIMARY),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(!app.has_active_adb_job(), egui::Button::new("Refresh"))
                    .clicked()
                {
                    app.refresh_device_info();
                }
            });
        });
        ui.add_space(8.0);
        status_pill(
            ui,
            app.device_info.state.label(),
            app.device_info.state.color(),
        );
        ui.add_space(8.0);

        let model = app
            .device_info
            .model
            .as_deref()
            .unwrap_or("No device model detected");
        ui.label(RichText::new(model).size(13.0).strong().color(TEXT_PRIMARY));
        if !app.device_info.serial.is_empty() {
            ui.label(
                RichText::new(format!("Serial {}", app.device_info.serial))
                    .size(11.0)
                    .color(TEXT_SECONDARY),
            );
        }
    });
}

fn storage_card(ui: &mut egui::Ui, app: &BackupApp) {
    let destination = std::path::PathBuf::from(&app.settings.destination_path);
    let free = crate::core::storage::available_space_for_path(&destination).unwrap_or(0);
    let total = crate::core::storage::total_space_for_path(&destination).unwrap_or(0);
    let used_fraction = if total > 0 {
        1.0 - (free as f32 / total as f32)
    } else {
        0.0
    };

    card(ui, |ui| {
        ui.label(
            RichText::new("Destination Space")
                .size(15.0)
                .strong()
                .color(TEXT_PRIMARY),
        );
        ui.add_space(8.0);
        ui.add(
            egui::ProgressBar::new(used_fraction)
                .fill(ACCENT)
                .desired_width(ui.available_width())
                .corner_radius(4),
        );
        ui.add_space(6.0);
        let text = if total > 0 {
            format!("{} free of {}", format_bytes(free), format_bytes(total))
        } else {
            "Choose a destination to check free space".to_string()
        };
        ui.label(RichText::new(text).size(12.0).color(TEXT_SECONDARY));
    });
}

fn recent_log_card(ui: &mut egui::Ui, app: &BackupApp) {
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Recent Activity")
                    .size(15.0)
                    .strong()
                    .color(TEXT_PRIMARY),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{} entries", app.log_entries.len()))
                        .size(11.0)
                        .color(TEXT_TERTIARY),
                );
            });
        });
        ui.add_space(8.0);
        ScrollArea::vertical()
            .id_salt("dashboard_log_scroll")
            .max_height(190.0)
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for entry in app.log_entries.iter().rev().take(12) {
                    ui.label(
                        RichText::new(entry.compact_line())
                            .size(11.0)
                            .color(TEXT_SECONDARY),
                    );
                }
                if app.log_entries.is_empty() {
                    ui.label(
                        RichText::new("No activity yet.")
                            .size(12.0)
                            .color(TEXT_TERTIARY),
                    );
                }
            });
    });
}

fn card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    Frame::new()
        .fill(BG_CARD)
        .stroke(Stroke::new(1.0, BORDER_CARD))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            add_contents(ui);
        });
    ui.add_space(12.0);
}
