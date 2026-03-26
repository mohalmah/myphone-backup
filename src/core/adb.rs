use super::{
    logging::LogEntry,
    models::{
        DeviceConnectionState, DeviceInfo, RemoteDirectory, RemoteFile, RemoteFolderEntry,
        RemoteFolderEntryKind, RemoteFolderPreview,
    },
};
use anyhow::{Context, Result, anyhow, bail};
use std::process::Command;
use std::{path::Path, sync::Arc};

pub struct AdbController {
    executable: String,
    observer: Option<Arc<dyn Fn(LogEntry) + Send + Sync>>,
}

struct CommandResult {
    stdout: String,
    stderr: String,
}

impl CommandResult {
    fn merged_output(&self) -> String {
        let stdout = self.stdout.trim();
        let stderr = self.stderr.trim();
        match (stdout.is_empty(), stderr.is_empty()) {
            (true, true) => String::new(),
            (false, true) => stdout.to_string(),
            (true, false) => stderr.to_string(),
            (false, false) => format!("{stdout}\n{stderr}"),
        }
    }
}

impl AdbController {
    pub fn with_observer(
        executable: String,
        observer: Arc<dyn Fn(LogEntry) + Send + Sync>,
    ) -> Self {
        Self {
            executable,
            observer: Some(observer),
        }
    }

    pub fn detect_device(&self) -> Result<DeviceInfo> {
        let output = self.run_command(&["devices", "-l"])?;
        let entries = output
            .stdout
            .lines()
            .skip(1)
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>();

        if entries.is_empty() {
            return Ok(DeviceInfo {
                serial: String::new(),
                model: None,
                state: DeviceConnectionState::Disconnected,
                message: "No Android device detected via ADB.".to_string(),
            });
        }

        if entries.len() > 1 {
            bail!(
                "Multiple Android devices were detected. Disconnect extras or extend the app to choose a serial."
            );
        }

        let line = entries[0];
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 2 {
            bail!("ADB returned an unexpected device line: {line}");
        }

        let serial = parts[0].to_string();
        let state = match parts[1] {
            "device" => DeviceConnectionState::Connected,
            "unauthorized" => DeviceConnectionState::Unauthorized,
            "offline" => DeviceConnectionState::Offline,
            _ => DeviceConnectionState::Disconnected,
        };

        let mut model = parts
            .iter()
            .find_map(|part| part.strip_prefix("model:"))
            .map(str::to_string);

        if model.is_none() && state == DeviceConnectionState::Connected {
            model = self
                .run_command(&["shell", "getprop", "ro.product.model"])
                .ok()
                .map(|output| output.stdout.trim().to_string())
                .filter(|value| !value.is_empty());
        }

        let message = match state {
            DeviceConnectionState::Connected => "ADB authorization granted.".to_string(),
            DeviceConnectionState::Unauthorized => {
                "Unlock the phone and accept the ADB authorization prompt.".to_string()
            }
            DeviceConnectionState::Offline => {
                "ADB sees the device, but the connection is offline.".to_string()
            }
            DeviceConnectionState::Disconnected => "No active ADB device found.".to_string(),
        };

        Ok(DeviceInfo {
            serial,
            model,
            state,
            message,
        })
    }

    pub fn list_remote_files_recursive(
        &self,
        source_directory: &str,
        source_label: &str,
        destination_subfolder: &str,
    ) -> Result<Vec<RemoteFile>> {
        let script = r#"if [ ! -d "$1" ]; then
    echo "__NOT_DIRECTORY__"
    exit 11
fi
find "$1" -type f | while IFS= read -r item; do
    size="$(stat -c %s "$item" 2>/dev/null)"
    if [ -z "$size" ]; then
        size="$(stat -f %z "$item" 2>/dev/null)"
    fi
    modified="$(stat -c %Y "$item" 2>/dev/null)"
    if [ -z "$modified" ]; then
        modified="$(stat -f %m "$item" 2>/dev/null)"
    fi
    printf 'F|%s|%s|%s\n' "${size:-0}" "${modified:-0}" "$item"
done"#;
        let output = self.run_shell_command(
            "sh",
            &["-c", script, "list-files-recursive", source_directory],
        )?;

        let mut files = Vec::new();
        let normalized_root = normalize_remote_root(source_directory);

        for line in output
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            if line == "__NOT_DIRECTORY__" {
                bail!("Remote folder does not exist: {source_directory}");
            }

            let mut parts = line.splitn(4, '|');
            if parts.next().unwrap_or_default() != "F" {
                continue;
            }

            let size_bytes = parts
                .next()
                .unwrap_or("0")
                .trim()
                .parse::<u64>()
                .unwrap_or(0);
            let modified_epoch_seconds = parts.next().unwrap_or("0").trim().parse::<i64>().ok();
            let remote_path = parts.next().unwrap_or_default().trim().to_string();
            if remote_path.is_empty() {
                continue;
            }
            let relative_path = relative_remote_path(&normalized_root, &remote_path);
            let name = relative_path
                .rsplit('/')
                .next()
                .unwrap_or(&remote_path)
                .to_string();

            files.push(RemoteFile {
                name,
                remote_path,
                size_bytes,
                modified_epoch_seconds,
                source_root: normalized_root.clone(),
                source_label: source_label.to_string(),
                destination_subfolder: destination_subfolder.to_string(),
                relative_path,
            });
        }

        files.sort_by(|left, right| {
            left.relative_path
                .to_lowercase()
                .cmp(&right.relative_path.to_lowercase())
        });
        Ok(files)
    }

    pub fn list_remote_directories(&self, source_directory: &str) -> Result<Vec<RemoteDirectory>> {
        let script = r#"if [ ! -d "$1" ]; then
    echo "__NOT_DIRECTORY__"
    exit 11
fi
find "$1" -mindepth 1 -maxdepth 1 -type d | while IFS= read -r item; do
    printf 'D|%s\n' "$item"
done"#;
        let output =
            self.run_shell_command("sh", &["-c", script, "list-directories", source_directory])?;

        let mut directories = Vec::new();
        for line in output
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            if line == "__NOT_DIRECTORY__" {
                bail!("Remote folder does not exist: {source_directory}");
            }
            let Some(full_path) = line.strip_prefix("D|") else {
                continue;
            };
            let full_path = full_path.trim().to_string();
            if full_path.is_empty() {
                continue;
            }
            let name = full_path
                .rsplit('/')
                .next()
                .unwrap_or(&full_path)
                .to_string();

            directories.push(RemoteDirectory { name, full_path });
        }

        directories.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
        Ok(directories)
    }

    pub fn pull_file(&self, remote_path: &str, destination: &Path) -> Result<String> {
        let destination = destination.to_string_lossy().to_string();
        let output = self.run_command(&["pull", remote_path, &destination])?;
        Ok(output.merged_output())
    }

    pub fn delete_file(&self, remote_path: &str) -> Result<()> {
        self.run_shell_command("rm", &[remote_path])?;
        Ok(())
    }

    pub fn remote_md5(&self, remote_path: &str) -> Result<String> {
        let output = self.run_shell_command("md5sum", &[remote_path])?;
        extract_md5(&output)
            .ok_or_else(|| anyhow!("Unable to parse remote md5sum output for {remote_path}"))
    }

    pub fn preview_remote_folder_contents(&self, folder_path: &str) -> Result<RemoteFolderPreview> {
        let script = r#"if [ ! -d "$1" ]; then
    echo "__NOT_DIRECTORY__"
    exit 11
fi
find "$1" -mindepth 1 | while IFS= read -r item; do
    if [ -d "$item" ]; then
        printf 'D|%s\n' "$item"
    else
        size="$(stat -c %s "$item" 2>/dev/null)"
        if [ -z "$size" ]; then
            size="$(stat -f %z "$item" 2>/dev/null)"
        fi
        printf 'F|%s|%s\n' "${size:-0}" "$item"
    fi
done"#;

        let output =
            self.run_shell_command("sh", &["-c", script, "preview-folder", folder_path])?;
        if output
            .lines()
            .any(|line| line.trim() == "__NOT_DIRECTORY__")
        {
            bail!("Remote folder does not exist: {folder_path}");
        }

        let mut entries = Vec::new();
        let mut file_count = 0usize;
        let mut directory_count = 0usize;
        let mut total_file_bytes = 0u64;

        for line in output
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            if let Some(path) = line.strip_prefix("D|") {
                directory_count += 1;
                entries.push(RemoteFolderEntry {
                    full_path: path.to_string(),
                    kind: RemoteFolderEntryKind::Directory,
                    size_bytes: None,
                });
            } else if let Some(rest) = line.strip_prefix("F|") {
                let mut parts = rest.splitn(2, '|');
                let size_bytes = parts
                    .next()
                    .unwrap_or("0")
                    .trim()
                    .parse::<u64>()
                    .unwrap_or(0);
                let full_path = parts.next().unwrap_or("").trim().to_string();
                if !full_path.is_empty() {
                    file_count += 1;
                    total_file_bytes = total_file_bytes.saturating_add(size_bytes);
                    entries.push(RemoteFolderEntry {
                        full_path,
                        kind: RemoteFolderEntryKind::File,
                        size_bytes: Some(size_bytes),
                    });
                }
            }
        }

        entries.sort_by(|left, right| match (&left.kind, &right.kind) {
            (RemoteFolderEntryKind::File, RemoteFolderEntryKind::Directory) => {
                std::cmp::Ordering::Less
            }
            (RemoteFolderEntryKind::Directory, RemoteFolderEntryKind::File) => {
                std::cmp::Ordering::Greater
            }
            (RemoteFolderEntryKind::File, RemoteFolderEntryKind::File) => right
                .size_bytes
                .unwrap_or(0)
                .cmp(&left.size_bytes.unwrap_or(0))
                .then_with(|| {
                    left.full_path
                        .to_lowercase()
                        .cmp(&right.full_path.to_lowercase())
                }),
            (RemoteFolderEntryKind::Directory, RemoteFolderEntryKind::Directory) => left
                .full_path
                .to_lowercase()
                .cmp(&right.full_path.to_lowercase()),
        });

        Ok(RemoteFolderPreview {
            root_path: folder_path.to_string(),
            entries,
            file_count,
            directory_count,
            total_file_bytes,
        })
    }

    pub fn delete_remote_folder_recursive(&self, folder_path: &str) -> Result<()> {
        self.run_shell_command("rm", &["-rf", folder_path])?;
        Ok(())
    }

    pub fn delete_remote_folder_contents(&self, folder_path: &str) -> Result<()> {
        let script = r#"if [ ! -d "$1" ]; then
    echo "__NOT_DIRECTORY__"
    exit 11
fi
find "$1" -mindepth 1 -maxdepth 1 | while IFS= read -r item; do
    rm -rf "$item"
done"#;
        let output =
            self.run_shell_command("sh", &["-c", script, "delete-folder-contents", folder_path])?;
        if output
            .lines()
            .any(|line| line.trim() == "__NOT_DIRECTORY__")
        {
            bail!("Remote folder does not exist: {folder_path}");
        }
        Ok(())
    }

    pub fn delete_remote_entries(&self, entries: &[RemoteFolderEntry]) -> Result<usize> {
        for entry in entries {
            match entry.kind {
                RemoteFolderEntryKind::Directory => {
                    self.run_shell_command("rm", &["-rf", &entry.full_path])?;
                }
                RemoteFolderEntryKind::File => {
                    self.run_shell_command("rm", &[&entry.full_path])?;
                }
            }
        }

        Ok(entries.len())
    }

    fn run_shell_command(&self, command: &str, args: &[&str]) -> Result<String> {
        let command_line = build_shell_command(command, args);
        let output = match self.run_command(&["shell", "-T", &command_line]) {
            Ok(output) => output,
            Err(error)
                if error
                    .to_string()
                    .contains("target doesn't support PTY args -Tt") =>
            {
                self.run_command(&["shell", &command_line])?
            }
            Err(error) => return Err(error),
        };
        Ok(output.stdout)
    }

    fn run_command(&self, args: &[&str]) -> Result<CommandResult> {
        let rendered_command = render_command_line(&self.executable, args);
        let output = Command::new(&self.executable)
            .args(args)
            .output()
            .with_context(|| {
                format!(
                    "Failed to launch ADB executable \"{}\". Set the ADB path in the UI if needed.",
                    self.executable
                )
            })?;

        let result = CommandResult {
            stdout: String::from_utf8_lossy(&output.stdout).replace('\r', ""),
            stderr: String::from_utf8_lossy(&output.stderr).replace('\r', ""),
        };

        self.emit_trace_log(LogEntry::trace(
            if output.status.success() {
                "ADB command completed"
            } else {
                "ADB command failed"
            },
            format_command_result(&rendered_command, output.status.code(), &result),
        ));

        if !output.status.success() {
            let detail = result.merged_output();
            bail!(
                "ADB command failed: {}",
                if detail.is_empty() {
                    "<no output>"
                } else {
                    &detail
                }
            );
        }

        Ok(result)
    }

    fn emit_trace_log(&self, entry: LogEntry) {
        if let Some(observer) = &self.observer {
            observer(entry);
        }
    }
}

fn build_shell_command(command: &str, args: &[&str]) -> String {
    let mut command_line = String::from(command);
    for arg in args {
        command_line.push(' ');
        command_line.push_str(&shell_quote(arg));
    }
    command_line
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r#"'\''"#))
}

fn extract_md5(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .find(|token| {
            token.len() == 32 && token.chars().all(|character| character.is_ascii_hexdigit())
        })
        .map(|value| value.to_lowercase())
}

fn normalize_remote_root(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    normalized.trim_end_matches('/').to_string()
}

fn relative_remote_path(root: &str, full_path: &str) -> String {
    let normalized_root = normalize_remote_root(root);
    let normalized_path = full_path.replace('\\', "/");
    normalized_path
        .strip_prefix(&(normalized_root.clone() + "/"))
        .unwrap_or(&normalized_path)
        .to_string()
}

fn render_command_line(executable: &str, args: &[&str]) -> String {
    let mut parts = Vec::with_capacity(args.len() + 1);
    parts.push(render_command_part(executable));
    for arg in args {
        parts.push(render_command_part(arg));
    }
    parts.join(" ")
}

fn render_command_part(part: &str) -> String {
    if part.is_empty() {
        return "\"\"".to_string();
    }

    if part.contains([' ', '\t', '"']) {
        format!("\"{}\"", part.replace('"', "\\\""))
    } else {
        part.to_string()
    }
}

fn format_command_result(
    command_line: &str,
    exit_code: Option<i32>,
    result: &CommandResult,
) -> String {
    let stdout = if result.stdout.trim().is_empty() {
        "<empty>".to_string()
    } else {
        result.stdout.trim().to_string()
    };
    let stderr = if result.stderr.trim().is_empty() {
        "<empty>".to_string()
    } else {
        result.stderr.trim().to_string()
    };

    format!(
        "Command:\n{command_line}\n\nExit code: {}\n\nStdout:\n{stdout}\n\nStderr:\n{stderr}",
        exit_code
            .map(|value| value.to_string())
            .unwrap_or_else(|| "terminated by signal".to_string())
    )
}

#[cfg(test)]
mod tests {
    use super::{build_shell_command, extract_md5, render_command_line};

    #[test]
    fn extracts_md5_from_md5sum_output() {
        let hash = extract_md5("0cc175b9c0f1b6a831c399e269772661  /sdcard/file.mp4");
        assert_eq!(hash.as_deref(), Some("0cc175b9c0f1b6a831c399e269772661"));
    }

    #[test]
    fn builds_shell_command_with_quoted_spaces() {
        let command = build_shell_command(
            "ls",
            &[
                "-1",
                "-p",
                "/sdcard/Android/media/com.whatsapp/WhatsApp Documents",
            ],
        );
        assert_eq!(
            command,
            "ls '-1' '-p' '/sdcard/Android/media/com.whatsapp/WhatsApp Documents'"
        );
    }

    #[test]
    fn builds_shell_command_for_rm_recursive() {
        let command = build_shell_command(
            "rm",
            &[
                "-rf",
                "/sdcard/Android/media/com.whatsapp/WhatsApp Documents",
            ],
        );
        assert_eq!(
            command,
            "rm '-rf' '/sdcard/Android/media/com.whatsapp/WhatsApp Documents'"
        );
    }

    #[test]
    fn renders_adb_command_line_with_quotes() {
        let command = render_command_line(
            "C:\\platform-tools\\adb.exe",
            &["shell", "ls '/sdcard/WhatsApp Documents'"],
        );
        assert_eq!(
            command,
            "C:\\platform-tools\\adb.exe shell \"ls '/sdcard/WhatsApp Documents'\""
        );
    }
}
