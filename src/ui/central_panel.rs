use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea,
    Stroke,
};

use crate::app::{AppTab, BackupApp, RemoteFolderPickerTarget};
use crate::core::models::{RemoteFolderEntryKind, RemoteFile};
use crate::ui::widgets::*;

pub(crate) fn render_header(ctx: &egui::Context, app: &mut BackupApp) {
    egui::TopBottomPanel::top("hero").show(ctx, |ui| {
        Frame::new()
            .fill(Color32::from_rgb(241, 234, 218))
            .inner_margin(Margin::same(14))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("ADB Smart Backup & Cleanup")
                                .heading()
                                .strong()
                                .color(Color32::from_rgb(50, 43, 34)),
                        );
                        ui.label(
                            RichText::new(
                                "Safe per-file backup, validation, and optional cleanup for Android media folders.",
                            )
                            .color(Color32::from_rgb(93, 81, 66)),
                        );
                    });

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        status_pill(
                            ui,
                            if app.is_running() { "RUNNING" } else { "IDLE" },
                            if app.is_running() {
                                Color32::from_rgb(198, 106, 44)
                            } else {
                                Color32::from_rgb(73, 121, 92)
                            },
                        );
                        status_pill(
                            ui,
                            app.device_info.state.label(),
                            app.device_info.state.color(),
                        );
                    });
                });
                ui.add_space(8.0);
                ui.label(RichText::new(&app.status_banner).color(Color32::from_rgb(72, 62, 50)));
                if let Some(error) = &app.error_banner {
                    ui.add_space(6.0);
                    ui.colored_label(Color32::from_rgb(168, 52, 33), error);
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(app.active_tab == AppTab::Backup, "Backup")
                        .clicked()
                    {
                        app.active_tab = AppTab::Backup;
                    }
                    if ui
                        .selectable_label(app.active_tab == AppTab::Cleanup, "Cleanup")
                        .clicked()
                    {
                        app.active_tab = AppTab::Cleanup;
                    }
                });
            });
    });
}

pub(crate) fn render_log_panel(ctx: &egui::Context, app: &mut BackupApp) {
    egui::TopBottomPanel::bottom("log_panel")
        .resizable(true)
        .default_height(230.0)
        .show(ctx, |ui| {
            let visible_log_count = app
                .log_entries
                .iter()
                .filter(|entry| app.show_detailed_logs || !entry.detailed_only)
                .count();

            ui.horizontal(|ui| {
                ui.label(RichText::new("Activity Log").strong());
                ui.checkbox(&mut app.show_detailed_logs, "Show very detailed logs");
                if ui.button("Clear").clicked() {
                    app.log_entries.clear();
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!(
                            "{} shown / {} total",
                            visible_log_count,
                            app.log_entries.len()
                        ))
                        .color(Color32::from_rgb(118, 104, 85)),
                    );
                });
            });
            ui.add_space(6.0);
            ScrollArea::vertical()
                .id_salt("activity_log_scroll")
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    for entry in app.log_entries.iter().rev() {
                        if !app.show_detailed_logs && entry.detailed_only {
                            continue;
                        }

                        if app.show_detailed_logs {
                            render_detailed_log_entry(ui, entry);
                            ui.add_space(8.0);
                        } else {
                            ui.monospace(entry.compact_line());
                        }
                    }
                });
        });
}

pub(crate) fn render_central_panel(ctx: &egui::Context, app: &mut BackupApp) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ScrollArea::vertical()
            .id_salt("central_panel_scroll")
            .auto_shrink([false; 2])
            .show(ui, |ui| {
        let total_progress = if app.progress.total_files == 0 {
            0.0
        } else {
            app.progress.completed_files as f32 / app.progress.total_files as f32
        };
        let mut backup_source_to_remove = None;
        let mut backup_source_to_pick = None;
        let backup_source_destination_to_pick = None;
        let mut backup_sources_changed = false;

        if app.active_tab == AppTab::Backup {
            summary_strip(ui, &app.progress, app.last_summary.as_ref());
            ui.add_space(8.0);

            // Progress bars (at the top, always visible)
            Frame::new()
                .fill(Color32::from_rgb(250, 247, 240))
                .stroke(Stroke::new(1.0, Color32::from_rgb(221, 211, 190)))
                .corner_radius(CornerRadius::same(14))
                .inner_margin(Margin::same(14))
                .show(ui, |ui| {
                    ui.label(RichText::new("Progress").strong());
                    ui.add_space(6.0);
                    let progress_response = ui.add(
                        egui::ProgressBar::new(total_progress)
                            .text(format!(
                                "{} / {} files",
                                app.progress.completed_files, app.progress.total_files
                            ))
                            .fill(Color32::from_rgb(73, 121, 92)),
                    );
                    progress_response.on_hover_text(progress_detail(&app.progress));
                    ui.add_space(4.0);
                    ui.add(
                        egui::ProgressBar::new(app.progress.current_file_progress)
                            .text(match &app.progress.current_file {
                                Some(current_file) => format!(
                                    "Current file: {}",
                                    display_text_for_ui(current_file)
                                ),
                                None => "Waiting to start".to_string(),
                            })
                            .fill(Color32::from_rgb(198, 106, 44)),
                    );
                });
            ui.add_space(8.0);

            Frame::new()
                .fill(Color32::WHITE)
                .stroke(Stroke::new(1.0, Color32::from_rgb(221, 211, 190)))
                .corner_radius(CornerRadius::same(14))
                .inner_margin(Margin::same(14))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Backup Sources").strong());
                        if ui.add_enabled(!app.has_active_adb_job(), egui::Button::new("+"))
                            .on_hover_text("Add custom source")
                            .clicked()
                        {
                            app.add_custom_backup_source();
                            backup_sources_changed = true;
                        }
                        if ui.add_enabled(!app.has_active_adb_job(), egui::Button::new("Scan"))
                            .on_hover_text("Scan configured sources on device")
                            .clicked()
                        {
                            app.refresh_backup_source_scan();
                        }
                        if ui.add_enabled(!app.has_active_adb_job(), egui::Button::new("Analyze"))
                            .on_hover_text("Analyze selected sources and calculate space")
                            .clicked()
                        {
                            app.request_backup_analysis();
                        }
                        if app.backup_source_library.is_scanning {
                            ui.spinner();
                        }
                        if app.backup_analysis.is_loading {
                            ui.spinner();
                            ui.label("Analyzing...");
                        }
                    });
                    ui.add_space(6.0);

                    // ── Destination folder row ──
                    ui.horizontal(|ui| {
                        ui.label("Dest:");
                        if ui.add(
                            egui::TextEdit::singleline(&mut app.settings.destination_path)
                                .desired_width(ui.available_width() - 40.0)
                                .hint_text("Destination folder..."),
                        ).changed() {
                            app.invalidate_backup_analysis();
                        }
                        if ui.add_enabled(!app.has_active_adb_job(), egui::Button::new("..."))
                            .on_hover_text("Browse for destination folder")
                            .clicked()
                        {
                            app.pick_local_destination_folder();
                        }
                    });
                    ui.add_space(4.0);

                    if let Some(error) = &app.backup_source_library.scan_error {
                        ui.colored_label(Color32::from_rgb(168, 52, 33), error);
                        ui.add_space(8.0);
                    }

                    ScrollArea::vertical()
                        .id_salt("backup_source_library_scroll")
                        .max_height(320.0)
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            let source_actions_enabled = !app.has_active_adb_job();
                            for (index, source) in app.settings.backup_sources.iter_mut().enumerate() {
                                let scan = app
                                    .backup_source_library
                                    .scan_results
                                    .iter()
                                    .find(|scan| scan.id == source.id);

                                Frame::new()
                                    .fill(Color32::from_rgb(250, 247, 240))
                                    .stroke(Stroke::new(1.0, Color32::from_rgb(228, 219, 203)))
                                    .corner_radius(CornerRadius::same(10))
                                    .inner_margin(Margin::same(8))
                                    .show(ui, |ui| {
                                        ui.horizontal_wrapped(|ui| {
                                            if ui.checkbox(&mut source.enabled, "").changed() {
                                                backup_sources_changed = true;
                                            }
                                            if ui.add(
                                                egui::TextEdit::singleline(&mut source.label).desired_width(140.0),
                                            ).changed() {
                                                backup_sources_changed = true;
                                            }
                                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                                if ui.add_enabled(source_actions_enabled, egui::Button::new("\u{2715}"))
                                                    .on_hover_text("Remove source").clicked()
                                                {
                                                    backup_source_to_remove = Some(index);
                                                }
                                                if ui.add_enabled(source_actions_enabled, egui::Button::new("\u{1F4C1}"))
                                                    .on_hover_text("Pick phone folder").clicked()
                                                {
                                                    backup_source_to_pick = Some(index);
                                                }
                                            });
                                        });

                                        // Source path (truncated, full on hover)
                                        ui.add(egui::Label::new(
                                            RichText::new(&source.source_path).small().color(Color32::from_rgb(118, 104, 85))
                                        ).wrap().truncate())
                                        .on_hover_text(format!("Source: {}", source.source_path));

                                        // Editable destination subfolder inline
                                        ui.horizontal(|ui| {
                                            ui.small("\u{2192}");
                                            if ui.add(
                                                egui::TextEdit::singleline(&mut source.destination_subfolder)
                                                    .desired_width(160.0)
                                                    .hint_text("subfolder"),
                                            ).changed() {
                                                backup_sources_changed = true;
                                            }
                                        });

                                        // Scan result inline
                                        if let Some(scan) = scan {
                                            if scan.exists {
                                                ui.small(format!("{} files | {}", scan.file_count, format_bytes(scan.total_bytes)));
                                            } else if let Some(error) = &scan.error {
                                                ui.colored_label(Color32::from_rgb(168, 52, 33), error);
                                            }
                                        }
                                    });
                                ui.add_space(4.0);
                            }
                        });
                });

            if let Some(index) = backup_source_to_remove {
                app.remove_backup_source(index);
            }
            if let Some(index) = backup_source_to_pick {
                app.open_backup_source_folder_picker(index);
            }
            if let Some(index) = backup_source_destination_to_pick {
                backup_sources_changed =
                    app.pick_backup_source_destination_folder(index) || backup_sources_changed;
            }
            if backup_sources_changed {
                app.detach_selected_presets_after_manual_changes();
                app.sync_legacy_source_path_from_sources();
                app.backup_source_library.scan_results.clear();
                app.invalidate_backup_analysis();
            }
            ui.add_space(14.0);
        }

        if app.active_tab == AppTab::Backup {
            ui.add_space(8.0);
        }
        let mut retry_target = None;

        if app.active_tab == AppTab::Backup {
            if let Some(analysis) = &app.backup_analysis.analysis {
                render_backup_analysis(ui, analysis, &mut app.analysis_file_filter);
                ui.add_space(14.0);
            }

            ui.horizontal(|ui| {
                ui.label(RichText::new("File Queue").strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{} files tracked", app.files.len()))
                            .color(Color32::from_rgb(118, 104, 85)),
                    );
                });
            });
            ui.add_space(8.0);

            Frame::new()
                .fill(Color32::WHITE)
                .stroke(Stroke::new(1.0, Color32::from_rgb(221, 211, 190)))
                .corner_radius(CornerRadius::same(14))
                .inner_margin(Margin::same(14))
                .show(ui, |ui| {
                    ScrollArea::vertical()
                        .id_salt("backup_file_queue_scroll")
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            egui::Grid::new("file_grid")
                                .striped(true)
                                .num_columns(5)
                                .min_col_width(110.0)
                                .spacing([12.0, 8.0])
                                .show(ui, |ui| {
                                    ui.label(RichText::new("File Name").strong());
                                    ui.label(RichText::new("Size").strong());
                                    ui.label(RichText::new("Status").strong());
                                    ui.label(RichText::new("Details").strong());
                                    ui.label(RichText::new("Action").strong());
                                    ui.end_row();

                                    for record in &app.files {
                                        ui.label(display_text_for_ui(&record.name));
                                        ui.label(format_bytes(record.size_bytes));
                                        ui.colored_label(
                                            record.status.color(),
                                            record.status.label(),
                                        );
                                        ui.label(display_text_for_ui(&record.detail));

                                        if record.status.is_retryable() && !app.is_running() {
                                            if ui.button("\u{21BB}").on_hover_text("Retry this file").clicked() {
                                                retry_target = Some(RemoteFile {
                                                    name: record.name.clone(),
                                                    remote_path: record.remote_path.clone(),
                                                    size_bytes: record.size_bytes,
                                                    modified_epoch_seconds: record
                                                        .modified_epoch_seconds,
                                                    source_root: record.source_root.clone(),
                                                    source_label: record.source_label.clone(),
                                                    destination_subfolder: record
                                                        .destination_subfolder
                                                        .clone(),
                                                    relative_path: record.relative_path.clone(),
                                                });
                                            }
                                        } else {
                                            ui.label("-");
                                        }
                                        ui.end_row();
                                    }
                                });

                            if app.files.is_empty() {
                                ui.add_space(12.0);
                                ui.label(
                                    "No files scanned yet. Start a run to populate the queue.",
                                );
                            }
                        });
                });
        }

        if app.active_tab == AppTab::Cleanup {
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Folder Cleanup Preview").strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(preview) = &app.folder_cleanup.preview {
                        ui.label(
                            RichText::new(format!("{} entries", preview.entries.len()))
                                .color(Color32::from_rgb(118, 104, 85)),
                        );
                    }
                });
            });
            ui.add_space(8.0);

            Frame::new()
                .fill(Color32::WHITE)
                .stroke(Stroke::new(1.0, Color32::from_rgb(221, 211, 190)))
                .corner_radius(CornerRadius::same(14))
                .inner_margin(Margin::same(14))
                .show(ui, |ui| {
                    ui.label(RichText::new("Selected folder").strong());
                    wrapped_path_text(ui, &app.folder_cleanup.folder_path);
                    ui.add_space(8.0);

                    if app.folder_cleanup.is_fetching_preview {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Fetching cleanup preview...");
                        });
                    }

                    if let Some(error) = &app.folder_cleanup.preview_error {
                        ui.colored_label(Color32::from_rgb(168, 52, 33), error);
                    }
                    if let Some(error) = &app.folder_cleanup.delete_error {
                        ui.colored_label(Color32::from_rgb(168, 52, 33), error);
                    }

                    if let Some(preview) = app.folder_cleanup.preview.clone() {
                        wrapped_text(ui, &cleanup_summary(&preview));
                        ui.small("Preview is ordered by size, with the largest files first.");
                        ui.add_space(8.0);

                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .add_enabled(
                                    !app.folder_cleanup.is_deleting,
                                    egui::Button::new("Select All"),
                                )
                                .clicked()
                            {
                                app.folder_cleanup.selected_paths = preview
                                    .entries
                                    .iter()
                                    .map(|entry| entry.full_path.clone())
                                    .collect();
                            }
                            if ui
                                .add_enabled(
                                    !app.folder_cleanup.is_deleting,
                                    egui::Button::new("Select Files Only"),
                                )
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
                                .add_enabled(
                                    !app.folder_cleanup.is_deleting,
                                    egui::Button::new("Clear Selection"),
                                )
                                .clicked()
                            {
                                app.folder_cleanup.selected_paths.clear();
                            }
                            ui.label(format!(
                                "{} checked",
                                app.folder_cleanup.selected_paths.len()
                            ));
                        });
                        ui.add_space(8.0);

                        ScrollArea::vertical()
                            .id_salt("cleanup_preview_scroll")
                            .max_height(360.0)
                            .auto_shrink([false; 2])
                            .show(ui, |ui| {
                                for entry in &preview.entries {
                                    Frame::new()
                                        .fill(Color32::from_rgb(250, 247, 240))
                                        .stroke(Stroke::new(
                                            1.0,
                                            Color32::from_rgb(228, 219, 203),
                                        ))
                                        .corner_radius(CornerRadius::same(12))
                                        .inner_margin(Margin::same(10))
                                        .show(ui, |ui| {
                                            ui.horizontal_wrapped(|ui| {
                                                let mut selected = app
                                                    .folder_cleanup
                                                    .selected_paths
                                                    .contains(&entry.full_path);
                                                if ui
                                                    .add_enabled(
                                                        !app.folder_cleanup.is_deleting,
                                                        egui::Checkbox::without_text(
                                                            &mut selected,
                                                        ),
                                                    )
                                                    .changed()
                                                {
                                                    if selected {
                                                        app.folder_cleanup
                                                            .selected_paths
                                                            .insert(entry.full_path.clone());
                                                    } else {
                                                        app.folder_cleanup
                                                            .selected_paths
                                                            .remove(&entry.full_path);
                                                    }
                                                }

                                                ui.colored_label(
                                                    Color32::from_rgb(67, 102, 153),
                                                    entry.kind.label(),
                                                );
                                                ui.label(format!(
                                                    "Size: {}",
                                                    match entry.kind {
                                                        RemoteFolderEntryKind::Directory => {
                                                            "folder".to_string()
                                                        }
                                                        RemoteFolderEntryKind::File => {
                                                            format_bytes(
                                                                entry.size_bytes.unwrap_or(0),
                                                            )
                                                        }
                                                    }
                                                ));
                                            });
                                            ui.add_space(4.0);
                                            wrapped_text(
                                                ui,
                                                entry
                                                    .full_path
                                                    .rsplit('/')
                                                    .next()
                                                    .unwrap_or(&entry.full_path),
                                            );
                                            ui.add_space(2.0);
                                            wrapped_path_text(ui, &entry.full_path);
                                        });
                                    ui.add_space(8.0);
                                }
                            });
                    } else if app.folder_cleanup.preview_error.is_none()
                        && app.folder_cleanup.delete_error.is_none()
                    {
                        ui.label(
                            "Click Fetch Contents to inspect the selected phone folder before deleting anything.",
                        );
                    }
                });
        }

        if let Some(remote_file) = retry_target {
            app.start_retry(remote_file);
        }
            }); // end ScrollArea
    });
}

pub(crate) fn render_remote_folder_picker(ctx: &egui::Context, app: &mut BackupApp) {
    if !app.remote_folder_picker.is_open {
        return;
    }

    let current_path = app.remote_folder_picker.current_path.clone();
    let picker_target = app.remote_folder_picker.target;
    let entries = app.remote_folder_picker.entries.clone();
    let error = app.remote_folder_picker.error.clone();
    let is_loading = app.remote_folder_picker.is_loading;
    let can_go_up = parent_remote_path(&current_path).is_some();
    let mut window_open = app.remote_folder_picker.is_open;
    let mut navigate_to = None;
    let mut select_current = false;
    let mut refresh_listing = false;
    let mut go_up = false;

    egui::Window::new(match picker_target {
        RemoteFolderPickerTarget::SourceFolder => "Select Backup Source Folder",
        RemoteFolderPickerTarget::CleanupFolder => "Select Folder For Cleanup",
        RemoteFolderPickerTarget::BackupSource(_) => "Select Backup Library Folder",
    })
    .open(&mut window_open)
    .collapsible(false)
    .resizable(true)
    .default_size([620.0, 420.0])
    .show(ctx, |ui| {
        ui.label("Browse directories on the connected Android device");
        ui.add_space(6.0);
        wrapped_path_text(ui, &current_path);
        ui.add_space(10.0);

        ui.horizontal(|ui| {
            if ui
                .add_enabled(can_go_up && !is_loading, egui::Button::new("Up"))
                .clicked()
            {
                go_up = true;
            }

            if ui
                .add_enabled(!is_loading, egui::Button::new("Refresh"))
                .clicked()
            {
                refresh_listing = true;
            }

            if ui.button("Use This Folder").clicked() {
                select_current = true;
            }
        });

        ui.add_space(10.0);

        if is_loading {
            ui.spinner();
            ui.label("Loading folders from device...");
        } else if let Some(error) = &error {
            ui.colored_label(Color32::from_rgb(168, 52, 33), error);
        } else if entries.is_empty() {
            ui.label("No subfolders found here. You can still use the current folder.");
        }

        ScrollArea::vertical()
            .id_salt("remote_folder_picker_scroll")
            .max_height(260.0)
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for directory in &entries {
                    let label = format!("[Dir] {}", directory.name);
                    if ui
                        .add_enabled(!is_loading, egui::Button::new(label))
                        .clicked()
                    {
                        navigate_to = Some(directory.full_path.clone());
                    }
                    wrapped_path_text(ui, &directory.full_path);
                    ui.add_space(6.0);
                }
            });
    });

    app.remote_folder_picker.is_open = window_open;

    if let Some(path) = navigate_to {
        app.request_remote_directory_listing(path);
    } else if go_up {
        if let Some(parent) = parent_remote_path(&current_path) {
            app.request_remote_directory_listing(parent);
        }
    } else if refresh_listing {
        app.request_remote_directory_listing(current_path.clone());
    } else if select_current {
        app.apply_remote_folder_picker_selection(current_path, picker_target);
    }
}
