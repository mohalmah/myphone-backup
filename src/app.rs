use crate::core::{
    config,
    logging::LogEntry,
    models::{
        BackupAnalysis, BackupPreset, BackupSourceConfig, BackupSourceScan, DeviceConnectionState,
        DeviceInfo, FileRecord, RemoteDirectory, RemoteFile,
        RemoteFolderEntryKind, RemoteFolderPreview, RunSummary, Settings, SyncProgress,
        guess_destination_subfolder, legacy_source_from_path,
    },
    sync::{self, SyncEvent, SyncHandle, SyncPlan},
};
use chrono::Local;
use eframe::egui::{self, Context};
use rfd::FileDialog;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::time::Duration;
use std::{collections::BTreeSet, path::PathBuf};

use crate::ui::widgets::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum RemoteFolderPickerTarget {
    #[default]
    SourceFolder,
    CleanupFolder,
    BackupSource(usize),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AppTab {
    #[default]
    Dashboard,
    Backup,
    Cleanup,
    Devices,
    Settings,
}

#[derive(Default)]
pub(crate) struct RemoteFolderPicker {
    pub(crate) is_open: bool,
    pub(crate) is_loading: bool,
    pub(crate) current_path: String,
    pub(crate) target: RemoteFolderPickerTarget,
    pub(crate) entries: Vec<RemoteDirectory>,
    pub(crate) receiver: Option<Receiver<Result<Vec<RemoteDirectory>, String>>>,
    pub(crate) error: Option<String>,
}

#[derive(Default)]
pub(crate) struct FolderCleanupState {
    pub(crate) folder_path: String,
    pub(crate) preview: Option<RemoteFolderPreview>,
    pub(crate) preview_receiver: Option<Receiver<Result<RemoteFolderPreview, String>>>,
    pub(crate) delete_receiver: Option<Receiver<Result<String, String>>>,
    pub(crate) is_fetching_preview: bool,
    pub(crate) is_deleting: bool,
    pub(crate) preview_error: Option<String>,
    pub(crate) delete_error: Option<String>,
    pub(crate) delete_armed: bool,
    pub(crate) selected_paths: BTreeSet<String>,
}

#[derive(Default)]
pub(crate) struct BackupAnalysisState {
    pub(crate) analysis: Option<BackupAnalysis>,
    pub(crate) receiver: Option<Receiver<Result<BackupAnalysis, String>>>,
    pub(crate) is_loading: bool,
    pub(crate) error: Option<String>,
}

#[derive(Default)]
pub(crate) struct BackupSourceLibraryState {
    pub(crate) scan_receiver: Option<Receiver<Result<Vec<BackupSourceScan>, String>>>,
    pub(crate) is_scanning: bool,
    pub(crate) scan_results: Vec<BackupSourceScan>,
    pub(crate) scan_error: Option<String>,
}

pub struct BackupApp {
    pub(crate) settings: Settings,
    pub(crate) device_info: DeviceInfo,
    pub(crate) device_probe_receiver: Option<Receiver<Result<DeviceInfo, String>>>,
    pub(crate) background_log_sender: Sender<LogEntry>,
    pub(crate) background_log_receiver: Receiver<LogEntry>,
    pub(crate) sync_receiver: Option<Receiver<SyncEvent>>,
    pub(crate) sync_handle: Option<SyncHandle>,
    pub(crate) remote_folder_picker: RemoteFolderPicker,
    pub(crate) folder_cleanup: FolderCleanupState,
    pub(crate) backup_analysis: BackupAnalysisState,
    pub(crate) backup_source_library: BackupSourceLibraryState,
    pub(crate) active_tab: AppTab,
    pub(crate) files: Vec<FileRecord>,
    pub(crate) progress: SyncProgress,
    pub(crate) log_entries: Vec<LogEntry>,
    pub(crate) show_detailed_logs: bool,
    pub(crate) analysis_file_filter: String,
    pub(crate) last_summary: Option<RunSummary>,
    pub(crate) selected_preset_names: Vec<String>,
    pub(crate) preset_name_input: String,
    pub(crate) status_banner: String,
    pub(crate) error_banner: Option<String>,
    pub(crate) nerd_mode: bool,
    pub(crate) last_backup_time: Option<String>,
}

impl BackupApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::ui::theme::apply_theme(&cc.egui_ctx);

        let mut settings = config::load_settings().unwrap_or_default();
        if settings.backup_sources.is_empty() && !settings.source_path.trim().is_empty() {
            settings.backup_sources.push(legacy_source_from_path(
                &settings.source_path,
                &guess_destination_subfolder(&settings.source_path),
            ));
        }
        let initial_cleanup_folder = settings.source_path.clone();
        let (background_log_sender, background_log_receiver) = channel();
        let preset_name_input = settings
            .presets
            .first()
            .map(|preset| preset.name.clone())
            .unwrap_or_else(|| "WhatsApp Videos".to_string());

        let mut app = Self {
            preset_name_input,
            selected_preset_names: Vec::new(),
            settings,
            device_info: DeviceInfo::default(),
            device_probe_receiver: None,
            background_log_sender,
            background_log_receiver,
            sync_receiver: None,
            sync_handle: None,
            remote_folder_picker: RemoteFolderPicker::default(),
            folder_cleanup: FolderCleanupState {
                folder_path: initial_cleanup_folder,
                ..Default::default()
            },
            backup_analysis: BackupAnalysisState::default(),
            backup_source_library: BackupSourceLibraryState::default(),
            active_tab: AppTab::Dashboard,
            files: Vec::new(),
            progress: SyncProgress::default(),
            log_entries: Vec::new(),
            show_detailed_logs: false,
            analysis_file_filter: String::new(),
            last_summary: None,
            status_banner: "Ready to scan your Android device.".to_string(),
            error_banner: None,
            nerd_mode: false,
            last_backup_time: None,
        };

        if config::settings_path().exists() {
            app.push_log("[INFO] Loaded settings from config/settings.json");
        } else {
            app.push_log("[INFO] Using default WhatsApp backup preset");
        }

        app.refresh_backup_source_scan();
        app.refresh_device_info();
        app
    }

    fn push_log_entry(&mut self, entry: LogEntry) {
        self.log_entries.push(entry);
        if self.log_entries.len() > 1_200 {
            let excess = self.log_entries.len() - 1_200;
            self.log_entries.drain(0..excess);
        }
    }

    fn push_log(&mut self, line: impl Into<String>) {
        self.push_log_entry(LogEntry::from_legacy_line(line));
    }

    fn poll_background_logs(&mut self) {
        loop {
            match self.background_log_receiver.try_recv() {
                Ok(entry) => self.push_log_entry(entry),
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
    }

    pub(crate) fn is_running(&self) -> bool {
        self.sync_receiver.is_some()
    }

    pub(crate) fn has_active_adb_job(&self) -> bool {
        self.is_running()
            || self.remote_folder_picker.is_loading
            || self.folder_cleanup.is_fetching_preview
            || self.folder_cleanup.is_deleting
            || self.backup_analysis.is_loading
            || self.backup_source_library.is_scanning
    }

    pub(crate) fn sync_legacy_source_path_from_sources(&mut self) {
        if let Some(source) = self
            .settings
            .backup_sources
            .iter()
            .find(|source| source.enabled && !source.source_path.trim().is_empty())
        {
            self.settings.source_path = source.source_path.clone();
        } else if let Some(source) = self
            .settings
            .backup_sources
            .iter()
            .find(|source| !source.source_path.trim().is_empty())
        {
            self.settings.source_path = source.source_path.clone();
        }
    }

    pub(crate) fn add_custom_backup_source(&mut self) {
        let next_index = self.settings.backup_sources.len() + 1;
        self.settings.backup_sources.push(BackupSourceConfig {
            id: format!("custom-{next_index}"),
            label: format!("Custom Folder {next_index}"),
            source_path: "/sdcard".to_string(),
            destination_subfolder: format!("Custom Folder {next_index}"),
            enabled: true,
            built_in: false,
        });
        self.sync_legacy_source_path_from_sources();
        self.invalidate_backup_analysis();
    }

    pub(crate) fn remove_backup_source(&mut self, index: usize) {
        if index >= self.settings.backup_sources.len() {
            return;
        }
        self.settings.backup_sources.remove(index);
        self.sync_legacy_source_path_from_sources();
        self.invalidate_backup_analysis();
    }

    pub(crate) fn open_backup_source_folder_picker(&mut self, index: usize) {
        let start_path = self
            .settings
            .backup_sources
            .get(index)
            .map(|source| normalize_remote_path(&source.source_path))
            .unwrap_or_else(|| "/sdcard".to_string());
        self.open_remote_folder_picker(RemoteFolderPickerTarget::BackupSource(index), start_path);
    }

    pub(crate) fn refresh_backup_source_scan(&mut self) {
        self.backup_source_library.is_scanning = true;
        self.backup_source_library.scan_error = None;
        self.backup_source_library.scan_receiver = Some(sync::start_backup_source_scan(
            self.settings.clone(),
            self.background_log_sender.clone(),
        ));
        self.status_banner = "Scanning configured backup sources...".to_string();
        self.push_log("[INFO] Scanning configured backup sources");
    }

    fn poll_backup_source_scan(&mut self) {
        let mut outcome = None;
        let mut disconnected = false;

        if let Some(receiver) = &self.backup_source_library.scan_receiver {
            match receiver.try_recv() {
                Ok(result) => outcome = Some(result),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => disconnected = true,
            }
        }

        if let Some(result) = outcome {
            self.backup_source_library.scan_receiver = None;
            self.backup_source_library.is_scanning = false;
            match result {
                Ok(results) => {
                    let available_count = results.iter().filter(|source| source.exists).count();
                    self.backup_source_library.scan_results = results;
                    self.backup_source_library.scan_error = None;
                    self.status_banner = format!(
                        "Source scan complete: {available_count} configured folder(s) available."
                    );
                    self.push_log(format!(
                        "[INFO] Source scan complete: {available_count} configured folder(s) available"
                    ));
                }
                Err(error) => {
                    self.backup_source_library.scan_error = Some(error.clone());
                    self.status_banner = "Backup source scan failed.".to_string();
                    self.push_log(format!("[ERROR] {error}"));
                }
            }
        } else if disconnected {
            self.backup_source_library.scan_receiver = None;
            self.backup_source_library.is_scanning = false;
        }
    }

    pub(crate) fn refresh_device_info(&mut self) {
        self.status_banner = "Checking ADB connection...".to_string();
        self.device_probe_receiver = Some(sync::start_device_probe(
            self.settings.adb_path.clone(),
            self.background_log_sender.clone(),
        ));
    }

    pub(crate) fn invalidate_backup_analysis(&mut self) {
        self.backup_analysis.analysis = None;
        self.backup_analysis.error = None;
        self.analysis_file_filter.clear();
    }

    pub(crate) fn clear_selected_preset_chips(&mut self) {
        self.selected_preset_names.clear();
    }

    pub(crate) fn selected_preset_count(&self) -> usize {
        self.selected_preset_names.len()
    }

    fn preset_by_name(&self, name: &str) -> Option<BackupPreset> {
        self.settings
            .presets
            .iter()
            .find(|preset| preset.name == name)
            .cloned()
    }

    fn preset_sources_for_loading(&self, preset: &BackupPreset) -> Vec<BackupSourceConfig> {
        if preset.sources.is_empty() {
            vec![legacy_source_from_path(
                &preset.source_path,
                &guess_destination_subfolder(&preset.source_path),
            )]
        } else {
            preset.sources.clone()
        }
    }

    fn apply_selected_preset_chips(&mut self) {
        let selected_presets = self
            .selected_preset_names
            .iter()
            .filter_map(|name| self.preset_by_name(name))
            .collect::<Vec<_>>();

        if selected_presets.is_empty() {
            return;
        }

        let mut merged_sources = Vec::new();
        let mut seen_paths = BTreeSet::new();

        for preset in &selected_presets {
            for mut source in self
                .preset_sources_for_loading(preset)
                .into_iter()
                .filter(|source| source.enabled && !source.source_path.trim().is_empty())
            {
                source.enabled = true;
                let dedupe_key = source.source_path.trim().to_lowercase();
                if seen_paths.insert(dedupe_key) {
                    merged_sources.push(source);
                }
            }
        }

        if merged_sources.is_empty() {
            self.status_banner =
                "The selected preset chips did not include any enabled source folders.".to_string();
            return;
        }

        let destination_source = selected_presets
            .first()
            .map(|preset| preset.destination_path.clone())
            .unwrap_or_default();
        let multiple_destinations = selected_presets
            .iter()
            .map(|preset| preset.destination_path.trim().to_lowercase())
            .collect::<BTreeSet<_>>()
            .len()
            > 1;

        self.settings.backup_sources = merged_sources;
        self.settings.destination_path = destination_source;
        self.sync_legacy_source_path_from_sources();
        self.backup_source_library.scan_results.clear();
        self.invalidate_backup_analysis();
        self.refresh_backup_source_scan();
        self.preset_name_input = if selected_presets.len() == 1 {
            selected_presets[0].name.clone()
        } else {
            format!("{} preset mix", selected_presets.len())
        };

        let status = if selected_presets.len() == 1 {
            format!("Loaded preset \"{}\".", selected_presets[0].name)
        } else if multiple_destinations {
            format!(
                "Loaded {} preset chips and merged their source folders. The destination root came from \"{}\".",
                selected_presets.len(),
                selected_presets[0].name
            )
        } else {
            format!(
                "Loaded {} preset chips and merged their source folders.",
                selected_presets.len()
            )
        };

        self.status_banner = status.clone();
        self.push_log(format!("[INFO] {status}"));
    }

    pub(crate) fn toggle_preset_chip_selection(&mut self, preset_name: &str) {
        if let Some(index) = self
            .selected_preset_names
            .iter()
            .position(|name| name == preset_name)
        {
            self.selected_preset_names.remove(index);
            if self.selected_preset_names.is_empty() {
                self.status_banner =
                    "Preset chip selection cleared. Current source library stays editable."
                        .to_string();
                self.push_log(format!("[INFO] Deselected preset chip \"{preset_name}\""));
                return;
            }
        } else {
            self.selected_preset_names.push(preset_name.to_string());
        }

        self.apply_selected_preset_chips();
    }

    pub(crate) fn detach_selected_presets_after_manual_changes(&mut self) {
        if !self.selected_preset_names.is_empty() {
            self.selected_preset_names.clear();
            self.push_log("[INFO] Detached preset chip selection after manual source changes");
            self.status_banner =
                "Preset chips were detached because the source library was edited manually."
                    .to_string();
        }
    }

    fn trigger_backup_analysis_if_ready(&mut self) {
        if self.has_active_adb_job() {
            return;
        }

        let has_source = self
            .settings
            .effective_backup_sources()
            .iter()
            .any(|source| source.enabled && !source.source_path.trim().is_empty());

        if !has_source || self.settings.destination_path.trim().is_empty() {
            return;
        }

        self.request_backup_analysis();
    }

    pub(crate) fn request_backup_analysis(&mut self) {
        self.backup_analysis.is_loading = true;
        self.backup_analysis.error = None;
        self.backup_analysis.receiver = Some(sync::start_backup_analysis(
            self.settings.clone(),
            self.background_log_sender.clone(),
        ));
        self.status_banner = "Analyzing backup source and local space...".to_string();
        self.push_log("[INFO] Running backup preflight analysis");
    }

    fn poll_backup_analysis(&mut self) {
        let mut outcome = None;
        let mut disconnected = false;

        if let Some(receiver) = &self.backup_analysis.receiver {
            match receiver.try_recv() {
                Ok(result) => outcome = Some(result),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => disconnected = true,
            }
        }

        if let Some(result) = outcome {
            self.backup_analysis.receiver = None;
            self.backup_analysis.is_loading = false;
            match result {
                Ok(analysis) => {
                    let total_bytes = analysis.preflight.total_bytes;
                    let total_files = analysis.preflight.total_files;
                    self.backup_analysis.analysis = Some(analysis);
                    self.backup_analysis.error = None;
                    self.status_banner = format!(
                        "Backup analysis ready: {} files, {} total.",
                        total_files,
                        format_bytes(total_bytes)
                    );
                    self.push_log(format!(
                        "[INFO] Backup analysis ready: {} files, {} total",
                        total_files,
                        format_bytes(total_bytes)
                    ));
                }
                Err(error) => {
                    self.backup_analysis.analysis = None;
                    self.backup_analysis.error = Some(error.clone());
                    self.status_banner = "Backup analysis failed.".to_string();
                    self.push_log(format!("[ERROR] {error}"));
                }
            }
        } else if disconnected {
            self.backup_analysis.receiver = None;
            self.backup_analysis.is_loading = false;
        }
    }

    pub(crate) fn pick_local_destination_folder(&mut self) {
        let initial_directory = initial_local_directory(&self.settings.destination_path);
        if let Some(folder) = FileDialog::new()
            .set_directory(initial_directory)
            .pick_folder()
        {
            let selected = folder.to_string_lossy().to_string();
            self.settings.destination_path = selected.clone();
            self.invalidate_backup_analysis();
            self.trigger_backup_analysis_if_ready();
            self.status_banner = format!("Selected local destination folder: {selected}");
            self.push_log(format!(
                "[INFO] Local destination folder selected: {selected}"
            ));
        }
    }

    pub(crate) fn pick_backup_source_destination_folder(&mut self, index: usize) -> bool {
        if index >= self.settings.backup_sources.len() {
            return false;
        }

        let current_subfolder = self.settings.backup_sources[index]
            .destination_subfolder
            .clone();

        let initial_directory = if self.settings.destination_path.trim().is_empty() {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        } else {
            let candidate = PathBuf::from(&self.settings.destination_path).join(&current_subfolder);
            if candidate.is_dir() {
                candidate
            } else {
                initial_local_directory(&self.settings.destination_path)
            }
        };

        let Some(folder) = FileDialog::new()
            .set_directory(initial_directory)
            .pick_folder()
        else {
            return false;
        };

        let destination_subfolder = derive_destination_subfolder(
            &folder,
            &self.settings.destination_path,
            &current_subfolder,
        );

        if let Some(source) = self.settings.backup_sources.get_mut(index) {
            source.destination_subfolder = destination_subfolder.clone();
        }

        self.status_banner = format!("Selected destination subfolder: {destination_subfolder}");
        self.push_log(format!(
            "[INFO] Source destination subfolder selected: {destination_subfolder}"
        ));
        self.invalidate_backup_analysis();
        self.backup_source_library.scan_results.clear();
        true
    }

    pub(crate) fn open_cleanup_folder_picker(&mut self) {
        let seed_path = if self.folder_cleanup.folder_path.trim().is_empty() {
            self.settings.source_path.clone()
        } else {
            self.folder_cleanup.folder_path.clone()
        };
        let start_path = normalize_remote_path(&seed_path);
        self.open_remote_folder_picker(RemoteFolderPickerTarget::CleanupFolder, start_path);
    }

    fn open_remote_folder_picker(&mut self, target: RemoteFolderPickerTarget, start_path: String) {
        self.remote_folder_picker.target = target;
        self.remote_folder_picker.is_open = true;
        self.request_remote_directory_listing(start_path);
    }

    pub(crate) fn request_remote_directory_listing(&mut self, path: String) {
        let normalized_path = normalize_remote_path(&path);
        self.remote_folder_picker.current_path = normalized_path.clone();
        self.remote_folder_picker.entries.clear();
        self.remote_folder_picker.error = None;
        self.remote_folder_picker.is_loading = true;
        self.remote_folder_picker.receiver = Some(sync::start_remote_directory_list(
            self.settings.adb_path.clone(),
            normalized_path,
            self.background_log_sender.clone(),
        ));
    }

    fn poll_remote_folder_picker(&mut self) {
        let mut outcome = None;
        let mut disconnected = false;

        if let Some(receiver) = &self.remote_folder_picker.receiver {
            match receiver.try_recv() {
                Ok(result) => outcome = Some(result),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => disconnected = true,
            }
        }

        if let Some(result) = outcome {
            self.remote_folder_picker.receiver = None;
            self.remote_folder_picker.is_loading = false;
            match result {
                Ok(entries) => {
                    self.remote_folder_picker.entries = entries;
                    self.remote_folder_picker.error = None;
                }
                Err(error) => {
                    self.remote_folder_picker.error = Some(error.clone());
                    self.push_log(format!("[ERROR] {error}"));
                }
            }
        } else if disconnected {
            self.remote_folder_picker.receiver = None;
            self.remote_folder_picker.is_loading = false;
        }
    }

    pub(crate) fn set_cleanup_folder_path(&mut self, path: String) {
        let normalized = normalize_remote_path(&path);
        if self.folder_cleanup.folder_path != normalized {
            self.folder_cleanup.folder_path = normalized;
            self.folder_cleanup.preview = None;
            self.folder_cleanup.preview_receiver = None;
            self.folder_cleanup.is_fetching_preview = false;
            self.folder_cleanup.preview_error = None;
            self.folder_cleanup.delete_error = None;
            self.folder_cleanup.delete_armed = false;
            self.folder_cleanup.selected_paths.clear();
        }
    }

    pub(crate) fn clear_cleanup_preview(&mut self) {
        self.folder_cleanup.preview = None;
        self.folder_cleanup.preview_receiver = None;
        self.folder_cleanup.is_fetching_preview = false;
        self.folder_cleanup.preview_error = None;
        self.folder_cleanup.delete_error = None;
        self.folder_cleanup.delete_armed = false;
        self.folder_cleanup.selected_paths.clear();
    }

    pub(crate) fn cleanup_preview_matches_path(&self) -> bool {
        self.folder_cleanup
            .preview
            .as_ref()
            .map(|preview| {
                preview.root_path == normalize_remote_path(&self.folder_cleanup.folder_path)
            })
            .unwrap_or(false)
    }

    pub(crate) fn selected_cleanup_entries(&self) -> Vec<crate::core::models::RemoteFolderEntry> {
        let Some(preview) = &self.folder_cleanup.preview else {
            return Vec::new();
        };

        let mut entries = preview
            .entries
            .iter()
            .filter(|entry| {
                self.folder_cleanup
                    .selected_paths
                    .contains(&entry.full_path)
            })
            .cloned()
            .collect::<Vec<_>>();

        entries.sort_by(|left, right| {
            left.full_path
                .len()
                .cmp(&right.full_path.len())
                .then_with(|| left.full_path.cmp(&right.full_path))
        });

        let mut filtered = Vec::new();
        for entry in entries {
            let covered_by_parent =
                filtered
                    .iter()
                    .any(|selected: &crate::core::models::RemoteFolderEntry| {
                        matches!(selected.kind, RemoteFolderEntryKind::Directory)
                            && entry
                                .full_path
                                .starts_with(&(selected.full_path.clone() + "/"))
                    });
            if !covered_by_parent {
                filtered.push(entry);
            }
        }

        filtered
    }

    fn begin_cleanup_delete(&mut self) -> Option<String> {
        let path = normalize_remote_path(&self.folder_cleanup.folder_path);

        if !self.cleanup_preview_matches_path() {
            self.folder_cleanup.delete_error =
                Some("Fetch folder contents before deleting.".to_string());
            return None;
        }

        if !self.folder_cleanup.delete_armed {
            self.folder_cleanup.delete_error =
                Some("Arm deletion first by checking the confirmation box.".to_string());
            return None;
        }

        self.folder_cleanup.delete_error = None;
        self.folder_cleanup.is_deleting = true;
        Some(path)
    }

    pub(crate) fn request_cleanup_delete_folder(&mut self) {
        let path = normalize_remote_path(&self.folder_cleanup.folder_path);

        if let Some(reason) = protected_cleanup_folder_reason(&path) {
            self.folder_cleanup.delete_error = Some(reason.to_string());
            return;
        }

        let Some(path) = self.begin_cleanup_delete() else {
            return;
        };

        self.folder_cleanup.delete_receiver = Some(sync::start_remote_folder_delete(
            self.settings.adb_path.clone(),
            path.clone(),
            self.background_log_sender.clone(),
        ));
        self.status_banner = format!("Deleting remote folder {path}...");
        self.push_log(format!("[INFO] Deleting remote folder recursively: {path}"));
    }

    pub(crate) fn request_cleanup_delete_contents_only(&mut self) {
        let path = normalize_remote_path(&self.folder_cleanup.folder_path);

        if let Some(reason) = protected_cleanup_folder_reason(&path) {
            self.folder_cleanup.delete_error = Some(reason.to_string());
            return;
        }

        let Some(path) = self.begin_cleanup_delete() else {
            return;
        };

        self.folder_cleanup.delete_receiver = Some(sync::start_remote_folder_contents_delete(
            self.settings.adb_path.clone(),
            path.clone(),
            self.background_log_sender.clone(),
        ));
        self.status_banner = format!("Deleting folder contents in {path}...");
        self.push_log(format!(
            "[INFO] Deleting folder contents but keeping folder: {path}"
        ));
    }

    pub(crate) fn request_cleanup_delete_selected(&mut self) {
        let Some(_) = self.begin_cleanup_delete() else {
            return;
        };

        let selected_entries = self.selected_cleanup_entries();
        if selected_entries.is_empty() {
            self.folder_cleanup.is_deleting = false;
            self.folder_cleanup.delete_error =
                Some("Select at least one file or folder from the preview first.".to_string());
            return;
        }

        if let Some(reason) = selected_entries
            .iter()
            .find_map(|entry| protected_cleanup_folder_reason(&entry.full_path))
        {
            self.folder_cleanup.is_deleting = false;
            self.folder_cleanup.delete_error = Some(reason.to_string());
            return;
        }

        let selected_count = selected_entries.len();
        self.folder_cleanup.delete_receiver = Some(sync::start_remote_entries_delete(
            self.settings.adb_path.clone(),
            selected_entries,
            self.background_log_sender.clone(),
        ));
        self.status_banner = format!("Deleting {selected_count} selected cleanup item(s)...");
        self.push_log(format!(
            "[INFO] Deleting {selected_count} selected cleanup item(s)"
        ));
    }

    pub(crate) fn request_cleanup_preview(&mut self) {
        if self.folder_cleanup.folder_path.trim().is_empty() {
            self.folder_cleanup.preview_error =
                Some("Choose a phone folder before fetching contents.".to_string());
            return;
        }

        let path = normalize_remote_path(&self.folder_cleanup.folder_path);
        self.folder_cleanup.folder_path = path.clone();
        self.folder_cleanup.preview = None;
        self.folder_cleanup.preview_error = None;
        self.folder_cleanup.delete_error = None;
        self.folder_cleanup.delete_armed = false;
        self.folder_cleanup.selected_paths.clear();
        self.folder_cleanup.is_fetching_preview = true;
        self.folder_cleanup.preview_receiver = Some(sync::start_remote_folder_preview(
            self.settings.adb_path.clone(),
            path.clone(),
            self.background_log_sender.clone(),
        ));
        self.status_banner = format!("Fetching folder contents for {path}...");
        self.push_log(format!("[INFO] Fetching cleanup preview for {path}"));
    }

    fn poll_cleanup_jobs(&mut self) {
        let mut preview_outcome = None;
        let mut preview_disconnected = false;

        if let Some(receiver) = &self.folder_cleanup.preview_receiver {
            match receiver.try_recv() {
                Ok(result) => preview_outcome = Some(result),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => preview_disconnected = true,
            }
        }

        if let Some(result) = preview_outcome {
            self.folder_cleanup.preview_receiver = None;
            self.folder_cleanup.is_fetching_preview = false;
            match result {
                Ok(preview) => {
                    let message = format!(
                        "Fetched cleanup preview: {} files, {} folders.",
                        preview.file_count, preview.directory_count
                    );
                    self.folder_cleanup.preview_error = None;
                    self.folder_cleanup.preview = Some(preview);
                    self.folder_cleanup.selected_paths.clear();
                    self.status_banner = message.clone();
                    self.push_log(format!("[INFO] {message}"));
                }
                Err(error) => {
                    self.folder_cleanup.preview = None;
                    self.folder_cleanup.preview_error = Some(error.clone());
                    self.status_banner = "Failed to fetch folder contents.".to_string();
                    self.push_log(format!("[ERROR] {error}"));
                }
            }
        } else if preview_disconnected {
            self.folder_cleanup.preview_receiver = None;
            self.folder_cleanup.is_fetching_preview = false;
        }

        let mut delete_outcome = None;
        let mut delete_disconnected = false;

        if let Some(receiver) = &self.folder_cleanup.delete_receiver {
            match receiver.try_recv() {
                Ok(result) => delete_outcome = Some(result),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => delete_disconnected = true,
            }
        }

        if let Some(result) = delete_outcome {
            self.folder_cleanup.delete_receiver = None;
            self.folder_cleanup.is_deleting = false;
            self.folder_cleanup.delete_armed = false;
            match result {
                Ok(message) => {
                    self.folder_cleanup.preview = None;
                    self.folder_cleanup.selected_paths.clear();
                    self.folder_cleanup.preview_error = None;
                    self.folder_cleanup.delete_error = None;
                    self.status_banner = message.clone();
                    self.push_log(format!("[INFO] {message}"));
                }
                Err(error) => {
                    self.folder_cleanup.delete_error = Some(error.clone());
                    self.status_banner = "Remote folder delete failed.".to_string();
                    self.push_log(format!("[ERROR] {error}"));
                }
            }
        } else if delete_disconnected {
            self.folder_cleanup.delete_receiver = None;
            self.folder_cleanup.is_deleting = false;
        }
    }

    pub(crate) fn save_settings(&mut self) {
        self.sync_legacy_source_path_from_sources();
        match config::save_settings(&self.settings) {
            Ok(()) => {
                self.status_banner = "Settings saved to config/settings.json".to_string();
                self.push_log("[INFO] Saved settings to config/settings.json");
            }
            Err(error) => {
                let message = format!("Failed to save settings: {error}");
                self.error_banner = Some(message.clone());
                self.push_log(format!("[ERROR] {message}"));
            }
        }
    }

    pub(crate) fn save_current_preset(&mut self) {
        let name = if self.preset_name_input.trim().is_empty() {
            format!("Preset {}", Local::now().format("%Y-%m-%d %H:%M"))
        } else {
            self.preset_name_input.trim().to_string()
        };

        let preset = BackupPreset {
            name: name.clone(),
            source_path: self.settings.source_path.clone(),
            destination_path: self.settings.destination_path.clone(),
            sources: self.settings.backup_sources.clone(),
        };

        if let Some(existing) = self
            .settings
            .presets
            .iter_mut()
            .find(|candidate| candidate.name.eq_ignore_ascii_case(&name))
        {
            *existing = preset;
        } else {
            self.settings.presets.push(preset);
            self.settings
                .presets
                .sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
        }

        self.selected_preset_names = vec![name.clone()];
        self.preset_name_input = name.clone();
        self.save_settings();
        self.status_banner = format!("Preset \"{name}\" saved.");
    }

    fn start_sync(&mut self, plan: SyncPlan, reset_state: bool) {
        if self.is_running() {
            return;
        }

        self.save_settings();
        self.error_banner = None;

        if reset_state {
            self.files.clear();
            self.progress = SyncProgress::default();
            self.last_summary = None;
            self.log_entries.clear();
            self.push_log("[INFO] Starting backup run");
        }

        let session = sync::start_sync(self.settings.clone(), plan);
        self.sync_receiver = Some(session.receiver);
        self.sync_handle = Some(session.handle);
        self.status_banner = "Backup in progress...".to_string();
    }

    pub(crate) fn start_full_backup(&mut self) {
        self.start_sync(SyncPlan::FullScan, true);
    }

    pub(crate) fn start_retry(&mut self, file: RemoteFile) {
        self.push_log(format!("[INFO] Retrying {}", file.name));
        self.start_sync(SyncPlan::RetrySingle(file), false);
    }

    fn upsert_file(&mut self, record: FileRecord) {
        if let Some(existing) = self
            .files
            .iter_mut()
            .find(|candidate| candidate.remote_path == record.remote_path)
        {
            *existing = record;
        } else {
            self.files.push(record);
            self.files
                .sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
        }
    }

    fn handle_sync_event(&mut self, event: SyncEvent) -> bool {
        match event {
            SyncEvent::Log(entry) => self.push_log_entry(entry),
            SyncEvent::Device(info) => self.device_info = info,
            SyncEvent::Analysis(analysis) => {
                self.backup_analysis.analysis = Some(analysis);
                self.backup_analysis.error = None;
            }
            SyncEvent::FileUpdated(record) => self.upsert_file(record),
            SyncEvent::Progress(progress) => self.progress = progress,
            SyncEvent::FatalError(message) => {
                self.error_banner = Some(message.clone());
                self.push_log(format!("[ERROR] {message}"));
            }
            SyncEvent::Finished(summary) => {
                self.last_summary = Some(summary.clone());
                self.last_backup_time = Some(chrono::Local::now().format("%A at %I:%M %p").to_string());
                if summary.cancelled {
                    self.status_banner = "Run stopped.".to_string();
                } else if summary.failed > 0 || summary.conflicts > 0 {
                    self.status_banner = "Run completed with issues.".to_string();
                } else {
                    self.status_banner = "Run completed successfully.".to_string();
                }
                return true;
            }
        }

        false
    }

    fn poll_device_probe(&mut self) {
        let mut outcome = None;
        let mut disconnected = false;

        if let Some(receiver) = &self.device_probe_receiver {
            match receiver.try_recv() {
                Ok(result) => outcome = Some(result),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => disconnected = true,
            }
        }

        if let Some(result) = outcome {
            self.device_probe_receiver = None;
            match result {
                Ok(info) => {
                    self.device_info = info.clone();
                    self.status_banner = match info.state {
                        DeviceConnectionState::Connected => {
                            "Device connected. Ready to back up.".to_string()
                        }
                        DeviceConnectionState::Unauthorized => {
                            "Device found, but ADB authorization is pending.".to_string()
                        }
                        DeviceConnectionState::Offline => {
                            "Device detected, but ADB reports it as offline.".to_string()
                        }
                        DeviceConnectionState::Disconnected => {
                            "No Android device detected.".to_string()
                        }
                    };
                }
                Err(error) => {
                    self.error_banner = Some(error.clone());
                    self.status_banner = "ADB is not ready yet.".to_string();
                    self.push_log(format!("[ERROR] {error}"));
                }
            }
        } else if disconnected {
            self.device_probe_receiver = None;
        }
    }

    fn poll_sync_events(&mut self) {
        let mut events = Vec::new();
        let mut disconnected = false;

        if let Some(receiver) = &self.sync_receiver {
            loop {
                match receiver.try_recv() {
                    Ok(event) => events.push(event),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }

        let mut finished = false;
        for event in events {
            finished |= self.handle_sync_event(event);
        }

        if finished || disconnected {
            self.sync_receiver = None;
            self.sync_handle = None;
        }
    }

    pub(crate) fn apply_remote_folder_picker_selection(
        &mut self,
        current_path: String,
        picker_target: RemoteFolderPickerTarget,
    ) {
        match picker_target {
            RemoteFolderPickerTarget::SourceFolder => {
                self.settings.source_path = current_path.clone();
                self.invalidate_backup_analysis();
                self.trigger_backup_analysis_if_ready();
                self.status_banner = format!("Selected phone source folder: {current_path}");
                self.push_log(format!(
                    "[INFO] Phone source folder selected: {current_path}"
                ));
            }
            RemoteFolderPickerTarget::CleanupFolder => {
                self.set_cleanup_folder_path(current_path.clone());
                self.status_banner = format!("Selected cleanup folder: {current_path}");
                self.push_log(format!("[INFO] Cleanup folder selected: {current_path}"));
            }
            RemoteFolderPickerTarget::BackupSource(index) => {
                if let Some(source) = self.settings.backup_sources.get_mut(index) {
                    source.source_path = current_path.clone();
                    if source.destination_subfolder.trim().is_empty() {
                        source.destination_subfolder =
                            guess_destination_subfolder(&current_path);
                    }
                    let source_label = source.label.clone();
                    let _ = source;
                    self.sync_legacy_source_path_from_sources();
                    self.invalidate_backup_analysis();
                    self.status_banner =
                        format!("Selected backup source folder: {current_path}");
                    self.push_log(format!(
                        "[INFO] Backup source folder selected for {}: {current_path}",
                        source_label
                    ));
                }
            }
        }
        self.remote_folder_picker.is_open = false;
    }
}

impl eframe::App for BackupApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // Match Fluent BG_BASE (243, 243, 243)
        [243.0 / 255.0, 243.0 / 255.0, 243.0 / 255.0, 1.0]
    }

    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.poll_background_logs();
        self.poll_device_probe();
        self.poll_sync_events();
        self.poll_remote_folder_picker();
        self.poll_cleanup_jobs();
        self.poll_backup_analysis();
        self.poll_backup_source_scan();

        if self.sync_receiver.is_some()
            || self.device_probe_receiver.is_some()
            || self.remote_folder_picker.receiver.is_some()
            || self.folder_cleanup.preview_receiver.is_some()
            || self.folder_cleanup.delete_receiver.is_some()
            || self.backup_analysis.receiver.is_some()
            || self.backup_source_library.scan_receiver.is_some()
        {
            ctx.request_repaint_after(Duration::from_millis(200));
        }

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
    }
}

