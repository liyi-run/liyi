use std::env;
use std::process;

use clap::Parser;

mod cli;

use cli::{Cli, Commands};
use liyi::diagnostics::CheckFlags;

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Check {
            paths,
            fix,
            fail_on_stale,
            fail_on_unreviewed,
            fail_on_req_changed,
            root,
            verbose,
        } => {
            let repo_root = root
                .or_else(|| {
                    liyi::discovery::find_repo_root(&env::current_dir().unwrap_or_default())
                })
                .unwrap_or_else(|| env::current_dir().unwrap_or_default());

            let flags = CheckFlags {
                fail_on_stale,
                fail_on_unreviewed,
                fail_on_req_changed,
            };

            let (diagnostics, exit_code) = liyi::check::run_check(&repo_root, &paths, fix, &flags);

            for d in &diagnostics {
                if !verbose && d.kind == liyi::diagnostics::DiagnosticKind::Current {
                    continue;
                }
                println!("{d}");
            }

            process::exit(exit_code as i32);
        }
        Commands::Reanchor {
            file,
            item,
            span,
            migrate,
        } => {
            if migrate && file.is_none() {
                eprintln!("--migrate requires a sidecar file path");
                process::exit(2);
            }

            let sidecar_path = match &file {
                Some(p) => p.as_path(),
                None => {
                    eprintln!("sidecar file path required");
                    process::exit(2);
                }
            };

            match liyi::reanchor::run_reanchor(sidecar_path, item.as_deref(), span, migrate) {
                Ok(()) => {
                    if migrate {
                        println!("Migration complete.");
                    } else {
                        println!("Reanchor complete.");
                    }
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(2);
                }
            }
        }
    }
}
