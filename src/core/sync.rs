use super::{
    adb::AdbController,
    config,
    logging::AppLogger,
    models::{
        DeviceConnectionState, ExistingFileBehavior, FileRecord, FileStatus, RemoteFile,
        RunSummary, Settings, SyncProgress,
    },
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
    LogLine(String),
    Device(super::models::DeviceInfo),
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

pub fn start_device_probe(adb_path: String) -> Receiver<Result<super::models::DeviceInfo, String>> {
    let (tx, rx) = channel();
    thread::spawn(move || {
        let controller = AdbController::new(adb_path);
        let result = controller
            .detect_device()
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

fn run_sync_worker(settings: Settings, plan: SyncPlan, tx: Sender<SyncEvent>, handle: SyncHandle) {
    let logger = AppLogger::new(config::logs_dir()).ok();
    let adb = AdbController::new(settings.adb_path.clone());

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

    let mut remote_files = match plan {
        SyncPlan::FullScan => {
            emit_info(&tx, &logger, "Scanning remote folder for files...");
            match adb.list_remote_files(&settings.source_path) {
                Ok(files) => files,
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
        }
        if outcome.conflict {
            summary.conflicts += 1;
            progress.failed_files += 1;
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
    progress: &mut SyncProgress,
    tx: &Sender<SyncEvent>,
    logger: &Option<AppLogger>,
    record: &mut FileRecord,
    remote_file: &RemoteFile,
) -> FileProcessOutcome {
    let mut outcome = FileProcessOutcome::default();
    let destination_root = PathBuf::from(&settings.destination_path);

    if let Err(error) = fs::create_dir_all(&destination_root) {
        let detail = format!(
            "Failed to create destination folder {}: {error}",
            destination_root.display()
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

            if settings.auto_delete_after_success {
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

fn emit_info(tx: &Sender<SyncEvent>, logger: &Option<AppLogger>, message: impl Into<String>) {
    let message = message.into();
    if let Some(logger) = logger {
        logger.log("INFO", &message);
    }
    let _ = tx.send(SyncEvent::LogLine(format!("[INFO] {message}")));
}

fn emit_error(tx: &Sender<SyncEvent>, logger: &Option<AppLogger>, message: impl Into<String>) {
    let message = message.into();
    if let Some(logger) = logger {
        logger.log("ERROR", &message);
    }
    let _ = tx.send(SyncEvent::LogLine(format!("[ERROR] {message}")));
}

fn wait_if_paused(handle: &SyncHandle, tx: &Sender<SyncEvent>, logger: &Option<AppLogger>) -> bool {
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
