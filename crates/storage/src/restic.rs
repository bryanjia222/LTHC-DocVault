use std::{
    io::Read,
    path::Path,
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use docvault_types::Document;
use serde_json::Value;
use tracing::{debug, error, info};

use crate::{ResticError, StorageError, StorageResult, VaultStorage};

/// Restic command ceilings. `cat config` / `init` / `version` are quick
/// metadata calls; backup/restore may move large data over a slow or cloud
/// link and need a generous bound. The poll loop checks the cancel flag and
/// the deadline this often.
const RESTIC_SHORT_TIMEOUT: Duration = Duration::from_secs(60);
const RESTIC_LONG_TIMEOUT: Duration = Duration::from_secs(600);
const RESTIC_POLL_INTERVAL: Duration = Duration::from_millis(100);

impl VaultStorage {
    pub(crate) fn ensure_restic_repo(&self, cancel: &AtomicBool) -> StorageResult<()> {
        debug!(repo = %self.paths.repo_dir.display(), "checking restic repository");
        let config = self.run_restic(["cat", "config"], cancel, RESTIC_SHORT_TIMEOUT)?;
        if config.status.success() {
            return Ok(());
        }

        info!(repo = %self.paths.repo_dir.display(), "initializing restic repository");
        let init = self.run_restic(["init"], cancel, RESTIC_SHORT_TIMEOUT)?;
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
        cancel: &AtomicBool,
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
            cancel,
            RESTIC_LONG_TIMEOUT,
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

    pub(crate) fn restic_restore(
        &self,
        snapshot_id: &str,
        target: &Path,
        cancel: &AtomicBool,
    ) -> StorageResult<()> {
        std::fs::create_dir_all(target)?;
        let target = target.display().to_string();
        let output = self.run_restic(
            ["restore", snapshot_id, "--target", target.as_str()],
            cancel,
            RESTIC_LONG_TIMEOUT,
        )?;
        if output.status.success() {
            info!(snapshot_id, target, "restic restore completed");
            Ok(())
        } else {
            let error = restic_failed("restore", output.stderr);
            error!(snapshot_id, target, %error, "restic restore failed");
            Err(error)
        }
    }

    fn run_restic<const N: usize>(
        &self,
        args: [&str; N],
        cancel: &AtomicBool,
        timeout: Duration,
    ) -> StorageResult<Output> {
        self.run_restic_command(args, None, cancel, timeout)
    }

    fn run_restic_in_dir<const N: usize>(
        &self,
        args: [&str; N],
        current_dir: &Path,
        cancel: &AtomicBool,
        timeout: Duration,
    ) -> StorageResult<Output> {
        self.run_restic_command(args, Some(current_dir), cancel, timeout)
    }

    /// Run a restic command, polling for completion so a cancel request or
    /// timeout can interrupt a stalled/cloud call (otherwise `Command::output`
    /// would block forever). stdout/stderr are drained on reader threads to
    /// avoid the child blocking on a full pipe buffer during long backups.
    fn run_restic_command<const N: usize>(
        &self,
        args: [&str; N],
        current_dir: Option<&Path>,
        cancel: &AtomicBool,
        timeout: Duration,
    ) -> StorageResult<Output> {
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
            .env("RESTIC_CACHE_DIR", &self.paths.cache_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(current_dir) = current_dir {
            command.current_dir(current_dir);
        }
        let mut child = command.spawn()?;
        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");
        let stdout_handle = thread::spawn(move || read_all(stdout));
        let stderr_handle = thread::spawn(move || read_all(stderr));

        let deadline = Instant::now() + timeout;
        let status = loop {
            if cancel.load(Ordering::Relaxed) {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_handle.join();
                let _ = stderr_handle.join();
                return Err(ResticError::Cancelled.into());
            }
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_handle.join();
                let _ = stderr_handle.join();
                return Err(ResticError::TimedOut.into());
            }
            thread::sleep(RESTIC_POLL_INTERVAL);
        };
        let stdout_buf = stdout_handle.join().unwrap_or_default();
        let stderr_buf = stderr_handle.join().unwrap_or_default();
        Ok(Output {
            status,
            stdout: stdout_buf,
            stderr: stderr_buf,
        })
    }

    /// Best-effort `restic version` capture for display. Empty when the binary
    /// is unavailable or exits non-zero. Cached once per vault session by
    /// `VaultStorage::init`/`open` rather than re-spawned on every config read.
    pub(crate) fn capture_restic_version(&self) -> String {
        let Ok(output) =
            self.run_restic(["version"], &crate::NEVER_CANCELLED, RESTIC_SHORT_TIMEOUT)
        else {
            return String::new();
        };
        if !output.status.success() {
            return String::new();
        }
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .unwrap_or("")
            .to_owned()
    }
}

fn read_all<R: Read>(mut reader: R) -> Vec<u8> {
    let mut buffer = Vec::new();
    let _ = reader.read_to_end(&mut buffer);
    buffer
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
    use std::{
        fs,
        path::Path,
        sync::Arc,
        sync::atomic::{AtomicBool, Ordering},
        thread,
        time::Duration,
    };

    use docvault_types::CommitMetadata;

    use crate::{BackupBackend, DocumentRef, VaultPaths};

    use super::*;

    #[test]
    fn extracts_snapshot_id_from_restic_json_summary() {
        let output = br#"{"message_type":"status","percent_done":0}
{"message_type":"summary","snapshot_id":"abc123"}
"#;

        assert_eq!(snapshot_id_from_backup_json(output).unwrap(), "abc123");
    }

    #[test]
    fn restic_backup_uses_expected_args_and_records_snapshot_id() {
        let temp_dir = tempfile::tempdir().unwrap();
        let paths = temp_paths(temp_dir.path());
        let log_path = temp_dir.path().join("restic.log");
        let restic_path = write_mock_restic(temp_dir.path(), &log_path, MockRestic::Success);
        write_restic_config(&paths, &restic_path);
        let source = write_ooxml_package(temp_dir.path(), "report.docx");
        let storage = VaultStorage::init(paths).unwrap();

        let (_, version) = storage
            .add_document_version(
                DocumentRef::Name("report".to_owned()),
                &source,
                CommitMetadata::default(),
                &crate::NEVER_CANCELLED,
            )
            .unwrap();

        assert_eq!(storage.backend(), BackupBackend::Restic);
        assert_eq!(version.snapshot_id.as_deref(), Some("snap123"));
        let log = fs::read_to_string(log_path).unwrap();
        assert!(log.contains("-r"));
        assert!(log.contains("backup"));
        assert!(log.contains("--json"));
        assert!(log.contains("--tag"));
        assert!(log.contains("docvault:"));
        assert!(log.contains("--host"));
        assert!(log.contains("package"));
    }

    #[test]
    fn restic_backup_failure_is_propagated() {
        let temp_dir = tempfile::tempdir().unwrap();
        let paths = temp_paths(temp_dir.path());
        let log_path = temp_dir.path().join("restic.log");
        let restic_path = write_mock_restic(temp_dir.path(), &log_path, MockRestic::BackupFails);
        write_restic_config(&paths, &restic_path);
        let source = write_ooxml_package(temp_dir.path(), "report.docx");
        let storage = VaultStorage::init(paths).unwrap();

        let error = storage
            .add_document_version(
                DocumentRef::Name("report".to_owned()),
                &source,
                CommitMetadata::default(),
                &crate::NEVER_CANCELLED,
            )
            .unwrap_err();

        assert!(matches!(
            error,
            StorageError::Restic(ResticError::Failed { command, stderr })
                if command == "backup" && stderr.contains("mock backup failed")
        ));
    }

    #[test]
    fn restic_version_cached_once_per_session() {
        let temp_dir = tempfile::tempdir().unwrap();
        let paths = temp_paths(temp_dir.path());
        let log_path = temp_dir.path().join("restic.log");
        let restic_path = write_mock_restic(temp_dir.path(), &log_path, MockRestic::Success);
        write_restic_config(&paths, &restic_path);
        let storage = VaultStorage::init(paths).unwrap();

        let first = storage.restic_version().to_owned();
        let second = storage.restic_version().to_owned();
        assert_eq!(first, "restic 0.19.1");
        assert_eq!(second, first);

        let log = fs::read_to_string(&log_path).unwrap();
        let version_calls = log.lines().filter(|line| line.contains(" version")).count();
        assert_eq!(
            version_calls, 1,
            "restic version should be invoked once at init, not on every read"
        );
    }

    /// A stalled restic call (cloud hang) must be interruptible by cancellation:
    /// the child is killed and `ResticError::Cancelled` propagates.
    #[test]
    fn restic_command_respects_cancellation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let paths = temp_paths(temp_dir.path());
        let log_path = temp_dir.path().join("restic.log");
        let restic_path = write_mock_restic(temp_dir.path(), &log_path, MockRestic::Hang);
        write_restic_config(&paths, &restic_path);
        let storage = VaultStorage::init(paths).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_for_thread = Arc::clone(&cancel);

        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            cancel_for_thread.store(true, Ordering::Relaxed);
        });

        let error = storage
            .run_restic(["backup"], &cancel, Duration::from_secs(30))
            .unwrap_err();
        handle.join().unwrap();

        assert!(matches!(
            error,
            StorageError::Restic(ResticError::Cancelled)
        ));
    }

    /// A stalled restic call that is not cancelled must hit the timeout ceiling
    /// rather than blocking forever.
    #[test]
    fn restic_command_times_out() {
        let temp_dir = tempfile::tempdir().unwrap();
        let paths = temp_paths(temp_dir.path());
        let log_path = temp_dir.path().join("restic.log");
        let restic_path = write_mock_restic(temp_dir.path(), &log_path, MockRestic::Hang);
        write_restic_config(&paths, &restic_path);
        let storage = VaultStorage::init(paths).unwrap();
        let cancel = AtomicBool::new(false);

        let error = storage
            .run_restic(["backup"], &cancel, Duration::from_millis(200))
            .unwrap_err();

        assert!(matches!(error, StorageError::Restic(ResticError::TimedOut)));
    }

    enum MockRestic {
        Success,
        BackupFails,
        /// Simulates a stalled/cloud call: `backup` never returns (so `try_wait`
        /// stays `None`); other subcommands behave like `Success` so init/open
        /// still complete.
        Hang,
    }

    fn temp_paths(root: &Path) -> VaultPaths {
        VaultPaths::new(root, root.join("data"), root.join("db.sqlite"))
    }

    fn write_restic_config(paths: &VaultPaths, restic_path: &Path) {
        fs::create_dir_all(&paths.root_dir).unwrap();
        fs::write(
            &paths.config_path,
            format!(
                "[storage]\nbackend = \"restic\"\ndata_dir = \"{}\"\nrepo_dir = \"{}\"\nrestic_path = \"{}\"\nrestic_password = \"test-password\"\n\n[database]\npath = \"{}\"\n",
                config_path(&paths.data_dir),
                config_path(&paths.repo_dir),
                config_path(restic_path),
                config_path(&paths.db_path)
            ),
        )
        .unwrap();
    }

    fn write_ooxml_package(root: &Path, file_name: &str) -> std::path::PathBuf {
        let source_dir = root.join("package-source");
        fs::create_dir_all(source_dir.join("word")).unwrap();
        fs::write(source_dir.join("[Content_Types].xml"), b"types").unwrap();
        fs::write(source_dir.join("word").join("document.xml"), b"document").unwrap();
        let package = root.join(file_name);
        docvault_ooxml::pack_package(&source_dir, &package).unwrap();
        package
    }

    fn write_mock_restic(root: &Path, log_path: &Path, behavior: MockRestic) -> std::path::PathBuf {
        let script_path = root.join(mock_restic_name());
        fs::write(&script_path, mock_restic_script(log_path, behavior)).unwrap();
        make_executable(&script_path);
        script_path
    }

    #[cfg(windows)]
    fn mock_restic_name() -> &'static str {
        "mock-restic.cmd"
    }

    #[cfg(not(windows))]
    fn mock_restic_name() -> &'static str {
        "mock-restic.sh"
    }

    #[cfg(windows)]
    fn mock_restic_script(log_path: &Path, behavior: MockRestic) -> String {
        if matches!(behavior, MockRestic::Hang) {
            return format!(
                "@echo off\n\
                 echo %*>>\"{}\"\n\
                 if \"%3\"==\"backup\" goto hang\n\
                 if \"%3\"==\"version\" echo restic 0.19.1\n\
                 exit /b 0\n\
                 :hang\n\
                 ping -n 2 127.0.0.1 > nul\n\
                 goto hang\n",
                log_path.display()
            );
        }
        let failure = if matches!(behavior, MockRestic::BackupFails) {
            "if \"%3\"==\"backup\" (\n  echo mock backup failed 1>&2\n  exit /b 9\n)\n"
        } else {
            ""
        };
        format!(
            "@echo off\n\
             echo %*>>\"{}\"\n\
             {}\n\
             if \"%3\"==\"backup\" echo {{\"message_type\":\"summary\",\"snapshot_id\":\"snap123\"}}\n\
             if \"%3\"==\"version\" echo restic 0.19.1\n\
             exit /b 0\n",
            log_path.display(),
            failure
        )
    }

    #[cfg(not(windows))]
    fn mock_restic_script(log_path: &Path, behavior: MockRestic) -> String {
        if matches!(behavior, MockRestic::Hang) {
            return format!(
                "#!/bin/sh\n\
                 printf '%s\\n' \"$*\" >> '{}'\n\
                 if [ \"$3\" = \"backup\" ]; then while :; do sleep 1; done; fi\n\
                 if [ \"$3\" = \"version\" ]; then printf '%s\\n' 'restic 0.19.1'; fi\n\
                 exit 0\n",
                log_path.display()
            );
        }
        let failure = if matches!(behavior, MockRestic::BackupFails) {
            "if [ \"$3\" = \"backup\" ]; then echo mock backup failed >&2; exit 9; fi\n"
        } else {
            ""
        };
        format!(
            "#!/bin/sh\n\
             printf '%s\\n' \"$*\" >> '{}'\n\
             {}\
             if [ \"$3\" = \"backup\" ]; then printf '%s\\n' '{{\"message_type\":\"summary\",\"snapshot_id\":\"snap123\"}}'; fi\n\
             if [ \"$3\" = \"version\" ]; then printf '%s\\n' 'restic 0.19.1'; fi\n\
             exit 0\n",
            log_path.display(),
            failure
        )
    }

    #[cfg(windows)]
    fn make_executable(_path: &Path) {}

    #[cfg(not(windows))]
    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }

    fn config_path(path: &Path) -> String {
        path.display().to_string().replace('\\', "/")
    }
}
