use std::env;
use std::io::{self, IsTerminal};
use std::process;

use clap::Parser;

mod cli;
mod tui_approve;

use cli::{Cli, Commands};
use liyi::diagnostics::CheckFlags;

/// Reset SIGPIPE to default behavior so piping into `head` etc. terminates
/// silently instead of panicking.
fn reset_sigpipe() {
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

/// CLI entrypoint. Dispatches to sub-commands; implementations of some are
/// delegated but some are not. Must exit process explicitly with return code
/// for spec compliance.
///
/// <!-- @liyi:intent=doc -->
fn main() {
    reset_sigpipe();
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
            level,
            prompt,
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

            if prompt {
                let output = liyi::prompt::build_prompt_output(&diagnostics, exit_code, &repo_root);
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output)
                        .expect("failed to serialize prompt output")
                );
                process::exit(exit_code as i32);
            }

            let github_actions = env::var("GITHUB_ACTIONS").is_ok_and(|v| v == "true");

            // Print summary first for immediate visibility
            let summary = liyi::diagnostics::format_summary(&diagnostics);
            println!("{summary}\n");

            let min_severity = match level {
                cli::DiagnosticLevel::All => None,
                cli::DiagnosticLevel::Warning => Some(liyi::diagnostics::Severity::Warning),
                cli::DiagnosticLevel::Error => Some(liyi::diagnostics::Severity::Error),
            };

            for d in &diagnostics {
                if !verbose && d.kind == liyi::diagnostics::DiagnosticKind::Current {
                    continue;
                }
                if let Some(min) = min_severity {
                    let dominated = match min {
                        liyi::diagnostics::Severity::Warning => {
                            d.severity == liyi::diagnostics::Severity::Info
                        }
                        liyi::diagnostics::Severity::Error => {
                            d.severity != liyi::diagnostics::Severity::Error
                        }
                        _ => false,
                    };
                    if dominated {
                        continue;
                    }
                }
                if github_actions {
                    println!(
                        "{}",
                        liyi::diagnostics::format_github_actions(d, &repo_root)
                    );
                } else {
                    println!("{}", d.display_with_root(&repo_root));
                }
            }

            process::exit(exit_code as i32);
        }
        Commands::Migrate { files } => {
            if files.is_empty() {
                eprintln!("at least one sidecar file or directory required");
                process::exit(2);
            }

            let targets = match liyi::discovery::resolve_sidecar_targets(&files) {
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
                match liyi::reanchor::run_reanchor(sidecar_path, None, None, true) {
                    Ok(()) => {
                        println!("Migrated: {}", sidecar_path.display());
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
        Commands::Init {
            source_file,
            force,
            no_discover,
            trivial_threshold,
        } => match source_file {
            Some(src) => {
                match liyi::init::init_sidecar(&src, force, !no_discover, trivial_threshold) {
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
        },
        Commands::Approve {
            paths,
            yes,
            dry_run,
            item,
            unreviewed_only,
            stale_only,
            req_only,
        } => {
            let targets = if paths.is_empty() {
                vec![env::current_dir().unwrap_or_default()]
            } else {
                paths
            };

            let kind_filter = if unreviewed_only {
                liyi::approve::ApproveFilter::UnreviewedOnly
            } else if stale_only {
                liyi::approve::ApproveFilter::StaleOnly
            } else if req_only {
                liyi::approve::ApproveFilter::ReqOnly
            } else {
                liyi::approve::ApproveFilter::All
            };

            let is_interactive = !yes && is_tty();

            if is_interactive {
                // Collect candidates, run TUI, apply decisions.
                let candidates = match liyi::approve::collect_approval_candidates(
                    &targets,
                    item.as_deref(),
                    kind_filter,
                ) {
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
                let candidates = match liyi::approve::collect_approval_candidates(
                    &targets,
                    item.as_deref(),
                    kind_filter,
                ) {
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
