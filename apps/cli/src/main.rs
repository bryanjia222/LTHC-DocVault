use std::{
    env,
    path::{Path, PathBuf},
    process::ExitCode,
};

use docvault_core::DocVault;
use docvault_storage::{DocumentRef, VaultPaths, VaultStorage};
use docvault_types::ImportMetadata;
use serde_json::json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Table,
    Json,
}

impl OutputFormat {
    fn parse(args: &[String]) -> Result<Self, String> {
        match parse_option_value(args, "--format").as_deref() {
            Some("table") | None => Ok(Self::Table),
            Some("json") => Ok(Self::Json),
            Some(other) => Err(format!("Unsupported format: {other}. Use table or json.")),
        }
    }
}

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("init") => {
            let storage =
                VaultStorage::init(VaultPaths::from_env()).map_err(|error| error.to_string())?;
            println!(
                "DocVault initialized at {}",
                storage.paths().root_dir.display()
            );
            Ok(())
        }
        Some("import") => {
            let source_path = args.get(1).ok_or_else(|| usage().to_owned())?;
            let document_ref = parse_import_ref(&args)?;
            let metadata = ImportMetadata {
                author: parse_option_value(&args, "--author"),
                note: parse_option_value(&args, "--note"),
            };
            let storage =
                VaultStorage::init(VaultPaths::from_env()).map_err(|error| error.to_string())?;
            let vault = DocVault::new(storage);
            let (_, version) = vault
                .import_document(source_path, document_ref.clone(), metadata)
                .map_err(|error| error.to_string())?;
            println!("Imported {} as {}", document_ref.display_name(), version.id);
            Ok(())
        }
        Some("list") => {
            let format = OutputFormat::parse(&args)?;
            let storage =
                VaultStorage::open(VaultPaths::from_env()).map_err(|error| error.to_string())?;
            let vault = DocVault::new(storage);
            let documents = vault.list_documents().map_err(|error| error.to_string())?;
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
                                    document.source_path.clone(),
                                    document.created_at.to_string(),
                                ]
                            })
                            .collect::<Vec<_>>();
                        print_table(&["ID", "NAME", "CURRENT", "SOURCE", "CREATED_AT"], &rows);
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
                                "source_path": document.source_path,
                                "created_at": document.created_at,
                            })
                        })
                        .collect::<Vec<_>>();
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&rows).map_err(|error| error.to_string())?
                    );
                }
            }
            Ok(())
        }
        Some("versions") => {
            let format = OutputFormat::parse(&args)?;
            let document_ref = parse_document_ref_arg(&args, 1)?;
            let storage =
                VaultStorage::open(VaultPaths::from_env()).map_err(|error| error.to_string())?;
            let vault = DocVault::new(storage);
            let versions = vault
                .list_versions(&document_ref)
                .map_err(|error| error.to_string())?;
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
                                    version.original_path.clone(),
                                    version
                                        .snapshot_id
                                        .as_deref()
                                        .unwrap_or(version.archive_path.as_str())
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
                                "original_path": version.original_path,
                                "archive_path": version.archive_path,
                                "backup_backend": version.backup_backend,
                                "snapshot_id": version.snapshot_id,
                                "author": version.author,
                                "note": version.note,
                                "created_at": version.created_at,
                            })
                        })
                        .collect::<Vec<_>>();
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&rows).map_err(|error| error.to_string())?
                    );
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
                .ok_or_else(|| usage().to_owned())?;
            let storage =
                VaultStorage::open(VaultPaths::from_env()).map_err(|error| error.to_string())?;
            let vault = DocVault::new(storage);
            let exported = vault
                .export_version(&document_ref, &requested_version, output_path)
                .map_err(|error| error.to_string())?;
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
            let storage =
                VaultStorage::open(VaultPaths::from_env()).map_err(|error| error.to_string())?;
            let vault = DocVault::new(storage);
            let exported = vault
                .checkout_version(&document_ref, &requested_version, output_path.as_ref())
                .map_err(|error| error.to_string())?;
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
            let storage =
                VaultStorage::open(VaultPaths::from_env()).map_err(|error| error.to_string())?;
            let vault = DocVault::new(storage);
            let current = vault
                .current_version(&document_ref)
                .map_err(|error| error.to_string())?;
            match (format, current) {
                (OutputFormat::Table, Some(version)) => print_versions_table(&[version]),
                (OutputFormat::Table, None) => println!("No current version"),
                (OutputFormat::Json, Some(version)) => println!(
                    "{}",
                    serde_json::to_string_pretty(&version_to_json(&version))
                        .map_err(|error| error.to_string())?
                ),
                (OutputFormat::Json, None) => println!("null"),
            }
            Ok(())
        }
        Some("track") => {
            let source_path = PathBuf::from(args.get(1).ok_or_else(|| usage().to_owned())?);
            let format = OutputFormat::parse(&args)?;
            let import_first = has_flag(&args, "--import");
            let metadata = ImportMetadata {
                author: parse_option_value(&args, "--author"),
                note: parse_option_value(&args, "--note"),
            };
            let storage =
                VaultStorage::init(VaultPaths::from_env()).map_err(|error| error.to_string())?;
            let vault = DocVault::new(storage);

            let tracked = if import_first {
                let document_ref = parse_track_document_ref(&args, &source_path)?
                    .unwrap_or_else(|| default_track_document_ref(&source_path));
                let (document, version) = vault
                    .import_document(&source_path, document_ref, metadata)
                    .map_err(|error| error.to_string())?;
                let tracked = vault
                    .track_document_path(&source_path, Some(&document.id))
                    .map_err(|error| error.to_string())?;
                match format {
                    OutputFormat::Table => {
                        println!("Imported {} as {}", document.name, version.id);
                    }
                    OutputFormat::Json => {}
                }
                tracked
            } else {
                if has_flag(&args, "--new") {
                    return Err("--new requires --import for track".to_owned());
                }
                let document_ref = parse_track_document_ref(&args, &source_path)?;
                vault
                    .track_path(&source_path, document_ref.as_ref())
                    .map_err(|error| error.to_string())?
            };

            match format {
                OutputFormat::Table => print_tracked_paths_table(&[tracked]),
                OutputFormat::Json => println!(
                    "{}",
                    serde_json::to_string_pretty(&tracked_path_to_json(&tracked))
                        .map_err(|error| error.to_string())?
                ),
            }
            Ok(())
        }
        Some("scan") => {
            let format = OutputFormat::parse(&args)?;
            let deep = has_flag(&args, "--deep");
            let storage =
                VaultStorage::open(VaultPaths::from_env()).map_err(|error| error.to_string())?;
            let vault = DocVault::new(storage);
            let scans = vault
                .scan_tracked_paths(deep)
                .map_err(|error| error.to_string())?;
            match format {
                OutputFormat::Table => {
                    if scans.is_empty() {
                        println!("No tracked paths found");
                    } else {
                        print_tracked_scans_table(&scans);
                    }
                }
                OutputFormat::Json => {
                    let rows = scans.iter().map(tracked_scan_to_json).collect::<Vec<_>>();
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&rows).map_err(|error| error.to_string())?
                    );
                }
            }
            Ok(())
        }
        _ => Err(usage().to_owned()),
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

fn parse_import_ref(args: &[String]) -> Result<DocumentRef, String> {
    if let Some(id_prefix) = parse_option_value(args, "--id") {
        return Ok(DocumentRef::IdPrefix(id_prefix));
    }

    let value = parse_option_value(args, "--name")
        .or_else(|| args.get(2).cloned())
        .ok_or_else(|| usage().to_owned())?;
    if has_flag(args, "--new") {
        if value.contains('@') {
            return Err("--new requires a plain document name".to_owned());
        }
        return Ok(DocumentRef::NewName(value));
    }
    parse_document_ref_value(&value)
}

fn parse_document_ref_arg(args: &[String], position: usize) -> Result<DocumentRef, String> {
    if let Some(id_prefix) = parse_option_value(args, "--id") {
        return Ok(DocumentRef::IdPrefix(id_prefix));
    }

    let value = args.get(position).ok_or_else(|| usage().to_owned())?;
    if value.starts_with("--") {
        return Err(usage().to_owned());
    }
    parse_document_ref_value(value)
}

fn parse_document_ref_value(value: &str) -> Result<DocumentRef, String> {
    if let Some((name, id_prefix)) = value.rsplit_once('@') {
        if name.is_empty() || id_prefix.is_empty() {
            return Err(format!("Invalid document reference: {value}"));
        }
        Ok(DocumentRef::NameAndIdPrefix {
            name: name.to_owned(),
            id_prefix: id_prefix.to_owned(),
        })
    } else {
        Ok(DocumentRef::Name(value.to_owned()))
    }
}

fn parse_track_document_ref(
    args: &[String],
    source_path: &Path,
) -> Result<Option<DocumentRef>, String> {
    if let Some(id_prefix) = parse_option_value(args, "--id") {
        return Ok(Some(DocumentRef::IdPrefix(id_prefix)));
    }

    let value = parse_option_value(args, "--name").or_else(|| {
        args.get(2)
            .filter(|value| !value.starts_with("--"))
            .cloned()
    });
    let Some(value) = value else {
        return Ok(None);
    };

    if has_flag(args, "--new") {
        if value.contains('@') {
            return Err("--new requires a plain document name".to_owned());
        }
        return Ok(Some(DocumentRef::NewName(value)));
    }

    if value == source_path.display().to_string() {
        return Ok(None);
    }
    parse_document_ref_value(&value).map(Some)
}

fn default_track_document_ref(source_path: &Path) -> DocumentRef {
    let name = source_path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("document")
        .to_owned();
    DocumentRef::Name(name)
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
                version.original_path.clone(),
                version
                    .snapshot_id
                    .as_deref()
                    .unwrap_or(version.archive_path.as_str())
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

fn print_tracked_paths_table(tracked_paths: &[docvault_types::TrackedPath]) {
    let rows = tracked_paths
        .iter()
        .map(|tracked_path| {
            vec![
                tracked_path.id.clone(),
                tracked_path
                    .document_id
                    .as_ref()
                    .map(|id| id.as_str().to_owned())
                    .unwrap_or_default(),
                tracked_path.path.clone(),
                tracked_path.stat_fingerprint.clone().unwrap_or_default(),
                tracked_path.content_fingerprint.clone().unwrap_or_default(),
                tracked_path
                    .last_scanned_at
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
                tracked_path
                    .last_deep_scanned_at
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
                tracked_path.created_at.to_string(),
            ]
        })
        .collect::<Vec<_>>();
    print_table(
        &[
            "ID",
            "DOCUMENT_ID",
            "PATH",
            "STAT_FINGERPRINT",
            "CONTENT_FINGERPRINT",
            "LAST_SCANNED_AT",
            "LAST_DEEP_SCANNED_AT",
            "CREATED_AT",
        ],
        &rows,
    );
}

fn print_tracked_scans_table(scans: &[docvault_types::TrackedScan]) {
    let rows = scans
        .iter()
        .map(|scan| {
            vec![
                scan.tracked_path.id.clone(),
                scan.tracked_path
                    .document_id
                    .as_ref()
                    .map(|id| id.as_str().to_owned())
                    .unwrap_or_default(),
                scan.tracked_path.path.clone(),
                scan.status.clone(),
                scan.stat_fingerprint.clone().unwrap_or_default(),
                scan.content_fingerprint.clone().unwrap_or_default(),
                scan.deep.to_string(),
                scan.scanned_at.to_string(),
            ]
        })
        .collect::<Vec<_>>();
    print_table(
        &[
            "ID",
            "DOCUMENT_ID",
            "PATH",
            "STATUS",
            "STAT_FINGERPRINT",
            "CONTENT_FINGERPRINT",
            "DEEP",
            "SCANNED_AT",
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
        "original_path": version.original_path,
        "archive_path": version.archive_path,
        "backup_backend": version.backup_backend,
        "snapshot_id": version.snapshot_id,
        "author": version.author,
        "note": version.note,
        "created_at": version.created_at,
    })
}

fn tracked_path_to_json(tracked_path: &docvault_types::TrackedPath) -> serde_json::Value {
    json!({
        "id": tracked_path.id,
        "document_id": tracked_path.document_id.as_ref().map(|id| id.as_str()),
        "path": tracked_path.path,
        "stat_fingerprint": tracked_path.stat_fingerprint,
        "content_fingerprint": tracked_path.content_fingerprint,
        "last_scanned_at": tracked_path.last_scanned_at,
        "last_deep_scanned_at": tracked_path.last_deep_scanned_at,
        "created_at": tracked_path.created_at,
    })
}

fn tracked_scan_to_json(scan: &docvault_types::TrackedScan) -> serde_json::Value {
    json!({
        "tracked_path": tracked_path_to_json(&scan.tracked_path),
        "stat_fingerprint": scan.stat_fingerprint,
        "content_fingerprint": scan.content_fingerprint,
        "status": scan.status,
        "changed": scan.changed,
        "exists": scan.exists,
        "deep": scan.deep,
        "scanned_at": scan.scanned_at,
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
  docvault import <path> <name|name@id-prefix> [--author <name>] [--note <text>] [--new]
  docvault import <path> --name <name> [--author <name>] [--note <text>] [--new]
  docvault import <path> --id <id-prefix> [--author <name>] [--note <text>]
  docvault list [--format table|json]
  docvault versions <name|name@id-prefix|--id <id-prefix>> [--format table|json]
  docvault current <name|name@id-prefix|--id <id-prefix>> [--format table|json]
  docvault export <name|name@id-prefix|--id <id-prefix>> [version|--version <version>] --output <path>
  docvault checkout <name|name@id-prefix|--id <id-prefix>> [version|--version <version>] [--output <path>]
  docvault track <path> [name|name@id-prefix|--name <name>|--id <id-prefix>] [--import] [--author <name>] [--note <text>] [--new] [--format table|json]
  docvault scan [--deep] [--format table|json]"
}
