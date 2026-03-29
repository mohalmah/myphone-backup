use eframe::egui::{self, Frame, Margin, RichText, ScrollArea, Stroke};

use crate::core::models::{ExistingFileBehavior, ValidationMode};
use crate::app::AppTab;
use crate::ui::theme::*;
use crate::ui::widgets::*;

pub(crate) fn render_side_panel(ctx: &egui::Context, app: &mut crate::app::BackupApp) {
    let window_width = ctx.screen_rect().width();
    let panel_width = (window_width * 0.25).clamp(260.0, 360.0);

    egui::SidePanel::left("settings_v2")
        .resizable(false)
        .exact_width(panel_width)
        .show_separator_line(false)
        .frame(Frame::new()
            .fill(BG_LAYER)
            .inner_margin(Margin::symmetric(12, 8))
            .stroke(Stroke::new(1.0, BORDER_CARD)))
        .show(ctx, |ui| {
            let adb_job_active = app.has_active_adb_job();

            ScrollArea::vertical()
                .id_salt("side_panel_scroll")
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    if app.active_tab == AppTab::Backup {
                        // ── Quick Presets ──
                        ui.label(RichText::new("Presets").size(12.0).strong().color(TEXT_SECONDARY));
                        ui.add_space(4.0);

                        let presets = app.settings.presets.clone();
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            ui.spacing_mut().item_spacing.y = 4.0;
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

                        if app.selected_preset_count() > 0 {
                            ui.add_space(2.0);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!(
                                    "{} active",
                                    app.selected_preset_count(),
                                )).size(11.0).color(TEXT_TERTIARY));
                                if ui.small_button("Clear").clicked() {
                                    app.clear_selected_preset_chips();
                                    app.status_banner =
                                        "Preset selection cleared.".to_string();
                                }
                            });
                        }

                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if ui.button("Save")
                                .on_hover_text("Save current layout as preset")
                                .clicked()
                            {
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
                        ui.add_space(4.0);
                    }

                    // ── Device ──
                    ui.horizontal(|ui| {
                        status_pill(ui, app.device_info.state.label(), app.device_info.state.color());
                        if let Some(model) = &app.device_info.model {
                            ui.label(RichText::new(model).size(12.0).color(TEXT_SECONDARY));
                        }
                        if ui.small_button("↻").on_hover_text("Refresh device").clicked() {
                            app.refresh_device_info();
                        }
                    })
                    .response
                    .on_hover_text(format!(
                        "Serial: {}\nADB: {}\n{}",
                        app.device_info.serial,
                        app.settings.adb_path,
                        app.device_info.message
                    ));

                    ui.add_space(4.0);

                    if app.active_tab == AppTab::Backup {
                        // ── Destination ──
                        ui.label(RichText::new("Destination").size(12.0).strong().color(TEXT_SECONDARY));
                        ui.horizontal(|ui| {
                            let response = ui.add(
                                egui::TextEdit::singleline(&mut app.settings.destination_path)
                                    .desired_width(ui.available_width() - 32.0)
                                    .hint_text("Select folder..."),
                            );
                            if response.changed() {
                                app.invalidate_backup_analysis();
                            }
                            if ui
                                .add_enabled(!adb_job_active, egui::Button::new("..."))
                                .on_hover_text("Browse")
                                .clicked()
                            {
                                app.pick_local_destination_folder();
                            }
                        });
                        let enabled_count = app.settings.effective_backup_sources().iter().filter(|s| s.enabled).count();
                        ui.label(RichText::new(format!("{enabled_count} sources")).size(11.0).color(TEXT_TERTIARY));
                        ui.add_space(4.0);

                        // ── Validation (collapsed) ──
                        egui::CollapsingHeader::new(RichText::new("Settings").size(12.0).strong().color(TEXT_SECONDARY))
                            .default_open(false)
                            .show(ui, |ui| {
                                ui.label(RichText::new("ADB path").size(12.0).color(TEXT_SECONDARY));
                                ui.add(
                                    egui::TextEdit::singleline(&mut app.settings.adb_path)
                                        .desired_width(f32::INFINITY)
                                        .hint_text("adb"),
                                );
                                ui.add_space(4.0);
                                egui::ComboBox::from_label("Validation")
                                    .selected_text(app.settings.validation_mode.label())
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut app.settings.validation_mode, ValidationMode::Size, ValidationMode::Size.label());
                                        ui.selectable_value(&mut app.settings.validation_mode, ValidationMode::Md5, ValidationMode::Md5.label());
                                    });
                                egui::ComboBox::from_label("Existing files")
                                    .selected_text(app.settings.existing_file_behavior.label())
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut app.settings.existing_file_behavior, ExistingFileBehavior::Skip, ExistingFileBehavior::Skip.label());
                                        ui.selectable_value(&mut app.settings.existing_file_behavior, ExistingFileBehavior::Validate, ExistingFileBehavior::Validate.label());
                                    });
                                ui.checkbox(&mut app.settings.auto_delete_after_success, "Auto-delete after validation");
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

                        // ── Run Controls ──
                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            let running = app.is_running();
                            let paused = app.sync_handle.as_ref().map(|h| h.is_paused()).unwrap_or(false);
                            if ui.add_enabled(!adb_job_active, egui::Button::new("▶ Start")).clicked() {
                                app.start_full_backup();
                            }
                            if ui.add_enabled(running, egui::Button::new(if paused { "▶ Resume" } else { "⏸ Pause" })).clicked() {
                                if let Some(handle) = &app.sync_handle { handle.toggle_pause(); }
                            }
                            if ui.add_enabled(running, egui::Button::new("⏹ Stop")).clicked() {
                                if let Some(handle) = &app.sync_handle { handle.request_stop(); }
                            }
                        });
                    }

                    if app.active_tab == AppTab::Cleanup {
                        // ── Cleanup folder ──
                        ui.label(RichText::new("Cleanup Folder").size(12.0).strong().color(TEXT_SECONDARY));
                        ui.horizontal(|ui| {
                            let mut cleanup_path = app.folder_cleanup.folder_path.clone();
                            if ui.add(
                                egui::TextEdit::singleline(&mut cleanup_path)
                                    .desired_width(ui.available_width() - 32.0)
                                    .hint_text("Phone folder..."),
                            ).changed() {
                                app.set_cleanup_folder_path(cleanup_path);
                            }
                            if ui.add_enabled(!adb_job_active, egui::Button::new("..."))
                                .on_hover_text("Select phone folder").clicked()
                            {
                                app.open_cleanup_folder_picker();
                            }
                        });
                        ui.horizontal(|ui| {
                            if ui.add_enabled(!adb_job_active, egui::Button::new("Fetch")).on_hover_text("Fetch contents").clicked() {
                                app.request_cleanup_preview();
                            }
                            if ui.add_enabled(!app.folder_cleanup.is_deleting, egui::Button::new("Clear")).on_hover_text("Clear preview").clicked() {
                                app.clear_cleanup_preview();
                            }
                        });

                        if let Some(reason) = protected_cleanup_folder_reason(&app.folder_cleanup.folder_path) {
                            ui.colored_label(ERROR, reason);
                        }
                        if app.folder_cleanup.is_fetching_preview {
                            ui.horizontal(|ui| { ui.spinner(); ui.label("Fetching..."); });
                        }
                        if let Some(preview) = &app.folder_cleanup.preview {
                            wrapped_text(ui, &cleanup_summary(preview));
                        }
                        if let Some(error) = &app.folder_cleanup.preview_error {
                            ui.colored_label(ERROR, error);
                        }
                        if let Some(error) = &app.folder_cleanup.delete_error {
                            ui.colored_label(ERROR, error);
                        }

                        ui.add_space(4.0);
                        settings_card(ui, "Delete Actions", |ui| {
                            ui.checkbox(&mut app.folder_cleanup.delete_armed, "I understand these actions permanently delete items");
                            let preview_matches_path = app.cleanup_preview_matches_path();
                            let selected_entries = app.selected_cleanup_entries();
                            let selected_count = selected_entries.len();
                            let selected_bytes: u64 = selected_entries.iter().map(|e| e.size_bytes.unwrap_or(0)).sum();
                            ui.label(RichText::new(format!("{} items | {}", selected_count, format_bytes(selected_bytes))).size(12.0).color(TEXT_SECONDARY));

                            let root_ok = preview_matches_path && app.folder_cleanup.delete_armed && !adb_job_active
                                && protected_cleanup_folder_reason(&app.folder_cleanup.folder_path).is_none();
                            let sel_ok = preview_matches_path && app.folder_cleanup.delete_armed && !adb_job_active && selected_count > 0
                                && selected_entries.iter().all(|e| protected_cleanup_folder_reason(&e.full_path).is_none());

                            ui.add_space(4.0);
                            if ui.add_enabled(root_ok, egui::Button::new("Delete Folder + Contents")).clicked() {
                                app.request_cleanup_delete_folder();
                            }
                            if ui.add_enabled(root_ok, egui::Button::new("Delete Contents Only")).clicked() {
                                app.request_cleanup_delete_contents_only();
                            }
                            if ui.add_enabled(sel_ok, egui::Button::new("Delete Checked")).clicked() {
                                app.request_cleanup_delete_selected();
                            }
                            if app.folder_cleanup.is_deleting {
                                ui.horizontal(|ui| { ui.spinner(); ui.label("Deleting..."); });
                            }
                        });
                    }
                });
        });
}
