use crate::app::{AppTab, BackupApp};
use crate::ui::theme::*;
use eframe::egui::{self, Align, Frame, Layout, Margin, RichText};

pub(crate) fn render_coming_soon_page(ctx: &egui::Context, app: &mut BackupApp, title: &str) {
    egui::CentralPanel::default()
        .frame(Frame::new().fill(BG_BASE).inner_margin(Margin::same(20)))
        .show(ctx, |ui| {
            ui.label(RichText::new(title).size(20.0).strong().color(TEXT_PRIMARY));
            ui.add_space(40.0);
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.label(RichText::new("🔧").size(48.0));
                ui.add_space(12.0);
                ui.label(
                    RichText::new("Coming Soon")
                        .size(22.0)
                        .strong()
                        .color(TEXT_SECONDARY),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new("This feature is under development")
                        .size(13.0)
                        .color(TEXT_TERTIARY),
                );
                ui.add_space(20.0);
                if ui
                    .button(RichText::new("Go to Dashboard").size(13.0))
                    .clicked()
                {
                    app.active_tab = AppTab::Dashboard;
                }
            });
        });
}
