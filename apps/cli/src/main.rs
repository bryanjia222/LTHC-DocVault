use std::{path::PathBuf, process::ExitCode};

use anyhow::{Result, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use docvault_core::DocVault;
use docvault_storage::{
    DocumentRef, StorageOverrides, VaultPaths, VaultStorage, write_initial_config,
};
use docvault_types::CommitMetadata;
use serde_json::json;
use tracing::error;
use tracing_subscriber::filter::LevelFilter;

#[derive(Debug, Parser)]
#[command(name = "docvault")]
#[command(about = "Local-first Office document version vault")]
struct Cli {
    #[command(flatten)]
    global: GlobalArgs,
    #[command(subcommand)]
    command: Command,
}

/// Options that apply to every subcommand. Configuration comes from the vault's
/// `config.toml` plus these explicit flags - never from `DOCVAULT_*` environment
/// variables (those were dropped to avoid silent, easily-overlooked overrides).
#[derive(Debug, Args)]
struct GlobalArgs {
    /// Vault root directory (defaults to ~/.DocVault).
    #[arg(long, value_name = "ROOT_DIR")]
    root_dir: Option<PathBuf>,
    /// Path to the restic binary. Overrides config `restic_path` and the bundled
    /// binary / PATH auto-discovery.
    #[arg(long, value_name = "RESTIC_PATH")]
    restic_path: Option<PathBuf>,
    /// Log level: error, warn, info, debug, trace.
    #[arg(long, value_name = "LOG_LEVEL", default_value = "warn")]
    log_level: String,
}

impl GlobalArgs {
    /// The vault root: the explicit `--root-dir`, or the platform default
    /// (`~/.DocVault`) when unset.
    fn root(&self) -> PathBuf {
        self.root_dir
            .clone()
            .unwrap_or_else(VaultPaths::default_root)
    }

    /// Overrides applied on top of `config.toml` when opening/initializing. Only
    /// `restic_path` is overridden (from `--restic-path`); backend and password
    /// always come from the config file.
    fn overrides(&self) -> StorageOverrides {
        StorageOverrides {
            backend: None,
            restic_path: self.restic_path.clone(),
            restic_password: None,
        }
    }
}

#[derive(Debug, Subcommand)]
enum Command {
    Init(InitArgs),
    Commit(CommitArgs),
    List(FormatArgs),
    Versions(DocumentFormatArgs),
    Current(DocumentFormatArgs),
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    Export(VersionOutputArgs),
    Checkout(CheckoutArgs),
}

#[derive(Debug, Args)]
struct InitArgs {
    /// Backup backend to initialize the vault with (`local-copy` needs no
    /// external binary; `restic` requires --restic-password).
    #[arg(long, default_value = "local-copy")]
    backend: String,
    /// Restic repository password (required when --backend restic).
    #[arg(long)]
    restic_password: Option<String>,
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    Show(FormatArgs),
}

#[derive(Debug, Args)]
struct CommitArgs {
    path: PathBuf,
    #[arg(value_name = "DOCUMENT")]
    document: Option<String>,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    id: Option<String>,
    #[arg(long)]
    author: Option<String>,
    #[arg(long)]
    note: Option<String>,
    #[arg(long = "new")]
    create_new: bool,
}

#[derive(Debug, Args)]
struct FormatArgs {
    #[arg(long, value_enum, default_value_t = OutputFormat::Table)]
    format: OutputFormat,
}

#[derive(Debug, Args)]
struct DocumentFormatArgs {
    #[command(flatten)]
    document: DocumentArgs,
    #[command(flatten)]
    format: FormatArgs,
}

#[derive(Debug, Args)]
struct DocumentArgs {
    #[arg(value_name = "DOCUMENT")]
    document: Option<String>,
    #[arg(long)]
    id: Option<String>,
}

#[derive(Debug, Args)]
struct VersionOutputArgs {
    #[command(flatten)]
    document: DocumentArgs,
    #[arg(value_name = "VERSION")]
    version: Option<String>,
    #[arg(long = "version")]
    version_id: Option<String>,
    #[arg(long)]
    output: PathBuf,
}

#[derive(Debug, Args)]
struct CheckoutArgs {
    #[command(flatten)]
    document: DocumentArgs,
    #[arg(value_name = "VERSION")]
    version: Option<String>,
    #[arg(long = "version")]
    version_id: Option<String>,
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Table,
    Json,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_tracing(&cli.global.log_level);
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            error!(%error, "docvault command failed");
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

fn init_tracing(level: &str) {
    let level = level.parse::<LevelFilter>().unwrap_or(LevelFilter::WARN);
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Init(args) => init_vault(&cli.global, &args),
        Command::Commit(args) => commit_document(&cli.global, args),
        Command::List(args) => list_documents(&cli.global, args.format),
        Command::Versions(args) => list_versions(&cli.global, args),
        Command::Current(args) => show_current(&cli.global, args),
        Command::Config { command } => match command {
            ConfigCommand::Show(args) => show_config(&cli.global, args.format),
        },
        Command::Export(args) => export_version(&cli.global, args),
        Command::Checkout(args) => checkout_version(&cli.global, args),
    }
}

fn init_vault(global: &GlobalArgs, args: &InitArgs) -> Result<()> {
    let paths = VaultPaths::from_root(global.root());
    // Write the chosen backend (and restic password) into config.toml before
    // init, so the vault persists the user's choice; restic_path is NOT
    // persisted (it is install-specific) - it comes from --restic-path or
    // auto-discovery at open time.
    write_initial_config(&paths, &args.backend, args.restic_password.as_deref())?;
    let storage = VaultStorage::init_with_overrides(paths, &global.overrides())?;
    println!(
        "DocVault initialized at {}",
        storage.paths().root_dir.display()
    );
    println!("Backend: {}", storage.backend().as_str());
    println!("Config: {}", storage.paths().config_path.display());
    println!("Data: {}", storage.paths().data_dir.display());
    println!("Database: {}", storage.paths().db_path.display());
    println!("Repository: {}", storage.paths().repo_dir.display());
    Ok(())
}

fn commit_document(global: &GlobalArgs, args: CommitArgs) -> Result<()> {
    let document_ref = args.document_ref()?;
    let metadata = CommitMetadata {
        author: args.author,
        note: args.note,
    };
    let storage = VaultStorage::init_with_overrides(
        VaultPaths::from_root(global.root()),
        &global.overrides(),
    )?;
    let vault = DocVault::new(storage);
    let (_, version) = vault.commit_document(
        &args.path,
        document_ref.clone(),
        metadata,
        &docvault_storage::NEVER_CANCELLED,
    )?;
    println!(
        "Committed {} as {}",
        document_ref.display_name(),
        version.id
    );
    Ok(())
}

fn list_documents(global: &GlobalArgs, format: OutputFormat) -> Result<()> {
    let storage = VaultStorage::open_with_overrides(
        VaultPaths::from_root(global.root()),
        &global.overrides(),
    )?;
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

fn list_versions(global: &GlobalArgs, args: DocumentFormatArgs) -> Result<()> {
    let document_ref = args.document.document_ref()?;
    let storage = VaultStorage::open_with_overrides(
        VaultPaths::from_root(global.root()),
        &global.overrides(),
    )?;
    let vault = DocVault::new(storage);
    let versions = vault.list_versions(&document_ref)?;
    match args.format.format {
        OutputFormat::Table => {
            if versions.is_empty() {
                println!("No versions found");
            } else {
                print_versions_table(&versions);
            }
        }
        OutputFormat::Json => {
            let rows = versions.iter().map(version_to_json).collect::<Vec<_>>();
            println!("{}", serde_json::to_string_pretty(&rows)?);
        }
    }
    Ok(())
}

fn show_current(global: &GlobalArgs, args: DocumentFormatArgs) -> Result<()> {
    let document_ref = args.document.document_ref()?;
    let storage = VaultStorage::open_with_overrides(
        VaultPaths::from_root(global.root()),
        &global.overrides(),
    )?;
    let vault = DocVault::new(storage);
    let current = vault.current_version(&document_ref)?;
    match (args.format.format, current) {
        (OutputFormat::Table, Some(version)) => print_versions_table(&[version]),
        (OutputFormat::Table, None) => println!("No current version"),
        (OutputFormat::Json, Some(version)) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&version_to_json(&version))?
            )
        }
        (OutputFormat::Json, None) => println!("null"),
    }
    Ok(())
}

fn show_config(global: &GlobalArgs, format: OutputFormat) -> Result<()> {
    let storage = VaultStorage::open_with_overrides(
        VaultPaths::from_root(global.root()),
        &global.overrides(),
    )?;
    let paths = storage.paths();
    match format {
        OutputFormat::Table => {
            print_table(
                &["KEY", "VALUE"],
                &[
                    vec!["backend".to_owned(), storage.backend().as_str().to_owned()],
                    vec![
                        "config_path".to_owned(),
                        paths.config_path.display().to_string(),
                    ],
                    vec!["root_dir".to_owned(), paths.root_dir.display().to_string()],
                    vec!["data_dir".to_owned(), paths.data_dir.display().to_string()],
                    vec!["db_path".to_owned(), paths.db_path.display().to_string()],
                    vec!["repo_dir".to_owned(), paths.repo_dir.display().to_string()],
                    vec![
                        "restic_path".to_owned(),
                        storage.restic_path().display().to_string(),
                    ],
                ],
            );
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "backend": storage.backend().as_str(),
                    "config_path": paths.config_path.display().to_string(),
                    "root_dir": paths.root_dir.display().to_string(),
                    "data_dir": paths.data_dir.display().to_string(),
                    "db_path": paths.db_path.display().to_string(),
                    "repo_dir": paths.repo_dir.display().to_string(),
                    "restic_path": storage.restic_path().display().to_string(),
                }))?
            );
        }
    }
    Ok(())
}

fn export_version(global: &GlobalArgs, args: VersionOutputArgs) -> Result<()> {
    let document_ref = args.document.document_ref()?;
    let requested_version = requested_version(args.version, args.version_id);
    let storage = VaultStorage::open_with_overrides(
        VaultPaths::from_root(global.root()),
        &global.overrides(),
    )?;
    let vault = DocVault::new(storage);
    let exported = vault.export_version(
        &document_ref,
        &requested_version,
        args.output,
        &docvault_storage::NEVER_CANCELLED,
    )?;
    println!("Exported to {}", exported.display());
    Ok(())
}

fn checkout_version(global: &GlobalArgs, args: CheckoutArgs) -> Result<()> {
    let document_ref = args.document.document_ref()?;
    let requested_version = requested_version(args.version, args.version_id);
    let storage = VaultStorage::open_with_overrides(
        VaultPaths::from_root(global.root()),
        &global.overrides(),
    )?;
    let vault = DocVault::new(storage);
    let exported = vault.checkout_version(
        &document_ref,
        &requested_version,
        args.output.as_ref(),
        &docvault_storage::NEVER_CANCELLED,
    )?;
    match exported {
        Some(path) => println!(
            "Checked out {requested_version} as current and exported to {}",
            path.display()
        ),
        None => println!("Checked out {requested_version} as current"),
    }
    Ok(())
}

impl CommitArgs {
    fn document_ref(&self) -> Result<DocumentRef> {
        if let Some(id_prefix) = &self.id {
            return Ok(DocumentRef::IdPrefix(id_prefix.clone()));
        }

        let Some(value) = self.name.as_ref().or(self.document.as_ref()) else {
            bail!(
                "document name is required; use --name <name> or a positional document reference"
            );
        };
        if self.create_new {
            if value.contains('@') {
                bail!("--new requires a plain document name");
            }
            return Ok(DocumentRef::NewName(value.clone()));
        }
        parse_document_ref_value(value)
    }
}

impl DocumentArgs {
    fn document_ref(&self) -> Result<DocumentRef> {
        if let Some(id_prefix) = &self.id {
            return Ok(DocumentRef::IdPrefix(id_prefix.clone()));
        }

        let Some(value) = &self.document else {
            bail!("document reference is required; use <name|name@id-prefix> or --id <id-prefix>");
        };
        parse_existing_document_ref_value(value)
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

fn requested_version(positional: Option<String>, option: Option<String>) -> String {
    option.or(positional).unwrap_or_else(|| "latest".to_owned())
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

fn parse_existing_document_ref_value(value: &str) -> Result<DocumentRef> {
    if is_uuid(value) {
        Ok(DocumentRef::IdPrefix(value.to_owned()))
    } else {
        parse_document_ref_value(value)
    }
}

fn is_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && [8, 13, 18, 23].iter().all(|index| bytes[*index] == b'-')
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| [8, 13, 18, 23].contains(&index) || byte.is_ascii_hexdigit())
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
            "FILENAME",
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
        "manifest": version.manifest,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_document_reference_as_name() {
        let document_ref = parse_existing_document_ref_value("report").unwrap();

        assert_eq!(document_ref, DocumentRef::Name("report".to_owned()));
    }

    #[test]
    fn parses_full_uuid_document_reference_as_id_prefix() {
        let id = "550e8400-e29b-41d4-a716-446655440000";
        let document_ref = parse_existing_document_ref_value(id).unwrap();

        assert_eq!(document_ref, DocumentRef::IdPrefix(id.to_owned()));
    }
}
