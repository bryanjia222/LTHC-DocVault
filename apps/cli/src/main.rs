use std::{env, path::PathBuf, process::ExitCode};

use docvault_core::DocVault;
use docvault_storage::{VaultPaths, VaultStorage};
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
            let name = parse_import_name(&args).ok_or_else(|| usage().to_owned())?;
            let metadata = ImportMetadata {
                author: parse_option_value(&args, "--author"),
                note: parse_option_value(&args, "--note"),
            };
            let storage =
                VaultStorage::init(VaultPaths::from_env()).map_err(|error| error.to_string())?;
            let vault = DocVault::new(storage);
            let (_, version) = vault
                .import_document(source_path, &name, metadata)
                .map_err(|error| error.to_string())?;
            println!("Imported {name} as {}", version.id);
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
                                    document.source_path.clone(),
                                    document.created_at.to_string(),
                                ]
                            })
                            .collect::<Vec<_>>();
                        print_table(&["ID", "NAME", "SOURCE", "CREATED_AT"], &rows);
                    }
                }
                OutputFormat::Json => {
                    let rows = documents
                        .iter()
                        .map(|document| {
                            json!({
                                "id": document.id.as_str(),
                                "name": document.name,
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
            let document_name = args.get(1).ok_or_else(|| usage().to_owned())?;
            let storage =
                VaultStorage::open(VaultPaths::from_env()).map_err(|error| error.to_string())?;
            let vault = DocVault::new(storage);
            let versions = vault
                .list_versions(document_name)
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
        Some("restore") => {
            let document_name = args.get(1).ok_or_else(|| usage().to_owned())?;
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
            let restored = vault
                .restore_version(document_name, &requested_version, output_path)
                .map_err(|error| error.to_string())?;
            println!("Restored to {}", restored.display());
            Ok(())
        }
        _ => Err(usage().to_owned()),
    }
}

fn parse_import_name(args: &[String]) -> Option<String> {
    parse_option_value(args, "--name").or_else(|| args.get(2).cloned())
}

fn parse_option_value(args: &[String], option: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == option)
        .map(|window| window[1].clone())
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
  docvault import <path> <name> [--author <name>] [--note <text>]
  docvault import <path> --name <name> [--author <name>] [--note <text>]
  docvault list [--format table|json]
  docvault versions <name> [--format table|json]
  docvault restore <name> [version|--version <version>] --output <path>"
}
