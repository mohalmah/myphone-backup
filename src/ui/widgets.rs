use arabic_reshaper::arabic_reshape;
use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea, Stroke,
};
use std::path::PathBuf;
use unicode_bidi::BidiInfo;

use crate::core::{
    logging::{LogEntry, LogLevel},
    models::{
        BackupAnalysis, BackupPreset, DeviceConnectionState, DeviceInfo, RemoteFolderPreview,
        RunSummary, SyncProgress, guess_destination_subfolder,
    },
};

pub(crate) fn icon_or_text(icon: &str, _fallback: &str) -> String {
    icon.to_string()
}

pub(crate) fn contains_arabic(text: &str) -> bool {
    text.chars().any(|character| {
        matches!(
            character as u32,
            0x0600..=0x06FF
                | 0x0750..=0x077F
                | 0x08A0..=0x08FF
                | 0xFB50..=0xFDFF
                | 0xFE70..=0xFEFF
        )
    })
}

pub(crate) fn display_text_for_ui(text: &str) -> String {
    if !contains_arabic(text) {
        return text.to_string();
    }

    text.lines()
        .map(|line| {
            let reshaped = arabic_reshape(line);
            let bidi = BidiInfo::new(&reshaped, None);
            if let Some(para) = bidi.paragraphs.first() {
                bidi.reorder_line(para, para.range.clone()).into_owned()
            } else {
                reshaped
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn wrapped_text(ui: &mut egui::Ui, text: &str) {
    ui.add(egui::Label::new(display_text_for_ui(text)).wrap());
}

pub(crate) fn wrapped_path_text(ui: &mut egui::Ui, text: &str) {
    ui.add(
        egui::Label::new(
            RichText::new(display_text_for_ui(text)).color(Color32::from_rgb(86, 74, 60)),
        )
        .wrap(),
    );
}

pub(crate) fn settings_card(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    Frame::new()
        .fill(Color32::from_rgb(250, 247, 240))
        .stroke(Stroke::new(1.0, Color32::from_rgb(221, 211, 190)))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.label(RichText::new(title).strong());
            ui.add_space(8.0);
            add_contents(ui);
        });
    ui.add_space(12.0);
}

#[derive(Clone, Copy)]
pub(crate) struct PresetBadge {
    pub(crate) icon: &'static str,
    pub(crate) color: Color32,
}

pub(crate) fn render_preset_chip(ui: &mut egui::Ui, preset: &BackupPreset, selected: bool) -> egui::Response {
    let badges = preset_badges(preset);
    let primary_badge = badges.first().cloned();

    let (fill, stroke_color, text_color) = if selected {
        let badge_color = primary_badge
            .as_ref()
            .map(|b| b.color)
            .unwrap_or(Color32::from_rgb(73, 121, 92));
        (
            badge_color.gamma_multiply(0.14),
            badge_color.gamma_multiply(0.55),
            badge_color,
        )
    } else {
        (
            Color32::from_rgb(255, 252, 246),
            Color32::from_rgb(221, 211, 190),
            Color32::from_rgb(90, 78, 64),
        )
    };

    let stroke_width = if selected { 2.0 } else { 1.0 };

    let inner = Frame::new()
        .fill(fill)
        .stroke(Stroke::new(stroke_width, stroke_color))
        .corner_radius(CornerRadius::same(255))
        .inner_margin(Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Show icon inline (larger, from the primary badge)
                if let Some(badge) = &primary_badge {
                    ui.label(
                        RichText::new(badge.icon)
                            .size(14.0)
                            .color(badge.color),
                    );
                }
                // Show additional badges if multiple apps
                for badge in badges.iter().skip(1) {
                    ui.label(
                        RichText::new(badge.icon)
                            .size(12.0)
                            .color(badge.color),
                    );
                }
                ui.label(
                    RichText::new(display_text_for_ui(&preset.name))
                        .strong()
                        .color(text_color),
                );
                if selected {
                    ui.label(
                        RichText::new("✓")
                            .strong()
                            .size(12.0)
                            .color(text_color),
                    );
                }
            });
        });

    let response = ui.interact(
        inner.response.rect,
        ui.make_persistent_id(("preset_chip", &preset.name)),
        egui::Sense::click(),
    );
    if response.hovered() {
        ui.output_mut(|output| output.cursor_icon = egui::CursorIcon::PointingHand);
    }

    response.on_hover_text(preset_chip_hover_text(preset))
}



pub(crate) fn preset_badges(preset: &BackupPreset) -> Vec<PresetBadge> {
    let mut badges = Vec::new();
    let name = preset.name.to_lowercase();
    let sources = if preset.sources.is_empty() {
        preset.source_path.to_lowercase()
    } else {
        preset
            .sources
            .iter()
            .map(|source| format!("{} {}", source.id, source.source_path))
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    let combined = format!("{name} {sources}");

    if combined.contains("whatsapp") {
        badges.push(PresetBadge {
            icon: "💬",
            color: Color32::from_rgb(42, 157, 93),
        });
    }
    if combined.contains("telegram") {
        badges.push(PresetBadge {
            icon: "✈",
            color: Color32::from_rgb(44, 127, 184),
        });
    }
    if combined.contains("download") {
        badges.push(PresetBadge {
            icon: "⬇",
            color: Color32::from_rgb(198, 106, 44),
        });
    }
    if combined.contains("camera") || combined.contains("/dcim/") {
        badges.push(PresetBadge {
            icon: "📷",
            color: Color32::from_rgb(129, 92, 51),
        });
    }

    if badges.is_empty() {
        badges.push(PresetBadge {
            icon: "📁",
            color: Color32::from_rgb(118, 104, 85),
        });
    }

    badges
}

pub(crate) fn preset_chip_hover_text(preset: &BackupPreset) -> String {
    let source_labels = if preset.sources.is_empty() {
        vec![guess_destination_subfolder(&preset.source_path)]
    } else {
        preset
            .sources
            .iter()
            .filter(|source| source.enabled)
            .map(|source| source.label.clone())
            .collect::<Vec<_>>()
    };

    let source_summary = if source_labels.is_empty() {
        "No enabled sources".to_string()
    } else {
        source_labels.join(", ")
    };

    format!(
        "{}\nDestination root: {}\nEnabled sources: {}",
        preset.name, preset.destination_path, source_summary
    )
}

pub(crate) fn status_pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    Frame::new()
        .fill(color.gamma_multiply(0.16))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.45)))
        .corner_radius(CornerRadius::same(255))
        .inner_margin(Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.colored_label(color, RichText::new(text).strong());
        });
}

pub(crate) fn device_summary(info: &DeviceInfo) -> String {
    match info.state {
        DeviceConnectionState::Connected => match &info.model {
            Some(model) => format!("{model}\nSerial: {}\n{}", info.serial, info.message),
            None => format!("Serial: {}\n{}", info.serial, info.message),
        },
        DeviceConnectionState::Unauthorized
        | DeviceConnectionState::Offline
        | DeviceConnectionState::Disconnected => info.message.clone(),
    }
}

pub(crate) fn summary_strip(ui: &mut egui::Ui, progress: &SyncProgress, summary: Option<&RunSummary>) {
    ui.horizontal_wrapped(|ui| {
        metric_chip(
            ui,
            "Total Files",
            progress.total_files.to_string(),
            Color32::from_rgb(73, 121, 92),
        );
        metric_chip(
            ui,
            "Processed",
            progress.completed_files.to_string(),
            Color32::from_rgb(198, 106, 44),
        );
        metric_chip(
            ui,
            "Speed",
            format!(
                "{}/s",
                format_bytes(progress.speed_bytes_per_sec.round() as u64)
            ),
            Color32::from_rgb(67, 102, 153),
        );
        metric_chip(
            ui,
            "ETA",
            progress
                .eta_seconds
                .map(format_duration)
                .unwrap_or_else(|| "n/a".to_string()),
            Color32::from_rgb(124, 92, 161),
        );
    });
    ui.add_space(10.0);

    if let Some(summary) = summary {
        ui.horizontal_wrapped(|ui| {
            metric_chip(
                ui,
                "Copied",
                summary.copied.to_string(),
                Color32::from_rgb(73, 121, 92),
            );
            metric_chip(
                ui,
                "Deleted",
                summary.deleted.to_string(),
                Color32::from_rgb(168, 52, 33),
            );
            metric_chip(
                ui,
                "Skipped",
                summary.skipped.to_string(),
                Color32::from_rgb(115, 95, 69),
            );
            metric_chip(
                ui,
                "Failed",
                summary.failed.to_string(),
                Color32::from_rgb(168, 52, 33),
            );
            metric_chip(
                ui,
                "Conflicts",
                summary.conflicts.to_string(),
                Color32::from_rgb(145, 92, 39),
            );
        });
    }
}

pub(crate) fn metric_chip(ui: &mut egui::Ui, label: &str, value: String, color: Color32) {
    Frame::new()
        .fill(color.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.45)))
        .corner_radius(CornerRadius::same(255))
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(color, RichText::new(label).strong());
                ui.label(value);
            });
        });
}

pub(crate) fn progress_detail(progress: &SyncProgress) -> String {
    let current = progress
        .current_file
        .clone()
        .map(|value| display_text_for_ui(&value))
        .unwrap_or_else(|| "None".to_string());
    format!(
        "Current file: {current} | Total data: {} | Processed data: {} | Failed files: {}",
        format_bytes(progress.total_bytes),
        format_bytes(progress.processed_bytes),
        progress.failed_files
    )
}

pub(crate) fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];

    if bytes == 0 {
        return "0 B".to_string();
    }

    let mut value = bytes as f64;
    let mut unit_index = 0usize;
    while value >= 1024.0 && unit_index < UNITS.len() - 1 {
        value /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{bytes} {}", UNITS[unit_index])
    } else {
        format!("{value:.1} {}", UNITS[unit_index])
    }
}

pub(crate) fn format_duration(seconds: f64) -> String {
    let rounded = seconds.round().max(0.0) as u64;
    let hours = rounded / 3600;
    let minutes = (rounded % 3600) / 60;
    let secs = rounded % 60;

    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {secs}s")
    } else {
        format!("{secs}s")
    }
}

pub(crate) fn cleanup_summary(preview: &RemoteFolderPreview) -> String {
    format!(
        "Root: {} | {} files | {} folders | {} total file data",
        preview.root_path,
        preview.file_count,
        preview.directory_count,
        format_bytes(preview.total_file_bytes)
    )
}

pub(crate) fn render_backup_analysis(ui: &mut egui::Ui, analysis: &BackupAnalysis, file_filter: &mut String) {
    let normalized_filter = file_filter.trim().to_lowercase();
    let filtered_files = analysis
        .files
        .iter()
        .filter(|file| {
            normalized_filter.is_empty()
                || file.name.to_lowercase().contains(&normalized_filter)
                || file.remote_path.to_lowercase().contains(&normalized_filter)
        })
        .collect::<Vec<_>>();

    Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0, Color32::from_rgb(221, 211, 190)))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Backup Source Analysis").strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{} files", analysis.preflight.total_files))
                            .color(Color32::from_rgb(118, 104, 85)),
                    );
                });
            });
            ui.add_space(8.0);

            Frame::new()
                .fill(Color32::from_rgb(250, 247, 240))
                .stroke(Stroke::new(1.0, Color32::from_rgb(228, 219, 203)))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::same(12))
                .show(ui, |ui| {
                    ScrollArea::vertical()
                        .id_salt("backup_analysis_summary_scroll")
                        .max_height(220.0)
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            wrapped_text(ui, &analysis.preflight.source_path);
                            wrapped_path_text(ui, &analysis.preflight.destination_path);
                            ui.add_space(6.0);
                            ui.label(format!(
                                "Total source data: {}",
                                format_bytes(analysis.preflight.total_bytes)
                            ));
                            ui.label(format!(
                                "Needs copy: {} across {} files",
                                format_bytes(analysis.preflight.bytes_to_copy),
                                analysis.preflight.files_to_copy
                            ));
                            ui.label(format!(
                                "Already present locally: {} | Conflicts: {}",
                                analysis.preflight.matching_local_files,
                                analysis.preflight.conflicting_local_files
                            ));
                            ui.label(format!(
                                "Destination free space: {}",
                                analysis
                                    .preflight
                                    .destination_available_bytes
                                    .map(format_bytes)
                                    .unwrap_or_else(|| "unknown".to_string())
                            ));
                            ui.label(format!(
                                "Destination space check: {}",
                                if analysis.preflight.destination_has_enough_space {
                                    "Enough space"
                                } else {
                                    "Not enough space"
                                }
                            ));
                            if let Some(system_drive) = &analysis.preflight.system_drive_path {
                                ui.label(format!(
                                    "System drive {} free: {}",
                                    system_drive,
                                    analysis
                                        .preflight
                                        .system_drive_available_bytes
                                        .map(format_bytes)
                                        .unwrap_or_else(|| "unknown".to_string())
                                ));
                            }
                            if let Some(error) = &analysis.preflight.destination_space_error {
                                ui.colored_label(Color32::from_rgb(168, 52, 33), error);
                            }
                            if let Some(warning) = &analysis.preflight.system_drive_warning {
                                ui.colored_label(Color32::from_rgb(145, 92, 39), warning);
                            }
                            if !analysis.source_summaries.is_empty() {
                                ui.add_space(8.0);
                                ui.label(RichText::new("Selected source folders").strong());
                                for source in &analysis.source_summaries {
                                    wrapped_text(
                                        ui,
                                        &format!(
                                            "{} | {} file(s) | {} | subfolder {}",
                                            source.label,
                                            source.file_count,
                                            format_bytes(source.total_bytes),
                                            if source.destination_subfolder.trim().is_empty() {
                                                "root".to_string()
                                            } else {
                                                source.destination_subfolder.clone()
                                            }
                                        ),
                                    );
                                    wrapped_path_text(ui, &source.source_path);
                                    ui.add_space(4.0);
                                }
                            }
                        });
                });

            ui.add_space(10.0);
            Frame::new()
                .fill(Color32::from_rgb(252, 249, 244))
                .stroke(Stroke::new(1.0, Color32::from_rgb(228, 219, 203)))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::same(12))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Source Files").strong());
                        ui.add_space(8.0);
                        ui.label("Filter");
                        ui.add(
                            egui::TextEdit::singleline(file_filter)
                                .hint_text("name or path")
                                .desired_width(240.0),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!(
                                    "{} shown / {} total",
                                    filtered_files.len(),
                                    analysis.files.len()
                                ))
                                .color(Color32::from_rgb(118, 104, 85)),
                            );
                        });
                    });
                    ui.add_space(8.0);

                    ScrollArea::vertical()
                        .id_salt("backup_source_files_scroll")
                        .max_height(320.0)
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            for file in &filtered_files {
                                Frame::new()
                                    .fill(Color32::from_rgb(250, 247, 240))
                                    .stroke(Stroke::new(1.0, Color32::from_rgb(228, 219, 203)))
                                    .corner_radius(CornerRadius::same(12))
                                    .inner_margin(Margin::same(10))
                                    .show(ui, |ui| {
                                        ui.horizontal_wrapped(|ui| {
                                            ui.label(
                                                RichText::new(format_bytes(file.size_bytes))
                                                    .color(Color32::from_rgb(67, 102, 153)),
                                            );
                                        });
                                        ui.add_space(4.0);
                                        wrapped_text(ui, &file.name);
                                        ui.add_space(2.0);
                                        wrapped_path_text(ui, &file.remote_path);
                                    });
                                ui.add_space(8.0);
                            }

                            if filtered_files.is_empty() {
                                ui.add_space(10.0);
                                ui.label("No source files match the current filter.");
                            }
                        });
                });
        });
}

pub(crate) fn render_detailed_log_entry(ui: &mut egui::Ui, entry: &LogEntry) {
    Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0, Color32::from_rgb(221, 211, 190)))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(
                    log_level_color(entry.level),
                    RichText::new(entry.level.label()).strong(),
                );
                ui.label(RichText::new(&entry.timestamp).monospace());
            });
            ui.add_space(4.0);
            ui.label(&entry.message);

            if let Some(detail) = &entry.detail {
                ui.add_space(8.0);
                Frame::new()
                    .fill(Color32::from_rgb(248, 244, 237))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(230, 220, 204)))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::same(10))
                    .show(ui, |ui| {
                        ui.add(egui::Label::new(RichText::new(detail).monospace()).wrap());
                    });
            }
        });
}

pub(crate) fn log_level_color(level: LogLevel) -> Color32 {
    match level {
        LogLevel::Info => Color32::from_rgb(73, 121, 92),
        LogLevel::Error => Color32::from_rgb(168, 52, 33),
        LogLevel::Trace => Color32::from_rgb(67, 102, 153),
    }
}

pub(crate) fn protected_cleanup_folder_reason(path: &str) -> Option<&'static str> {
    if path.trim().is_empty() {
        return Some("Choose a phone folder first.");
    }

    let normalized = normalize_remote_path(path);
    match normalized.as_str() {
        "/" => Some("Deleting the root folder is blocked."),
        "/sdcard" => Some("Deleting /sdcard is blocked."),
        "/storage" => Some("Deleting /storage is blocked."),
        "/storage/emulated" => Some("Deleting /storage/emulated is blocked."),
        "/storage/emulated/0" => Some("Deleting /storage/emulated/0 is blocked."),
        "/sdcard/Android" => Some("Deleting /sdcard/Android is blocked."),
        "/sdcard/Android/media" => Some("Deleting /sdcard/Android/media is blocked."),
        _ => None,
    }
}

pub(crate) fn initial_local_directory(path: &str) -> PathBuf {
    let candidate = PathBuf::from(path);
    if candidate.is_dir() {
        return candidate;
    }

    if let Some(parent) = candidate.parent() {
        if parent.is_dir() {
            return parent.to_path_buf();
        }
    }

    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub(crate) fn derive_destination_subfolder(selected_folder: &PathBuf, destination_root: &str, fallback: &str) -> String {
    let selected = selected_folder.to_string_lossy().to_string();
    let root = destination_root.trim();

    if root.is_empty() {
        if let Some(name) = selected_folder.file_name() {
            return name.to_string_lossy().to_string();
        }
        return fallback.to_string();
    }

    let root_path = PathBuf::from(root);
    if let Ok(relative) = selected_folder.strip_prefix(&root_path) {
        let relative_text = relative.to_string_lossy().replace('\\', "/");
        let trimmed = relative_text.trim_matches('/').to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }

    if let Some(name) = selected_folder.file_name() {
        return name.to_string_lossy().to_string();
    }

    if !selected.trim().is_empty() {
        return selected;
    }

    fallback.to_string()
}

pub(crate) fn normalize_remote_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return "/sdcard".to_string();
    }

    let normalized = trimmed.replace('\\', "/");
    if normalized == "/" {
        "/".to_string()
    } else {
        normalized.trim_end_matches('/').to_string()
    }
}

pub(crate) fn parent_remote_path(path: &str) -> Option<String> {
    let normalized = normalize_remote_path(path);
    if normalized == "/" {
        return None;
    }

    let trimmed = normalized.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) => Some("/".to_string()),
        Some(index) => Some(trimmed[..index].to_string()),
        None => None,
    }
}
