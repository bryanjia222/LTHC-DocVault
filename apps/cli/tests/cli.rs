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
        .stdout(contains("import"))
        .stdout(contains("restore"));
}

#[test]
fn init_then_list_empty_vault() {
    let temp_dir = tempfile::tempdir().unwrap();

    docvault(temp_dir.path())
        .arg("init")
        .assert()
        .success()
        .stdout(contains("DocVault initialized"));

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
        fs::read(export_dir.join("report.docx")).unwrap(),
        b"version one"
    );
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
    command
        .env("DOCVAULT_ROOT_DIR", root)
        .env("DOCVAULT_BACKUP_BACKEND", "local-copy");
    command
}

fn write_source(root: &Path, file_name: &str, contents: &[u8]) -> std::path::PathBuf {
    let path = root.join(file_name);
    fs::write(&path, contents).unwrap();
    path
}

fn json_stdout(output: std::process::Output) -> Value {
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
