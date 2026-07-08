use std::{path::Path, process::Command};

use docvault_types::Document;
use serde_json::Value;
use tracing::{debug, error, info};

use crate::{ResticError, StorageError, StorageResult, VaultStorage};

impl VaultStorage {
    pub(crate) fn ensure_restic_repo(&self) -> StorageResult<()> {
        debug!(repo = %self.paths.repo_dir.display(), "checking restic repository");
        let config = self.run_restic(["cat", "config"])?;
        if config.status.success() {
            return Ok(());
        }

        info!(repo = %self.paths.repo_dir.display(), "initializing restic repository");
        let init = self.run_restic(["init"])?;
        if init.status.success() {
            Ok(())
        } else {
            let error = restic_failed("init", init.stderr);
            error!(repo = %self.paths.repo_dir.display(), %error, "failed to initialize restic repository");
            Err(error)
        }
    }

    pub(crate) fn restic_backup(
        &self,
        document: &Document,
        version_id: &str,
        package_dir: &Path,
    ) -> StorageResult<String> {
        let parent = package_dir
            .parent()
            .ok_or_else(|| StorageError::InvalidFileName(package_dir.to_path_buf()))?;
        let tag = format!("docvault:{}:{version_id}", document.id.as_str());
        let output = self.run_restic_in_dir(
            [
                "backup",
                "--json",
                "--tag",
                tag.as_str(),
                "--host",
                "docvault",
                "package",
            ],
            parent,
        )?;
        if !output.status.success() {
            let error = restic_failed("backup", output.stderr);
            error!(
                document_id = document.id.as_str(),
                version_id,
                %error,
                "restic backup failed"
            );
            return Err(error);
        }
        let snapshot_id = snapshot_id_from_backup_json(&output.stdout)?;
        info!(
            document_id = document.id.as_str(),
            version_id,
            snapshot_id = snapshot_id.as_str(),
            "restic backup completed"
        );
        Ok(snapshot_id)
    }

    pub(crate) fn restic_restore(&self, snapshot_id: &str, target: &Path) -> StorageResult<()> {
        std::fs::create_dir_all(target)?;
        let target = target.display().to_string();
        let output = self.run_restic(["restore", snapshot_id, "--target", target.as_str()])?;
        if output.status.success() {
            info!(snapshot_id, target, "restic restore completed");
            Ok(())
        } else {
            let error = restic_failed("restore", output.stderr);
            error!(snapshot_id, target, %error, "restic restore failed");
            Err(error)
        }
    }

    fn run_restic<const N: usize>(&self, args: [&str; N]) -> StorageResult<std::process::Output> {
        self.run_restic_command(args, None)
    }

    fn run_restic_in_dir<const N: usize>(
        &self,
        args: [&str; N],
        current_dir: &Path,
    ) -> StorageResult<std::process::Output> {
        self.run_restic_command(args, Some(current_dir))
    }

    fn run_restic_command<const N: usize>(
        &self,
        args: [&str; N],
        current_dir: Option<&Path>,
    ) -> StorageResult<std::process::Output> {
        debug!(
            restic = %self.settings.restic_path.display(),
            repo = %self.paths.repo_dir.display(),
            args = ?args,
            current_dir = ?current_dir,
            "running restic command"
        );
        let mut command = Command::new(&self.settings.restic_path);
        command
            .args(["-r", self.paths.repo_dir.to_string_lossy().as_ref()])
            .args(args)
            .env("RESTIC_PASSWORD", &self.settings.restic_password)
            .env("RESTIC_CACHE_DIR", &self.paths.cache_dir);
        if let Some(current_dir) = current_dir {
            command.current_dir(current_dir);
        }
        Ok(command.output()?)
    }
}

fn restic_failed(command: &str, stderr: Vec<u8>) -> StorageError {
    ResticError::Failed {
        command: command.to_owned(),
        stderr: String::from_utf8_lossy(&stderr).trim().to_owned(),
    }
    .into()
}

fn snapshot_id_from_backup_json(stdout: &[u8]) -> StorageResult<String> {
    let output = String::from_utf8_lossy(stdout);
    for line in output.lines() {
        let value: Value = serde_json::from_str(line)?;
        if value.get("message_type").and_then(Value::as_str) == Some("summary")
            && let Some(snapshot_id) = value.get("snapshot_id").and_then(Value::as_str)
        {
            return Ok(snapshot_id.to_owned());
        }
    }
    Err(ResticError::SnapshotMissing.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_snapshot_id_from_restic_json_summary() {
        let output = br#"{"message_type":"status","percent_done":0}
{"message_type":"summary","snapshot_id":"abc123"}
"#;

        assert_eq!(snapshot_id_from_backup_json(output).unwrap(), "abc123");
    }
}
