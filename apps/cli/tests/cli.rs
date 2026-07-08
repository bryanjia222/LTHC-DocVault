use assert_cmd::Command;
use predicates::str::contains;

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

    let mut init = Command::cargo_bin("docvault").unwrap();
    init.env("DOCVAULT_ROOT_DIR", temp_dir.path())
        .env("DOCVAULT_BACKUP_BACKEND", "local-copy")
        .arg("init")
        .assert()
        .success()
        .stdout(contains("DocVault initialized"));

    let mut list = Command::cargo_bin("docvault").unwrap();
    list.env("DOCVAULT_ROOT_DIR", temp_dir.path())
        .env("DOCVAULT_BACKUP_BACKEND", "local-copy")
        .arg("list")
        .assert()
        .success()
        .stdout(contains("No documents found"));
}
