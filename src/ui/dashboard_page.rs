use crate::app::{AppTab, BackupApp};
use crate::ui::theme::*;
use crate::ui::widgets::*;
use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea, Stroke,
};

pub(crate) fn render_dashboard_page(ctx: &egui::Context, app: &mut BackupApp) {
    egui::CentralPanel::default()
        .frame(Frame::new().fill(BG_BASE).inner_margin(Margin::same(20)))
        .show(ctx, |ui| {
            ui.label(
                RichText::new("Dashboard")
                    .size(20.0)
                    .strong()
                    .color(TEXT_PRIMARY),
            );
            ui.add_space(14.0);

            ScrollArea::vertical()
                .id_salt("dashboard_scroll")
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    device_card(ui, app);
                    ui.add_space(10.0);
                    storage_row(ui, app);
                    ui.add_space(10.0);
                    action_buttons(ui, app);
                    ui.add_space(10.0);
                    log_card(ui, app);
                });
        });
}

fn device_card(ui: &mut egui::Ui, app: &mut BackupApp) {
    Frame::new()
        .fill(BG_CARD)
        .stroke(Stroke::new(1.0, BORDER_CARD))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                status_pill(
                    ui,
                    app.device_info.state.label(),
                    app.device_info.state.color(),
                );
                if let Some(model) = &app.device_info.model {
                    let label = if app.device_info.serial.is_empty() {
                        model.clone()
                    } else {
                        format!("{model} ({})", app.device_info.serial)
                    };
                    ui.label(RichText::new(label).size(13.0).strong().color(TEXT_PRIMARY));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(RichText::new("↻ Re-scan").size(12.0)).clicked() {
                        app.refresh_device_info();
                    }
                });
            });
            ui.add_space(4.0);
            let last = app
                .last_backup_time
                .as_deref()
                .unwrap_or("No backup this session");
            ui.label(
                RichText::new(format!("LAST BACKUP: {last}"))
                    .size(11.0)
                    .color(TEXT_SECONDARY),
            );
            if let Some(err) = &app.error_banner {
                ui.colored_label(ERROR, RichText::new(err).size(11.0));
            }
            if !app.status_banner.is_empty() {
                ui.label(
                    RichText::new(&app.status_banner)
                        .size(11.0)
                        .color(TEXT_SECONDARY),
                );
            }
        });
}

fn storage_row(ui: &mut egui::Ui, app: &BackupApp) {
    let dest = std::path::PathBuf::from(&app.settings.destination_path);
    let (pc_free, pc_total) = {
        let free = crate::core::storage::available_space_for_path(&dest).unwrap_or(0);
        let total = crate::core::storage::total_space_for_path(&dest).unwrap_or(0);
        (free, total)
    };
    let pc_used_frac = if pc_total > 0 {
        1.0 - (pc_free as f32 / pc_total as f32)
    } else {
        0.0
    };

    ui.columns(2, |cols| {
        // PC Storage
        Frame::new()
            .fill(BG_CARD)
            .stroke(Stroke::new(1.0, BORDER_CARD))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(14))
            .show(&mut cols[0], |ui| {
                ui.label(
                    RichText::new("PC STORAGE")
                        .size(10.0)
                        .strong()
                        .color(TEXT_SECONDARY),
                );
                ui.add_space(4.0);
                ui.add(
                    egui::ProgressBar::new(pc_used_frac)
                        .fill(ACCENT)
                        .desired_width(ui.available_width())
                        .corner_radius(3),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(if pc_total > 0 {
                        format!(
                            "{} free / {} total",
                            format_bytes(pc_free),
                            format_bytes(pc_total)
                        )
                    } else {
                        "Set destination to see storage".to_string()
                    })
                    .size(11.0)
                    .color(TEXT_SECONDARY),
                );
            });

        // Phone Storage (placeholder)
        Frame::new()
            .fill(BG_CARD)
            .stroke(Stroke::new(1.0, BORDER_CARD))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(14))
            .show(&mut cols[1], |ui| {
                ui.label(
                    RichText::new("PHONE STORAGE")
                        .size(10.0)
                        .strong()
                        .color(TEXT_SECONDARY),
                );
                ui.add_space(4.0);
                ui.add(
                    egui::ProgressBar::new(0.0)
                        .fill(ACCENT)
                        .desired_width(ui.available_width())
                        .corner_radius(3),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Connect device to see storage")
                        .size(11.0)
                        .color(TEXT_TERTIARY),
                );
            });
    });
}

fn action_buttons(ui: &mut egui::Ui, app: &mut BackupApp) {
    let adb_active = app.has_active_adb_job();
    ui.columns(2, |cols| {
        let w0 = cols[0].available_width();
        let start_btn = egui::Button::new(
            RichText::new("↺  Start New Backup")
                .size(13.0)
                .color(Color32::WHITE),
        )
        .fill(ACCENT)
        .corner_radius(CornerRadius::same(6))
        .min_size(egui::vec2(w0, 36.0));
        if cols[0].add_enabled(!adb_active, start_btn).clicked() {
            app.active_tab = AppTab::Backup;
        }

        let w1 = cols[1].available_width();
        let cleanup_btn = egui::Button::new(
            RichText::new("🧹  Cleanup Phone")
                .size(13.0)
                .color(TEXT_PRIMARY),
        )
        .fill(BG_CARD)
        .stroke(Stroke::new(1.0, BORDER_CARD))
        .corner_radius(CornerRadius::same(6))
        .min_size(egui::vec2(w1, 36.0));
        if cols[1].add(cleanup_btn).clicked() {
            app.active_tab = AppTab::Cleanup;
        }
    });
}

fn log_card(ui: &mut egui::Ui, app: &BackupApp) {
    Frame::new()
        .fill(BG_CARD)
        .stroke(Stroke::new(1.0, BORDER_CARD))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.label(RichText::new("Log").size(13.0).strong().color(TEXT_PRIMARY));
            ui.add_space(6.0);
            ScrollArea::vertical()
                .id_salt("dashboard_log_scroll")
                .max_height(160.0)
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    for entry in app.log_entries.iter().rev().take(10) {
                        ui.monospace(entry.compact_line());
                    }
                    if app.log_entries.is_empty() {
                        ui.label(
                            RichText::new("No activity yet.")
                                .size(12.0)
                                .color(TEXT_TERTIARY),
                        );
                    }
                });
        });
}
