use std::{env, path::PathBuf, process::ExitCode};

use anyhow::{Result, anyhow, bail};
use docvault_core::DocVault;
use docvault_storage::{DocumentRef, VaultPaths, VaultStorage};
use docvault_types::CommitMetadata;
use serde_json::json;
use tracing::error;
use tracing_subscriber::filter::LevelFilter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Table,
    Json,
}

impl OutputFormat {
    fn parse(args: &[String]) -> Result<Self> {
        match parse_option_value(args, "--format").as_deref() {
            Some("table") | None => Ok(Self::Table),
            Some("json") => Ok(Self::Json),
            Some(other) => bail!("Unsupported format: {other}. Use table or json."),
        }
    }
}

fn main() -> ExitCode {
    init_tracing();
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            error!(%error, "docvault command failed");
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

fn init_tracing() {
    let level = env::var("DOCVAULT_LOG_LEVEL")
        .ok()
        .and_then(|value| value.parse::<LevelFilter>().ok())
        .unwrap_or(LevelFilter::WARN);
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();
}

fn run(args: Vec<String>) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("init") => {
            let storage = VaultStorage::init(VaultPaths::from_env())?;
            println!(
                "DocVault initialized at {}",
                storage.paths().root_dir.display()
            );
            Ok(())
        }
        Some("commit") => {
            if has_flag(&args, "--track") {
                bail!("--track was removed from commit");
            }
            let source_path = args.get(1).ok_or_else(|| anyhow!(usage()))?;
            let document_ref = parse_commit_ref(&args)?;
            let metadata = CommitMetadata {
                author: parse_option_value(&args, "--author"),
                note: parse_option_value(&args, "--note"),
            };
            let storage = VaultStorage::init(VaultPaths::from_env())?;
            let vault = DocVault::new(storage);
            let (_, version) =
                vault.commit_document(source_path, document_ref.clone(), metadata)?;
            println!(
                "Committed {} as {}",
                document_ref.display_name(),
                version.id
            );
            Ok(())
        }
        Some("list") => {
            let format = OutputFormat::parse(&args)?;
            let storage = VaultStorage::open(VaultPaths::from_env())?;
            let vault = DocVault::new(storage);
            let documents = vault.list_documents()?;
            match format {
                OutputFormat::Table => {
                    if documents.is_empty() {
                        println!("No documents found");
                    } else {
                        let rows = documents
                            .iter()
                            .map(|document| {
                                vec![
                                    document.id.as_str().to_owned(),
                                    document.name.clone(),
                                    document.current_version_id.clone().unwrap_or_default(),
                                    document.created_at.to_string(),
                                ]
                            })
                            .collect::<Vec<_>>();
                        print_table(&["ID", "NAME", "CURRENT", "CREATED_AT"], &rows);
                    }
                }
                OutputFormat::Json => {
                    let rows = documents
                        .iter()
                        .map(|document| {
                            json!({
                                "id": document.id.as_str(),
                                "name": document.name,
                                "current_version_id": document.current_version_id,
                                "created_at": document.created_at,
                            })
                        })
                        .collect::<Vec<_>>();
                    println!("{}", serde_json::to_string_pretty(&rows)?);
                }
            }
            Ok(())
        }
        Some("versions") => {
            let format = OutputFormat::parse(&args)?;
            let document_ref = parse_document_ref_arg(&args, 1)?;
            let storage = VaultStorage::open(VaultPaths::from_env())?;
            let vault = DocVault::new(storage);
            let versions = vault.list_versions(&document_ref)?;
            match format {
                OutputFormat::Table => {
                    if versions.is_empty() {
                        println!("No versions found");
                    } else {
                        let rows = versions
                            .iter()
                            .map(|version| {
                                vec![
                                    version.id.clone(),
                                    version.number.to_string(),
                                    version.parent_version_id.clone().unwrap_or_default(),
                                    version.backup_backend.clone(),
                                    version.original_filename.clone(),
                                    version
                                        .snapshot_id
                                        .as_deref()
                                        .unwrap_or(version.archive_reference.as_str())
                                        .to_owned(),
                                    version.author.clone().unwrap_or_default(),
                                    version.note.clone().unwrap_or_default(),
                                ]
                            })
                            .collect::<Vec<_>>();
                        print_table(
                            &[
                                "ID",
                                "NUMBER",
                                "PARENT",
                                "BACKEND",
                                "FILENAME",
                                "REFERENCE",
                                "AUTHOR",
                                "NOTE",
                            ],
                            &rows,
                        );
                    }
                }
                OutputFormat::Json => {
                    let rows = versions
                        .iter()
                        .map(|version| {
                            json!({
                                "id": version.id,
                                "document_id": version.document_id.as_str(),
                                "number": version.number,
                                "parent_version_id": version.parent_version_id,
                                "original_filename": version.original_filename,
                                "archive_reference": version.archive_reference,
                                "backup_backend": version.backup_backend,
                                "snapshot_id": version.snapshot_id,
                                "author": version.author,
                                "note": version.note,
                                "created_at": version.created_at,
                            })
                        })
                        .collect::<Vec<_>>();
                    println!("{}", serde_json::to_string_pretty(&rows)?);
                }
            }
            Ok(())
        }
        Some("export") | Some("restore") => {
            let document_ref = parse_document_ref_arg(&args, 1)?;
            let requested_version = parse_option_value(&args, "--version")
                .or_else(|| {
                    args.get(2)
                        .filter(|value| !value.starts_with("--"))
                        .cloned()
                })
                .unwrap_or_else(|| "latest".to_owned());
            let output_path = parse_option_value(&args, "--output")
                .map(PathBuf::from)
                .ok_or_else(|| anyhow!(usage()))?;
            let storage = VaultStorage::open(VaultPaths::from_env())?;
            let vault = DocVault::new(storage);
            let exported = vault.export_version(&document_ref, &requested_version, output_path)?;
            println!("Exported to {}", exported.display());
            Ok(())
        }
        Some("checkout") => {
            let document_ref = parse_document_ref_arg(&args, 1)?;
            let requested_version = parse_option_value(&args, "--version")
                .or_else(|| {
                    args.get(2)
                        .filter(|value| !value.starts_with("--"))
                        .cloned()
                })
                .unwrap_or_else(|| "latest".to_owned());
            let output_path = parse_option_value(&args, "--output").map(PathBuf::from);
            let storage = VaultStorage::open(VaultPaths::from_env())?;
            let vault = DocVault::new(storage);
            let exported =
                vault.checkout_version(&document_ref, &requested_version, output_path.as_ref())?;
            match exported {
                Some(path) => println!(
                    "Checked out {requested_version} and exported to {}",
                    path.display()
                ),
                None => println!("Checked out {requested_version}"),
            }
            Ok(())
        }
        Some("current") => {
            let format = OutputFormat::parse(&args)?;
            let document_ref = parse_document_ref_arg(&args, 1)?;
            let storage = VaultStorage::open(VaultPaths::from_env())?;
            let vault = DocVault::new(storage);
            let current = vault.current_version(&document_ref)?;
            match (format, current) {
                (OutputFormat::Table, Some(version)) => print_versions_table(&[version]),
                (OutputFormat::Table, None) => println!("No current version"),
                (OutputFormat::Json, Some(version)) => println!(
                    "{}",
                    serde_json::to_string_pretty(&version_to_json(&version))?
                ),
                (OutputFormat::Json, None) => println!("null"),
            }
            Ok(())
        }
        _ => bail!("{}", usage()),
    }
}

trait DocumentRefDisplay {
    fn display_name(&self) -> &str;
}

impl DocumentRefDisplay for DocumentRef {
    fn display_name(&self) -> &str {
        match self {
            DocumentRef::Name(name) => name,
            DocumentRef::NewName(name) => name,
            DocumentRef::IdPrefix(id_prefix) => id_prefix,
            DocumentRef::NameAndIdPrefix { name, .. } => name,
        }
    }
}

fn parse_commit_ref(args: &[String]) -> Result<DocumentRef> {
    if let Some(id_prefix) = parse_option_value(args, "--id") {
        return Ok(DocumentRef::IdPrefix(id_prefix));
    }

    let value = parse_option_value(args, "--name")
        .or_else(|| args.get(2).cloned())
        .ok_or_else(|| anyhow!(usage()))?;
    if has_flag(args, "--new") {
        if value.contains('@') {
            bail!("--new requires a plain document name");
        }
        return Ok(DocumentRef::NewName(value));
    }
    parse_document_ref_value(&value)
}

fn parse_document_ref_arg(args: &[String], position: usize) -> Result<DocumentRef> {
    if let Some(id_prefix) = parse_option_value(args, "--id") {
        return Ok(DocumentRef::IdPrefix(id_prefix));
    }

    let value = args.get(position).ok_or_else(|| anyhow!(usage()))?;
    if value.starts_with("--") {
        bail!("{}", usage());
    }
    parse_document_ref_value(value)
}

fn parse_document_ref_value(value: &str) -> Result<DocumentRef> {
    if let Some((name, id_prefix)) = value.rsplit_once('@') {
        if name.is_empty() || id_prefix.is_empty() {
            bail!("Invalid document reference: {value}");
        }
        Ok(DocumentRef::NameAndIdPrefix {
            name: name.to_owned(),
            id_prefix: id_prefix.to_owned(),
        })
    } else {
        Ok(DocumentRef::Name(value.to_owned()))
    }
}

fn parse_option_value(args: &[String], option: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == option)
        .map(|window| window[1].clone())
}

fn has_flag(args: &[String], option: &str) -> bool {
    args.iter().any(|arg| arg == option)
}

fn print_table(headers: &[&str], rows: &[Vec<String>]) {
    let normalized_rows = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| normalize_cell(cell))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let widths = headers
        .iter()
        .enumerate()
        .map(|(index, header)| {
            normalized_rows
                .iter()
                .filter_map(|row| row.get(index))
                .map(String::len)
                .max()
                .unwrap_or(0)
                .max(header.len())
        })
        .collect::<Vec<_>>();

    print_table_row(
        &headers
            .iter()
            .map(|header| header.to_string())
            .collect::<Vec<_>>(),
        &widths,
    );
    print_table_row(
        &widths
            .iter()
            .map(|width| "-".repeat(*width))
            .collect::<Vec<_>>(),
        &widths,
    );
    for row in normalized_rows {
        print_table_row(&row, &widths);
    }
}

fn print_versions_table(versions: &[docvault_types::Version]) {
    let rows = versions
        .iter()
        .map(|version| {
            vec![
                version.id.clone(),
                version.number.to_string(),
                version.parent_version_id.clone().unwrap_or_default(),
                version.backup_backend.clone(),
                version.original_filename.clone(),
                version
                    .snapshot_id
                    .as_deref()
                    .unwrap_or(version.archive_reference.as_str())
                    .to_owned(),
                version.author.clone().unwrap_or_default(),
                version.note.clone().unwrap_or_default(),
            ]
        })
        .collect::<Vec<_>>();
    print_table(
        &[
            "ID",
            "NUMBER",
            "PARENT",
            "BACKEND",
            "SOURCE",
            "REFERENCE",
            "AUTHOR",
            "NOTE",
        ],
        &rows,
    );
}

fn version_to_json(version: &docvault_types::Version) -> serde_json::Value {
    json!({
        "id": version.id,
        "document_id": version.document_id.as_str(),
        "number": version.number,
        "parent_version_id": version.parent_version_id,
        "original_filename": version.original_filename,
        "archive_reference": version.archive_reference,
        "backup_backend": version.backup_backend,
        "snapshot_id": version.snapshot_id,
        "author": version.author,
        "note": version.note,
        "created_at": version.created_at,
    })
}

fn print_table_row(row: &[String], widths: &[usize]) {
    for (index, width) in widths.iter().enumerate() {
        if index > 0 {
            print!("  ");
        }
        let value = row.get(index).map(String::as_str).unwrap_or("");
        print!("{value:<width$}");
    }
    println!();
}

fn normalize_cell(value: &str) -> String {
    value.replace(['\r', '\n', '\t'], " ")
}

fn usage() -> &'static str {
    "Usage:
  docvault init
  docvault commit <path> <name|name@id-prefix> [--author <name>] [--note <text>] [--new]
  docvault commit <path> --name <name> [--author <name>] [--note <text>] [--new]
  docvault commit <path> --id <id-prefix> [--author <name>] [--note <text>]
  docvault list [--format table|json]
  docvault versions <name|name@id-prefix|--id <id-prefix>> [--format table|json]
  docvault current <name|name@id-prefix|--id <id-prefix>> [--format table|json]
  docvault export <name|name@id-prefix|--id <id-prefix>> [version|--version <version>] --output <path>
  docvault checkout <name|name@id-prefix|--id <id-prefix>> [version|--version <version>] [--output <path>]"
}
