use crate::app::{AppTab, BackupApp};
use crate::core::{
    config,
    models::{ExistingFileBehavior, ValidationMode},
};
use crate::ui::theme::*;
use crate::ui::widgets::*;
use eframe::egui::{
    self, Align, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea, Stroke,
};

pub(crate) fn render_settings_page(ctx: &egui::Context, app: &mut BackupApp) {
    egui::CentralPanel::default()
        .frame(Frame::new().fill(BG_BASE).inner_margin(Margin::same(22)))
        .show(ctx, |ui| {
            ScrollArea::vertical()
                .id_salt("settings_page_scroll")
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    header(ui, app);
                    ui.add_space(18.0);

                    if ui.available_width() >= 900.0 {
                        ui.horizontal_top(|ui| {
                            ui.vertical(|ui| {
                                ui.set_width((ui.available_width() * 0.52).max(430.0));
                                adb_card(ui, app);
                                safety_card(ui, app);
                            });
                            ui.add_space(14.0);
                            ui.vertical(|ui| {
                                paths_card(ui, app);
                                logs_card(ui, app);
                            });
                        });
                    } else {
                        adb_card(ui, app);
                        safety_card(ui, app);
                        paths_card(ui, app);
                        logs_card(ui, app);
                    }
                });
        });
}

fn header(ui: &mut egui::Ui, app: &mut BackupApp) {
    ui.horizontal_wrapped(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new("Settings")
                    .size(25.0)
                    .strong()
                    .color(TEXT_PRIMARY),
            );
            ui.label(
                RichText::new("Runtime controls for ADB, validation, safety, and logs.")
                    .size(12.0)
                    .color(TEXT_SECONDARY),
            );
        });

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add_enabled(
                    !app.has_active_adb_job(),
                    egui::Button::new("Save settings"),
                )
                .clicked()
            {
                app.save_settings();
            }
            if ui.button("Back to backup").clicked() {
                app.active_tab = AppTab::Backup;
            }
        });
    });
}

fn adb_card(ui: &mut egui::Ui, app: &mut BackupApp) {
    card(ui, "ADB Connection", |ui| {
        ui.label(
            RichText::new("ADB executable")
                .size(11.0)
                .color(TEXT_SECONDARY),
        );
        ui.horizontal(|ui| {
            let path_width = (ui.available_width() - 92.0).max(180.0);
            ui.add(
                egui::TextEdit::singleline(&mut app.settings.adb_path)
                    .desired_width(path_width)
                    .hint_text("adb or C:\\platform-tools\\adb.exe"),
            );
            if ui
                .add_enabled(!app.has_active_adb_job(), egui::Button::new("Browse"))
                .clicked()
            {
                app.pick_adb_executable();
            }
        });

        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            status_pill(
                ui,
                app.device_info.state.label(),
                app.device_info.state.color(),
            );
            if ui
                .add_enabled(
                    !app.has_active_adb_job(),
                    egui::Button::new("Refresh device"),
                )
                .clicked()
            {
                app.refresh_device_info();
            }
        });

        ui.add_space(8.0);
        let model = app
            .device_info
            .model
            .as_deref()
            .unwrap_or("No authorized device detected yet");
        ui.label(RichText::new(model).size(12.0).strong().color(TEXT_PRIMARY));
        if !app.device_info.serial.trim().is_empty() {
            ui.label(
                RichText::new(format!("Serial {}", app.device_info.serial))
                    .size(11.0)
                    .color(TEXT_SECONDARY),
            );
        }
    });
}

fn safety_card(ui: &mut egui::Ui, app: &mut BackupApp) {
    card(ui, "Backup Safety Defaults", |ui| {
        ui.checkbox(&mut app.settings.dry_run, "Simulation mode by default");
        ui.checkbox(
            &mut app.settings.auto_delete_after_success,
            "Delete from phone after validated backup",
        );
        ui.add_space(8.0);

        egui::ComboBox::from_label("Validation")
            .selected_text(app.settings.validation_mode.label())
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut app.settings.validation_mode,
                    ValidationMode::Size,
                    ValidationMode::Size.label(),
                );
                ui.selectable_value(
                    &mut app.settings.validation_mode,
                    ValidationMode::Md5,
                    ValidationMode::Md5.label(),
                );
            });

        egui::ComboBox::from_label("Existing local files")
            .selected_text(app.settings.existing_file_behavior.label())
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut app.settings.existing_file_behavior,
                    ExistingFileBehavior::Skip,
                    ExistingFileBehavior::Skip.label(),
                );
                ui.selectable_value(
                    &mut app.settings.existing_file_behavior,
                    ExistingFileBehavior::Validate,
                    ExistingFileBehavior::Validate.label(),
                );
            });

        ui.add_space(8.0);
        let mut only_recent = app.settings.only_last_days.is_some();
        if ui
            .checkbox(&mut only_recent, "Only copy recent files")
            .changed()
        {
            app.settings.only_last_days = if only_recent { Some(7) } else { None };
            app.invalidate_backup_analysis();
        }
        if let Some(days) = &mut app.settings.only_last_days {
            let mut days_changed = false;
            ui.horizontal(|ui| {
                ui.label("Recent days");
                if ui.add(egui::DragValue::new(days).range(1..=365)).changed() {
                    days_changed = true;
                }
            });
            if days_changed {
                app.invalidate_backup_analysis();
            }
        }

        ui.add_space(10.0);
        ui.label(
            RichText::new(
                "Recommendation: keep simulation on until you trust the selected folders.",
            )
            .size(11.0)
            .color(TEXT_SECONDARY),
        );
    });
}

fn paths_card(ui: &mut egui::Ui, app: &mut BackupApp) {
    card(ui, "Folders", |ui| {
        ui.label(
            RichText::new("Default backup destination")
                .size(11.0)
                .color(TEXT_SECONDARY),
        );
        wrapped_path_text(ui, &app.settings.destination_path);
        ui.add_space(8.0);
        if ui
            .add_enabled(
                !app.has_active_adb_job(),
                egui::Button::new("Choose destination"),
            )
            .clicked()
        {
            app.pick_local_destination_folder();
        }

        ui.add_space(14.0);
        ui.separator();
        ui.add_space(10.0);

        ui.label(
            RichText::new("Settings file")
                .size(11.0)
                .color(TEXT_SECONDARY),
        );
        wrapped_path_text(ui, &config::settings_path().display().to_string());
        ui.add_space(8.0);
        ui.label(
            RichText::new("Logs folder")
                .size(11.0)
                .color(TEXT_SECONDARY),
        );
        wrapped_path_text(ui, &config::logs_dir().display().to_string());
    });
}

fn logs_card(ui: &mut egui::Ui, app: &mut BackupApp) {
    card(ui, "Activity Log", |ui| {
        ui.checkbox(&mut app.nerd_mode, "Show activity log panel");
        ui.checkbox(&mut app.show_detailed_logs, "Show ADB command details");
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            metric_chip(
                ui,
                "Entries",
                app.log_entries.len().to_string(),
                TEXT_PRIMARY,
            );
            metric_chip(
                ui,
                "Detailed",
                app.log_entries
                    .iter()
                    .filter(|entry| entry.detailed_only)
                    .count()
                    .to_string(),
                ACCENT,
            );
        });
        ui.add_space(10.0);
        if ui.button("Clear activity log").clicked() {
            app.log_entries.clear();
        }
    });
}

fn card(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    Frame::new()
        .fill(BG_CARD)
        .stroke(Stroke::new(1.0, BORDER_CARD))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.label(RichText::new(title).size(15.0).strong().color(TEXT_PRIMARY));
            ui.add_space(10.0);
            add_contents(ui);
        });
    ui.add_space(12.0);
}
