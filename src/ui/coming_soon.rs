use crate::app::{AppTab, BackupApp};
use crate::ui::theme::*;
use eframe::egui::{self, Align, CornerRadius, Frame, Layout, Margin, RichText, Stroke};

pub(crate) fn render_coming_soon_page(ctx: &egui::Context, app: &mut BackupApp, title: &str) {
    egui::CentralPanel::default()
        .frame(Frame::new().fill(BG_BASE).inner_margin(Margin::same(20)))
        .show(ctx, |ui| {
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.add_space(80.0);
                Frame::new()
                    .fill(BG_CARD)
                    .stroke(Stroke::new(1.0, BORDER_CARD))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::same(24))
                    .show(ui, |ui| {
                        ui.set_max_width(420.0);
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new(title).size(24.0).strong().color(TEXT_PRIMARY));
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new(
                                    "This workspace is reserved for a later device view.",
                                )
                                .size(13.0)
                                .color(TEXT_SECONDARY),
                            );
                            ui.add_space(18.0);
                            if ui
                                .button(RichText::new("Back to dashboard").size(13.0))
                                .clicked()
                            {
                                app.active_tab = AppTab::Dashboard;
                            }
                        });
                    });
            });
        });
}
