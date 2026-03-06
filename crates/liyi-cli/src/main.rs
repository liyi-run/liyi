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
            dry_run,
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

            let (diagnostics, exit_code) =
                liyi::check::run_check(&repo_root, &paths, fix, dry_run, &flags);

            for d in &diagnostics {
                if !verbose && d.kind == liyi::diagnostics::DiagnosticKind::Current {
                    continue;
                }
                println!("{d}");
            }

            // Print summary line
            let summary = liyi::diagnostics::format_summary(&diagnostics);
            println!("\n{summary}");

            process::exit(exit_code as i32);
        }
        Commands::Reanchor {
            files,
            item,
            span,
            migrate,
        } => {
            if migrate && files.is_empty() {
                eprintln!("--migrate requires at least one sidecar file path");
                process::exit(2);
            }

            if files.is_empty() {
                eprintln!("at least one sidecar file or directory required");
                process::exit(2);
            }

            let targets = match liyi::reanchor::resolve_reanchor_targets(&files) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(2);
                }
            };

            if targets.is_empty() {
                eprintln!("no .liyi.jsonc files found in the given paths");
                process::exit(2);
            }

            let mut errors = 0;
            for sidecar_path in &targets {
                match liyi::reanchor::run_reanchor(sidecar_path, item.as_deref(), span, migrate) {
                    Ok(()) => {
                        if migrate {
                            println!("Migrated: {}", sidecar_path.display());
                        } else {
                            println!("Reanchored: {}", sidecar_path.display());
                        }
                    }
                    Err(e) => {
                        eprintln!("Error ({}): {e}", sidecar_path.display());
                        errors += 1;
                    }
                }
            }

            if errors > 0 {
                process::exit(2);
            }
        }
        Commands::Init { source_file, force } => {
            match source_file {
                Some(src) => {
                    match liyi::init::init_sidecar(&src, force) {
                        Ok(path) => println!("Created: {}", path.display()),
                        Err(e) => {
                            eprintln!("Error: {e}");
                            process::exit(1);
                        }
                    }
                }
                None => {
                    let root = env::current_dir().unwrap_or_default();
                    match liyi::init::init_agents_md(&root, force) {
                        Ok(path) => println!("Initialized: {}", path.display()),
                        Err(e) => {
                            eprintln!("Error: {e}");
                            process::exit(1);
                        }
                    }
                }
            }
        }
    }
}
