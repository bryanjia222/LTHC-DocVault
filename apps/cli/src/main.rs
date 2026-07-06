use std::{env, path::PathBuf, process::ExitCode};

use docvault_core::DocVault;
use docvault_storage::{VaultPaths, VaultStorage};

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
            let storage =
                VaultStorage::init(VaultPaths::from_env()).map_err(|error| error.to_string())?;
            let vault = DocVault::new(storage);
            let (_, version) = vault
                .import_document(source_path, &name)
                .map_err(|error| error.to_string())?;
            println!("Imported {name} as {}", version.id);
            Ok(())
        }
        Some("list") => {
            let storage =
                VaultStorage::open(VaultPaths::from_env()).map_err(|error| error.to_string())?;
            let vault = DocVault::new(storage);
            let documents = vault.list_documents().map_err(|error| error.to_string())?;
            if documents.is_empty() {
                println!("No documents found");
            } else {
                for document in documents {
                    println!(
                        "{}\t{}\t{}",
                        document.id.as_str(),
                        document.name,
                        document.source_path
                    );
                }
            }
            Ok(())
        }
        Some("versions") => {
            let document_name = args.get(1).ok_or_else(|| usage().to_owned())?;
            let storage =
                VaultStorage::open(VaultPaths::from_env()).map_err(|error| error.to_string())?;
            let vault = DocVault::new(storage);
            let versions = vault
                .list_versions(document_name)
                .map_err(|error| error.to_string())?;
            if versions.is_empty() {
                println!("No versions found");
            } else {
                for version in versions {
                    println!(
                        "{}\t{}\t{}",
                        version.id, version.original_path, version.archive_path
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

fn usage() -> &'static str {
    "Usage:
  docvault init
  docvault import <path> <name>
  docvault import <path> --name <name>
  docvault list
  docvault versions <name>
  docvault restore <name> [version|--version <version>] --output <path>"
}
