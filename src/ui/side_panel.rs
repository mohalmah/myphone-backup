use eframe::egui::{self, Color32, Frame, Margin, RichText, ScrollArea, Stroke, UiBuilder};

use crate::core::models::{ExistingFileBehavior, ValidationMode};
use crate::app::AppTab;
use crate::ui::widgets::*;

pub(crate) fn render_side_panel(ctx: &egui::Context, app: &mut crate::app::BackupApp) {
    let window_width = ctx.screen_rect().width();
    let panel_width = (window_width * 0.28).clamp(300.0, 420.0);

    egui::SidePanel::left("settings_panel")
        .resizable(false)
        .exact_width(panel_width)
        .show_separator_line(false)
        .frame(Frame::new()
            .fill(Color32::from_rgb(247, 241, 230))
            .inner_margin(Margin::same(10))
            .stroke(Stroke::new(1.0, Color32::from_rgb(221, 211, 190))))
        .show(ctx, |ui| {
            let adb_job_active = app.has_active_adb_job();
            let panel_rect = ui.available_rect_before_wrap();

            // Reserve bottom strip for Run Controls
            let controls_height = 52.0;
            let controls_rect = egui::Rect::from_min_max(
                egui::pos2(panel_rect.min.x, panel_rect.max.y - controls_height),
                panel_rect.max,
            );
            let scroll_rect = egui::Rect::from_min_max(
                panel_rect.min,
                egui::pos2(panel_rect.max.x, controls_rect.min.y),
            );

            // Run Controls at bottom (always visible, not scrolled)
            if app.active_tab == AppTab::Backup {
                ui.allocate_new_ui(UiBuilder::new().max_rect(controls_rect), |ui| {
                    ui.separator();
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let running = app.is_running();
                        let paused = app.sync_handle.as_ref().map(|h| h.is_paused()).unwrap_or(false);

                        if ui.add_enabled(!adb_job_active, egui::Button::new("▶ Start")).clicked() {
                            app.start_full_backup();
                        }
                        if ui.add_enabled(running, egui::Button::new(if paused { "▶ Resume" } else { "⏸ Pause" })).clicked() {
                            if let Some(handle) = &app.sync_handle {
                                handle.toggle_pause();
                            }
                        }
                        if ui.add_enabled(running, egui::Button::new("⏹ Stop")).clicked() {
                            if let Some(handle) = &app.sync_handle {
                                handle.request_stop();
                            }
                        }
                    });
                });
            }

            // Scrollable content above
            ui.allocate_new_ui(UiBuilder::new().max_rect(scroll_rect), |ui| {
                ScrollArea::vertical()
                    .id_salt("side_panel_scroll")
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                    if app.active_tab == AppTab::Backup {
                        // ── Quick Presets (always visible at the top) ──
                        ui.add_space(4.0);
                        ui.label(RichText::new("Quick Presets").strong().size(14.0));
                        ui.add_space(4.0);

                        let presets = app.settings.presets.clone();
                        ui.horizontal_wrapped(|ui| {
                            for preset in &presets {
                                let is_selected = app
                                    .selected_preset_names
                                    .iter()
                                    .any(|name| name == &preset.name);
                                let response = render_preset_chip(ui, preset, is_selected);
                                if response.clicked() {
                                    app.toggle_preset_chip_selection(&preset.name);
                                }
                            }
                        });

                        ui.add_space(4.0);
                        if app.selected_preset_count() > 0 {
                            ui.horizontal_wrapped(|ui| {
                                ui.small(format!(
                                    "{} active: {}",
                                    app.selected_preset_count(),
                                    app.selected_preset_names.join(", ")
                                ));
                                if ui.small_button("Clear").clicked() {
                                    app.clear_selected_preset_chips();
                                    app.status_banner =
                                        "Preset chip selection cleared. The current source library stays loaded."
                                            .to_string();
                                }
                            });
                        }

                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            if ui.small_button("Save Settings").clicked() {
                                app.save_settings();
                            }
                            if ui.small_button("Save as Preset").clicked() {
                                app.save_current_preset();
                            }
                            ui.add(
                                egui::TextEdit::singleline(&mut app.preset_name_input)
                                    .desired_width(140.0)
                                    .hint_text("Preset name"),
                            );
                        });
                        ui.add_space(6.0);
                        ui.separator();
                    }

                    settings_card(ui, "Connection", |ui| {
                        ui.label("ADB executable");
                        ui.add(
                            egui::TextEdit::singleline(&mut app.settings.adb_path)
                                .desired_width(f32::INFINITY),
                        );
                        ui.add_space(8.0);
                        if ui
                            .add_sized(
                                [ui.available_width(), 32.0],
                                egui::Button::new("Refresh Device"),
                            )
                            .clicked()
                        {
                            app.refresh_device_info();
                        }

                        ui.add_space(10.0);
                        ui.label(RichText::new("Current Device").strong());
                        wrapped_text(ui, &device_summary(&app.device_info));
                    });

                    if app.active_tab == AppTab::Backup {
                    settings_card(ui, "Backup Destination", |ui| {
                        ui.label("Local destination folder");
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut app.settings.destination_path)
                                    .desired_width(f32::INFINITY),
                            )
                            .changed()
                        {
                            app.invalidate_backup_analysis();
                        }
                        if ui
                            .add_enabled_ui(!adb_job_active, |ui| {
                                ui.add_sized(
                                    [ui.available_width(), 32.0],
                                    egui::Button::new("Select Windows Folder..."),
                                )
                            })
                            .inner
                            .clicked()
                        {
                            app.pick_local_destination_folder();
                        }
                        ui.small("This is the root backup folder. Each selected source can keep its own subfolder inside it.");
                        ui.add_space(8.0);
                        wrapped_text(
                            ui,
                            &format!(
                                "{} source folder(s) selected for backup",
                                app.settings
                                    .effective_backup_sources()
                                    .iter()
                                    .filter(|source| source.enabled)
                                    .count()
                            ),
                        );
                        ui.add_space(8.0);
                        if ui
                            .add_enabled_ui(!adb_job_active, |ui| {
                                ui.add_sized(
                                    [ui.available_width(), 32.0],
                                    egui::Button::new("Scan Configured Sources"),
                                )
                            })
                            .inner
                            .clicked()
                        {
                            app.refresh_backup_source_scan();
                        }
                        if ui
                            .add_enabled_ui(!adb_job_active, |ui| {
                                ui.add_sized(
                                    [ui.available_width(), 32.0],
                                    egui::Button::new("Analyze Selected Sources"),
                                )
                            })
                            .inner
                            .clicked()
                        {
                            app.request_backup_analysis();
                        }
                        if app.backup_analysis.is_loading {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Analyzing...");
                            });
                        }
                        if let Some(error) = &app.backup_analysis.error {
                            ui.colored_label(Color32::from_rgb(168, 52, 33), error);
                        }
                    });
                    }

                    if app.active_tab == AppTab::Cleanup {
                    settings_card(ui, "Cleanup Folder", |ui| {
                        ui.label("Phone folder to clean up");
                        let mut cleanup_path = app.folder_cleanup.folder_path.clone();
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut cleanup_path)
                                    .desired_width(f32::INFINITY),
                            )
                            .changed()
                        {
                            app.set_cleanup_folder_path(cleanup_path);
                        }
                        if ui
                            .add_enabled_ui(!adb_job_active, |ui| {
                                ui.add_sized(
                                    [ui.available_width(), 32.0],
                                    egui::Button::new("Select Phone Folder..."),
                                )
                            })
                            .inner
                            .clicked()
                        {
                            app.open_cleanup_folder_picker();
                        }
                        ui.small(
                            "Fetch contents first, then choose whether to delete the full folder, keep the folder and delete only its contents, or delete only checked files and subfolders.",
                        );
                        ui.add_space(8.0);
                        ui.columns(2, |columns| {
                            if columns[0]
                                .add_enabled_ui(!adb_job_active, |ui| {
                                    ui.add_sized(
                                        [ui.available_width(), 32.0],
                                        egui::Button::new("Fetch Contents"),
                                    )
                                })
                                .inner
                                .clicked()
                            {
                                app.request_cleanup_preview();
                            }
                            if columns[1]
                                .add_enabled_ui(!app.folder_cleanup.is_deleting, |ui| {
                                    ui.add_sized(
                                        [ui.available_width(), 32.0],
                                        egui::Button::new("Clear Preview"),
                                    )
                                })
                                .inner
                                .clicked()
                            {
                                app.clear_cleanup_preview();
                            }
                        });

                        if let Some(reason) =
                            protected_cleanup_folder_reason(&app.folder_cleanup.folder_path)
                        {
                            ui.colored_label(Color32::from_rgb(168, 52, 33), reason);
                        }

                        if app.folder_cleanup.is_fetching_preview {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Fetching folder contents from device...");
                            });
                        }

                        if let Some(preview) = &app.folder_cleanup.preview {
                            wrapped_text(ui, &cleanup_summary(preview));
                        }
                        if let Some(error) = &app.folder_cleanup.preview_error {
                            ui.colored_label(Color32::from_rgb(168, 52, 33), error);
                        }
                        if let Some(error) = &app.folder_cleanup.delete_error {
                            ui.colored_label(Color32::from_rgb(168, 52, 33), error);
                        }
                    });

                    settings_card(ui, "Delete Actions", |ui| {
                        ui.checkbox(
                            &mut app.folder_cleanup.delete_armed,
                            "I understand these cleanup actions permanently delete items on the phone",
                        );

                        let preview_matches_path = app.cleanup_preview_matches_path();
                        let selected_entries = app.selected_cleanup_entries();
                        let selected_count = selected_entries.len();
                        let selected_bytes = selected_entries
                            .iter()
                            .map(|entry| entry.size_bytes.unwrap_or(0))
                            .sum::<u64>();

                        wrapped_text(
                            ui,
                            &format!(
                                "Selected: {} item(s) | {}",
                                selected_count,
                                format_bytes(selected_bytes)
                            ),
                        );

                        let root_delete_allowed = preview_matches_path
                            && app.folder_cleanup.delete_armed
                            && !adb_job_active
                            && protected_cleanup_folder_reason(
                                &app.folder_cleanup.folder_path,
                            )
                            .is_none();
                        let selected_delete_allowed = preview_matches_path
                            && app.folder_cleanup.delete_armed
                            && !adb_job_active
                            && selected_count > 0
                            && selected_entries.iter().all(|entry| {
                                protected_cleanup_folder_reason(&entry.full_path).is_none()
                            });

                        ui.add_space(6.0);
                        if ui
                            .add_enabled_ui(root_delete_allowed, |ui| {
                                ui.add_sized(
                                    [ui.available_width(), 32.0],
                                    egui::Button::new("Delete Folder + Contents"),
                                )
                            })
                            .inner
                            .clicked()
                        {
                            app.request_cleanup_delete_folder();
                        }
                        if ui
                            .add_enabled_ui(root_delete_allowed, |ui| {
                                ui.add_sized(
                                    [ui.available_width(), 32.0],
                                    egui::Button::new("Delete Contents Only"),
                                )
                            })
                            .inner
                            .clicked()
                        {
                            app.request_cleanup_delete_contents_only();
                        }
                        if ui
                            .add_enabled_ui(selected_delete_allowed, |ui| {
                                ui.add_sized(
                                    [ui.available_width(), 32.0],
                                    egui::Button::new("Delete Checked Items"),
                                )
                            })
                            .inner
                            .clicked()
                        {
                            app.request_cleanup_delete_selected();
                        }

                        if app.folder_cleanup.is_deleting {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Running cleanup delete on device...");
                            });
                        }
                    });
                    }

                    if app.active_tab == AppTab::Backup {
                    settings_card(ui, "Validation", |ui| {
                        egui::ComboBox::from_label("Validation mode")
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

                        ui.checkbox(
                            &mut app.settings.auto_delete_after_success,
                            "Auto delete on device after successful validation",
                        );
                        ui.checkbox(&mut app.settings.dry_run, "Dry-run mode (simulate only)");

                        let mut filter_recent = app.settings.only_last_days.is_some();
                        if ui
                            .checkbox(&mut filter_recent, "Copy only recent files")
                            .changed()
                        {
                            app.settings.only_last_days =
                                if filter_recent { Some(7) } else { None };
                        }
                        if let Some(days) = &mut app.settings.only_last_days {
                            ui.horizontal(|ui| {
                                ui.label("Days");
                                ui.add(egui::DragValue::new(days).range(1..=365));
                            });
                        }
                    });

                    }
                });
            });
        });
}
