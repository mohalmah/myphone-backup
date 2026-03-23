use super::models::{
    DeviceConnectionState, DeviceInfo, RemoteDirectory, RemoteFile, RemoteFolderEntry,
    RemoteFolderEntryKind, RemoteFolderPreview,
};
use anyhow::{Context, Result, anyhow, bail};
use std::path::Path;
use std::process::Command;

pub struct AdbController {
    executable: String,
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
    pub fn new(executable: String) -> Self {
        Self { executable }
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

    pub fn list_remote_files(&self, source_directory: &str) -> Result<Vec<RemoteFile>> {
        let output = self.run_shell_command("ls", &["-1", "-p", source_directory])?;

        let mut files = Vec::new();
        for line in output
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            if line.ends_with('/') {
                continue;
            }

            let remote_path = format!("{}/{}", source_directory.trim_end_matches('/'), line);
            let (size_bytes, modified_epoch_seconds) = self.remote_file_metadata(&remote_path)?;

            files.push(RemoteFile {
                name: line.to_string(),
                remote_path,
                size_bytes,
                modified_epoch_seconds,
            });
        }

        files.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
        Ok(files)
    }

    pub fn list_remote_directories(&self, source_directory: &str) -> Result<Vec<RemoteDirectory>> {
        let output = self.run_shell_command("ls", &["-1", "-p", source_directory])?;

        let mut directories = Vec::new();
        let base = source_directory.trim_end_matches('/');
        for line in output
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            if !line.ends_with('/') {
                continue;
            }

            let name = line.trim_end_matches('/').to_string();
            let full_path = if base.is_empty() {
                format!("/{}", name)
            } else {
                format!("{base}/{}", name)
            };

            directories.push(RemoteDirectory { name, full_path });
        }

        directories.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
        Ok(directories)
    }

    pub fn remote_file_metadata(&self, remote_path: &str) -> Result<(u64, Option<i64>)> {
        let output = self.run_shell_command("stat", &["-c", "%s|%Y", remote_path])?;
        let trimmed = output.trim();
        let mut parts = trimmed.split('|');

        let size = parts
            .next()
            .context("Missing remote file size")?
            .trim()
            .parse::<u64>()
            .with_context(|| {
                format!("Invalid remote size metadata for {remote_path}: {trimmed}")
            })?;
        let modified = parts
            .next()
            .and_then(|value| value.trim().parse::<i64>().ok());

        Ok((size, modified))
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

        let output = self.run_shell_command("sh", &["-c", script, "preview-folder", folder_path])?;
        if output.lines().any(|line| line.trim() == "__NOT_DIRECTORY__") {
            bail!("Remote folder does not exist: {folder_path}");
        }

        let mut entries = Vec::new();
        let mut file_count = 0usize;
        let mut directory_count = 0usize;
        let mut total_file_bytes = 0u64;

        for line in output.lines().map(str::trim).filter(|line| !line.is_empty()) {
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

        entries.sort_by(|left, right| left.full_path.to_lowercase().cmp(&right.full_path.to_lowercase()));

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

    fn run_shell_command(&self, command: &str, args: &[&str]) -> Result<String> {
        let command_line = build_shell_command(command, args);
        let output = self.run_command(&["shell", &command_line])?;
        Ok(output.stdout)
    }

    fn run_command(&self, args: &[&str]) -> Result<CommandResult> {
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

#[cfg(test)]
mod tests {
    use super::{build_shell_command, extract_md5};

    #[test]
    fn extracts_md5_from_md5sum_output() {
        let hash = extract_md5("0cc175b9c0f1b6a831c399e269772661  /sdcard/file.mp4");
        assert_eq!(hash.as_deref(), Some("0cc175b9c0f1b6a831c399e269772661"));
    }

    #[test]
    fn builds_shell_command_with_quoted_spaces() {
        let command = build_shell_command(
            "ls",
            &["-1", "-p", "/sdcard/Android/media/com.whatsapp/WhatsApp Documents"],
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
            &["-rf", "/sdcard/Android/media/com.whatsapp/WhatsApp Documents"],
        );
        assert_eq!(
            command,
            "rm '-rf' '/sdcard/Android/media/com.whatsapp/WhatsApp Documents'"
        );
    }
}
