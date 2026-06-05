use crate::app::BackupApp;
use crate::core::models::RemoteFolderEntryKind;
use crate::ui::theme::*;
use crate::ui::widgets::*;
use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea, Stroke,
};

pub(crate) fn render_cleanup_page(ctx: &egui::Context, app: &mut BackupApp) {
    let adb_job_active = app.has_active_adb_job();

    egui::CentralPanel::default()
        .frame(Frame::new().fill(BG_BASE).inner_margin(Margin::same(18)))
        .show(ctx, |ui| {
            ScrollArea::vertical()
                .id_salt("cleanup_page_scroll")
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    header(ui, app);
                    ui.add_space(16.0);

                    if ui.available_width() >= 960.0 {
                        ui.horizontal_top(|ui| {
                            ui.vertical(|ui| {
                                ui.set_width(350.0);
                                folder_card(ui, app, adb_job_active);
                                delete_plan_card(ui, app, adb_job_active);
                            });
                            ui.add_space(14.0);
                            ui.vertical(|ui| {
                                preview_card(ui, app, adb_job_active);
                            });
                        });
                    } else {
                        folder_card(ui, app, adb_job_active);
                        delete_plan_card(ui, app, adb_job_active);
                        preview_card(ui, app, adb_job_active);
                    }
                });
        });
}

fn header(ui: &mut egui::Ui, app: &BackupApp) {
    ui.horizontal_wrapped(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new("Cleanup Review")
                    .size(25.0)
                    .strong()
                    .color(TEXT_PRIMARY),
            );
            ui.label(
                RichText::new(
                    "Preview a phone folder first. Delete only after you arm the action.",
                )
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
            if app.folder_cleanup.is_deleting {
                status_pill(ui, "DELETING", ERROR);
            } else if app.folder_cleanup.preview.is_some() {
                status_pill(ui, "PREVIEW READY", SUCCESS);
            } else {
                status_pill(ui, "NEEDS PREVIEW", WARNING);
            }
        });
    });
}

fn folder_card(ui: &mut egui::Ui, app: &mut BackupApp, adb_job_active: bool) {
    card(ui, "Folder", |ui| {
        ui.label(
            RichText::new("Phone folder")
                .size(11.0)
                .color(TEXT_SECONDARY),
        );
        ui.add(
            egui::TextEdit::singleline(&mut app.folder_cleanup.folder_path)
                .desired_width(f32::INFINITY)
                .hint_text("/sdcard/..."),
        );
        ui.add_space(8.0);

        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!adb_job_active, egui::Button::new("Browse phone"))
                .clicked()
            {
                app.open_cleanup_folder_picker();
            }
            if ui
                .add_enabled(!adb_job_active, egui::Button::new("Preview contents"))
                .clicked()
            {
                app.request_cleanup_preview();
            }
            if ui
                .add_enabled(
                    !adb_job_active && app.folder_cleanup.preview.is_some(),
                    egui::Button::new("Clear preview"),
                )
                .clicked()
            {
                app.clear_cleanup_preview();
            }
            let can_go_up = parent_remote_path(&app.folder_cleanup.folder_path).is_some()
                && app.folder_cleanup.preview.is_some();
            if ui
                .add_enabled(can_go_up && !adb_job_active, egui::Button::new("Up"))
                .clicked()
            {
                if let Some(parent) = parent_remote_path(&app.folder_cleanup.folder_path) {
                    app.set_cleanup_folder_path(parent);
                    app.request_cleanup_preview();
                }
            }
        });

        ui.add_space(10.0);
        if app.folder_cleanup.is_fetching_preview {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Fetching folder contents...").size(12.0));
            });
        } else if let Some(preview) = &app.folder_cleanup.preview {
            ui.label(
                RichText::new(cleanup_summary(preview))
                    .size(11.0)
                    .color(TEXT_SECONDARY),
            );
        } else {
            ui.label(
                RichText::new("No cleanup preview has been loaded yet.")
                    .size(12.0)
                    .color(TEXT_TERTIARY),
            );
        }

        if let Some(error) = &app.folder_cleanup.preview_error {
            ui.add_space(6.0);
            ui.colored_label(ERROR, error);
        }
        if let Some(error) = &app.folder_cleanup.delete_error {
            ui.add_space(6.0);
            ui.colored_label(ERROR, error);
        }
    });
}

fn delete_plan_card(ui: &mut egui::Ui, app: &mut BackupApp, adb_job_active: bool) {
    let preview_matches = app.cleanup_preview_matches_path();
    let selected_entries = app.selected_cleanup_entries();
    let selected_count = selected_entries.len();
    let selected_bytes: u64 = selected_entries
        .iter()
        .map(|entry| entry.size_bytes.unwrap_or(0))
        .sum();
    let root_blocked = protected_cleanup_folder_reason(&app.folder_cleanup.folder_path);
    let selected_blocked = selected_entries
        .iter()
        .find_map(|entry| protected_cleanup_folder_reason(&entry.full_path));

    card(ui, "Delete Plan", |ui| {
        ui.label(
            RichText::new(format!(
                "{} selected | {}",
                selected_count,
                format_bytes(selected_bytes)
            ))
            .size(12.0)
            .color(TEXT_SECONDARY),
        );
        ui.add_space(8.0);

        ui.checkbox(
            &mut app.folder_cleanup.delete_armed,
            "I understand this permanently deletes phone files",
        );
        ui.add_space(10.0);

        let root_ok = preview_matches
            && app.folder_cleanup.delete_armed
            && !adb_job_active
            && root_blocked.is_none();
        let selected_ok = preview_matches
            && app.folder_cleanup.delete_armed
            && !adb_job_active
            && selected_count > 0
            && selected_blocked.is_none();

        if delete_button(ui, "Delete selected", selected_ok).clicked() {
            app.request_cleanup_delete_selected();
        }
        ui.add_space(6.0);
        if ui
            .add_enabled(root_ok, egui::Button::new("Delete contents only"))
            .clicked()
        {
            app.request_cleanup_delete_contents_only();
        }
        ui.add_space(4.0);
        if ui
            .add_enabled(root_ok, egui::Button::new("Delete folder and contents"))
            .clicked()
        {
            app.request_cleanup_delete_folder();
        }

        if app.folder_cleanup.is_deleting {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Deleting selected phone items...");
            });
        }
        if !preview_matches {
            ui.add_space(8.0);
            ui.label(
                RichText::new("Preview this exact folder before delete buttons unlock.")
                    .size(11.0)
                    .color(WARNING),
            );
        }
        if let Some(reason) = root_blocked.or(selected_blocked) {
            ui.add_space(8.0);
            ui.colored_label(ERROR, reason);
        }
    });
}

fn preview_card(ui: &mut egui::Ui, app: &mut BackupApp, adb_job_active: bool) {
    card(ui, "Contents", |ui| {
        let Some(preview) = app.folder_cleanup.preview.clone() else {
            ui.label(
                RichText::new("Preview a folder to see files and subfolders here.")
                    .size(13.0)
                    .color(TEXT_SECONDARY),
            );
            return;
        };

        ui.horizontal_wrapped(|ui| {
            metric_chip(ui, "Files", preview.file_count.to_string(), TEXT_PRIMARY);
            metric_chip(
                ui,
                "Folders",
                preview.directory_count.to_string(),
                TEXT_PRIMARY,
            );
            metric_chip(ui, "Size", format_bytes(preview.total_file_bytes), ACCENT);
        });
        ui.add_space(10.0);

        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!adb_job_active, egui::Button::new("Select all"))
                .clicked()
            {
                app.folder_cleanup.selected_paths = preview
                    .entries
                    .iter()
                    .map(|entry| entry.full_path.clone())
                    .collect();
            }
            if ui
                .add_enabled(!adb_job_active, egui::Button::new("Files only"))
                .clicked()
            {
                app.folder_cleanup.selected_paths = preview
                    .entries
                    .iter()
                    .filter(|entry| entry.kind == RemoteFolderEntryKind::File)
                    .map(|entry| entry.full_path.clone())
                    .collect();
            }
            if ui
                .add_enabled(!adb_job_active, egui::Button::new("Clear"))
                .clicked()
            {
                app.folder_cleanup.selected_paths.clear();
            }
            ui.label(
                RichText::new("Largest files are listed first.")
                    .size(11.0)
                    .color(TEXT_TERTIARY),
            );
        });
        ui.add_space(10.0);

        ScrollArea::vertical()
            .id_salt("cleanup_preview_entries_scroll")
            .max_height(520.0)
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for entry in &preview.entries {
                    cleanup_entry(ui, app, entry, adb_job_active);
                    ui.add_space(6.0);
                }
            });
    });
}

fn cleanup_entry(
    ui: &mut egui::Ui,
    app: &mut BackupApp,
    entry: &crate::core::models::RemoteFolderEntry,
    adb_job_active: bool,
) {
    let selected = app.folder_cleanup.selected_paths.contains(&entry.full_path);
    let name = entry
        .full_path
        .rsplit('/')
        .next()
        .unwrap_or(&entry.full_path)
        .to_string();
    let kind = entry.kind.label();
    let size = entry
        .size_bytes
        .map(format_bytes)
        .unwrap_or_else(|| "Folder".to_string());

    Frame::new()
        .fill(if selected {
            ACCENT.gamma_multiply(0.06)
        } else {
            BG_CARD
        })
        .stroke(Stroke::new(
            if selected { 1.2 } else { 1.0 },
            if selected { ACCENT } else { BORDER_CARD },
        ))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let mut checked = selected;
                if ui
                    .add_enabled(
                        !app.folder_cleanup.is_deleting,
                        egui::Checkbox::without_text(&mut checked),
                    )
                    .changed()
                {
                    if checked {
                        app.folder_cleanup
                            .selected_paths
                            .insert(entry.full_path.clone());
                    } else {
                        app.folder_cleanup.selected_paths.remove(&entry.full_path);
                    }
                }

                ui.vertical(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(kind)
                                .size(11.0)
                                .strong()
                                .color(TEXT_SECONDARY),
                        );
                        ui.label(RichText::new(size).size(11.0).strong().color(ACCENT));
                    });
                    ui.label(
                        RichText::new(display_text_for_ui(&name))
                            .size(12.0)
                            .color(TEXT_PRIMARY),
                    );
                    wrapped_path_text(ui, &entry.full_path);
                });

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !adb_job_active,
                            egui::Button::new(
                                RichText::new(if selected { "Selected" } else { "Select" })
                                    .size(11.0)
                                    .color(if selected { SUCCESS } else { ACCENT }),
                            )
                            .fill(Color32::TRANSPARENT),
                        )
                        .clicked()
                    {
                        app.folder_cleanup
                            .selected_paths
                            .insert(entry.full_path.clone());
                    }
                });
            });
        });
}

fn delete_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(
            RichText::new(label)
                .size(12.0)
                .strong()
                .color(Color32::WHITE),
        )
        .fill(ERROR)
        .corner_radius(CornerRadius::same(6))
        .min_size(egui::vec2(ui.available_width(), 34.0)),
    )
}

fn card(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    Frame::new()
        .fill(BG_CARD)
        .stroke(Stroke::new(1.0, BORDER_CARD))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.label(RichText::new(title).size(14.0).strong().color(TEXT_PRIMARY));
            ui.add_space(10.0);
            add_contents(ui);
        });
    ui.add_space(12.0);
}
