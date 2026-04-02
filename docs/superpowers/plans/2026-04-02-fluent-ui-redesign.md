# Fluent UI Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the current two-panel egui layout with a Windows 11 Fluent Design nav-rail app shell: Dashboard, Backup (3-column wizard), Cleanup (file browser + options panel), plus Devices and Settings stubs — all wired through a persistent 88 px left nav rail with a Nerd Mode raw-log toggle.

**Architecture:** New page modules (`dashboard_page`, `backup_page`, `cleanup_page`, `coming_soon`, `nav_rail`) each own their egui panel calls. The old `side_panel.rs` and `central_panel.rs` are deleted. `app.rs` `update()` dispatches to the correct page renderer after painting the nav rail and optional nerd-mode bottom panel.

**Tech Stack:** Rust, eframe 0.31 / egui 0.31, fs2 (disk space), chrono (timestamps). Build verification: `cargo build` (no unit-test framework — this is a GUI app; correctness is confirmed by `cargo build` + visual run).

---

## File Map

| Action | Path | Responsibility |
|--------|------|----------------|
| Modify | `src/app.rs` | AppTab enum, new fields, update() wiring, nerd panel |
| Modify | `src/core/storage.rs` | Add `total_space_for_path` |
| Create | `src/ui/nav_rail.rs` | Left 88 px nav rail |
| Create | `src/ui/coming_soon.rs` | Shared "Coming Soon" page |
| Create | `src/ui/dashboard_page.rs` | Dashboard: device, storage, actions, log |
| Create | `src/ui/backup_page.rs` | 3-column backup wizard + remote folder picker |
| Create | `src/ui/cleanup_page.rs` | File browser + right options panel |
| Modify | `src/ui/mod.rs` | Export new modules, remove old ones |
| Delete | `src/ui/side_panel.rs` | Replaced by backup_page + cleanup_page |
| Delete | `src/ui/central_panel.rs` | Replaced by per-page modules |

---

## Task 1: Extend AppTab and add new BackupApp fields

**Files:**
- Modify: `src/app.rs:29-34` (AppTab enum)
- Modify: `src/app.rs:77-100` (BackupApp struct)
- Modify: `src/app.rs:121-147` (BackupApp::new)
- Modify: `src/app.rs:1021-1031` (SyncEvent::Finished handler)

- [ ] **Step 1: Replace the AppTab enum** in `src/app.rs` (currently lines 29–34):

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AppTab {
    #[default]
    Dashboard,
    Backup,
    Cleanup,
    Devices,
    Settings,
}
```

- [ ] **Step 2: Add two fields to the BackupApp struct** (after `error_banner`):

```rust
pub(crate) nerd_mode: bool,
pub(crate) last_backup_time: Option<String>,
```

- [ ] **Step 3: Initialize the new fields in `BackupApp::new`** — change the existing `active_tab: AppTab::Backup` line and add the two new fields:

```rust
active_tab: AppTab::Dashboard,
// ... (existing fields unchanged) ...
nerd_mode: false,
last_backup_time: None,
```

- [ ] **Step 4: Set `last_backup_time` on run completion** — in the `SyncEvent::Finished` arm (around line 1021), add one line after `self.last_summary = Some(summary.clone());`:

```rust
self.last_backup_time = Some(chrono::Local::now().format("%A at %-I:%M %p").to_string());
```

- [ ] **Step 5: Verify it compiles**

```
cd c:\dev\myphone-backup\myphone-backup
cargo check 2>&1 | tail -5
```

Expected: warnings about unused imports from old callers, no errors.

- [ ] **Step 6: Commit**

```bash
git add src/app.rs
git commit -m "refactor: extend AppTab (Dashboard/Devices/Settings), add nerd_mode + last_backup_time"
```

---

## Task 2: Add `total_space_for_path` to storage module

**Files:**
- Modify: `src/core/storage.rs`

- [ ] **Step 1: Add the function** at the end of `src/core/storage.rs`:

```rust
pub fn total_space_for_path(path: &Path) -> Result<u64> {
    let lookup_path = existing_ancestor(path)
        .with_context(|| format!("No existing parent found for {}", path.display()))?;
    fs2::total_space(&lookup_path)
        .with_context(|| format!("Failed to read total space for {}", lookup_path.display()))
}
```

- [ ] **Step 2: Verify**

```
cargo check 2>&1 | tail -5
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/core/storage.rs
git commit -m "feat: add total_space_for_path to storage module"
```

---

## Task 3: Create `coming_soon.rs`

**Files:**
- Create: `src/ui/coming_soon.rs`

- [ ] **Step 1: Create the file**

```rust
use eframe::egui::{self, Align, Frame, Layout, Margin, RichText};
use crate::app::{AppTab, BackupApp};
use crate::ui::theme::*;

pub(crate) fn render_coming_soon_page(ctx: &egui::Context, app: &mut BackupApp, title: &str) {
    egui::CentralPanel::default()
        .frame(Frame::new().fill(BG_BASE).inner_margin(Margin::same(20)))
        .show(ctx, |ui| {
            ui.label(RichText::new(title).size(20.0).strong().color(TEXT_PRIMARY));
            ui.add_space(40.0);
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.label(RichText::new("🔧").size(48.0));
                ui.add_space(12.0);
                ui.label(RichText::new("Coming Soon").size(22.0).strong().color(TEXT_SECONDARY));
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
```

- [ ] **Step 2: Add to `src/ui/mod.rs`** — append:

```rust
pub mod coming_soon;
```

- [ ] **Step 3: Verify**

```
cargo check 2>&1 | tail -5
```

- [ ] **Step 4: Commit**

```bash
git add src/ui/coming_soon.rs src/ui/mod.rs
git commit -m "feat: add coming_soon page stub for Devices and Settings"
```

---

## Task 4: Create `nav_rail.rs`

**Files:**
- Create: `src/ui/nav_rail.rs`

- [ ] **Step 1: Create the file**

```rust
use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Stroke,
};
use crate::app::{AppTab, BackupApp};
use crate::ui::theme::*;

pub(crate) fn render_nav_rail(ctx: &egui::Context, app: &mut BackupApp) {
    egui::SidePanel::left("nav_rail")
        .resizable(false)
        .exact_width(88.0)
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(BG_LAYER)
                .stroke(Stroke::new(1.0, BORDER_CARD))
                .inner_margin(Margin::same(0)),
        )
        .show(ctx, |ui| {
            ui.set_min_height(ui.available_height());

            // Hamburger (decorative — always expanded)
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.label(RichText::new("☰").size(18.0).color(TEXT_PRIMARY));
            });
            ui.add_space(8.0);

            // Primary nav items
            nav_item(ui, app, AppTab::Dashboard, "⊞", "Dashboard", false);
            nav_item(ui, app, AppTab::Backup, "📦", "Backup", false);
            nav_item(ui, app, AppTab::Cleanup, "🧹", "Cleanup", false);
            nav_item(ui, app, AppTab::Devices, "📱", "Devices", true);

            // Bottom-pinned items
            ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                // Nerd mode toggle
                ui.add_space(6.0);
                let nerd_color = if app.nerd_mode { ACCENT } else { TEXT_TERTIARY };
                let inner = Frame::new()
                    .fill(Color32::TRANSPARENT)
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::symmetric(4, 6))
                    .show(ui, |ui| {
                        ui.set_min_width(72.0);
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new("⌨").size(14.0).color(nerd_color));
                            ui.label(
                                RichText::new(if app.nerd_mode { "Nerd ✓" } else { "Nerd" })
                                    .size(9.0)
                                    .color(nerd_color),
                            );
                        });
                    });
                let resp = ui.interact(
                    inner.response.rect,
                    ui.make_persistent_id("nerd_toggle"),
                    egui::Sense::click(),
                );
                if resp.hovered() {
                    ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    app.nerd_mode = !app.nerd_mode;
                }

                // Settings nav item (bottom-pinned)
                nav_item(ui, app, AppTab::Settings, "⚙", "Settings", false);
            });
        });
}

fn nav_item(
    ui: &mut egui::Ui,
    app: &mut BackupApp,
    tab: AppTab,
    icon: &str,
    label: &str,
    coming_soon: bool,
) {
    let is_active = app.active_tab == tab;
    let text_color = if coming_soon {
        TEXT_TERTIARY
    } else if is_active {
        ACCENT
    } else {
        TEXT_PRIMARY
    };
    let fill = if is_active { BG_CARD } else { Color32::TRANSPARENT };

    let inner = Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(4, 8))
        .show(ui, |ui| {
            ui.set_min_width(72.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(icon).size(16.0).color(text_color));
                ui.label(RichText::new(label).size(9.5).color(text_color));
            });
        });

    // Accent left-edge indicator bar for active item
    if is_active {
        let rect = inner.response.rect;
        ui.painter().rect_filled(
            egui::Rect::from_min_size(rect.left_top(), egui::vec2(3.0, rect.height())),
            0.0,
            ACCENT,
        );
    }

    let response = ui.interact(
        inner.response.rect,
        ui.make_persistent_id(("nav_item", label)),
        egui::Sense::click(),
    );
    if response.hovered() && !coming_soon {
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
    }
    if response.clicked() && !coming_soon {
        app.active_tab = tab;
    }

    ui.add_space(2.0);
}
```

- [ ] **Step 2: Add to `src/ui/mod.rs`**:

```rust
pub mod nav_rail;
```

- [ ] **Step 3: Verify**

```
cargo check 2>&1 | tail -5
```

- [ ] **Step 4: Commit**

```bash
git add src/ui/nav_rail.rs src/ui/mod.rs
git commit -m "feat: add Windows 11 nav rail with icon+label items and nerd mode toggle"
```

---

## Task 5: Create `dashboard_page.rs`

**Files:**
- Create: `src/ui/dashboard_page.rs`

- [ ] **Step 1: Create the file**

```rust
use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea, Stroke,
};
use crate::app::{AppTab, BackupApp};
use crate::ui::theme::*;
use crate::ui::widgets::*;

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

fn device_card(ui: &mut egui::Ui, app: &BackupApp) {
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
                    ui.label(
                        RichText::new(format!("{model} ({})", app.device_info.serial))
                            .size(13.0)
                            .strong()
                            .color(TEXT_PRIMARY),
                    );
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .button(RichText::new("↻ Re-scan").size(12.0))
                        .clicked()
                    {
                        // re-scan is triggered via app method, but this is called with &BackupApp.
                        // The button click is handled by the caller; use on_hover only here.
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
                ui.label(RichText::new(&app.status_banner).size(11.0).color(TEXT_SECONDARY));
            }
        });
}

fn storage_row(ui: &mut egui::Ui, app: &BackupApp) {
    ui.horizontal(|ui| {
        let half_w = (ui.available_width() - 10.0) / 2.0;

        // PC Storage
        let dest = std::path::PathBuf::from(&app.settings.destination_path);
        let (pc_free, pc_total) = {
            let free = crate::core::storage::available_space_for_path(&dest)
                .unwrap_or(0);
            let total = crate::core::storage::total_space_for_path(&dest)
                .unwrap_or(0);
            (free, total)
        };
        let pc_used_frac = if pc_total > 0 {
            1.0 - (pc_free as f32 / pc_total as f32)
        } else {
            0.0
        };

        Frame::new()
            .fill(BG_CARD)
            .stroke(Stroke::new(1.0, BORDER_CARD))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(14))
            .show(ui, |ui| {
                ui.set_min_width(half_w);
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

        ui.add_space(10.0);

        // Phone Storage (not available via ADB yet — placeholder)
        Frame::new()
            .fill(BG_CARD)
            .stroke(Stroke::new(1.0, BORDER_CARD))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(14))
            .show(ui, |ui| {
                ui.set_min_width(half_w);
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
    ui.horizontal(|ui| {
        let btn_w = (ui.available_width() - 10.0) / 2.0;

        let start_btn = egui::Button::new(
            RichText::new("↺  Start New Backup")
                .size(13.0)
                .color(Color32::WHITE),
        )
        .fill(ACCENT)
        .corner_radius(CornerRadius::same(6))
        .min_size(egui::vec2(btn_w, 36.0));

        if ui
            .add_enabled(!app.has_active_adb_job(), start_btn)
            .clicked()
        {
            app.active_tab = AppTab::Backup;
        }

        ui.add_space(10.0);

        let cleanup_btn = egui::Button::new(
            RichText::new("🧹  Cleanup Phone").size(13.0).color(TEXT_PRIMARY),
        )
        .fill(BG_CARD)
        .stroke(Stroke::new(1.0, BORDER_CARD))
        .corner_radius(CornerRadius::same(6))
        .min_size(egui::vec2(btn_w, 36.0));

        if ui.add(cleanup_btn).clicked() {
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
```

**Note:** The Re-scan button in `device_card` is read-only because the function takes `&BackupApp`. Move it to take `&mut BackupApp` if you want the button to work — see Step 2.

- [ ] **Step 2: Make device_card mutable** — change its signature and wire up re-scan:

In `dashboard_page.rs`, change:
```rust
fn device_card(ui: &mut egui::Ui, app: &BackupApp) {
```
to:
```rust
fn device_card(ui: &mut egui::Ui, app: &mut BackupApp) {
```

And replace the comment-only re-scan button with:
```rust
ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
    if ui
        .button(RichText::new("↻ Re-scan").size(12.0))
        .clicked()
    {
        app.refresh_device_info();
    }
});
```

- [ ] **Step 3: Add to `src/ui/mod.rs`**:

```rust
pub mod dashboard_page;
```

- [ ] **Step 4: Verify**

```
cargo check 2>&1 | tail -5
```

- [ ] **Step 5: Commit**

```bash
git add src/ui/dashboard_page.rs src/ui/mod.rs
git commit -m "feat: add Dashboard page with device card, storage bars, actions, log"
```

---

## Task 6: Create `backup_page.rs`

**Files:**
- Create: `src/ui/backup_page.rs`

This file absorbs the backup tab content from `central_panel.rs` plus `render_remote_folder_picker`, restructured into the 3-column wizard layout.

- [ ] **Step 1: Create `src/ui/backup_page.rs`**

```rust
use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea, Stroke,
};
use crate::app::{BackupApp, RemoteFolderPickerTarget};
use crate::core::models::{ExistingFileBehavior, RemoteFile, ValidationMode};
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

            ui.columns(3, |cols| {
                // ── Column 1: Step 1 — Source Folders ──
                {
                    let ui = &mut cols[0];
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
                    if ui.add_enabled(!adb_job_active, egui::Button::new("＋ Add Custom Phone Folder"))
                        .clicked()
                    {
                        app.add_custom_backup_source();
                        backup_sources_changed = true;
                    }
                }

                // ── Column 2: Step 2 — Destination ──
                {
                    let ui = &mut cols[1];
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
                }

                // ── Column 3: Step 3 — Analyze & Configure ──
                {
                    let ui = &mut cols[2];
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
                }
            }); // end ui.columns

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
                            // Toggle switch approximated with a button
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
            if ui.button("Use This Folder").clicked() {
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
```

- [ ] **Step 2: Add to `src/ui/mod.rs`**:

```rust
pub mod backup_page;
```

- [ ] **Step 3: Verify**

```
cargo check 2>&1 | tail -10
```

Fix any type errors before proceeding.

- [ ] **Step 4: Commit**

```bash
git add src/ui/backup_page.rs src/ui/mod.rs
git commit -m "feat: add Backup page with 3-column wizard and remote folder picker"
```

---

## Task 7: Create `cleanup_page.rs`

**Files:**
- Create: `src/ui/cleanup_page.rs`

- [ ] **Step 1: Create the file**

```rust
use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea, Stroke,
};
use crate::app::BackupApp;
use crate::core::models::RemoteFolderEntryKind;
use crate::ui::theme::*;
use crate::ui::widgets::*;

pub(crate) fn render_cleanup_page(ctx: &egui::Context, app: &mut BackupApp) {
    let adb_job_active = app.has_active_adb_job();

    // Right panel must be registered BEFORE CentralPanel in egui
    egui::SidePanel::right("cleanup_right_panel")
        .resizable(false)
        .exact_width(220.0)
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(BG_LAYER)
                .stroke(Stroke::new(1.0, BORDER_CARD))
                .inner_margin(Margin::same(12)),
        )
        .show(ctx, |ui| {
            right_panel(ui, app, adb_job_active);
        });

    // Central panel: breadcrumb + file browser
    egui::CentralPanel::default()
        .frame(Frame::new().fill(BG_BASE).inner_margin(Margin::same(12)))
        .show(ctx, |ui| {
            ui.label(
                RichText::new("Cleanup")
                    .size(20.0)
                    .strong()
                    .color(TEXT_PRIMARY),
            );
            ui.add_space(8.0);
            breadcrumb_bar(ui, app, adb_job_active);
            ui.add_space(8.0);
            file_browser(ui, app, adb_job_active);
        });
}

fn breadcrumb_bar(ui: &mut egui::Ui, app: &mut BackupApp, adb_job_active: bool) {
    Frame::new()
        .fill(BG_CARD)
        .stroke(Stroke::new(1.0, BORDER_CARD))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Back/forward (back = go up)
                let can_go_up = parent_remote_path(&app.folder_cleanup.folder_path).is_some()
                    && app.folder_cleanup.preview.is_some();
                if ui.add_enabled(can_go_up && !adb_job_active, egui::Button::new("‹")).clicked() {
                    if let Some(parent) = parent_remote_path(&app.folder_cleanup.folder_path) {
                        app.set_cleanup_folder_path(parent);
                        app.request_cleanup_preview();
                    }
                }
                if ui.add_enabled(false, egui::Button::new("›")).clicked() {}

                // Current path
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    let mut path = app.folder_cleanup.folder_path.clone();
                    if ui.add(
                        egui::TextEdit::singleline(&mut path)
                            .desired_width(ui.available_width() - 72.0)
                            .hint_text("Phone folder..."),
                    ).changed() {
                        app.set_cleanup_folder_path(path);
                    }
                });

                // Icons: pick folder + refresh
                if ui.add_enabled(!adb_job_active, egui::Button::new("\u{1F4C1}"))
                    .on_hover_text("Pick phone folder").clicked()
                {
                    app.open_cleanup_folder_picker();
                }
                if ui.add_enabled(!adb_job_active, egui::Button::new("↻"))
                    .on_hover_text("Refresh").clicked()
                {
                    app.request_cleanup_preview();
                }
            });
        });
}

fn file_browser(ui: &mut egui::Ui, app: &mut BackupApp, adb_job_active: bool) {
    if app.folder_cleanup.is_fetching_preview {
        ui.horizontal(|ui| { ui.spinner(); ui.label("Fetching..."); });
        return;
    }

    if let Some(error) = &app.folder_cleanup.preview_error.clone() {
        ui.colored_label(ERROR, error);
    }
    if let Some(error) = &app.folder_cleanup.delete_error.clone() {
        ui.colored_label(ERROR, error);
    }

    let Some(preview) = app.folder_cleanup.preview.clone() else {
        ui.label(
            RichText::new("Click ↻ Refresh to inspect the selected folder before deleting anything.")
                .size(12.0)
                .color(TEXT_TERTIARY),
        );
        return;
    };

    // Sort bar + bulk select
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Sort by").size(11.0).color(TEXT_SECONDARY));
        ui.label(RichText::new("Largest first").size(11.0).color(TEXT_PRIMARY));
        ui.add_space(16.0);
        if ui.add_enabled(!adb_job_active, egui::Button::new("Select All")).clicked() {
            app.folder_cleanup.selected_paths =
                preview.entries.iter().map(|e| e.full_path.clone()).collect();
        }
        if ui.add_enabled(!adb_job_active, egui::Button::new("Files Only")).clicked() {
            app.folder_cleanup.selected_paths = preview
                .entries
                .iter()
                .filter(|e| e.kind == RemoteFolderEntryKind::File)
                .map(|e| e.full_path.clone())
                .collect();
        }
        if ui.add_enabled(!adb_job_active, egui::Button::new("Clear Selection")).clicked() {
            app.folder_cleanup.selected_paths.clear();
        }
        ui.label(
            RichText::new(format!("{} checked", app.folder_cleanup.selected_paths.len()))
                .size(11.0)
                .color(TEXT_TERTIARY),
        );
    });

    ui.add_space(6.0);

    // Column header row
    Frame::new()
        .fill(BG_LAYER)
        .inner_margin(Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(24.0); // checkbox width
                ui.label(RichText::new("Name ▲").size(11.0).strong().color(TEXT_SECONDARY));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(50.0); // Select link width
                    ui.label(RichText::new("Size").size(11.0).strong().color(TEXT_SECONDARY));
                });
            });
        });

    ScrollArea::vertical()
        .id_salt("cleanup_file_browser_scroll")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            for entry in &preview.entries {
                let is_selected = app.folder_cleanup.selected_paths.contains(&entry.full_path);
                Frame::new()
                    .fill(if is_selected { ACCENT.gamma_multiply(0.06) } else { BG_CARD })
                    .stroke(Stroke::new(1.0, BORDER_CARD))
                    .inner_margin(Margin::symmetric(8, 5))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let mut selected = is_selected;
                            if ui.add_enabled(
                                !app.folder_cleanup.is_deleting,
                                egui::Checkbox::without_text(&mut selected),
                            ).changed() {
                                if selected {
                                    app.folder_cleanup.selected_paths.insert(entry.full_path.clone());
                                } else {
                                    app.folder_cleanup.selected_paths.remove(&entry.full_path);
                                }
                            }

                            let icon = match entry.kind {
                                RemoteFolderEntryKind::Directory => "📁",
                                RemoteFolderEntryKind::File => "📄",
                            };
                            let name = entry.full_path.rsplit('/').next().unwrap_or(&entry.full_path);
                            ui.label(RichText::new(format!("{icon} {}", display_text_for_ui(name))).size(12.0));

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.add_enabled(!adb_job_active,
                                    egui::Button::new(RichText::new("Select").size(11.0).color(ACCENT))
                                        .fill(Color32::TRANSPARENT)
                                ).clicked() {
                                    app.folder_cleanup.selected_paths.insert(entry.full_path.clone());
                                }
                                let size_text = match entry.kind {
                                    RemoteFolderEntryKind::Directory => "—".to_string(),
                                    RemoteFolderEntryKind::File => format_bytes(entry.size_bytes.unwrap_or(0)),
                                };
                                ui.label(RichText::new(size_text).size(11.0).color(TEXT_SECONDARY));
                            });
                        });
                    });
                ui.add_space(2.0);
            }
        });
}

fn right_panel(ui: &mut egui::Ui, app: &mut BackupApp, adb_job_active: bool) {
    // Progress / Results card
    if app.is_running() {
        Frame::new()
            .fill(BG_CARD)
            .stroke(Stroke::new(1.0, BORDER_CARD))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.label(RichText::new("Results").size(12.0).strong());
                ui.add_space(4.0);
                if let Some(file) = &app.progress.current_file {
                    ui.label(RichText::new("Current file:").size(10.0).color(TEXT_SECONDARY));
                    ui.label(RichText::new(display_text_for_ui(file)).size(10.0).wrap());
                }
                ui.add_space(4.0);
                ui.label(RichText::new(format!("Files: {} / {}", app.progress.completed_files, app.progress.total_files)).size(11.0));
                ui.label(RichText::new(format!("Speed: {}/s", format_bytes(app.progress.speed_bytes_per_sec.round() as u64))).size(11.0));
                if let Some(eta) = app.progress.eta_seconds {
                    ui.label(RichText::new(format!("ETA: {}", format_duration(eta))).size(11.0));
                }
                ui.add_space(6.0);
                let running = app.is_running();
                let paused = app.sync_handle.as_ref().map(|h| h.is_paused()).unwrap_or(false);
                ui.horizontal(|ui| {
                    if ui.add_enabled(running, egui::Button::new(if paused { "▶" } else { "⏸" })).clicked() {
                        if let Some(h) = &app.sync_handle { h.toggle_pause(); }
                    }
                    if ui.add_enabled(running, egui::Button::new("⏹")).clicked() {
                        if let Some(h) = &app.sync_handle { h.request_stop(); }
                    }
                });
            });
        ui.add_space(10.0);
    }

    // Cleanup Options card
    let preview_matches = app.cleanup_preview_matches_path();
    let selected_entries = app.selected_cleanup_entries();
    let selected_count = selected_entries.len();
    let selected_bytes: u64 = selected_entries.iter().map(|e| e.size_bytes.unwrap_or(0)).sum();

    Frame::new()
        .fill(BG_CARD)
        .stroke(Stroke::new(1.0, BORDER_CARD))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.label(RichText::new("Cleanup Options").size(12.0).strong());
            ui.add_space(6.0);

            ui.label(
                RichText::new(format!("{selected_count} items | {}", format_bytes(selected_bytes)))
                    .size(11.0)
                    .color(TEXT_SECONDARY),
            );
            ui.add_space(4.0);

            ui.checkbox(
                &mut app.folder_cleanup.delete_armed,
                "I understand these actions permanently delete items",
            );
            ui.add_space(6.0);

            let root_ok = preview_matches
                && app.folder_cleanup.delete_armed
                && !adb_job_active
                && protected_cleanup_folder_reason(&app.folder_cleanup.folder_path).is_none();
            let sel_ok = preview_matches
                && app.folder_cleanup.delete_armed
                && !adb_job_active
                && selected_count > 0
                && selected_entries.iter().all(|e| protected_cleanup_folder_reason(&e.full_path).is_none());

            if ui.add_enabled(root_ok, egui::Button::new("Delete Entire Folder")).clicked() {
                app.request_cleanup_delete_folder();
            }
            ui.add_space(2.0);
            if ui.add_enabled(root_ok, egui::Button::new("Delete Contents Only")).clicked() {
                app.request_cleanup_delete_contents_only();
            }
            ui.add_space(6.0);

            let del_btn = egui::Button::new(
                RichText::new("DELETE SELECTED").size(12.0).color(Color32::WHITE).strong(),
            )
            .fill(ERROR)
            .corner_radius(CornerRadius::same(5))
            .min_size(egui::vec2(ui.available_width(), 32.0));

            if ui.add_enabled(sel_ok, del_btn).clicked() {
                app.request_cleanup_delete_selected();
            }

            if app.folder_cleanup.is_deleting {
                ui.add_space(4.0);
                ui.horizontal(|ui| { ui.spinner(); ui.label("Deleting..."); });
            }
        });

    if let Some(reason) = protected_cleanup_folder_reason(&app.folder_cleanup.folder_path) {
        ui.add_space(6.0);
        ui.colored_label(ERROR, reason);
    }
}
```

- [ ] **Step 2: Add to `src/ui/mod.rs`**:

```rust
pub mod cleanup_page;
```

- [ ] **Step 3: Verify**

```
cargo check 2>&1 | tail -10
```

- [ ] **Step 4: Commit**

```bash
git add src/ui/cleanup_page.rs src/ui/mod.rs
git commit -m "feat: add Cleanup page with file browser and delete options panel"
```

---

## Task 8: Wire `app.rs` update() and add Nerd Mode panel

**Files:**
- Modify: `src/app.rs:1158-1183` (update method)

- [ ] **Step 1: Replace the 5 render lines** in `update()` (currently lines 1178–1182) with:

```rust
        crate::ui::nav_rail::render_nav_rail(ctx, self);

        if self.nerd_mode {
            egui::TopBottomPanel::bottom("nerd_log")
                .resizable(true)
                .default_height(200.0)
                .min_height(80.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            eframe::egui::RichText::new("Raw Log")
                                .strong()
                                .color(crate::ui::theme::TEXT_SECONDARY),
                        );
                        if ui.button("Clear").clicked() {
                            self.log_entries.clear();
                        }
                    });
                    ui.add_space(4.0);
                    eframe::egui::ScrollArea::vertical()
                        .id_salt("nerd_log_scroll")
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            for entry in self.log_entries.iter().rev() {
                                ui.monospace(entry.compact_line());
                            }
                        });
                });
        }

        match self.active_tab {
            crate::app::AppTab::Dashboard => {
                crate::ui::dashboard_page::render_dashboard_page(ctx, self);
            }
            crate::app::AppTab::Backup => {
                crate::ui::backup_page::render_backup_page(ctx, self);
            }
            crate::app::AppTab::Cleanup => {
                crate::ui::cleanup_page::render_cleanup_page(ctx, self);
            }
            crate::app::AppTab::Devices => {
                crate::ui::coming_soon::render_coming_soon_page(ctx, self, "Devices");
            }
            crate::app::AppTab::Settings => {
                crate::ui::coming_soon::render_coming_soon_page(ctx, self, "Settings");
            }
        }

        crate::ui::backup_page::render_remote_folder_picker(ctx, self);
```

- [ ] **Step 2: Verify**

```
cargo check 2>&1 | tail -10
```

Expected: errors about old modules still being declared in mod.rs — that's fine, fixed next task.

- [ ] **Step 3: Commit**

```bash
git add src/app.rs
git commit -m "refactor: wire new page renderers in update(), add nerd mode log panel"
```

---

## Task 9: Remove old files and finalize mod.rs

**Files:**
- Delete: `src/ui/side_panel.rs`
- Delete: `src/ui/central_panel.rs`
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: Replace `src/ui/mod.rs` entirely**

```rust
pub mod theme;
pub mod widgets;
pub mod nav_rail;
pub mod coming_soon;
pub mod dashboard_page;
pub mod backup_page;
pub mod cleanup_page;
```

- [ ] **Step 2: Delete old files**

```bash
rm src/ui/side_panel.rs
rm src/ui/central_panel.rs
```

- [ ] **Step 3: Full build**

```
cargo build 2>&1 | tail -20
```

Expected: clean build, no errors. Address any remaining compile errors before committing.

- [ ] **Step 4: Run the app and visually verify**

```
cargo run
```

Verify:
- Nav rail appears on the left with all 5 items
- Dashboard loads by default with device card, storage bars, action buttons, log
- Clicking Backup shows the 3-column wizard
- Clicking Cleanup shows file browser + right panel
- Clicking Devices / Settings shows "Coming Soon"
- Nerd mode toggle (⌨ Nerd) in nav rail bottom shows/hides raw log panel

- [ ] **Step 5: Commit**

```bash
git add src/ui/mod.rs
git rm src/ui/side_panel.rs src/ui/central_panel.rs
git commit -m "refactor: remove old side_panel and central_panel, finalize mod.rs"
```

---

## Task 10: Version bump, tag, push, and GitHub release

**Files:**
- Modify: `Cargo.toml` (version field)

- [ ] **Step 1: Bump version in `Cargo.toml`** — change `version = "0.4.0"` to:

```toml
version = "0.5.0"
```

- [ ] **Step 2: Final build check**

```
cargo build --release 2>&1 | tail -5
```

Expected: clean build.

- [ ] **Step 3: Commit version bump**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: bump version to v0.5.0"
```

- [ ] **Step 4: Tag and push**

```bash
git tag v0.5.0
git push origin main
git push origin v0.5.0
```

- [ ] **Step 5: Create GitHub release**

```bash
gh release create v0.5.0 \
  --title "v0.5.0: Windows 11 Fluent Design UI Redesign" \
  --notes "$(cat <<'EOF'
## What's New in v0.5.0

### Full Windows 11 Fluent Design Redesign

- **Navigation Rail**: Persistent 88 px left nav with icon + label items for Dashboard, Backup, Cleanup, Devices, and Settings
- **Dashboard**: At-a-glance view with device status, PC storage bar, quick action buttons (Start Backup, Cleanup Phone), and recent log
- **Backup Page**: Redesigned as a 3-column wizard — Step 1 (source folders + presets), Step 2 (destination), Step 3 (preflight check + backup options + actions)
- **Cleanup Page**: File browser with sortable list, checkboxes, and a dedicated right panel for delete options
- **Devices & Settings**: Coming Soon placeholders — nav items are present and active, full functionality in a future release
- **Nerd Mode**: Toggle the ⌨ Nerd button in the nav rail footer to reveal a raw log panel at the bottom of the window

### Notes
- All existing backup and cleanup functionality is preserved
- No changes to settings format — existing config files load as-is
EOF
)"
```

---

## Self-Review Checklist

| Spec requirement | Task |
|---|---|
| Nav rail 88 px, always expanded, icons + labels | Task 4 |
| Active item: ACCENT indicator bar | Task 4 |
| Settings pinned to bottom | Task 4 |
| Hamburger decorative | Task 4 |
| Dashboard: device card + re-scan | Task 5 |
| Dashboard: PC storage bar | Task 5 |
| Dashboard: phone storage placeholder | Task 5 |
| Dashboard: Start New Backup + Cleanup Phone buttons | Task 5 |
| Dashboard: log card (last 10 entries) | Task 5 |
| Backup: 3-column wizard | Task 6 |
| Backup: presets in column 1 | Task 6 |
| Backup: auto-delete toggle bar | Task 6 |
| Backup: progress bar during run | Task 6 |
| Backup: remote folder picker preserved | Task 6 |
| Cleanup: breadcrumb bar | Task 7 |
| Cleanup: file browser with checkboxes | Task 7 |
| Cleanup: right panel with options | Task 7 |
| Cleanup: ARE YOU SURE modal | Uses existing `request_cleanup_delete_*` methods which handle confirmation internally |
| Devices/Settings: Coming Soon | Task 3 |
| Nerd Mode bottom panel | Task 8 |
| AppTab::Dashboard as default | Task 1 |
| Version 0.5.0 + GitHub release | Task 10 |
| Delete side_panel.rs + central_panel.rs | Task 9 |
