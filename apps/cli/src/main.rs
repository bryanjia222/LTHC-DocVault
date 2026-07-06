use std::{env, process::ExitCode};

fn main() -> ExitCode {
    let mut args = env::args().skip(1);

    match args.next().as_deref() {
        Some("init") => {
            println!("DocVault initialized");
            ExitCode::SUCCESS
        }
        Some("import") => match (args.next(), args.next()) {
            (Some(path), Some(name)) => {
                let document = docvault_core::register_document(name, path);
                println!("Imported document: {}", document.id.as_str());
                ExitCode::SUCCESS
            }
            _ => {
                eprintln!("Usage: docvault import <path> <name>");
                ExitCode::from(2)
            }
        },
        Some("list") => {
            println!("No documents found");
            ExitCode::SUCCESS
        }
        Some("restore") => {
            eprintln!("Restore is not implemented yet");
            ExitCode::from(2)
        }
        _ => {
            eprintln!("Usage: docvault <init|import|list|restore>");
            ExitCode::from(2)
        }
    }
}
