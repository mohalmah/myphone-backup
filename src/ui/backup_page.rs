use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea, Stroke,
};
use std::path::PathBuf;

use crate::app::{BackupApp, RemoteFolderPickerTarget};
use crate::core::models::{ExistingFileBehavior, RemoteFile, ValidationMode};
use crate::ui::theme::*;
use crate::ui::widgets::*;

pub(crate) fn render_backup_page(ctx: &egui::Context, app: &mut BackupApp) {
    egui::CentralPanel::default()
        .frame(Frame::new().fill(BG_BASE).inner_margin(Margin::same(18)))
        .show(ctx, |ui| {
            ScrollArea::vertical()
                .id_salt("backup_page_scroll")
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    page_header(ui, app);
                    ui.add_space(16.0);

                    if ui.available_width() >= 980.0 {
                        ui.horizontal_top(|ui| {
                            ui.vertical(|ui| {
                                ui.set_width((ui.available_width() * 0.58).max(520.0));
                                plan_column(ui, app);
                            });
                            ui.add_space(14.0);
                            ui.vertical(|ui| {
                                run_column(ui, app);
                            });
                        });
                    } else {
                        plan_column(ui, app);
                        ui.add_space(14.0);
                        run_column(ui, app);
                    }

                    ui.add_space(14.0);
                    details_section(ui, app);
                });
        });
}

fn page_header(ui: &mut egui::Ui, app: &BackupApp) {
    ui.horizontal_wrapped(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new("Backup Plan")
                    .size(25.0)
                    .strong()
                    .color(TEXT_PRIMARY),
            );
            ui.label(
                RichText::new("Pick what moves, where it lands, then run.")
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
            status_pill(ui, &format!("{} FOLDERS", active_source_count(app)), ACCENT);
        });
    });
}

fn plan_column(ui: &mut egui::Ui, app: &mut BackupApp) {
    card(ui, "Source Packs", |ui| {
        source_pack_chips(ui, app);
    });

    card(ui, "Destination", |ui| {
        destination_summary(ui, app);
    });

    card(ui, "Selected Phone Folders", |ui| {
        source_library(ui, app);
    });
}

fn run_column(ui: &mut egui::Ui, app: &mut BackupApp) {
    readiness_card(ui, app);
    safety_card(ui, app);
    controls_card(ui, app);
}

fn source_pack_chips(ui: &mut egui::Ui, app: &mut BackupApp) {
    let presets = app.settings.presets.clone();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
        for preset in &presets {
            let selected = app
                .selected_preset_names
                .iter()
                .any(|name| name == &preset.name);
            if render_preset_chip(ui, preset, selected).clicked() {
                app.toggle_preset_chip_selection(&preset.name);
            }
        }
    });

    ui.add_space(10.0);
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                app.selected_preset_count() > 0,
                egui::Button::new("Clear packs"),
            )
            .clicked()
        {
            app.clear_selected_preset_chips();
            app.status_banner =
                "Preset selection cleared. Current folders stay loaded.".to_string();
        }
        ui.label(
            RichText::new(format!("{} active", app.selected_preset_count()))
                .size(11.0)
                .color(TEXT_TERTIARY),
        );
    });

    egui::CollapsingHeader::new("Save this setup as a preset")
        .id_salt("save_preset_details")
        .default_open(false)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut app.preset_name_input)
                        .desired_width(ui.available_width())
                        .hint_text("Preset name"),
                );
                if ui.button("Save").clicked() {
                    app.save_current_preset();
                }
            });
        });
}

fn destination_summary(ui: &mut egui::Ui, app: &mut BackupApp) {
    ui.horizontal_wrapped(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new("Root folder")
                    .size(11.0)
                    .color(TEXT_SECONDARY),
            );
            wrapped_path_text(
                ui,
                &empty_fallback(&app.settings.destination_path, "Not selected"),
            );
        });

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add_enabled(
                    !app.has_active_adb_job(),
                    egui::Button::new("Choose folder"),
                )
                .clicked()
            {
                app.pick_local_destination_folder();
            }
        });
    });

    egui::CollapsingHeader::new("Edit destination path")
        .id_salt("destination_path_details")
        .default_open(false)
        .show(ui, |ui| {
            if ui
                .add(
                    egui::TextEdit::singleline(&mut app.settings.destination_path)
                        .desired_width(f32::INFINITY)
                        .hint_text("Local backup folder"),
                )
                .changed()
            {
                app.invalidate_backup_analysis();
            }
        });
}

fn source_library(ui: &mut egui::Ui, app: &mut BackupApp) {
    let adb_job_active = app.has_active_adb_job();
    let mut remove_index = None;
    let mut pick_phone_index = None;
    let mut pick_destination_index = None;
    let mut changed = false;

    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(!adb_job_active, egui::Button::new("Add folder"))
            .clicked()
        {
            app.add_custom_backup_source();
            changed = true;
        }

        if ui
            .add_enabled(!adb_job_active, egui::Button::new("Scan folders"))
            .clicked()
        {
            app.refresh_backup_source_scan();
        }

        if app.backup_source_library.is_scanning {
            ui.spinner();
            if let Some(started) = app.backup_source_library.started_at {
                ui.label(
                    RichText::new(format_duration(started.elapsed().as_secs_f64()))
                        .size(11.0)
                        .color(TEXT_TERTIARY),
                );
            }
            if ui.small_button("Cancel").clicked() {
                app.cancel_backup_source_scan();
            }
        }
    });

    if let Some(error) = &app.backup_source_library.scan_error {
        ui.add_space(6.0);
        ui.colored_label(ERROR, error);
    }

    ui.add_space(10.0);
    let destination_root = app.settings.destination_path.clone();
    let scan_results = app.backup_source_library.scan_results.clone();

    ScrollArea::vertical()
        .id_salt("source_library_scroll")
        .max_height(460.0)
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            for (index, source) in app.settings.backup_sources.iter_mut().enumerate() {
                let scan = scan_results.iter().find(|scan| scan.id == source.id);
                compact_source_row(
                    ui,
                    index,
                    source,
                    scan,
                    &destination_root,
                    adb_job_active,
                    &mut changed,
                    &mut remove_index,
                    &mut pick_phone_index,
                    &mut pick_destination_index,
                );
                ui.add_space(6.0);
            }
        });

    if let Some(index) = remove_index {
        app.remove_backup_source(index);
        changed = true;
    }
    if let Some(index) = pick_phone_index {
        app.open_backup_source_folder_picker(index);
    }
    if let Some(index) = pick_destination_index {
        if app.pick_backup_source_destination_folder(index) {
            changed = true;
        }
    }

    if changed {
        app.detach_selected_presets_after_manual_changes();
        app.sync_legacy_source_path_from_sources();
        app.backup_source_library.scan_results.clear();
        app.invalidate_backup_analysis();
    }
}

#[allow(clippy::too_many_arguments)]
fn compact_source_row(
    ui: &mut egui::Ui,
    index: usize,
    source: &mut crate::core::models::BackupSourceConfig,
    scan: Option<&crate::core::models::BackupSourceScan>,
    destination_root: &str,
    adb_job_active: bool,
    changed: &mut bool,
    remove_index: &mut Option<usize>,
    pick_phone_index: &mut Option<usize>,
    pick_destination_index: &mut Option<usize>,
) {
    let stroke = if source.enabled {
        Stroke::new(1.0, BORDER_CARD)
    } else {
        Stroke::new(1.0, BG_CARD_HOVER)
    };

    Frame::new()
        .fill(BG_CARD)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.checkbox(&mut source.enabled, "").changed() {
                    *changed = true;
                }

                ui.vertical(|ui| {
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut source.label)
                                .desired_width(190.0)
                                .font(egui::TextStyle::Button),
                        )
                        .changed()
                    {
                        *changed = true;
                    }
                    ui.label(scan_summary(scan));
                });

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(!adb_job_active, egui::Button::new("Remove"))
                        .clicked()
                    {
                        *remove_index = Some(index);
                    }
                    if ui
                        .add_enabled(!adb_job_active, egui::Button::new("PC"))
                        .on_hover_text("Choose PC destination for this folder")
                        .clicked()
                    {
                        *pick_destination_index = Some(index);
                    }
                    if ui
                        .add_enabled(!adb_job_active, egui::Button::new("Phone"))
                        .on_hover_text("Choose phone folder")
                        .clicked()
                    {
                        *pick_phone_index = Some(index);
                    }
                });
            });

            ui.add_space(6.0);
            wrapped_path_text(
                ui,
                &resolved_destination_path(destination_root, &source.destination_subfolder),
            );

            egui::CollapsingHeader::new("Folder paths")
                .id_salt(("source_details", source.id.clone()))
                .default_open(false)
                .show(ui, |ui| {
                    ui.label(RichText::new("Phone").size(11.0).color(TEXT_SECONDARY));
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut source.source_path)
                                .desired_width(f32::INFINITY),
                        )
                        .changed()
                    {
                        *changed = true;
                    }

                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("PC subfolder")
                            .size(11.0)
                            .color(TEXT_SECONDARY),
                    );
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut source.destination_subfolder)
                                .desired_width(f32::INFINITY)
                                .hint_text("Subfolder inside the destination root"),
                        )
                        .changed()
                    {
                        *changed = true;
                    }
                });
        });
}

fn readiness_card(ui: &mut egui::Ui, app: &mut BackupApp) {
    card(ui, "Ready Check", |ui| {
        if let Some(analysis) = &app.backup_analysis.analysis {
            let preflight = &analysis.preflight;
            hero_metric(ui, format_bytes(preflight.bytes_to_copy), "to copy");
            stat_line(ui, "Files", preflight.total_files.to_string(), TEXT_PRIMARY);
            stat_line(
                ui,
                "New or changed",
                preflight.files_to_copy.to_string(),
                ACCENT,
            );
            stat_line(
                ui,
                "Destination free",
                preflight
                    .destination_available_bytes
                    .map(format_bytes)
                    .unwrap_or_else(|| "Unknown".to_string()),
                if preflight.destination_has_enough_space {
                    SUCCESS
                } else {
                    ERROR
                },
            );
            stat_line(
                ui,
                "Conflicts",
                preflight.conflicting_local_files.to_string(),
                WARNING,
            );

            if let Some(warning) = &preflight.system_drive_warning {
                ui.add_space(6.0);
                ui.colored_label(WARNING, warning);
            }
            if let Some(error) = &preflight.destination_space_error {
                ui.add_space(6.0);
                ui.colored_label(ERROR, error);
            }
        } else if app.backup_analysis.is_loading {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Checking folders and disk space...");
                if ui.small_button("Cancel").clicked() {
                    app.cancel_backup_analysis();
                }
            });
        } else {
            hero_metric(
                ui,
                format!("{} folders", active_source_count(app)),
                "selected",
            );
            ui.label(
                RichText::new("Analyze once before a real run to confirm file counts and space.")
                    .size(12.0)
                    .color(TEXT_SECONDARY),
            );
        }

        if let Some(error) = &app.backup_analysis.error {
            ui.add_space(6.0);
            ui.colored_label(ERROR, error);
        }

        ui.add_space(10.0);
        if ui
            .add_enabled(!app.has_active_adb_job(), egui::Button::new("Analyze"))
            .clicked()
        {
            app.request_backup_analysis();
        }
    });
}

fn safety_card(ui: &mut egui::Ui, app: &mut BackupApp) {
    card(ui, "Safety", |ui| {
        ui.checkbox(&mut app.settings.dry_run, "Simulation mode");
        ui.checkbox(
            &mut app.settings.auto_delete_after_success,
            "Delete from phone after validated backup",
        );

        egui::CollapsingHeader::new("Advanced options")
            .id_salt("backup_advanced_options")
            .default_open(false)
            .show(ui, |ui| {
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

                egui::ComboBox::from_label("Existing files")
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

                let mut filter_recent = app.settings.only_last_days.is_some();
                if ui
                    .checkbox(&mut filter_recent, "Only recent files")
                    .changed()
                {
                    app.settings.only_last_days = if filter_recent { Some(7) } else { None };
                }
                if let Some(days) = &mut app.settings.only_last_days {
                    ui.horizontal(|ui| {
                        ui.label("Days");
                        ui.add(egui::DragValue::new(days).range(1..=365));
                    });
                }

                ui.add_space(8.0);
                ui.label(
                    RichText::new("ADB executable")
                        .size(11.0)
                        .color(TEXT_SECONDARY),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut app.settings.adb_path)
                        .desired_width(f32::INFINITY)
                        .hint_text("adb"),
                );
            });
    });
}

fn controls_card(ui: &mut egui::Ui, app: &mut BackupApp) {
    card(ui, "Run", |ui| {
        let running = app.is_running();
        let paused = app
            .sync_handle
            .as_ref()
            .map(|handle| handle.is_paused())
            .unwrap_or(false);

        let start_button = egui::Button::new(
            RichText::new("Start Backup")
                .size(14.0)
                .strong()
                .color(Color32::WHITE),
        )
        .fill(ACCENT)
        .corner_radius(CornerRadius::same(6))
        .min_size(egui::vec2(ui.available_width(), 38.0));
        if ui
            .add_enabled(!app.has_active_adb_job(), start_button)
            .clicked()
        {
            app.start_full_backup();
        }

        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !app.has_active_adb_job(),
                    egui::Button::new("Run simulation"),
                )
                .clicked()
            {
                let previous = app.settings.dry_run;
                app.settings.dry_run = true;
                app.start_full_backup();
                app.settings.dry_run = previous;
            }

            if ui
                .add_enabled(
                    running,
                    egui::Button::new(if paused { "Resume" } else { "Pause" }),
                )
                .clicked()
            {
                if let Some(handle) = &app.sync_handle {
                    handle.toggle_pause();
                }
            }

            if ui.add_enabled(running, egui::Button::new("Stop")).clicked() {
                if let Some(handle) = &app.sync_handle {
                    handle.request_stop();
                }
            }
        });

        ui.add_space(12.0);
        let total_progress = if app.progress.total_files == 0 {
            0.0
        } else {
            app.progress.completed_files as f32 / app.progress.total_files as f32
        };
        ui.add(
            egui::ProgressBar::new(total_progress)
                .desired_width(ui.available_width())
                .fill(ACCENT)
                .text(format!(
                    "{} / {} files",
                    app.progress.completed_files, app.progress.total_files
                )),
        )
        .on_hover_text(progress_detail(&app.progress));

        if app.progress.speed_bytes_per_sec > 0.0 {
            ui.label(
                RichText::new(format!(
                    "{} /s | ETA {}",
                    format_bytes(app.progress.speed_bytes_per_sec as u64),
                    app.progress
                        .eta_seconds
                        .map(format_duration)
                        .unwrap_or_else(|| "n/a".to_string())
                ))
                .size(11.0)
                .color(TEXT_SECONDARY),
            );
        }
    });
}

fn details_section(ui: &mut egui::Ui, app: &mut BackupApp) {
    if let Some(analysis) = &app.backup_analysis.analysis.clone() {
        egui::CollapsingHeader::new("Analysis details")
            .id_salt("analysis_details")
            .default_open(false)
            .show(ui, |ui| {
                render_backup_analysis(ui, analysis, &mut app.analysis_file_filter);
            });
        ui.add_space(10.0);
    }

    egui::CollapsingHeader::new("Recent file queue")
        .id_salt("recent_file_queue_details")
        .default_open(app.is_running() || !app.files.is_empty())
        .show(ui, |ui| {
            recent_file_queue(ui, app);
        });
}

fn recent_file_queue(ui: &mut egui::Ui, app: &mut BackupApp) {
    if app.files.is_empty() {
        ui.label(
            RichText::new("No backup run has started in this session.")
                .size(12.0)
                .color(TEXT_TERTIARY),
        );
        return;
    }

    let mut retry_target: Option<RemoteFile> = None;
    ScrollArea::vertical()
        .id_salt("recent_file_queue_scroll")
        .max_height(280.0)
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            for record in app.files.iter().rev().take(60) {
                Frame::new()
                    .fill(BG_CARD)
                    .stroke(Stroke::new(1.0, BORDER_CARD))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::same(8))
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.colored_label(record.status.color(), record.status.label());
                            ui.label(
                                RichText::new(format_bytes(record.size_bytes))
                                    .color(TEXT_SECONDARY),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if record.status.is_retryable()
                                    && !app.is_running()
                                    && ui.button("Retry").clicked()
                                {
                                    retry_target = Some(RemoteFile {
                                        name: record.name.clone(),
                                        remote_path: record.remote_path.clone(),
                                        size_bytes: record.size_bytes,
                                        modified_epoch_seconds: record.modified_epoch_seconds,
                                        source_root: record.source_root.clone(),
                                        source_label: record.source_label.clone(),
                                        destination_subfolder: record.destination_subfolder.clone(),
                                        relative_path: record.relative_path.clone(),
                                    });
                                }
                            });
                        });
                        wrapped_text(ui, &record.name);
                        ui.label(
                            RichText::new(display_text_for_ui(&record.detail))
                                .size(11.0)
                                .color(TEXT_SECONDARY),
                        );
                    });
                ui.add_space(6.0);
            }
        });

    if let Some(file) = retry_target {
        app.start_retry(file);
    }
}

fn card(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    Frame::new()
        .fill(BG_CARD)
        .stroke(Stroke::new(1.0, BORDER_CARD))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.label(RichText::new(title).size(13.0).strong().color(TEXT_PRIMARY));
            ui.add_space(8.0);
            add_contents(ui);
        });
    ui.add_space(10.0);
}

fn hero_metric(ui: &mut egui::Ui, value: String, label: &str) {
    ui.label(RichText::new(value).size(27.0).strong().color(TEXT_PRIMARY));
    ui.label(RichText::new(label).size(11.0).color(TEXT_SECONDARY));
}

fn stat_line(ui: &mut egui::Ui, label: &str, value: String, color: Color32) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(12.0).color(TEXT_SECONDARY));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(value).size(12.0).strong().color(color));
        });
    });
}

fn active_source_count(app: &BackupApp) -> usize {
    app.settings
        .effective_backup_sources()
        .iter()
        .filter(|source| source.enabled && !source.source_path.trim().is_empty())
        .count()
}

fn scan_summary(scan: Option<&crate::core::models::BackupSourceScan>) -> RichText {
    match scan {
        Some(scan) if scan.exists => RichText::new(format!(
            "{} files | {}",
            scan.file_count,
            format_bytes(scan.total_bytes)
        ))
        .size(11.0)
        .color(SUCCESS),
        Some(scan) => RichText::new(
            scan.error
                .clone()
                .unwrap_or_else(|| "Folder unavailable".to_string()),
        )
        .size(11.0)
        .color(ERROR),
        None => RichText::new("Not scanned").size(11.0).color(TEXT_TERTIARY),
    }
}

fn resolved_destination_path(destination_root: &str, destination_subfolder: &str) -> String {
    let subfolder = destination_subfolder.trim();
    if subfolder.is_empty() {
        return if destination_root.trim().is_empty() {
            "No destination root selected".to_string()
        } else {
            destination_root.to_string()
        };
    }

    let subfolder_path = PathBuf::from(subfolder);
    if subfolder_path.is_absolute() {
        return subfolder_path.display().to_string();
    }

    if destination_root.trim().is_empty() {
        return subfolder.to_string();
    }

    PathBuf::from(destination_root)
        .join(subfolder_path)
        .display()
        .to_string()
}

fn empty_fallback(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value.to_string()
    }
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
            if ui
                .add_enabled(!is_loading, egui::Button::new("Use This Folder"))
                .clicked()
            {
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
                    if ui
                        .add_enabled(
                            !is_loading,
                            egui::Button::new(format!("[Dir] {}", directory.name)),
                        )
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
