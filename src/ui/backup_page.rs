use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea, Stroke,
};
use crate::app::{BackupApp, RemoteFolderPickerTarget};
use crate::core::models::{ExistingFileBehavior, ValidationMode};
use crate::ui::theme::*;
use crate::ui::widgets::*;

pub(crate) fn render_backup_page(ctx: &egui::Context, app: &mut BackupApp) {
    egui::CentralPanel::default()
        .frame(Frame::new().fill(BG_BASE).inner_margin(Margin::same(16)))
        .show(ctx, |ui| {
            ui.label(
                RichText::new("Backup")
                    .size(20.0)
                    .strong()
                    .color(TEXT_PRIMARY),
            );
            ui.add_space(12.0);

            let adb_job_active = app.has_active_adb_job();

            // ── Three-column wizard layout ──
            let mut backup_source_to_remove: Option<usize> = None;
            let mut backup_source_to_pick: Option<usize> = None;
            let mut backup_sources_changed = false;

            ScrollArea::both()
                .id_salt("backup_main_scroll")
                .auto_shrink([false; 2])
                .show(ui, |ui| {
            let total_w = ui.available_width().max(900.0);
            let col_w = (total_w - 16.0) / 3.0;

            ui.horizontal_top(|ui| {
                // ── Column 1: Step 1 — Source Folders ──
                ui.vertical(|ui| {
                    ui.set_width(col_w);
                    ui.label(
                        RichText::new("Step 1: Select Source Folders")
                            .size(12.0)
                            .strong()
                            .color(TEXT_SECONDARY),
                    );
                    ui.add_space(6.0);

                    // Presets
                    let presets = app.settings.presets.clone();
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.spacing_mut().item_spacing.y = 4.0;
                        for preset in &presets {
                            let is_selected = app
                                .selected_preset_names
                                .iter()
                                .any(|n| n == &preset.name);
                            if render_preset_chip(ui, preset, is_selected).clicked() {
                                app.toggle_preset_chip_selection(&preset.name);
                            }
                        }
                    });
                    if app.selected_preset_count() > 0 {
                        ui.add_space(2.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{} active", app.selected_preset_count()))
                                    .size(11.0)
                                    .color(TEXT_TERTIARY),
                            );
                            if ui.small_button("Clear").clicked() {
                                app.clear_selected_preset_chips();
                                app.status_banner = "Preset selection cleared.".to_string();
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Save").on_hover_text("Save current layout as preset").clicked() {
                            app.save_current_preset();
                        }
                        ui.add(
                            egui::TextEdit::singleline(&mut app.preset_name_input)
                                .desired_width(ui.available_width())
                                .hint_text("Preset name"),
                        );
                    });
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(6.0);

                    // Source list
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Sources").size(12.0).strong().color(TEXT_SECONDARY));
                        if ui.add_enabled(!adb_job_active, egui::Button::new("+"))
                            .on_hover_text("Add custom source").clicked()
                        {
                            app.add_custom_backup_source();
                            backup_sources_changed = true;
                        }
                        if ui.add_enabled(!adb_job_active, egui::Button::new("Scan"))
                            .on_hover_text("Scan sources on device").clicked()
                        {
                            app.refresh_backup_source_scan();
                        }
                        if app.backup_source_library.is_scanning { ui.spinner(); }
                    });
                    ui.add_space(4.0);
                    ScrollArea::vertical()
                        .id_salt("backup_source_col1_scroll")
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            for (index, source) in app.settings.backup_sources.iter_mut().enumerate() {
                                let scan = app.backup_source_library.scan_results.iter()
                                    .find(|s| s.id == source.id);
                                Frame::new()
                                    .fill(BG_LAYER)
                                    .stroke(Stroke::new(1.0, BORDER_CARD))
                                    .corner_radius(CornerRadius::same(6))
                                    .inner_margin(Margin::same(8))
                                    .show(ui, |ui| {
                                        ui.horizontal_wrapped(|ui| {
                                            if ui.checkbox(&mut source.enabled, "").changed() {
                                                backup_sources_changed = true;
                                            }
                                            if ui.add(
                                                egui::TextEdit::singleline(&mut source.label)
                                                    .desired_width(120.0),
                                            ).changed() {
                                                backup_sources_changed = true;
                                            }
                                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                                if ui.add_enabled(
                                                    !adb_job_active,
                                                    egui::Button::new("\u{2715}"),
                                                ).on_hover_text("Remove source").clicked() {
                                                    backup_source_to_remove = Some(index);
                                                }
                                                if ui.add_enabled(
                                                    !adb_job_active,
                                                    egui::Button::new("\u{1F4C1}"),
                                                ).on_hover_text("Pick phone folder").clicked() {
                                                    backup_source_to_pick = Some(index);
                                                }
                                            });
                                        });
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(&source.source_path).small().color(TEXT_SECONDARY)
                                            ).wrap().truncate(),
                                        ).on_hover_text(source.source_path.clone());
                                        ui.horizontal(|ui| {
                                            ui.small("\u{2192}");
                                            if ui.add(
                                                egui::TextEdit::singleline(&mut source.destination_subfolder)
                                                    .desired_width(140.0)
                                                    .hint_text("subfolder"),
                                            ).changed() {
                                                backup_sources_changed = true;
                                            }
                                        });
                                        if let Some(scan) = scan {
                                            if scan.exists {
                                                ui.small(format!(
                                                    "{} files | {}",
                                                    scan.file_count,
                                                    format_bytes(scan.total_bytes)
                                                ));
                                            } else if let Some(error) = &scan.error {
                                                ui.colored_label(ERROR, error);
                                            }
                                        }
                                    });
                                ui.add_space(4.0);
                            }
                        });
                    ui.add_space(4.0);
                    if ui.add_enabled(!adb_job_active, egui::Button::new("+ Add Custom Phone Folder"))
                        .clicked()
                    {
                        app.add_custom_backup_source();
                        backup_sources_changed = true;
                    }
                }); // end col 1

                ui.add_space(8.0);

                // ── Column 2: Step 2 — Destination ──
                ui.vertical(|ui| {
                    ui.set_width(col_w);
                    ui.label(
                        RichText::new("Step 2: Choose PC Destination")
                            .size(12.0)
                            .strong()
                            .color(TEXT_SECONDARY),
                    );
                    ui.add_space(6.0);
                    Frame::new()
                        .fill(BG_CARD)
                        .stroke(Stroke::new(1.0, BORDER_CARD))
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(Margin::same(10))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                if ui.add(
                                    egui::TextEdit::singleline(&mut app.settings.destination_path)
                                        .desired_width(ui.available_width() - 36.0)
                                        .hint_text("Folder"),
                                ).changed() {
                                    app.invalidate_backup_analysis();
                                }
                                if ui.add_enabled(!adb_job_active, egui::Button::new("..."))
                                    .on_hover_text("Browse").clicked()
                                {
                                    app.pick_local_destination_folder();
                                }
                            });
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(format!("📁 {}", app.settings.destination_path))
                                    .size(11.0)
                                    .color(TEXT_SECONDARY),
                            );
                        });

                    if let Some(error) = &app.backup_source_library.scan_error.clone() {
                        ui.add_space(6.0);
                        ui.colored_label(ERROR, error);
                    }

                    if let Some(analysis) = &app.backup_analysis.analysis.clone() {
                        ui.add_space(12.0);
                        render_backup_analysis(ui, analysis, &mut app.analysis_file_filter);
                    }
                    if app.backup_analysis.is_loading {
                        ui.add_space(6.0);
                        ui.horizontal(|ui| { ui.spinner(); ui.label("Analyzing..."); });
                    }
                    if let Some(err) = &app.backup_analysis.error.clone() {
                        ui.colored_label(ERROR, err);
                    }

                    // File queue summary
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("File Queue").size(12.0).strong().color(TEXT_SECONDARY));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("{} files", app.files.len()))
                                    .size(11.0)
                                    .color(TEXT_TERTIARY),
                            );
                        });
                    });
                    ui.add_space(4.0);
                    summary_strip(ui, &app.progress, app.last_summary.as_ref());
                }); // end col 2

                ui.add_space(8.0);

                // ── Column 3: Step 3 — Analyze & Configure ──
                ui.vertical(|ui| {
                    ui.set_width(col_w);
                    ui.label(
                        RichText::new("Step 3: Analyze & Configure")
                            .size(12.0)
                            .strong()
                            .color(TEXT_SECONDARY),
                    );
                    ui.add_space(6.0);

                    // Preflight Check card
                    Frame::new()
                        .fill(BG_CARD)
                        .stroke(Stroke::new(1.0, BORDER_CARD))
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(Margin::same(10))
                        .show(ui, |ui| {
                            ui.label(RichText::new("Preflight Check").size(12.0).strong());
                            ui.add_space(4.0);
                            if let Some(analysis) = &app.backup_analysis.analysis {
                                let p = &analysis.preflight;
                                ui.label(RichText::new(format!("Total files to back up: {}", p.total_files)).size(11.0));
                                ui.label(RichText::new(format!("Total size: {}", format_bytes(p.total_bytes))).size(11.0));
                                let space_color = if p.destination_has_enough_space { SUCCESS } else { ERROR };
                                let space_text = if p.destination_has_enough_space {
                                    format!("Free space on dest: {} (Enough space)", p.destination_available_bytes.map(format_bytes).unwrap_or_else(|| "?".to_string()))
                                } else {
                                    "Not enough space on destination".to_string()
                                };
                                ui.colored_label(space_color, RichText::new(space_text).size(11.0));
                                ui.label(RichText::new(format!("Conflicts: {} (Will be skipped)", p.conflicting_local_files)).size(11.0).color(WARNING));
                            } else {
                                if ui.add_enabled(!adb_job_active, egui::Button::new("Analyze"))
                                    .on_hover_text("Analyze sources and calculate space").clicked()
                                {
                                    app.request_backup_analysis();
                                }
                                ui.label(RichText::new("Click Analyze to inspect sources").size(11.0).color(TEXT_TERTIARY));
                            }
                        });

                    ui.add_space(8.0);

                    // Backup Options card
                    Frame::new()
                        .fill(BG_CARD)
                        .stroke(Stroke::new(1.0, BORDER_CARD))
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(Margin::same(10))
                        .show(ui, |ui| {
                            ui.label(RichText::new("Backup Options").size(12.0).strong());
                            ui.add_space(6.0);
                            egui::ComboBox::from_label("Validation Mode")
                                .selected_text(app.settings.validation_mode.label())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut app.settings.validation_mode, ValidationMode::Size, ValidationMode::Size.label());
                                    ui.selectable_value(&mut app.settings.validation_mode, ValidationMode::Md5, ValidationMode::Md5.label());
                                });
                            egui::ComboBox::from_label("Existing Files")
                                .selected_text(app.settings.existing_file_behavior.label())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut app.settings.existing_file_behavior, ExistingFileBehavior::Skip, ExistingFileBehavior::Skip.label());
                                    ui.selectable_value(&mut app.settings.existing_file_behavior, ExistingFileBehavior::Validate, ExistingFileBehavior::Validate.label());
                                });
                            ui.checkbox(&mut app.settings.dry_run, "Dry-run (simulate)");
                            let mut filter_recent = app.settings.only_last_days.is_some();
                            if ui.checkbox(&mut filter_recent, "Recent files only").changed() {
                                app.settings.only_last_days = if filter_recent { Some(7) } else { None };
                            }
                            if let Some(days) = &mut app.settings.only_last_days {
                                ui.horizontal(|ui| {
                                    ui.label("Days");
                                    ui.add(egui::DragValue::new(days).range(1..=365));
                                });
                            }
                        });

                    ui.add_space(10.0);

                    // Action buttons
                    ui.horizontal(|ui| {
                        let running = app.is_running();
                        let paused = app.sync_handle.as_ref().map(|h| h.is_paused()).unwrap_or(false);

                        let start_btn = egui::Button::new(
                            RichText::new("Start Backup").size(12.0).color(Color32::WHITE),
                        ).fill(ACCENT).corner_radius(CornerRadius::same(5));
                        if ui.add_enabled(!adb_job_active, start_btn).clicked() {
                            app.start_full_backup();
                        }

                        if ui.add_enabled(running, egui::Button::new(
                            if paused { "▶ Resume" } else { "⏸ Pause" }
                        )).clicked() {
                            if let Some(handle) = &app.sync_handle { handle.toggle_pause(); }
                        }
                        if ui.add_enabled(running, egui::Button::new("⏹ Stop")).clicked() {
                            if let Some(handle) = &app.sync_handle { handle.request_stop(); }
                        }
                    });

                    ui.add_space(6.0);

                    let dry_btn = egui::Button::new(
                        RichText::new("Run Dry-run").size(12.0),
                    ).stroke(Stroke::new(1.0, BORDER_CARD));
                    if ui.add_enabled(!adb_job_active, dry_btn).clicked() {
                        let was_dry = app.settings.dry_run;
                        app.settings.dry_run = true;
                        app.start_full_backup();
                        app.settings.dry_run = was_dry;
                    }

                    // ADB path (settings)
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(6.0);
                    ui.label(RichText::new("ADB path").size(11.0).color(TEXT_SECONDARY));
                    ui.add(
                        egui::TextEdit::singleline(&mut app.settings.adb_path)
                            .desired_width(f32::INFINITY)
                            .hint_text("adb"),
                    );
                }); // end col 3
            }); // end ui.horizontal_top

            // ── Auto-delete toggle / progress bar ──
            ui.add_space(10.0);
            if app.is_running() {
                let total_progress = if app.progress.total_files == 0 {
                    0.0
                } else {
                    app.progress.completed_files as f32 / app.progress.total_files as f32
                };
                Frame::new()
                    .fill(BG_CARD)
                    .stroke(Stroke::new(1.0, BORDER_CARD))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.add(
                            egui::ProgressBar::new(total_progress)
                                .text(format!(
                                    "{} / {} files",
                                    app.progress.completed_files,
                                    app.progress.total_files
                                ))
                                .fill(ACCENT)
                                .desired_width(ui.available_width())
                                .corner_radius(2),
                        )
                        .on_hover_text(progress_detail(&app.progress));
                    });
            } else {
                Frame::new()
                    .fill(BG_CARD)
                    .stroke(Stroke::new(1.0, BORDER_CARD))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let toggle_text = if app.settings.auto_delete_after_success { "ON" } else { "OFF" };
                            let toggle_color = if app.settings.auto_delete_after_success { ACCENT } else { TEXT_TERTIARY };
                            if ui.button(RichText::new(toggle_text).size(11.0).color(toggle_color)).clicked() {
                                app.settings.auto_delete_after_success = !app.settings.auto_delete_after_success;
                            }
                            ui.label(
                                RichText::new("AUTO-DELETE AFTER SUCCESSFUL BACKUP")
                                    .size(12.0)
                                    .strong()
                                    .color(TEXT_PRIMARY),
                            );
                        });
                    });
            }

            }); // end ScrollArea::both

            // ── Handle deferred mutations ──
            if let Some(index) = backup_source_to_remove {
                app.remove_backup_source(index);
                backup_sources_changed = true;
            }
            if let Some(index) = backup_source_to_pick {
                app.open_backup_source_folder_picker(index);
            }
            if backup_sources_changed {
                app.detach_selected_presets_after_manual_changes();
                app.sync_legacy_source_path_from_sources();
                app.backup_source_library.scan_results.clear();
                app.invalidate_backup_analysis();
            }
        });
}

/// Remote folder picker overlay window (call unconditionally from update())
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
            if ui.add_enabled(can_go_up && !is_loading, egui::Button::new("Up")).clicked() {
                go_up = true;
            }
            if ui.add_enabled(!is_loading, egui::Button::new("Refresh")).clicked() {
                refresh_listing = true;
            }
            if ui.add_enabled(!is_loading, egui::Button::new("Use This Folder")).clicked() {
                select_current = true;
            }
        });

        ui.add_space(10.0);

        if is_loading {
            ui.spinner();
            ui.label("Loading folders from device...");
        } else if let Some(error) = &error {
            ui.colored_label(ERROR, error);
        } else if entries.is_empty() {
            ui.label("No subfolders found here. You can still use the current folder.");
        }

        ScrollArea::vertical()
            .id_salt("remote_folder_picker_scroll")
            .max_height(260.0)
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for directory in &entries {
                    if ui.add_enabled(!is_loading, egui::Button::new(format!("[Dir] {}", directory.name))).clicked() {
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
