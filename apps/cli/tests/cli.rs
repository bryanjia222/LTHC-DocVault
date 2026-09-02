use assert_cmd::Command;
use predicates::str::contains;
use serde_json::Value;
use std::{fs, path::Path};

#[test]
fn help_lists_core_commands() {
    let mut command = Command::cargo_bin("docvault").unwrap();

    command
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("init"))
        .stdout(contains("commit"))
        .stdout(contains("checkout"));
}

#[test]
fn command_failure_is_persisted_to_cli_log() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("vault");

    docvault(&root).arg("init").assert().success();

    let missing_source = root.join("missing.docx");
    docvault(&root)
        .args([
            "commit",
            missing_source.to_str().unwrap(),
            "--name",
            "report",
        ])
        .assert()
        .failure()
        .stderr(contains("OOXML error: I/O error:"));

    let log_dir = root.join("logs");
    let log_path = fs::read_dir(&log_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("docvault-cli.log"))
        })
        .expect("rolling CLI log file");
    let log = fs::read_to_string(&log_path).unwrap();
    assert!(log.contains("docvault command failed"));
}

#[test]
fn parse_failure_is_persisted_to_cli_log() {
    // An explicit root is trusted even when the subcommand itself is invalid;
    // this keeps the diagnostic out of the developer's real default vault.
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("vault");

    docvault(&root)
        .arg("definitely-not-a-command")
        .assert()
        .failure()
        .stderr(contains("unrecognized subcommand"));

    let log_dir = root.join("logs");
    let log_path = fs::read_dir(&log_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("docvault-cli.log"))
        })
        .expect("rolling CLI log file");
    let log = fs::read_to_string(&log_path).unwrap();
    assert!(log.contains("command-line parse failed"));
}

#[test]
fn init_then_list_empty_vault() {
    let temp_dir = tempfile::tempdir().unwrap();

    docvault(temp_dir.path())
        .arg("init")
        .assert()
        .success()
        .stdout(contains("DocVault initialized"))
        .stdout(contains("Backend: local-copy"))
        .stdout(contains("Repository:"));

    docvault(temp_dir.path())
        .arg("list")
        .assert()
        .success()
        .stdout(contains("No documents found"));
}

#[test]
fn commit_list_versions_and_export_workflow() {
    let temp_dir = tempfile::tempdir().unwrap();
    let source = write_source(temp_dir.path(), "report.docx", b"version one");
    let export_dir = temp_dir.path().join("exports");

    docvault(temp_dir.path())
        .args([
            "commit",
            source.to_str().unwrap(),
            "--name",
            "quarterly report",
        ])
        .assert()
        .success()
        .stdout(contains("Committed quarterly report as v1"));

    let documents = json_stdout(
        docvault(temp_dir.path())
            .args(["list", "--format", "json"])
            .output()
            .unwrap(),
    );
    assert_eq!(documents[0]["name"], "quarterly report");
    assert_eq!(documents[0]["current_version_id"], "v1");

    let versions = json_stdout(
        docvault(temp_dir.path())
            .args(["versions", "quarterly report", "--format", "json"])
            .output()
            .unwrap(),
    );
    assert_eq!(versions[0]["id"], "v1");
    assert_eq!(versions[0]["original_filename"], "report.docx");
    assert_eq!(
        versions[0]["manifest"]["entries"][0]["path"],
        "[Content_Types].xml"
    );

    docvault(temp_dir.path())
        .args([
            "export",
            "quarterly report",
            "v1",
            "--output",
            export_dir.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(contains("Exported to"));
    assert_eq!(
        read_document_xml(&export_dir.join("report.docx")),
        b"version one"
    );
}

#[test]
fn config_show_outputs_effective_paths_as_json() {
    let temp_dir = tempfile::tempdir().unwrap();
    docvault(temp_dir.path()).arg("init").assert().success();

    let config = json_stdout(
        docvault(temp_dir.path())
            .args(["config", "show", "--format", "json"])
            .output()
            .unwrap(),
    );

    assert_eq!(config["backend"], "local-copy");
    assert!(config["data_dir"].as_str().unwrap().contains("data"));
    assert!(config["repo_dir"].as_str().unwrap().contains("repo"));
    assert!(config["db_path"].as_str().unwrap().contains("db.sqlite"));
}

#[test]
fn commit_author_note_are_visible_in_versions_json() {
    let temp_dir = tempfile::tempdir().unwrap();
    let source = write_source(temp_dir.path(), "report.docx", b"version one");

    docvault(temp_dir.path())
        .args([
            "commit",
            source.to_str().unwrap(),
            "--name",
            "report",
            "--author",
            "Bryan",
            "--note",
            "initial version",
        ])
        .assert()
        .success();

    let versions = json_stdout(
        docvault(temp_dir.path())
            .args(["versions", "report", "--format", "json"])
            .output()
            .unwrap(),
    );

    assert_eq!(versions[0]["author"], "Bryan");
    assert_eq!(versions[0]["note"], "initial version");
}

#[test]
fn checkout_changes_current_but_export_does_not() {
    let temp_dir = tempfile::tempdir().unwrap();
    let first = write_source(temp_dir.path(), "report.docx", b"version one");
    let second = write_source(temp_dir.path(), "report-2.docx", b"version two");

    docvault(temp_dir.path())
        .args(["commit", first.to_str().unwrap(), "--name", "report"])
        .assert()
        .success();
    docvault(temp_dir.path())
        .args(["commit", second.to_str().unwrap(), "--name", "report"])
        .assert()
        .success();

    docvault(temp_dir.path())
        .args([
            "export",
            "report",
            "v1",
            "--output",
            temp_dir.path().join("exported.docx").to_str().unwrap(),
        ])
        .assert()
        .success();
    let current_after_export = json_stdout(
        docvault(temp_dir.path())
            .args(["current", "report", "--format", "json"])
            .output()
            .unwrap(),
    );
    assert_eq!(current_after_export["id"], "v2");

    docvault(temp_dir.path())
        .args(["checkout", "report", "v1"])
        .assert()
        .success()
        .stdout(contains("Checked out v1"));
    let current_after_checkout = json_stdout(
        docvault(temp_dir.path())
            .args(["current", "report", "--format", "json"])
            .output()
            .unwrap(),
    );
    assert_eq!(current_after_checkout["id"], "v1");
}

#[test]
fn json_format_outputs_parseable_json() {
    let temp_dir = tempfile::tempdir().unwrap();
    let source = write_source(temp_dir.path(), "report.docx", b"version one");
    docvault(temp_dir.path())
        .args(["commit", source.to_str().unwrap(), "--name", "report"])
        .assert()
        .success();

    for args in [
        vec!["list", "--format", "json"],
        vec!["versions", "report", "--format", "json"],
        vec!["current", "report", "--format", "json"],
    ] {
        json_stdout(docvault(temp_dir.path()).args(args).output().unwrap());
    }
}

fn docvault(root: &Path) -> Command {
    let mut command = Command::cargo_bin("docvault").unwrap();
    // Config comes from the vault's config.toml + explicit flags (no env vars).
    // --root-dir points the CLI at the test vault; the default backend is
    // local-copy, so no restic binary is needed.
    command.args(["--root-dir", root.to_str().unwrap()]);
    command
}

fn write_source(root: &Path, file_name: &str, contents: &[u8]) -> std::path::PathBuf {
    let source_dir = root.join("package-source").join(file_name);
    fs::create_dir_all(source_dir.join("word")).unwrap();
    fs::write(source_dir.join("[Content_Types].xml"), b"types").unwrap();
    fs::write(source_dir.join("word").join("document.xml"), contents).unwrap();
    let path = root.join(file_name);
    docvault_ooxml::pack_package(source_dir, &path).unwrap();
    path
}

fn read_document_xml(package_path: &Path) -> Vec<u8> {
    let temp_dir = tempfile::tempdir().unwrap();
    docvault_ooxml::unpack_package(package_path, temp_dir.path()).unwrap();
    fs::read(temp_dir.path().join("word").join("document.xml")).unwrap()
}

fn json_stdout(output: std::process::Output) -> Value {
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
