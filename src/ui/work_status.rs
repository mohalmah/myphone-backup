use crate::app::BackupApp;
use crate::ui::theme::*;
use crate::ui::widgets::{display_text_for_ui, format_bytes, format_duration};
use eframe::egui::{self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Stroke};
use std::time::{Duration, Instant};

struct WorkStatus {
    title: String,
    detail: String,
    elapsed_from: Option<Instant>,
    progress: Option<f32>,
    progress_text: String,
    warning: bool,
}

pub(crate) fn render_work_status_bar(ctx: &egui::Context, app: &mut BackupApp) {
    if !app.has_active_adb_job() {
        return;
    }

    ctx.request_repaint_after(Duration::from_millis(120));
    let status = current_status(app);
    let latest_log = app
        .log_entries
        .iter()
        .rev()
        .find(|entry| !entry.detailed_only)
        .map(|entry| entry.message.clone());

    egui::TopBottomPanel::top("adb_work_status_bar")
        .exact_height(if status.progress.is_some() {
            92.0
        } else {
            76.0
        })
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(Color32::from_rgb(240, 247, 255))
                .stroke(Stroke::new(1.0, Color32::from_rgb(199, 224, 248)))
                .inner_margin(Margin::symmetric(18, 10)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                activity_dot(ui, status.warning);
                ui.add_space(8.0);

                ui.vertical(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(&status.title)
                                .size(14.0)
                                .strong()
                                .color(if status.warning { ERROR } else { ACCENT }),
                        );
                        if let Some(started) = status.elapsed_from {
                            ui.label(
                                RichText::new(format_duration(started.elapsed().as_secs_f64()))
                                    .size(11.0)
                                    .color(TEXT_SECONDARY),
                            );
                        }
                    });
                    ui.label(
                        RichText::new(display_text_for_ui(&status.detail))
                            .size(12.0)
                            .color(TEXT_PRIMARY),
                    );
                    if let Some(log) = latest_log {
                        ui.label(
                            RichText::new(format!("Latest: {}", display_text_for_ui(&log)))
                                .size(11.0)
                                .color(TEXT_SECONDARY),
                        );
                    }

                    if let Some(progress) = status.progress {
                        ui.add_space(4.0);
                        ui.add(
                            egui::ProgressBar::new(progress.clamp(0.0, 1.0))
                                .desired_width(ui.available_width().max(180.0))
                                .fill(if status.warning { WARNING } else { ACCENT })
                                .text(status.progress_text),
                        );
                    }
                });

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    status_actions(ui, app);
                });
            });
        });
}

fn current_status(app: &BackupApp) -> WorkStatus {
    if app.is_running() {
        let paused = app
            .sync_handle
            .as_ref()
            .map(|handle| handle.is_paused())
            .unwrap_or(false);
        let title = if paused {
            "Backup paused".to_string()
        } else {
            "ADB backup is running".to_string()
        };
        let detail = app
            .progress
            .current_file
            .as_deref()
            .map(|file| format!("Processing {}", display_text_for_ui(file)))
            .unwrap_or_else(|| {
                "Preparing file list, transfer, validation, and cleanup steps.".to_string()
            });
        let progress = if app.progress.total_files > 0 {
            Some(app.progress.completed_files as f32 / app.progress.total_files as f32)
        } else if app.progress.current_file_progress > 0.0 {
            Some(app.progress.current_file_progress)
        } else {
            None
        };
        let mut progress_text = format!(
            "{} / {} files",
            app.progress.completed_files, app.progress.total_files
        );
        if app.progress.speed_bytes_per_sec > 0.0 {
            progress_text.push_str(&format!(
                " | {}/s",
                format_bytes(app.progress.speed_bytes_per_sec.round() as u64)
            ));
        }
        if let Some(eta) = app.progress.eta_seconds {
            progress_text.push_str(&format!(" | ETA {}", format_duration(eta)));
        }

        return WorkStatus {
            title,
            detail,
            elapsed_from: None,
            progress,
            progress_text,
            warning: paused,
        };
    }

    if app.folder_cleanup.is_deleting {
        return WorkStatus {
            title: "ADB is deleting phone content".to_string(),
            detail: "Do not unplug the phone. Waiting for the delete command to finish safely."
                .to_string(),
            elapsed_from: app.folder_cleanup.delete_started_at,
            progress: None,
            progress_text: String::new(),
            warning: true,
        };
    }

    if app.folder_cleanup.is_fetching_preview {
        return WorkStatus {
            title: "Fetching cleanup preview".to_string(),
            detail: format!(
                "Reading files and folders in {}",
                app.folder_cleanup.folder_path
            ),
            elapsed_from: app.folder_cleanup.preview_started_at,
            progress: None,
            progress_text: String::new(),
            warning: false,
        };
    }

    if app.backup_analysis.is_loading {
        return WorkStatus {
            title: "Analyzing backup plan".to_string(),
            detail: "Scanning phone folders and checking destination/system drive free space."
                .to_string(),
            elapsed_from: app.backup_analysis.started_at,
            progress: None,
            progress_text: String::new(),
            warning: false,
        };
    }

    if app.backup_source_library.is_scanning {
        return WorkStatus {
            title: "Scanning source folders".to_string(),
            detail: "Counting configured phone folders and measuring their total size.".to_string(),
            elapsed_from: app.backup_source_library.started_at,
            progress: None,
            progress_text: String::new(),
            warning: false,
        };
    }

    if app.remote_folder_picker.is_loading {
        return WorkStatus {
            title: "Browsing phone folders".to_string(),
            detail: format!(
                "Listing directories in {}",
                app.remote_folder_picker.current_path
            ),
            elapsed_from: app.remote_folder_picker.started_at,
            progress: None,
            progress_text: String::new(),
            warning: false,
        };
    }

    WorkStatus {
        title: "Checking ADB connection".to_string(),
        detail: "Running device detection. If the phone asks for authorization, tap Allow."
            .to_string(),
        elapsed_from: app.device_probe_started_at,
        progress: None,
        progress_text: String::new(),
        warning: false,
    }
}

fn activity_dot(ui: &mut egui::Ui, warning: bool) {
    let color = if warning { WARNING } else { ACCENT };
    Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.35)))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            ui.spinner();
        });
}

fn status_actions(ui: &mut egui::Ui, app: &mut BackupApp) {
    if app.is_running() {
        let paused = app
            .sync_handle
            .as_ref()
            .map(|handle| handle.is_paused())
            .unwrap_or(false);
        if ui
            .add(egui::Button::new(if paused { "Resume" } else { "Pause" }))
            .clicked()
        {
            if let Some(handle) = &app.sync_handle {
                handle.toggle_pause();
            }
        }
        if ui
            .add(
                egui::Button::new(RichText::new("Stop").strong().color(Color32::WHITE)).fill(ERROR),
            )
            .clicked()
        {
            if let Some(handle) = &app.sync_handle {
                handle.request_stop();
            }
        }
    } else if app.backup_analysis.is_loading {
        if ui.button("Cancel analysis").clicked() {
            app.cancel_backup_analysis();
        }
    } else if app.backup_source_library.is_scanning {
        if ui.button("Cancel scan").clicked() {
            app.cancel_backup_source_scan();
        }
    }
}
