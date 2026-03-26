use crate::core::{
    config,
    logging::{LogEntry, LogLevel},
    models::{
        BackupAnalysis, BackupPreset, BackupSourceConfig, BackupSourceScan, DeviceConnectionState,
        DeviceInfo, ExistingFileBehavior, FileRecord, RemoteDirectory, RemoteFile,
        RemoteFolderEntryKind, RemoteFolderPreview, RunSummary, Settings, SyncProgress,
        ValidationMode, guess_destination_subfolder, legacy_source_from_path,
    },
    sync::{self, SyncEvent, SyncHandle, SyncPlan},
};
use arabic_reshaper::arabic_reshape;
use chrono::Local;
use eframe::egui::{
    self, Align, Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, Frame,
    Layout, Margin, RichText, ScrollArea, Stroke,
};
use rfd::FileDialog;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::time::Duration;
use std::{collections::BTreeSet, path::PathBuf};
use unicode_bidi::BidiInfo;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum RemoteFolderPickerTarget {
    #[default]
    SourceFolder,
    CleanupFolder,
    BackupSource(usize),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum AppTab {
    #[default]
    Backup,
    Cleanup,
}

#[derive(Default)]
struct RemoteFolderPicker {
    is_open: bool,
    is_loading: bool,
    current_path: String,
    target: RemoteFolderPickerTarget,
    entries: Vec<RemoteDirectory>,
    receiver: Option<Receiver<Result<Vec<RemoteDirectory>, String>>>,
    error: Option<String>,
}

#[derive(Default)]
struct FolderCleanupState {
    folder_path: String,
    preview: Option<RemoteFolderPreview>,
    preview_receiver: Option<Receiver<Result<RemoteFolderPreview, String>>>,
    delete_receiver: Option<Receiver<Result<String, String>>>,
    is_fetching_preview: bool,
    is_deleting: bool,
    preview_error: Option<String>,
    delete_error: Option<String>,
    delete_armed: bool,
    selected_paths: BTreeSet<String>,
}

#[derive(Default)]
struct BackupAnalysisState {
    analysis: Option<BackupAnalysis>,
    receiver: Option<Receiver<Result<BackupAnalysis, String>>>,
    is_loading: bool,
    error: Option<String>,
}

#[derive(Default)]
struct BackupSourceLibraryState {
    scan_receiver: Option<Receiver<Result<Vec<BackupSourceScan>, String>>>,
    is_scanning: bool,
    scan_results: Vec<BackupSourceScan>,
    scan_error: Option<String>,
}

pub struct BackupApp {
    settings: Settings,
    device_info: DeviceInfo,
    device_probe_receiver: Option<Receiver<Result<DeviceInfo, String>>>,
    background_log_sender: Sender<LogEntry>,
    background_log_receiver: Receiver<LogEntry>,
    sync_receiver: Option<Receiver<SyncEvent>>,
    sync_handle: Option<SyncHandle>,
    remote_folder_picker: RemoteFolderPicker,
    folder_cleanup: FolderCleanupState,
    backup_analysis: BackupAnalysisState,
    backup_source_library: BackupSourceLibraryState,
    active_tab: AppTab,
    files: Vec<FileRecord>,
    progress: SyncProgress,
    log_entries: Vec<LogEntry>,
    show_detailed_logs: bool,
    analysis_file_filter: String,
    last_summary: Option<RunSummary>,
    selected_preset_names: Vec<String>,
    preset_name_input: String,
    status_banner: String,
    error_banner: Option<String>,
}

impl BackupApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        apply_theme(&cc.egui_ctx);

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
            active_tab: AppTab::Backup,
            files: Vec::new(),
            progress: SyncProgress::default(),
            log_entries: Vec::new(),
            show_detailed_logs: false,
            analysis_file_filter: String::new(),
            last_summary: None,
            status_banner: "Ready to scan your Android device.".to_string(),
            error_banner: None,
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

    fn is_running(&self) -> bool {
        self.sync_receiver.is_some()
    }

    fn has_active_adb_job(&self) -> bool {
        self.is_running()
            || self.remote_folder_picker.is_loading
            || self.folder_cleanup.is_fetching_preview
            || self.folder_cleanup.is_deleting
            || self.backup_analysis.is_loading
            || self.backup_source_library.is_scanning
    }

    fn sync_legacy_source_path_from_sources(&mut self) {
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

    fn add_custom_backup_source(&mut self) {
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

    fn remove_backup_source(&mut self, index: usize) {
        if index >= self.settings.backup_sources.len() {
            return;
        }
        self.settings.backup_sources.remove(index);
        self.sync_legacy_source_path_from_sources();
        self.invalidate_backup_analysis();
    }

    fn open_backup_source_folder_picker(&mut self, index: usize) {
        let start_path = self
            .settings
            .backup_sources
            .get(index)
            .map(|source| normalize_remote_path(&source.source_path))
            .unwrap_or_else(|| "/sdcard".to_string());
        self.open_remote_folder_picker(RemoteFolderPickerTarget::BackupSource(index), start_path);
    }

    fn refresh_backup_source_scan(&mut self) {
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

    fn refresh_device_info(&mut self) {
        self.status_banner = "Checking ADB connection...".to_string();
        self.device_probe_receiver = Some(sync::start_device_probe(
            self.settings.adb_path.clone(),
            self.background_log_sender.clone(),
        ));
    }

    fn invalidate_backup_analysis(&mut self) {
        self.backup_analysis.analysis = None;
        self.backup_analysis.error = None;
        self.analysis_file_filter.clear();
    }

    fn clear_selected_preset_chips(&mut self) {
        self.selected_preset_names.clear();
    }

    fn selected_preset_count(&self) -> usize {
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

    fn toggle_preset_chip_selection(&mut self, preset_name: &str) {
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

    fn detach_selected_presets_after_manual_changes(&mut self) {
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

    fn request_backup_analysis(&mut self) {
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

    fn pick_local_destination_folder(&mut self) {
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

    fn pick_backup_source_destination_folder(&mut self, index: usize) -> bool {
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

    fn open_cleanup_folder_picker(&mut self) {
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

    fn request_remote_directory_listing(&mut self, path: String) {
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

    fn set_cleanup_folder_path(&mut self, path: String) {
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

    fn clear_cleanup_preview(&mut self) {
        self.folder_cleanup.preview = None;
        self.folder_cleanup.preview_receiver = None;
        self.folder_cleanup.is_fetching_preview = false;
        self.folder_cleanup.preview_error = None;
        self.folder_cleanup.delete_error = None;
        self.folder_cleanup.delete_armed = false;
        self.folder_cleanup.selected_paths.clear();
    }

    fn cleanup_preview_matches_path(&self) -> bool {
        self.folder_cleanup
            .preview
            .as_ref()
            .map(|preview| {
                preview.root_path == normalize_remote_path(&self.folder_cleanup.folder_path)
            })
            .unwrap_or(false)
    }

    fn selected_cleanup_entries(&self) -> Vec<crate::core::models::RemoteFolderEntry> {
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

    fn request_cleanup_delete_folder(&mut self) {
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

    fn request_cleanup_delete_contents_only(&mut self) {
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

    fn request_cleanup_delete_selected(&mut self) {
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

    fn request_cleanup_preview(&mut self) {
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

    fn save_settings(&mut self) {
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

    fn save_current_preset(&mut self) {
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

    fn start_full_backup(&mut self) {
        self.start_sync(SyncPlan::FullScan, true);
    }

    fn start_retry(&mut self, file: RemoteFile) {
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

    fn show_remote_folder_picker(&mut self, ctx: &Context) {
        if !self.remote_folder_picker.is_open {
            return;
        }

        let current_path = self.remote_folder_picker.current_path.clone();
        let picker_target = self.remote_folder_picker.target;
        let entries = self.remote_folder_picker.entries.clone();
        let error = self.remote_folder_picker.error.clone();
        let is_loading = self.remote_folder_picker.is_loading;
        let can_go_up = parent_remote_path(&current_path).is_some();
        let mut window_open = self.remote_folder_picker.is_open;
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

                if ui.button("Use This Folder").clicked() {
                    select_current = true;
                }
            });

            ui.add_space(10.0);

            if is_loading {
                ui.spinner();
                ui.label("Loading folders from device...");
            } else if let Some(error) = &error {
                ui.colored_label(Color32::from_rgb(168, 52, 33), error);
            } else if entries.is_empty() {
                ui.label("No subfolders found here. You can still use the current folder.");
            }

            ScrollArea::vertical()
                .id_salt("remote_folder_picker_scroll")
                .max_height(260.0)
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    for directory in &entries {
                        let label = format!("[Dir] {}", directory.name);
                        if ui
                            .add_enabled(!is_loading, egui::Button::new(label))
                            .clicked()
                        {
                            navigate_to = Some(directory.full_path.clone());
                        }
                        wrapped_path_text(ui, &directory.full_path);
                        ui.add_space(6.0);
                    }
                });
        });

        self.remote_folder_picker.is_open = window_open;

        if let Some(path) = navigate_to {
            self.request_remote_directory_listing(path);
        } else if go_up {
            if let Some(parent) = parent_remote_path(&current_path) {
                self.request_remote_directory_listing(parent);
            }
        } else if refresh_listing {
            self.request_remote_directory_listing(current_path.clone());
        } else if select_current {
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
}

impl eframe::App for BackupApp {
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

        egui::TopBottomPanel::top("hero").show(ctx, |ui| {
            Frame::new()
                .fill(Color32::from_rgb(241, 234, 218))
                .inner_margin(Margin::same(14))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new("ADB Smart Backup & Cleanup")
                                    .heading()
                                    .strong()
                                    .color(Color32::from_rgb(50, 43, 34)),
                            );
                            ui.label(
                                RichText::new(
                                    "Safe per-file backup, validation, and optional cleanup for Android media folders.",
                                )
                                .color(Color32::from_rgb(93, 81, 66)),
                            );
                        });

                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            status_pill(
                                ui,
                                if self.is_running() { "RUNNING" } else { "IDLE" },
                                if self.is_running() {
                                    Color32::from_rgb(198, 106, 44)
                                } else {
                                    Color32::from_rgb(73, 121, 92)
                                },
                            );
                            status_pill(
                                ui,
                                self.device_info.state.label(),
                                self.device_info.state.color(),
                            );
                        });
                    });
                    ui.add_space(10.0);
                    ui.label(RichText::new(&self.status_banner).color(Color32::from_rgb(72, 62, 50)));
                    if let Some(error) = &self.error_banner {
                        ui.add_space(8.0);
                        ui.colored_label(Color32::from_rgb(168, 52, 33), error);
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(self.active_tab == AppTab::Backup, "Backup")
                            .clicked()
                        {
                            self.active_tab = AppTab::Backup;
                        }
                        if ui
                            .selectable_label(self.active_tab == AppTab::Cleanup, "Cleanup")
                            .clicked()
                        {
                            self.active_tab = AppTab::Cleanup;
                        }
                    });
                });
        });

        egui::SidePanel::left("settings_panel")
            .resizable(true)
            .min_width(320.0)
            .default_width(420.0)
            .max_width(520.0)
            .show(ctx, |ui| {
                let adb_job_active = self.has_active_adb_job();
                ScrollArea::vertical()
                    .id_salt("settings_panel_scroll")
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        if self.active_tab == AppTab::Backup {
                            // ── Quick Presets (always visible at the top) ──
                            ui.add_space(4.0);
                            ui.label(RichText::new("Quick Presets").strong().size(14.0));
                            ui.add_space(4.0);

                            let presets = self.settings.presets.clone();
                            ui.horizontal_wrapped(|ui| {
                                for preset in &presets {
                                    let is_selected = self
                                        .selected_preset_names
                                        .iter()
                                        .any(|name| name == &preset.name);
                                    let response = render_preset_chip(ui, preset, is_selected);
                                    if response.clicked() {
                                        self.toggle_preset_chip_selection(&preset.name);
                                    }
                                }
                            });

                            ui.add_space(4.0);
                            if self.selected_preset_count() > 0 {
                                ui.horizontal_wrapped(|ui| {
                                    ui.small(format!(
                                        "{} active: {}",
                                        self.selected_preset_count(),
                                        self.selected_preset_names.join(", ")
                                    ));
                                    if ui.small_button("Clear").clicked() {
                                        self.clear_selected_preset_chips();
                                        self.status_banner =
                                            "Preset chip selection cleared. The current source library stays loaded."
                                                .to_string();
                                    }
                                });
                            }

                            ui.add_space(4.0);
                            ui.horizontal_wrapped(|ui| {
                                if ui.small_button("Save Settings").clicked() {
                                    self.save_settings();
                                }
                                if ui.small_button("Save as Preset").clicked() {
                                    self.save_current_preset();
                                }
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.preset_name_input)
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
                                egui::TextEdit::singleline(&mut self.settings.adb_path)
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
                                self.refresh_device_info();
                            }

                            ui.add_space(10.0);
                            ui.label(RichText::new("Current Device").strong());
                            wrapped_text(ui, &device_summary(&self.device_info));
                        });

                        if self.active_tab == AppTab::Backup {
                        settings_card(ui, "Backup Destination", |ui| {
                            ui.label("Local destination folder");
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut self.settings.destination_path)
                                        .desired_width(f32::INFINITY),
                                )
                                .changed()
                            {
                                self.invalidate_backup_analysis();
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
                                self.pick_local_destination_folder();
                            }
                            ui.small("This is the root backup folder. Each selected source can keep its own subfolder inside it.");
                            ui.add_space(8.0);
                            wrapped_text(
                                ui,
                                &format!(
                                    "{} source folder(s) selected for backup",
                                    self.settings
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
                                self.refresh_backup_source_scan();
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
                                self.request_backup_analysis();
                            }
                            if self.backup_analysis.is_loading {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label("Analyzing...");
                                });
                            }
                            if let Some(error) = &self.backup_analysis.error {
                                ui.colored_label(Color32::from_rgb(168, 52, 33), error);
                            }
                        });
                        }

                        if self.active_tab == AppTab::Cleanup {
                        settings_card(ui, "Cleanup Folder", |ui| {
                            ui.label("Phone folder to clean up");
                            let mut cleanup_path = self.folder_cleanup.folder_path.clone();
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut cleanup_path)
                                        .desired_width(f32::INFINITY),
                                )
                                .changed()
                            {
                                self.set_cleanup_folder_path(cleanup_path);
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
                                self.open_cleanup_folder_picker();
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
                                    self.request_cleanup_preview();
                                }
                                if columns[1]
                                    .add_enabled_ui(!self.folder_cleanup.is_deleting, |ui| {
                                        ui.add_sized(
                                            [ui.available_width(), 32.0],
                                            egui::Button::new("Clear Preview"),
                                        )
                                    })
                                    .inner
                                    .clicked()
                                {
                                    self.clear_cleanup_preview();
                                }
                            });

                            if let Some(reason) =
                                protected_cleanup_folder_reason(&self.folder_cleanup.folder_path)
                            {
                                ui.colored_label(Color32::from_rgb(168, 52, 33), reason);
                            }

                            if self.folder_cleanup.is_fetching_preview {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label("Fetching folder contents from device...");
                                });
                            }

                            if let Some(preview) = &self.folder_cleanup.preview {
                                wrapped_text(ui, &cleanup_summary(preview));
                            }
                            if let Some(error) = &self.folder_cleanup.preview_error {
                                ui.colored_label(Color32::from_rgb(168, 52, 33), error);
                            }
                            if let Some(error) = &self.folder_cleanup.delete_error {
                                ui.colored_label(Color32::from_rgb(168, 52, 33), error);
                            }
                        });

                        settings_card(ui, "Delete Actions", |ui| {
                            ui.checkbox(
                                &mut self.folder_cleanup.delete_armed,
                                "I understand these cleanup actions permanently delete items on the phone",
                            );

                            let preview_matches_path = self.cleanup_preview_matches_path();
                            let selected_entries = self.selected_cleanup_entries();
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
                                && self.folder_cleanup.delete_armed
                                && !adb_job_active
                                && protected_cleanup_folder_reason(
                                    &self.folder_cleanup.folder_path,
                                )
                                .is_none();
                            let selected_delete_allowed = preview_matches_path
                                && self.folder_cleanup.delete_armed
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
                                self.request_cleanup_delete_folder();
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
                                self.request_cleanup_delete_contents_only();
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
                                self.request_cleanup_delete_selected();
                            }

                            if self.folder_cleanup.is_deleting {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label("Running cleanup delete on device...");
                                });
                            }
                        });
                        }

                        if self.active_tab == AppTab::Backup {
                        settings_card(ui, "Validation", |ui| {
                            egui::ComboBox::from_label("Validation mode")
                                .selected_text(self.settings.validation_mode.label())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut self.settings.validation_mode,
                                        ValidationMode::Size,
                                        ValidationMode::Size.label(),
                                    );
                                    ui.selectable_value(
                                        &mut self.settings.validation_mode,
                                        ValidationMode::Md5,
                                        ValidationMode::Md5.label(),
                                    );
                                });

                            egui::ComboBox::from_label("Existing local files")
                                .selected_text(self.settings.existing_file_behavior.label())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut self.settings.existing_file_behavior,
                                        ExistingFileBehavior::Skip,
                                        ExistingFileBehavior::Skip.label(),
                                    );
                                    ui.selectable_value(
                                        &mut self.settings.existing_file_behavior,
                                        ExistingFileBehavior::Validate,
                                        ExistingFileBehavior::Validate.label(),
                                    );
                                });

                            ui.checkbox(
                                &mut self.settings.auto_delete_after_success,
                                "Auto delete on device after successful validation",
                            );
                            ui.checkbox(&mut self.settings.dry_run, "Dry-run mode (simulate only)");

                            let mut filter_recent = self.settings.only_last_days.is_some();
                            if ui
                                .checkbox(&mut filter_recent, "Copy only recent files")
                                .changed()
                            {
                                self.settings.only_last_days =
                                    if filter_recent { Some(7) } else { None };
                            }
                            if let Some(days) = &mut self.settings.only_last_days {
                                ui.horizontal(|ui| {
                                    ui.label("Days");
                                    ui.add(egui::DragValue::new(days).range(1..=365));
                                });
                            }
                        });

                        settings_card(ui, "Run Controls", |ui| {
                            ui.horizontal(|ui| {
                                if ui
                                    .add_enabled(
                                        !adb_job_active,
                                        egui::Button::new("Start Backup"),
                                    )
                                    .clicked()
                                {
                                    self.start_full_backup();
                                }

                                let paused = self
                                    .sync_handle
                                    .as_ref()
                                    .map(SyncHandle::is_paused)
                                    .unwrap_or(false);
                                if ui
                                    .add_enabled(
                                        self.is_running(),
                                        egui::Button::new(if paused { "Resume" } else { "Pause" }),
                                    )
                                    .clicked()
                                {
                                    if let Some(handle) = &self.sync_handle {
                                        handle.toggle_pause();
                                    }
                                }

                                if ui
                                    .add_enabled(self.is_running(), egui::Button::new("Stop"))
                                    .clicked()
                                {
                                    if let Some(handle) = &self.sync_handle {
                                        handle.request_stop();
                                    }
                                }
                            });

                            ui.add_space(8.0);
                            ui.label(
                                "Deletion is always per-file and only attempted after validation passes.",
                            );
                        });
                        }
                    });
            });

        egui::TopBottomPanel::bottom("log_panel")
            .resizable(true)
            .default_height(230.0)
            .show(ctx, |ui| {
                let visible_log_count = self
                    .log_entries
                    .iter()
                    .filter(|entry| self.show_detailed_logs || !entry.detailed_only)
                    .count();

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Activity Log").strong());
                    ui.checkbox(&mut self.show_detailed_logs, "Show very detailed logs");
                    if ui.button("Clear").clicked() {
                        self.log_entries.clear();
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!(
                                "{} shown / {} total",
                                visible_log_count,
                                self.log_entries.len()
                            ))
                            .color(Color32::from_rgb(118, 104, 85)),
                        );
                    });
                });
                ui.add_space(6.0);
                ScrollArea::vertical()
                    .id_salt("activity_log_scroll")
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        for entry in self.log_entries.iter().rev() {
                            if !self.show_detailed_logs && entry.detailed_only {
                                continue;
                            }

                            if self.show_detailed_logs {
                                render_detailed_log_entry(ui, entry);
                                ui.add_space(8.0);
                            } else {
                                ui.monospace(entry.compact_line());
                            }
                        }
                    });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let total_progress = if self.progress.total_files == 0 {
                0.0
            } else {
                self.progress.completed_files as f32 / self.progress.total_files as f32
            };
            let mut backup_source_to_remove = None;
            let mut backup_source_to_pick = None;
            let mut backup_source_destination_to_pick = None;
            let mut backup_sources_changed = false;

            if self.active_tab == AppTab::Backup {
                summary_strip(ui, &self.progress, self.last_summary.as_ref());
                ui.add_space(10.0);

                Frame::new()
                    .fill(Color32::WHITE)
                    .stroke(Stroke::new(1.0, Color32::from_rgb(221, 211, 190)))
                    .corner_radius(CornerRadius::same(14))
                    .inner_margin(Margin::same(14))
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("Backup Source Library").strong());
                            if ui
                                .add_enabled(
                                    !self.has_active_adb_job(),
                                    egui::Button::new("Add Custom Source"),
                                )
                                .clicked()
                            {
                                self.add_custom_backup_source();
                                backup_sources_changed = true;
                            }
                            if ui
                                .add_enabled(
                                    !self.has_active_adb_job(),
                                    egui::Button::new("Scan Sources"),
                                )
                                .clicked()
                            {
                                self.refresh_backup_source_scan();
                            }
                            if self.backup_source_library.is_scanning {
                                ui.spinner();
                                ui.label("Scanning...");
                            }
                        });
                        ui.add_space(6.0);

                        // ── Destination folder row with folder selector ──
                        ui.horizontal(|ui| {
                            ui.label("Destination:");
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut self.settings.destination_path)
                                        .desired_width(ui.available_width() - 120.0),
                                )
                                .changed()
                            {
                                self.invalidate_backup_analysis();
                            }
                            if ui
                                .add_enabled(
                                    !self.has_active_adb_job(),
                                    egui::Button::new("Browse..."),
                                )
                                .clicked()
                            {
                                self.pick_local_destination_folder();
                            }
                        });
                        ui.add_space(8.0);

                        if let Some(error) = &self.backup_source_library.scan_error {
                            ui.colored_label(Color32::from_rgb(168, 52, 33), error);
                            ui.add_space(8.0);
                        }

                        ScrollArea::vertical()
                            .id_salt("backup_source_library_scroll")
                            .max_height(320.0)
                            .auto_shrink([false; 2])
                            .show(ui, |ui| {
                                let source_actions_enabled = !self.has_active_adb_job();
                                for (index, source) in self.settings.backup_sources.iter_mut().enumerate() {
                                    let scan = self
                                        .backup_source_library
                                        .scan_results
                                        .iter()
                                        .find(|scan| scan.id == source.id);

                                    Frame::new()
                                        .fill(Color32::from_rgb(250, 247, 240))
                                        .stroke(Stroke::new(
                                            1.0,
                                            Color32::from_rgb(228, 219, 203),
                                        ))
                                        .corner_radius(CornerRadius::same(12))
                                        .inner_margin(Margin::same(12))
                                        .show(ui, |ui| {
                                            ui.horizontal_wrapped(|ui| {
                                                if ui.checkbox(&mut source.enabled, "").changed() {
                                                    backup_sources_changed = true;
                                                }
                                                if ui
                                                    .add(
                                                        egui::TextEdit::singleline(&mut source.label)
                                                            .desired_width(180.0),
                                                    )
                                                    .changed()
                                                {
                                                    backup_sources_changed = true;
                                                }
                                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                                    if ui
                                                        .add_enabled(
                                                            source_actions_enabled,
                                                            egui::Button::new("Remove"),
                                                        )
                                                        .clicked()
                                                    {
                                                        backup_source_to_remove = Some(index);
                                                    }
                                                    if ui
                                                        .add_enabled(
                                                            source_actions_enabled,
                                                            egui::Button::new("Pick Folder"),
                                                        )
                                                        .clicked()
                                                    {
                                                        backup_source_to_pick = Some(index);
                                                    }
                                                });
                                            });
                                            ui.add_space(6.0);

                                            if ui
                                                .add(
                                                    egui::TextEdit::singleline(&mut source.source_path)
                                                        .hint_text("/sdcard/...")
                                                        .desired_width(f32::INFINITY),
                                                )
                                                .changed()
                                            {
                                                backup_sources_changed = true;
                                            }
                                            ui.add_space(6.0);
                                            ui.horizontal_wrapped(|ui| {
                                                ui.label("Destination subfolder");
                                                if ui
                                                    .add(
                                                        egui::TextEdit::singleline(
                                                            &mut source.destination_subfolder,
                                                        )
                                                        .desired_width(220.0),
                                                    )
                                                    .changed()
                                                {
                                                    backup_sources_changed = true;
                                                }
                                                if ui
                                                    .add_enabled(
                                                        source_actions_enabled,
                                                        egui::Button::new("Pick Destination"),
                                                    )
                                                    .clicked()
                                                {
                                                    backup_source_destination_to_pick = Some(index);
                                                }
                                            });
                                            ui.add_space(6.0);

                                            if let Some(scan) = scan {
                                                if scan.exists {
                                                    wrapped_text(
                                                        ui,
                                                        &format!(
                                                            "{} file(s) | {} | copies into {}",
                                                            scan.file_count,
                                                            format_bytes(scan.total_bytes),
                                                            if source.destination_subfolder.trim().is_empty() {
                                                                "root".to_string()
                                                            } else {
                                                                source.destination_subfolder.clone()
                                                            }
                                                        ),
                                                    );
                                                } else if let Some(error) = &scan.error {
                                                    ui.colored_label(
                                                        Color32::from_rgb(168, 52, 33),
                                                        error,
                                                    );
                                                }
                                            } else {
                                                ui.small("Not scanned yet.");
                                            }
                                        });
                                    ui.add_space(10.0);
                                }
                            });
                    });

                if let Some(index) = backup_source_to_remove {
                    self.remove_backup_source(index);
                }
                if let Some(index) = backup_source_to_pick {
                    self.open_backup_source_folder_picker(index);
                }
                if let Some(index) = backup_source_destination_to_pick {
                    backup_sources_changed =
                        self.pick_backup_source_destination_folder(index) || backup_sources_changed;
                }
                if backup_sources_changed {
                    self.detach_selected_presets_after_manual_changes();
                    self.sync_legacy_source_path_from_sources();
                    self.backup_source_library.scan_results.clear();
                    self.invalidate_backup_analysis();
                }
                ui.add_space(14.0);
            }

            if self.active_tab == AppTab::Backup {
            Frame::new()
                .fill(Color32::from_rgb(250, 247, 240))
                .stroke(Stroke::new(1.0, Color32::from_rgb(221, 211, 190)))
                .corner_radius(CornerRadius::same(14))
                .inner_margin(Margin::same(14))
                .show(ui, |ui| {
                    ui.label(RichText::new("Progress").strong());
                    ui.add_space(8.0);
                    ui.add(
                        egui::ProgressBar::new(total_progress)
                            .text(format!(
                                "{} / {} files complete",
                                self.progress.completed_files, self.progress.total_files
                            ))
                            .fill(Color32::from_rgb(73, 121, 92)),
                    );
                    ui.add_space(6.0);
                    ui.add(
                        egui::ProgressBar::new(self.progress.current_file_progress)
                            .text(match &self.progress.current_file {
                                Some(current_file) => format!(
                                    "Current file: {}",
                                    display_text_for_ui(current_file)
                                ),
                                None => "Waiting to start".to_string(),
                            })
                            .fill(Color32::from_rgb(198, 106, 44)),
                    );
                    ui.add_space(8.0);
                    ui.label(progress_detail(&self.progress));
                });

            ui.add_space(14.0);
            }
            let mut retry_target = None;

            if self.active_tab == AppTab::Backup {
                if let Some(analysis) = &self.backup_analysis.analysis {
                    render_backup_analysis(ui, analysis, &mut self.analysis_file_filter);
                    ui.add_space(14.0);
                }

                ui.horizontal(|ui| {
                    ui.label(RichText::new("File Queue").strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("{} files tracked", self.files.len()))
                                .color(Color32::from_rgb(118, 104, 85)),
                        );
                    });
                });
                ui.add_space(8.0);

                Frame::new()
                    .fill(Color32::WHITE)
                    .stroke(Stroke::new(1.0, Color32::from_rgb(221, 211, 190)))
                    .corner_radius(CornerRadius::same(14))
                    .inner_margin(Margin::same(14))
                    .show(ui, |ui| {
                        ScrollArea::vertical()
                            .id_salt("backup_file_queue_scroll")
                            .auto_shrink([false; 2])
                            .show(ui, |ui| {
                                egui::Grid::new("file_grid")
                                    .striped(true)
                                    .num_columns(5)
                                    .min_col_width(110.0)
                                    .spacing([12.0, 8.0])
                                    .show(ui, |ui| {
                                        ui.label(RichText::new("File Name").strong());
                                        ui.label(RichText::new("Size").strong());
                                        ui.label(RichText::new("Status").strong());
                                        ui.label(RichText::new("Details").strong());
                                        ui.label(RichText::new("Action").strong());
                                        ui.end_row();

                                        for record in &self.files {
                                            ui.label(display_text_for_ui(&record.name));
                                            ui.label(format_bytes(record.size_bytes));
                                            ui.colored_label(
                                                record.status.color(),
                                                record.status.label(),
                                            );
                                            ui.label(display_text_for_ui(&record.detail));

                                            if record.status.is_retryable() && !self.is_running() {
                                                if ui.button("Retry").clicked() {
                                                    retry_target = Some(RemoteFile {
                                                        name: record.name.clone(),
                                                        remote_path: record.remote_path.clone(),
                                                        size_bytes: record.size_bytes,
                                                        modified_epoch_seconds: record
                                                            .modified_epoch_seconds,
                                                        source_root: record.source_root.clone(),
                                                        source_label: record.source_label.clone(),
                                                        destination_subfolder: record
                                                            .destination_subfolder
                                                            .clone(),
                                                        relative_path: record.relative_path.clone(),
                                                    });
                                                }
                                            } else {
                                                ui.label("-");
                                            }
                                            ui.end_row();
                                        }
                                    });

                                if self.files.is_empty() {
                                    ui.add_space(12.0);
                                    ui.label(
                                        "No files scanned yet. Start a run to populate the queue.",
                                    );
                                }
                            });
                    });
            }

            if self.active_tab == AppTab::Cleanup {
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Folder Cleanup Preview").strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(preview) = &self.folder_cleanup.preview {
                            ui.label(
                                RichText::new(format!("{} entries", preview.entries.len()))
                                    .color(Color32::from_rgb(118, 104, 85)),
                            );
                        }
                    });
                });
                ui.add_space(8.0);

                Frame::new()
                    .fill(Color32::WHITE)
                    .stroke(Stroke::new(1.0, Color32::from_rgb(221, 211, 190)))
                    .corner_radius(CornerRadius::same(14))
                    .inner_margin(Margin::same(14))
                    .show(ui, |ui| {
                        ui.label(RichText::new("Selected folder").strong());
                        wrapped_path_text(ui, &self.folder_cleanup.folder_path);
                        ui.add_space(8.0);

                        if self.folder_cleanup.is_fetching_preview {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Fetching cleanup preview...");
                            });
                        }

                        if let Some(error) = &self.folder_cleanup.preview_error {
                            ui.colored_label(Color32::from_rgb(168, 52, 33), error);
                        }
                        if let Some(error) = &self.folder_cleanup.delete_error {
                            ui.colored_label(Color32::from_rgb(168, 52, 33), error);
                        }

                        if let Some(preview) = self.folder_cleanup.preview.clone() {
                            wrapped_text(ui, &cleanup_summary(&preview));
                            ui.small("Preview is ordered by size, with the largest files first.");
                            ui.add_space(8.0);

                            ui.horizontal_wrapped(|ui| {
                                if ui
                                    .add_enabled(
                                        !self.folder_cleanup.is_deleting,
                                        egui::Button::new("Select All"),
                                    )
                                    .clicked()
                                {
                                    self.folder_cleanup.selected_paths = preview
                                        .entries
                                        .iter()
                                        .map(|entry| entry.full_path.clone())
                                        .collect();
                                }
                                if ui
                                    .add_enabled(
                                        !self.folder_cleanup.is_deleting,
                                        egui::Button::new("Select Files Only"),
                                    )
                                    .clicked()
                                {
                                    self.folder_cleanup.selected_paths = preview
                                        .entries
                                        .iter()
                                        .filter(|entry| entry.kind == RemoteFolderEntryKind::File)
                                        .map(|entry| entry.full_path.clone())
                                        .collect();
                                }
                                if ui
                                    .add_enabled(
                                        !self.folder_cleanup.is_deleting,
                                        egui::Button::new("Clear Selection"),
                                    )
                                    .clicked()
                                {
                                    self.folder_cleanup.selected_paths.clear();
                                }
                                ui.label(format!(
                                    "{} checked",
                                    self.folder_cleanup.selected_paths.len()
                                ));
                            });
                            ui.add_space(8.0);

                            ScrollArea::vertical()
                                .id_salt("cleanup_preview_scroll")
                                .max_height(360.0)
                                .auto_shrink([false; 2])
                                .show(ui, |ui| {
                                    for entry in &preview.entries {
                                        Frame::new()
                                            .fill(Color32::from_rgb(250, 247, 240))
                                            .stroke(Stroke::new(
                                                1.0,
                                                Color32::from_rgb(228, 219, 203),
                                            ))
                                            .corner_radius(CornerRadius::same(12))
                                            .inner_margin(Margin::same(10))
                                            .show(ui, |ui| {
                                                ui.horizontal_wrapped(|ui| {
                                                    let mut selected = self
                                                        .folder_cleanup
                                                        .selected_paths
                                                        .contains(&entry.full_path);
                                                    if ui
                                                        .add_enabled(
                                                            !self.folder_cleanup.is_deleting,
                                                            egui::Checkbox::without_text(
                                                                &mut selected,
                                                            ),
                                                        )
                                                        .changed()
                                                    {
                                                        if selected {
                                                            self.folder_cleanup
                                                                .selected_paths
                                                                .insert(entry.full_path.clone());
                                                        } else {
                                                            self.folder_cleanup
                                                                .selected_paths
                                                                .remove(&entry.full_path);
                                                        }
                                                    }

                                                    ui.colored_label(
                                                        Color32::from_rgb(67, 102, 153),
                                                        entry.kind.label(),
                                                    );
                                                    ui.label(format!(
                                                        "Size: {}",
                                                        match entry.kind {
                                                            RemoteFolderEntryKind::Directory => {
                                                                "folder".to_string()
                                                            }
                                                            RemoteFolderEntryKind::File => {
                                                                format_bytes(
                                                                    entry.size_bytes.unwrap_or(0),
                                                                )
                                                            }
                                                        }
                                                    ));
                                                });
                                                ui.add_space(4.0);
                                                wrapped_text(
                                                    ui,
                                                    entry
                                                        .full_path
                                                        .rsplit('/')
                                                        .next()
                                                        .unwrap_or(&entry.full_path),
                                                );
                                                ui.add_space(2.0);
                                                wrapped_path_text(ui, &entry.full_path);
                                            });
                                        ui.add_space(8.0);
                                    }
                                });
                        } else if self.folder_cleanup.preview_error.is_none()
                            && self.folder_cleanup.delete_error.is_none()
                        {
                            ui.label(
                                "Click Fetch Contents to inspect the selected phone folder before deleting anything.",
                            );
                        }
                    });
            }

            if let Some(remote_file) = retry_target {
                self.start_retry(remote_file);
            }
        });

        self.show_remote_folder_picker(ctx);
    }
}

fn apply_theme(ctx: &Context) {
    install_text_fonts(ctx);

    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = Color32::from_rgb(247, 241, 230);
    visuals.extreme_bg_color = Color32::from_rgb(255, 252, 246);
    visuals.faint_bg_color = Color32::from_rgb(247, 241, 230);
    visuals.override_text_color = Some(Color32::from_rgb(51, 43, 35));
    visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(248, 243, 236);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(221, 211, 190));
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(244, 238, 227);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(234, 225, 207);
    visuals.widgets.active.bg_fill = Color32::from_rgb(225, 213, 188);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(66, 56, 45));
    visuals.window_fill = Color32::from_rgb(247, 241, 230);
    visuals.selection.bg_fill = Color32::from_rgb(198, 106, 44);
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.interact_size = egui::vec2(44.0, 28.0);
    ctx.set_style(style);
}

fn install_text_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    let fallback_fonts = [
        ("windows_tahoma", "C:\\Windows\\Fonts\\tahoma.ttf"),
        ("windows_arial", "C:\\Windows\\Fonts\\arial.ttf"),
        ("windows_segoe_ui", "C:\\Windows\\Fonts\\segoeui.ttf"),
    ];

    for (font_name, path) in fallback_fonts.into_iter().rev() {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert(font_name.to_string(), FontData::from_owned(bytes).into());
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .insert(0, font_name.to_string());
            fonts
                .families
                .entry(FontFamily::Monospace)
                .or_default()
                .insert(0, font_name.to_string());
        }
    }

    ctx.set_fonts(fonts);
}

fn contains_arabic(text: &str) -> bool {
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

fn display_text_for_ui(text: &str) -> String {
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

fn wrapped_text(ui: &mut egui::Ui, text: &str) {
    ui.add(egui::Label::new(display_text_for_ui(text)).wrap());
}

fn wrapped_path_text(ui: &mut egui::Ui, text: &str) {
    ui.add(
        egui::Label::new(
            RichText::new(display_text_for_ui(text)).color(Color32::from_rgb(86, 74, 60)),
        )
        .wrap(),
    );
}

fn settings_card(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
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
struct PresetBadge {
    icon: &'static str,
    color: Color32,
}

fn render_preset_chip(ui: &mut egui::Ui, preset: &BackupPreset, selected: bool) -> egui::Response {
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



fn preset_badges(preset: &BackupPreset) -> Vec<PresetBadge> {
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

fn preset_chip_hover_text(preset: &BackupPreset) -> String {
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

fn status_pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    Frame::new()
        .fill(color.gamma_multiply(0.16))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.45)))
        .corner_radius(CornerRadius::same(255))
        .inner_margin(Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.colored_label(color, RichText::new(text).strong());
        });
}

fn device_summary(info: &DeviceInfo) -> String {
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

fn summary_strip(ui: &mut egui::Ui, progress: &SyncProgress, summary: Option<&RunSummary>) {
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

fn metric_chip(ui: &mut egui::Ui, label: &str, value: String, color: Color32) {
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

fn progress_detail(progress: &SyncProgress) -> String {
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

fn format_bytes(bytes: u64) -> String {
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

fn format_duration(seconds: f64) -> String {
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

fn cleanup_summary(preview: &RemoteFolderPreview) -> String {
    format!(
        "Root: {} | {} files | {} folders | {} total file data",
        preview.root_path,
        preview.file_count,
        preview.directory_count,
        format_bytes(preview.total_file_bytes)
    )
}

fn render_backup_analysis(ui: &mut egui::Ui, analysis: &BackupAnalysis, file_filter: &mut String) {
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

fn render_detailed_log_entry(ui: &mut egui::Ui, entry: &LogEntry) {
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

fn log_level_color(level: LogLevel) -> Color32 {
    match level {
        LogLevel::Info => Color32::from_rgb(73, 121, 92),
        LogLevel::Error => Color32::from_rgb(168, 52, 33),
        LogLevel::Trace => Color32::from_rgb(67, 102, 153),
    }
}

fn protected_cleanup_folder_reason(path: &str) -> Option<&'static str> {
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

fn initial_local_directory(path: &str) -> PathBuf {
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

fn derive_destination_subfolder(selected_folder: &PathBuf, destination_root: &str, fallback: &str) -> String {
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

fn normalize_remote_path(path: &str) -> String {
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

fn parent_remote_path(path: &str) -> Option<String> {
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
