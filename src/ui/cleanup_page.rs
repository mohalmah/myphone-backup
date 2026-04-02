use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea, Stroke,
};
use crate::app::BackupApp;
use crate::core::models::RemoteFolderEntryKind;
use crate::ui::theme::*;
use crate::ui::widgets::*;

pub(crate) fn render_cleanup_page(ctx: &egui::Context, app: &mut BackupApp) {
    let adb_job_active = app.has_active_adb_job();

    // Right panel must be registered BEFORE CentralPanel in egui
    egui::SidePanel::right("cleanup_right_panel")
        .resizable(false)
        .exact_width(220.0)
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(BG_LAYER)
                .stroke(Stroke::new(1.0, BORDER_CARD))
                .inner_margin(Margin::same(12)),
        )
        .show(ctx, |ui| {
            right_panel(ui, app, adb_job_active);
        });

    // Central panel: breadcrumb + file browser
    egui::CentralPanel::default()
        .frame(Frame::new().fill(BG_BASE).inner_margin(Margin::same(12)))
        .show(ctx, |ui| {
            ui.label(
                RichText::new("Cleanup")
                    .size(20.0)
                    .strong()
                    .color(TEXT_PRIMARY),
            );
            ui.add_space(8.0);
            breadcrumb_bar(ui, app, adb_job_active);
            ui.add_space(8.0);
            file_browser(ui, app, adb_job_active);
        });
}

fn breadcrumb_bar(ui: &mut egui::Ui, app: &mut BackupApp, adb_job_active: bool) {
    Frame::new()
        .fill(BG_CARD)
        .stroke(Stroke::new(1.0, BORDER_CARD))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Back/forward (back = go up)
                let can_go_up = parent_remote_path(&app.folder_cleanup.folder_path).is_some()
                    && app.folder_cleanup.preview.is_some();
                if ui.add_enabled(can_go_up && !adb_job_active, egui::Button::new("‹")).clicked() {
                    if let Some(parent) = parent_remote_path(&app.folder_cleanup.folder_path) {
                        app.set_cleanup_folder_path(parent);
                        app.request_cleanup_preview();
                    }
                }
                if ui.add_enabled(false, egui::Button::new("›")).clicked() {}

                // Current path
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    let mut path = app.folder_cleanup.folder_path.clone();
                    if ui.add(
                        egui::TextEdit::singleline(&mut path)
                            .desired_width(ui.available_width() - 72.0)
                            .hint_text("Phone folder..."),
                    ).changed() {
                        app.set_cleanup_folder_path(path);
                    }
                });

                // Icons: pick folder + refresh
                if ui.add_enabled(!adb_job_active, egui::Button::new("\u{1F4C1}"))
                    .on_hover_text("Pick phone folder").clicked()
                {
                    app.open_cleanup_folder_picker();
                }
                if ui.add_enabled(!adb_job_active, egui::Button::new("↻"))
                    .on_hover_text("Refresh").clicked()
                {
                    app.request_cleanup_preview();
                }
            });
        });
}

fn file_browser(ui: &mut egui::Ui, app: &mut BackupApp, adb_job_active: bool) {
    if app.folder_cleanup.is_fetching_preview {
        ui.horizontal(|ui| { ui.spinner(); ui.label("Fetching..."); });
        return;
    }

    if let Some(error) = &app.folder_cleanup.preview_error.clone() {
        ui.colored_label(ERROR, error);
    }
    if let Some(error) = &app.folder_cleanup.delete_error.clone() {
        ui.colored_label(ERROR, error);
    }

    let Some(preview) = app.folder_cleanup.preview.clone() else {
        ui.label(
            RichText::new("Click ↻ Refresh to inspect the selected folder before deleting anything.")
                .size(12.0)
                .color(TEXT_TERTIARY),
        );
        return;
    };

    // Sort bar + bulk select
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Sort by").size(11.0).color(TEXT_SECONDARY));
        ui.label(RichText::new("Largest first").size(11.0).color(TEXT_PRIMARY));
        ui.add_space(16.0);
        if ui.add_enabled(!adb_job_active, egui::Button::new("Select All")).clicked() {
            app.folder_cleanup.selected_paths =
                preview.entries.iter().map(|e| e.full_path.clone()).collect();
        }
        if ui.add_enabled(!adb_job_active, egui::Button::new("Files Only")).clicked() {
            app.folder_cleanup.selected_paths = preview
                .entries
                .iter()
                .filter(|e| e.kind == RemoteFolderEntryKind::File)
                .map(|e| e.full_path.clone())
                .collect();
        }
        if ui.add_enabled(!adb_job_active, egui::Button::new("Clear Selection")).clicked() {
            app.folder_cleanup.selected_paths.clear();
        }
        ui.label(
            RichText::new(format!("{} checked", app.folder_cleanup.selected_paths.len()))
                .size(11.0)
                .color(TEXT_TERTIARY),
        );
    });

    ui.add_space(6.0);

    // Column header row
    Frame::new()
        .fill(BG_LAYER)
        .inner_margin(Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(24.0); // checkbox width
                ui.label(RichText::new("Name ▲").size(11.0).strong().color(TEXT_SECONDARY));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(50.0); // Select link width
                    ui.label(RichText::new("Size").size(11.0).strong().color(TEXT_SECONDARY));
                });
            });
        });

    ScrollArea::vertical()
        .id_salt("cleanup_file_browser_scroll")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            for entry in &preview.entries {
                let is_selected = app.folder_cleanup.selected_paths.contains(&entry.full_path);
                Frame::new()
                    .fill(if is_selected { ACCENT.gamma_multiply(0.06) } else { BG_CARD })
                    .stroke(Stroke::new(1.0, BORDER_CARD))
                    .inner_margin(Margin::symmetric(8, 5))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let mut selected = is_selected;
                            if ui.add_enabled(
                                !app.folder_cleanup.is_deleting,
                                egui::Checkbox::without_text(&mut selected),
                            ).changed() {
                                if selected {
                                    app.folder_cleanup.selected_paths.insert(entry.full_path.clone());
                                } else {
                                    app.folder_cleanup.selected_paths.remove(&entry.full_path);
                                }
                            }

                            let icon = match entry.kind {
                                RemoteFolderEntryKind::Directory => "📁",
                                RemoteFolderEntryKind::File => "📄",
                            };
                            let name = entry.full_path.rsplit('/').next().unwrap_or(&entry.full_path);
                            ui.label(RichText::new(format!("{icon} {}", display_text_for_ui(name))).size(12.0));

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.add_enabled(!adb_job_active,
                                    egui::Button::new(RichText::new("Select").size(11.0).color(ACCENT))
                                        .fill(Color32::TRANSPARENT)
                                ).clicked() {
                                    app.folder_cleanup.selected_paths.insert(entry.full_path.clone());
                                }
                                let size_text = match entry.kind {
                                    RemoteFolderEntryKind::Directory => "—".to_string(),
                                    RemoteFolderEntryKind::File => format_bytes(entry.size_bytes.unwrap_or(0)),
                                };
                                ui.label(RichText::new(size_text).size(11.0).color(TEXT_SECONDARY));
                            });
                        });
                    });
                ui.add_space(2.0);
            }
        });
}

fn right_panel(ui: &mut egui::Ui, app: &mut BackupApp, adb_job_active: bool) {
    // Progress / Results card
    if app.is_running() {
        Frame::new()
            .fill(BG_CARD)
            .stroke(Stroke::new(1.0, BORDER_CARD))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.label(RichText::new("Results").size(12.0).strong());
                ui.add_space(4.0);
                if let Some(file) = &app.progress.current_file {
                    ui.label(RichText::new("Current file:").size(10.0).color(TEXT_SECONDARY));
                    ui.add(egui::Label::new(RichText::new(display_text_for_ui(file)).size(10.0)).wrap());
                }
                ui.add_space(4.0);
                ui.label(RichText::new(format!("Files: {} / {}", app.progress.completed_files, app.progress.total_files)).size(11.0));
                ui.label(RichText::new(format!("Speed: {}/s", format_bytes(app.progress.speed_bytes_per_sec.round() as u64))).size(11.0));
                if let Some(eta) = app.progress.eta_seconds {
                    ui.label(RichText::new(format!("ETA: {}", format_duration(eta))).size(11.0));
                }
                ui.add_space(6.0);
                let running = app.is_running();
                let paused = app.sync_handle.as_ref().map(|h| h.is_paused()).unwrap_or(false);
                ui.horizontal(|ui| {
                    if ui.add_enabled(running, egui::Button::new(if paused { "▶" } else { "⏸" })).clicked() {
                        if let Some(h) = &app.sync_handle { h.toggle_pause(); }
                    }
                    if ui.add_enabled(running, egui::Button::new("⏹")).clicked() {
                        if let Some(h) = &app.sync_handle { h.request_stop(); }
                    }
                });
            });
        ui.add_space(10.0);
    }

    // Cleanup Options card
    let preview_matches = app.cleanup_preview_matches_path();
    let selected_entries = app.selected_cleanup_entries();
    let selected_count = selected_entries.len();
    let selected_bytes: u64 = selected_entries.iter().map(|e| e.size_bytes.unwrap_or(0)).sum();

    Frame::new()
        .fill(BG_CARD)
        .stroke(Stroke::new(1.0, BORDER_CARD))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.label(RichText::new("Cleanup Options").size(12.0).strong());
            ui.add_space(6.0);

            ui.label(
                RichText::new(format!("{selected_count} items | {}", format_bytes(selected_bytes)))
                    .size(11.0)
                    .color(TEXT_SECONDARY),
            );
            ui.add_space(4.0);

            ui.checkbox(
                &mut app.folder_cleanup.delete_armed,
                "I understand these actions permanently delete items",
            );
            ui.add_space(6.0);

            let root_ok = preview_matches
                && app.folder_cleanup.delete_armed
                && !adb_job_active
                && protected_cleanup_folder_reason(&app.folder_cleanup.folder_path).is_none();
            let sel_ok = preview_matches
                && app.folder_cleanup.delete_armed
                && !adb_job_active
                && selected_count > 0
                && selected_entries.iter().all(|e| protected_cleanup_folder_reason(&e.full_path).is_none());

            if ui.add_enabled(root_ok, egui::Button::new("Delete Entire Folder")).clicked() {
                app.request_cleanup_delete_folder();
            }
            ui.add_space(2.0);
            if ui.add_enabled(root_ok, egui::Button::new("Delete Contents Only")).clicked() {
                app.request_cleanup_delete_contents_only();
            }
            ui.add_space(6.0);

            let del_btn = egui::Button::new(
                RichText::new("DELETE SELECTED").size(12.0).color(Color32::WHITE).strong(),
            )
            .fill(ERROR)
            .corner_radius(CornerRadius::same(5))
            .min_size(egui::vec2(ui.available_width(), 32.0));

            if ui.add_enabled(sel_ok, del_btn).clicked() {
                app.request_cleanup_delete_selected();
            }

            if app.folder_cleanup.is_deleting {
                ui.add_space(4.0);
                ui.horizontal(|ui| { ui.spinner(); ui.label("Deleting..."); });
            }
        });

    if let Some(reason) = protected_cleanup_folder_reason(&app.folder_cleanup.folder_path) {
        ui.add_space(6.0);
        ui.colored_label(ERROR, reason);
    }
}
