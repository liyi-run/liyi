use std::env;
use std::io::{self, IsTerminal};
use std::process;

use clap::Parser;

mod cli;
mod tui_approve;

use cli::{Cli, Commands};
use liyi::diagnostics::CheckFlags;

/// CLI entrypoint. Dispatches to sub-commands; implementations of some are
/// delegated but some are not. Must exit process explicitly with return code
/// for spec compliance.
///
/// <!-- @liyi:intent=doc -->
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
            fail_on_untracked,
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
                fail_on_untracked,
            };

            let (diagnostics, exit_code) =
                liyi::check::run_check(&repo_root, &paths, fix, dry_run, &flags);

            // Print summary first for immediate visibility
            let summary = liyi::diagnostics::format_summary(&diagnostics);
            println!("{summary}\n");

            for d in &diagnostics {
                if !verbose && d.kind == liyi::diagnostics::DiagnosticKind::Current {
                    continue;
                }
                println!("{}", d.display_with_root(&repo_root));
            }

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
        Commands::Init { source_file, force } => match source_file {
            Some(src) => match liyi::init::init_sidecar(&src, force) {
                Ok(path) => println!("Created: {}", path.display()),
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
                }
            },
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
        },
        Commands::Approve {
            paths,
            yes,
            dry_run,
            item,
        } => {
            let targets = if paths.is_empty() {
                vec![env::current_dir().unwrap_or_default()]
            } else {
                paths
            };

            let is_interactive = !yes && is_tty();

            if is_interactive {
                // Collect candidates, run TUI, apply decisions.
                let candidates =
                    match liyi::approve::collect_approval_candidates(&targets, item.as_deref()) {
                        Ok(c) => c,
                        Err(e) => {
                            eprintln!("Error: {e}");
                            process::exit(2);
                        }
                    };

                if candidates.is_empty() {
                    println!("nothing to approve");
                    return;
                }

                let decisions = match tui_approve::run_tui(&candidates) {
                    Ok(d) => d,
                    Err(e) => {
                        eprintln!("TUI error: {e}");
                        process::exit(2);
                    }
                };

                match liyi::approve::apply_approval_decisions(&candidates, &decisions, dry_run) {
                    Ok(results) => {
                        let total_approved: usize = results.iter().map(|r| r.approved).sum();
                        let total_skipped: usize = results.iter().map(|r| r.skipped).sum();
                        let total_rejected: usize = results.iter().map(|r| r.rejected).sum();
                        println!(
                            "{total_approved} approved, {total_skipped} skipped, {total_rejected} rejected"
                        );
                    }
                    Err(e) => {
                        eprintln!("Error: {e}");
                        process::exit(2);
                    }
                }
            } else {
                // Batch mode: collect + auto-approve all.
                let candidates =
                    match liyi::approve::collect_approval_candidates(&targets, item.as_deref()) {
                        Ok(c) => c,
                        Err(e) => {
                            eprintln!("Error: {e}");
                            process::exit(2);
                        }
                    };

                let decisions = vec![liyi::approve::Decision::Yes; candidates.len()];
                match liyi::approve::apply_approval_decisions(&candidates, &decisions, dry_run) {
                    Ok(results) => {
                        let total_approved: usize = results.iter().map(|r| r.approved).sum();
                        let total_skipped: usize = results.iter().map(|r| r.skipped).sum();
                        let total_rejected: usize = results.iter().map(|r| r.rejected).sum();
                        println!(
                            "{total_approved} approved, {total_skipped} skipped, {total_rejected} rejected"
                        );
                    }
                    Err(e) => {
                        eprintln!("Error: {e}");
                        process::exit(2);
                    }
                }
            }
        }
    }
}

/// Check if stderr is a TTY (for interactive mode detection).
/// Uses stderr since the TUI renders there.
///
/// <!-- @liyi:intent=doc -->
fn is_tty() -> bool {
    io::stderr().is_terminal()
}
