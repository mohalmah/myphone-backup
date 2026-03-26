use super::{
    adb::AdbController,
    config,
    logging::{AppLogger, LogEntry},
    models::{
        BackupAnalysis, BackupPreflight, BackupSourceConfig, BackupSourceScan,
        DeviceConnectionState, ExistingFileBehavior, FileRecord, FileStatus, RemoteDirectory,
        RemoteFile, RemoteFolderEntry, RemoteFolderPreview, RunSummary, Settings, SyncProgress,
    },
    storage,
    validator::validate_remote_vs_local,
};
use chrono::Utc;
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, Sender, channel},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub enum SyncEvent {
    Log(LogEntry),
    Device(super::models::DeviceInfo),
    Analysis(BackupAnalysis),
    FileUpdated(FileRecord),
    Progress(SyncProgress),
    Finished(RunSummary),
    FatalError(String),
}

#[derive(Clone)]
pub struct SyncHandle {
    pause_requested: Arc<AtomicBool>,
    stop_requested: Arc<AtomicBool>,
}

impl SyncHandle {
    pub fn request_stop(&self) {
        self.stop_requested.store(true, Ordering::SeqCst);
    }

    pub fn toggle_pause(&self) {
        let currently_paused = self.pause_requested.load(Ordering::SeqCst);
        self.pause_requested
            .store(!currently_paused, Ordering::SeqCst);
    }

    pub fn is_paused(&self) -> bool {
        self.pause_requested.load(Ordering::SeqCst)
    }
}

pub struct SyncSession {
    pub receiver: Receiver<SyncEvent>,
    pub handle: SyncHandle,
}

#[derive(Clone)]
pub enum SyncPlan {
    FullScan,
    RetrySingle(RemoteFile),
}

pub fn start_device_probe(
    adb_path: String,
    log_tx: Sender<LogEntry>,
) -> Receiver<Result<super::models::DeviceInfo, String>> {
    let (tx, rx) = channel();
    thread::spawn(move || {
        let logger = AppLogger::new(config::logs_dir()).ok().map(Arc::new);
        let controller =
            AdbController::with_observer(adb_path, make_background_log_observer(log_tx, logger));
        let result: Result<super::models::DeviceInfo, String> = controller
            .detect_device()
            .map_err(|error| error.to_string());
        let _ = tx.send(result);
    });
    rx
}

pub fn start_remote_directory_list(
    adb_path: String,
    path: String,
    log_tx: Sender<LogEntry>,
) -> Receiver<Result<Vec<RemoteDirectory>, String>> {
    let (tx, rx) = channel();
    thread::spawn(move || {
        let logger = AppLogger::new(config::logs_dir()).ok().map(Arc::new);
        let controller =
            AdbController::with_observer(adb_path, make_background_log_observer(log_tx, logger));
        let result: Result<Vec<RemoteDirectory>, String> = controller
            .list_remote_directories(&path)
            .map_err(|error| error.to_string());
        let _ = tx.send(result);
    });
    rx
}

pub fn start_remote_folder_preview(
    adb_path: String,
    path: String,
    log_tx: Sender<LogEntry>,
) -> Receiver<Result<RemoteFolderPreview, String>> {
    let (tx, rx) = channel();
    thread::spawn(move || {
        let logger = AppLogger::new(config::logs_dir()).ok().map(Arc::new);
        let controller =
            AdbController::with_observer(adb_path, make_background_log_observer(log_tx, logger));
        let result: Result<RemoteFolderPreview, String> = controller
            .preview_remote_folder_contents(&path)
            .map_err(|error| error.to_string());
        let _ = tx.send(result);
    });
    rx
}

pub fn start_remote_folder_delete(
    adb_path: String,
    path: String,
    log_tx: Sender<LogEntry>,
) -> Receiver<Result<String, String>> {
    let (tx, rx) = channel();
    thread::spawn(move || {
        let logger = AppLogger::new(config::logs_dir()).ok().map(Arc::new);
        let controller =
            AdbController::with_observer(adb_path, make_background_log_observer(log_tx, logger));
        let result: Result<String, String> = controller
            .delete_remote_folder_recursive(&path)
            .map(|()| format!("Deleted remote folder and all contents: {path}"))
            .map_err(|error| error.to_string());
        let _ = tx.send(result);
    });
    rx
}

pub fn start_remote_folder_contents_delete(
    adb_path: String,
    path: String,
    log_tx: Sender<LogEntry>,
) -> Receiver<Result<String, String>> {
    let (tx, rx) = channel();
    thread::spawn(move || {
        let logger = AppLogger::new(config::logs_dir()).ok().map(Arc::new);
        let controller =
            AdbController::with_observer(adb_path, make_background_log_observer(log_tx, logger));
        let result: Result<String, String> = controller
            .delete_remote_folder_contents(&path)
            .map(|()| format!("Deleted folder contents but kept folder: {path}"))
            .map_err(|error| error.to_string());
        let _ = tx.send(result);
    });
    rx
}

pub fn start_remote_entries_delete(
    adb_path: String,
    entries: Vec<RemoteFolderEntry>,
    log_tx: Sender<LogEntry>,
) -> Receiver<Result<String, String>> {
    let (tx, rx) = channel();
    thread::spawn(move || {
        let logger = AppLogger::new(config::logs_dir()).ok().map(Arc::new);
        let controller =
            AdbController::with_observer(adb_path, make_background_log_observer(log_tx, logger));
        let result: Result<String, String> = controller
            .delete_remote_entries(&entries)
            .map(|count| format!("Deleted {count} selected item(s) from the phone."))
            .map_err(|error| error.to_string());
        let _ = tx.send(result);
    });
    rx
}

pub fn start_backup_analysis(
    settings: Settings,
    log_tx: Sender<LogEntry>,
) -> Receiver<Result<BackupAnalysis, String>> {
    let (tx, rx) = channel();
    thread::spawn(move || {
        let logger = AppLogger::new(config::logs_dir()).ok().map(Arc::new);
        let adb = AdbController::with_observer(
            settings.adb_path.clone(),
            make_background_log_observer(log_tx, logger),
        );
        let result: Result<BackupAnalysis, String> =
            collect_remote_files_for_settings(&adb, &settings)
                .and_then(|(remote_files, source_summaries)| {
                    build_backup_analysis(&settings, remote_files, source_summaries)
                })
                .map_err(|error| error.to_string());
        let _ = tx.send(result);
    });
    rx
}

pub fn start_backup_source_scan(
    settings: Settings,
    log_tx: Sender<LogEntry>,
) -> Receiver<Result<Vec<BackupSourceScan>, String>> {
    let (tx, rx) = channel();
    thread::spawn(move || {
        let logger = AppLogger::new(config::logs_dir()).ok().map(Arc::new);
        let adb = AdbController::with_observer(
            settings.adb_path.clone(),
            make_background_log_observer(log_tx, logger),
        );
        let result: Result<Vec<BackupSourceScan>, String> =
            scan_backup_sources(&adb, &settings.effective_backup_sources())
                .map_err(|error| error.to_string());
        let _ = tx.send(result);
    });
    rx
}

pub fn start_sync(settings: Settings, plan: SyncPlan) -> SyncSession {
    let (tx, rx) = channel();
    let handle = SyncHandle {
        pause_requested: Arc::new(AtomicBool::new(false)),
        stop_requested: Arc::new(AtomicBool::new(false)),
    };
    let worker_handle = handle.clone();

    thread::spawn(move || run_sync_worker(settings, plan, tx, worker_handle));

    SyncSession {
        receiver: rx,
        handle,
    }
}

#[derive(Default)]
struct FileProcessOutcome {
    copied: bool,
    deleted: bool,
    skipped: bool,
    failed: bool,
    conflict: bool,
    dry_run: bool,
}

fn collect_remote_files_for_settings(
    adb: &AdbController,
    settings: &Settings,
) -> anyhow::Result<(Vec<RemoteFile>, Vec<BackupSourceScan>)> {
    let sources = settings.effective_backup_sources();
    let source_summaries = scan_backup_sources(adb, &sources)?;
    let selected_sources = sources
        .into_iter()
        .filter(|source| source.enabled)
        .collect::<Vec<_>>();

    if selected_sources.is_empty() {
        anyhow::bail!("Select at least one backup source folder first.");
    }

    let mut remote_files = Vec::new();
    for source in selected_sources {
        let files = adb.list_remote_files_recursive(
            &source.source_path,
            &source.label,
            &source.destination_subfolder,
        )?;
        remote_files.extend(files);
    }

    Ok((remote_files, source_summaries))
}

fn scan_backup_sources(
    adb: &AdbController,
    sources: &[BackupSourceConfig],
) -> anyhow::Result<Vec<BackupSourceScan>> {
    let mut scans = Vec::new();

    for source in sources {
        if source.source_path.trim().is_empty() {
            scans.push(BackupSourceScan {
                id: source.id.clone(),
                label: source.label.clone(),
                source_path: source.source_path.clone(),
                destination_subfolder: source.destination_subfolder.clone(),
                enabled: source.enabled,
                exists: false,
                error: Some("Source path is empty.".to_string()),
                ..Default::default()
            });
            continue;
        }

        match adb.list_remote_files_recursive(
            &source.source_path,
            &source.label,
            &source.destination_subfolder,
        ) {
            Ok(files) => scans.push(BackupSourceScan {
                id: source.id.clone(),
                label: source.label.clone(),
                source_path: source.source_path.clone(),
                destination_subfolder: source.destination_subfolder.clone(),
                enabled: source.enabled,
                file_count: files.len(),
                total_bytes: files.iter().map(|file| file.size_bytes).sum(),
                exists: true,
                error: None,
            }),
            Err(error) => scans.push(BackupSourceScan {
                id: source.id.clone(),
                label: source.label.clone(),
                source_path: source.source_path.clone(),
                destination_subfolder: source.destination_subfolder.clone(),
                enabled: source.enabled,
                exists: false,
                error: Some(error.to_string()),
                ..Default::default()
            }),
        }
    }

    scans.sort_by(|left, right| {
        right
            .total_bytes
            .cmp(&left.total_bytes)
            .then_with(|| left.label.to_lowercase().cmp(&right.label.to_lowercase()))
    });

    Ok(scans)
}

fn build_source_summaries_from_files(remote_files: &[RemoteFile]) -> Vec<BackupSourceScan> {
    let mut summaries = Vec::<BackupSourceScan>::new();

    for file in remote_files {
        if let Some(summary) = summaries
            .iter_mut()
            .find(|summary| summary.source_path == file.source_root)
        {
            summary.file_count += 1;
            summary.total_bytes = summary.total_bytes.saturating_add(file.size_bytes);
        } else {
            summaries.push(BackupSourceScan {
                id: file.source_label.to_lowercase().replace(' ', "-"),
                label: file.source_label.clone(),
                source_path: file.source_root.clone(),
                destination_subfolder: file.destination_subfolder.clone(),
                enabled: true,
                file_count: 1,
                total_bytes: file.size_bytes,
                exists: true,
                error: None,
            });
        }
    }

    summaries.sort_by(|left, right| {
        right
            .total_bytes
            .cmp(&left.total_bytes)
            .then_with(|| left.label.to_lowercase().cmp(&right.label.to_lowercase()))
    });
    summaries
}

fn run_sync_worker(settings: Settings, plan: SyncPlan, tx: Sender<SyncEvent>, handle: SyncHandle) {
    let logger = AppLogger::new(config::logs_dir()).ok().map(Arc::new);
    let adb = AdbController::with_observer(
        settings.adb_path.clone(),
        make_sync_log_observer(tx.clone(), logger.clone()),
    );

    match adb.detect_device() {
        Ok(device_info) => {
            let _ = tx.send(SyncEvent::Device(device_info.clone()));
            if device_info.state != DeviceConnectionState::Connected {
                let message = device_info.message;
                emit_error(&tx, &logger, &message);
                let summary = RunSummary {
                    cancelled: true,
                    ..Default::default()
                };
                let _ = tx.send(SyncEvent::Finished(summary));
                return;
            }
        }
        Err(error) => {
            let message = error.to_string();
            emit_error(&tx, &logger, &message);
            let _ = tx.send(SyncEvent::FatalError(message));
            let summary = RunSummary {
                cancelled: true,
                ..Default::default()
            };
            let _ = tx.send(SyncEvent::Finished(summary));
            return;
        }
    }

    let mut scanned_source_summaries = Vec::new();
    let mut remote_files = match plan {
        SyncPlan::FullScan => {
            emit_info(
                &tx,
                &logger,
                "Scanning selected backup folders for files...",
            );
            match collect_remote_files_for_settings(&adb, &settings) {
                Ok((files, source_summaries)) => {
                    scanned_source_summaries = source_summaries;
                    emit_info(
                        &tx,
                        &logger,
                        format!(
                            "Selected {} backup folder(s) for this run.",
                            scanned_source_summaries
                                .iter()
                                .filter(|source| source.enabled)
                                .count()
                        ),
                    );
                    files
                }
                Err(error) => {
                    let message = error.to_string();
                    emit_error(&tx, &logger, &message);
                    let _ = tx.send(SyncEvent::FatalError(message));
                    let summary = RunSummary {
                        cancelled: true,
                        ..Default::default()
                    };
                    let _ = tx.send(SyncEvent::Finished(summary));
                    return;
                }
            }
        }
        SyncPlan::RetrySingle(file) => vec![file],
    };

    if let Some(days) = settings.only_last_days {
        let threshold = Utc::now().timestamp() - i64::from(days) * 86_400;
        remote_files.retain(|file| {
            file.modified_epoch_seconds
                .is_some_and(|value| value >= threshold)
        });
        emit_info(
            &tx,
            &logger,
            format!(
                "Recent-file filter enabled: {} day(s). {} file(s) remain.",
                days,
                remote_files.len()
            ),
        );
    }

    let analysis = match build_backup_analysis(
        &settings,
        remote_files.clone(),
        if scanned_source_summaries.is_empty() {
            build_source_summaries_from_files(&remote_files)
        } else {
            scanned_source_summaries.clone()
        },
    ) {
        Ok(analysis) => analysis,
        Err(error) => {
            let message = error.to_string();
            emit_error(&tx, &logger, &message);
            let _ = tx.send(SyncEvent::FatalError(message));
            let summary = RunSummary {
                cancelled: true,
                ..Default::default()
            };
            let _ = tx.send(SyncEvent::Finished(summary));
            return;
        }
    };

    let destination_issue = analysis.preflight.destination_space_error.clone();
    let enough_destination_space = analysis.preflight.destination_has_enough_space;
    let _ = tx.send(SyncEvent::Analysis(analysis.clone()));
    emit_info(
        &tx,
        &logger,
        format!(
            "Preflight: {} files, {} total, {} to copy, destination free {}.",
            analysis.preflight.total_files,
            format_bytes(analysis.preflight.total_bytes),
            format_bytes(analysis.preflight.bytes_to_copy),
            analysis
                .preflight
                .destination_available_bytes
                .map(format_bytes)
                .unwrap_or_else(|| "unknown".to_string())
        ),
    );
    if let Some(warning) = &analysis.preflight.system_drive_warning {
        emit_info(&tx, &logger, warning.clone());
    }
    if !settings.dry_run {
        if let Some(message) = destination_issue {
            emit_error(&tx, &logger, message.clone());
            let _ = tx.send(SyncEvent::FatalError(message));
            let summary = RunSummary {
                cancelled: true,
                ..Default::default()
            };
            let _ = tx.send(SyncEvent::Finished(summary));
            return;
        }

        if !enough_destination_space {
            let message = format!(
                "Not enough free space in destination folder. Required {}, available {}.",
                format_bytes(analysis.preflight.bytes_to_copy),
                analysis
                    .preflight
                    .destination_available_bytes
                    .map(format_bytes)
                    .unwrap_or_else(|| "unknown".to_string())
            );
            emit_error(&tx, &logger, message.clone());
            let _ = tx.send(SyncEvent::FatalError(message));
            let summary = RunSummary {
                cancelled: true,
                ..Default::default()
            };
            let _ = tx.send(SyncEvent::Finished(summary));
            return;
        }
    }

    let mut summary = RunSummary {
        total_files: remote_files.len(),
        ..Default::default()
    };
    let mut progress = SyncProgress {
        total_files: remote_files.len(),
        total_bytes: remote_files.iter().map(|file| file.size_bytes).sum(),
        ..Default::default()
    };
    let start_time = Instant::now();
    let _ = tx.send(SyncEvent::Progress(progress.clone()));
    let mut deletion_suspended_after_error = false;

    if remote_files.is_empty() {
        emit_info(&tx, &logger, "No files matched the current filters.");
        let _ = tx.send(SyncEvent::Finished(summary));
        return;
    }

    let destination_root = PathBuf::from(&settings.destination_path);

    for remote_file in remote_files {
        if handle.stop_requested.load(Ordering::SeqCst) {
            summary.cancelled = true;
            emit_info(&tx, &logger, "Stop requested. Finishing current run.");
            break;
        }

        if wait_if_paused(&handle, &tx, &logger) {
            summary.cancelled = true;
            break;
        }

        let mut record = FileRecord::from_remote(&remote_file, &destination_root);
        update_stage(
            &tx,
            &mut progress,
            &mut record,
            FileStatus::Queued,
            "Queued for processing",
            0.05,
        );
        emit_info(&tx, &logger, format!("File detected: {}", record.name));

        let outcome = process_file(
            &adb,
            &settings,
            deletion_suspended_after_error,
            &mut progress,
            &tx,
            &logger,
            &mut record,
            &remote_file,
        );

        if outcome.copied {
            summary.copied += 1;
        }
        if outcome.deleted {
            summary.deleted += 1;
        }
        if outcome.skipped {
            summary.skipped += 1;
        }
        if outcome.failed {
            summary.failed += 1;
            progress.failed_files += 1;
            deletion_suspended_after_error = true;
        }
        if outcome.conflict {
            summary.conflicts += 1;
            progress.failed_files += 1;
            deletion_suspended_after_error = true;
        }
        if outcome.dry_run {
            summary.dry_run_actions += 1;
        }

        progress.completed_files += 1;
        progress.processed_bytes = progress
            .processed_bytes
            .saturating_add(remote_file.size_bytes);
        progress.current_file = Some(record.name.clone());
        progress.current_file_progress = 1.0;

        let elapsed = start_time.elapsed().as_secs_f64();
        progress.speed_bytes_per_sec = if elapsed > 0.0 {
            progress.processed_bytes as f64 / elapsed
        } else {
            0.0
        };

        let remaining_bytes = progress
            .total_bytes
            .saturating_sub(progress.processed_bytes);
        progress.eta_seconds = if progress.speed_bytes_per_sec > 0.0 {
            Some(remaining_bytes as f64 / progress.speed_bytes_per_sec)
        } else {
            None
        };

        let _ = tx.send(SyncEvent::Progress(progress.clone()));
    }

    let final_message = format!(
        "Run finished. total={}, copied={}, deleted={}, skipped={}, failed={}, conflicts={}, dry_run={}",
        summary.total_files,
        summary.copied,
        summary.deleted,
        summary.skipped,
        summary.failed,
        summary.conflicts,
        summary.dry_run_actions
    );
    emit_info(&tx, &logger, final_message);
    let _ = tx.send(SyncEvent::Finished(summary));
}

fn process_file(
    adb: &AdbController,
    settings: &Settings,
    deletion_suspended_after_error: bool,
    progress: &mut SyncProgress,
    tx: &Sender<SyncEvent>,
    logger: &Option<Arc<AppLogger>>,
    record: &mut FileRecord,
    remote_file: &RemoteFile,
) -> FileProcessOutcome {
    let mut outcome = FileProcessOutcome::default();
    let local_parent = record
        .local_path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(&settings.destination_path));

    if let Err(error) = fs::create_dir_all(&local_parent) {
        let detail = format!(
            "Failed to create destination folder {}: {error}",
            local_parent.display()
        );
        update_stage(
            tx,
            progress,
            record,
            FileStatus::Failed,
            detail.clone(),
            1.0,
        );
        emit_error(tx, logger, detail);
        outcome.failed = true;
        return outcome;
    }

    let local_exists = record.local_path.exists();
    if local_exists {
        match fs::metadata(&record.local_path) {
            Ok(metadata) => {
                let local_size = metadata.len();
                if local_size != remote_file.size_bytes {
                    let detail = format!(
                        "Conflict: local file exists with different size (local {} vs remote {}).",
                        local_size, remote_file.size_bytes
                    );
                    update_stage(
                        tx,
                        progress,
                        record,
                        FileStatus::Conflict,
                        detail.clone(),
                        1.0,
                    );
                    emit_error(tx, logger, detail);
                    outcome.conflict = true;
                    return outcome;
                }
            }
            Err(error) => {
                let detail = format!("Failed to inspect existing local file: {error}");
                update_stage(
                    tx,
                    progress,
                    record,
                    FileStatus::Failed,
                    detail.clone(),
                    1.0,
                );
                emit_error(tx, logger, detail);
                outcome.failed = true;
                return outcome;
            }
        }

        if settings.existing_file_behavior == ExistingFileBehavior::Skip {
            let detail = "File already exists locally with matching size. Skipped.".to_string();
            update_stage(
                tx,
                progress,
                record,
                FileStatus::Skipped,
                detail.clone(),
                1.0,
            );
            emit_info(tx, logger, detail);
            outcome.skipped = true;
            return outcome;
        }
    }

    if !local_exists {
        if settings.dry_run {
            let detail =
                "Dry-run: would copy this file, validate it, and keep the device unchanged."
                    .to_string();
            update_stage(
                tx,
                progress,
                record,
                FileStatus::DryRun,
                detail.clone(),
                1.0,
            );
            emit_info(tx, logger, detail);
            outcome.dry_run = true;
            return outcome;
        }

        update_stage(
            tx,
            progress,
            record,
            FileStatus::Copying,
            "Copying from device...",
            0.35,
        );

        let mut copied_successfully = false;
        let mut last_error = None;

        for attempt in 1..=3u8 {
            record.attempts = attempt;
            let _ = tx.send(SyncEvent::FileUpdated(record.clone()));

            match adb.pull_file(&remote_file.remote_path, &record.local_path) {
                Ok(command_output) => {
                    emit_info(
                        tx,
                        logger,
                        format!("Copied successfully: {} ({command_output})", record.name),
                    );
                    copied_successfully = true;
                    break;
                }
                Err(error) => {
                    let message = format!(
                        "Copy attempt {} of 3 failed for {}: {}",
                        attempt, record.name, error
                    );
                    emit_error(tx, logger, message.clone());
                    last_error = Some(message);
                    if attempt < 3 {
                        thread::sleep(Duration::from_secs(1));
                    }
                }
            }
        }

        if !copied_successfully {
            let detail = last_error.unwrap_or_else(|| "Copy failed after 3 attempts.".to_string());
            update_stage(
                tx,
                progress,
                record,
                FileStatus::Failed,
                detail.clone(),
                1.0,
            );
            outcome.failed = true;
            return outcome;
        }

        outcome.copied = true;
    }

    update_stage(
        tx,
        progress,
        record,
        FileStatus::Validating,
        "Validating local backup...",
        0.82,
    );

    match validate_remote_vs_local(
        adb,
        remote_file,
        &record.local_path,
        settings.validation_mode,
    ) {
        Ok(report) if report.passed => {
            emit_info(
                tx,
                logger,
                format!("Validation passed for {} ({})", record.name, report.detail),
            );

            if settings.auto_delete_after_success && !deletion_suspended_after_error {
                if settings.dry_run {
                    let detail = "Dry-run: validation passed and the device file would be deleted."
                        .to_string();
                    update_stage(
                        tx,
                        progress,
                        record,
                        FileStatus::DryRun,
                        detail.clone(),
                        1.0,
                    );
                    emit_info(tx, logger, detail);
                    outcome.dry_run = true;
                    return outcome;
                }

                match adb.delete_file(&remote_file.remote_path) {
                    Ok(()) => {
                        let detail = "Deleted from device after successful validation.".to_string();
                        update_stage(
                            tx,
                            progress,
                            record,
                            FileStatus::Deleted,
                            detail.clone(),
                            1.0,
                        );
                        emit_info(tx, logger, format!("Deleted from device: {}", record.name));
                        outcome.deleted = true;
                    }
                    Err(error) => {
                        let detail = format!(
                            "Validation passed, but delete failed. Device file was kept: {error}"
                        );
                        update_stage(
                            tx,
                            progress,
                            record,
                            FileStatus::Failed,
                            detail.clone(),
                            1.0,
                        );
                        emit_error(tx, logger, detail);
                        outcome.failed = true;
                    }
                }
            } else {
                let detail = if local_exists {
                    "Existing local copy validated successfully.".to_string()
                } else if settings.auto_delete_after_success && deletion_suspended_after_error {
                    "Copied and validated successfully. Device file was kept because deletion was suspended after an earlier error.".to_string()
                } else {
                    "Copied and validated successfully.".to_string()
                };
                update_stage(tx, progress, record, FileStatus::Done, detail.clone(), 1.0);
                emit_info(tx, logger, detail);
            }
        }
        Ok(report) => {
            let detail = report.detail;
            if local_exists {
                update_stage(
                    tx,
                    progress,
                    record,
                    FileStatus::Conflict,
                    detail.clone(),
                    1.0,
                );
                emit_error(tx, logger, detail);
                outcome.conflict = true;
            } else {
                update_stage(
                    tx,
                    progress,
                    record,
                    FileStatus::Failed,
                    detail.clone(),
                    1.0,
                );
                emit_error(tx, logger, detail);
                outcome.failed = true;
            }
        }
        Err(error) => {
            let detail = format!("Validation failed: {error}");
            update_stage(
                tx,
                progress,
                record,
                FileStatus::Failed,
                detail.clone(),
                1.0,
            );
            emit_error(tx, logger, detail);
            outcome.failed = true;
        }
    }

    outcome
}

fn update_stage(
    tx: &Sender<SyncEvent>,
    progress: &mut SyncProgress,
    record: &mut FileRecord,
    status: FileStatus,
    detail: impl Into<String>,
    stage_progress: f32,
) {
    record.status = status;
    record.detail = detail.into();
    progress.current_file = Some(record.name.clone());
    progress.current_file_progress = stage_progress;
    let _ = tx.send(SyncEvent::FileUpdated(record.clone()));
    let _ = tx.send(SyncEvent::Progress(progress.clone()));
}

fn emit_info(tx: &Sender<SyncEvent>, logger: &Option<Arc<AppLogger>>, message: impl Into<String>) {
    emit_log_entry(tx, logger, LogEntry::info(message.into()));
}

fn emit_error(tx: &Sender<SyncEvent>, logger: &Option<Arc<AppLogger>>, message: impl Into<String>) {
    emit_log_entry(tx, logger, LogEntry::error(message.into()));
}

fn emit_log_entry(tx: &Sender<SyncEvent>, logger: &Option<Arc<AppLogger>>, entry: LogEntry) {
    if let Some(logger) = logger {
        logger.log_entry(&entry);
    }
    let _ = tx.send(SyncEvent::Log(entry));
}

fn make_sync_log_observer(
    tx: Sender<SyncEvent>,
    logger: Option<Arc<AppLogger>>,
) -> Arc<dyn Fn(LogEntry) + Send + Sync> {
    Arc::new(move |entry| {
        if let Some(logger) = &logger {
            logger.log_entry(&entry);
        }
        let _ = tx.send(SyncEvent::Log(entry));
    })
}

fn make_background_log_observer(
    log_tx: Sender<LogEntry>,
    logger: Option<Arc<AppLogger>>,
) -> Arc<dyn Fn(LogEntry) + Send + Sync> {
    Arc::new(move |entry| {
        if let Some(logger) = &logger {
            logger.log_entry(&entry);
        }
        let _ = log_tx.send(entry);
    })
}

fn wait_if_paused(
    handle: &SyncHandle,
    tx: &Sender<SyncEvent>,
    logger: &Option<Arc<AppLogger>>,
) -> bool {
    if !handle.pause_requested.load(Ordering::SeqCst) {
        return false;
    }

    emit_info(tx, logger, "Run paused.");
    while handle.pause_requested.load(Ordering::SeqCst) {
        if handle.stop_requested.load(Ordering::SeqCst) {
            emit_info(tx, logger, "Stop requested while paused.");
            return true;
        }
        thread::sleep(Duration::from_millis(250));
    }
    emit_info(tx, logger, "Run resumed.");
    false
}

fn build_backup_analysis(
    settings: &Settings,
    remote_files: Vec<RemoteFile>,
    source_summaries: Vec<BackupSourceScan>,
) -> anyhow::Result<BackupAnalysis> {
    let destination_root = PathBuf::from(&settings.destination_path);
    let selected_source_count = source_summaries
        .iter()
        .filter(|source| source.enabled)
        .count();
    let mut preflight = BackupPreflight {
        source_path: match selected_source_count {
            0 => settings.source_path.clone(),
            1 => source_summaries
                .iter()
                .find(|source| source.enabled)
                .map(|source| source.source_path.clone())
                .unwrap_or_else(|| settings.source_path.clone()),
            _ => format!("{selected_source_count} selected source folders"),
        },
        destination_path: settings.destination_path.clone(),
        total_files: remote_files.len(),
        total_bytes: remote_files.iter().map(|file| file.size_bytes).sum(),
        ..Default::default()
    };

    for remote_file in &remote_files {
        let local_path = FileRecord::from_remote(remote_file, &destination_root).local_path;
        match fs::metadata(&local_path) {
            Ok(metadata) if metadata.len() == remote_file.size_bytes => {
                preflight.matching_local_files += 1;
            }
            Ok(_) => {
                preflight.conflicting_local_files += 1;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                preflight.files_to_copy += 1;
                preflight.bytes_to_copy = preflight
                    .bytes_to_copy
                    .saturating_add(remote_file.size_bytes);
            }
            Err(error) => {
                preflight.destination_space_error = Some(format!(
                    "Failed to inspect local destination {}: {error}",
                    local_path.display()
                ));
            }
        }
    }

    match storage::available_space_for_path(&destination_root) {
        Ok(bytes) => {
            preflight.destination_available_bytes = Some(bytes);
            preflight.destination_has_enough_space = bytes >= preflight.bytes_to_copy;
        }
        Err(error) => {
            preflight.destination_space_error = Some(error.to_string());
            preflight.destination_has_enough_space = false;
        }
    }

    if let Some(system_drive) = storage::system_drive_root() {
        preflight.system_drive_path = Some(system_drive.display().to_string());
        if let Ok(bytes) = storage::available_space_for_path(&system_drive) {
            preflight.system_drive_available_bytes = Some(bytes);
            if bytes < 1_073_741_824 {
                preflight.system_drive_warning = Some(format!(
                    "Warning: system drive {} has only {} free. Transfers write directly to the destination, but Windows may still need some headroom.",
                    system_drive.display(),
                    format_bytes(bytes)
                ));
            }
        }
    }

    Ok(BackupAnalysis {
        files: remote_files,
        source_summaries,
        preflight,
    })
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
