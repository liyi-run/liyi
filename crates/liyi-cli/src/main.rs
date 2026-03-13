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
                println!("{}", d.display_with_root(&repo_root));
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
        Commands::Rehash { paths } => {
            let targets = if paths.is_empty() {
                vec![std::env::current_dir().unwrap_or_default()]
            } else {
                paths
            };

            let targets = match liyi::discovery::resolve_sidecar_targets(&targets) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(2);
                }
            };

            if targets.is_empty() {
                eprintln!("no .liyi.jsonc files found");
                process::exit(2);
            }

            let mut total_rehashed = 0usize;
            let mut errors = 0usize;
            for sidecar_path in &targets {
                match rehash_sidecar(sidecar_path) {
                    Ok(n) => {
                        if n > 0 {
                            println!("{}: {n} hashes updated", sidecar_path.display());
                        }
                        total_rehashed += n;
                    }
                    Err(e) => {
                        eprintln!("Error ({}): {e}", sidecar_path.display());
                        errors += 1;
                    }
                }
            }

            println!("\n{total_rehashed} hashes rehashed across {} files", targets.len());
            if errors > 0 {
                process::exit(2);
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

/// Re-hash a single sidecar file: read source, recompute all hashes with
/// `hash_span_t`, write back.  Returns the number of hashes updated.
fn rehash_sidecar(sidecar_path: &std::path::Path) -> Result<usize, String> {
    let raw = std::fs::read_to_string(sidecar_path)
        .map_err(|e| format!("read sidecar: {e}"))?;
    let mut sidecar = liyi::sidecar::parse_sidecar(&raw)?;

    // Derive source path from sidecar location (strip .liyi.jsonc suffix).
    let source_path = {
        let name = sidecar_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("bad sidecar filename")?;
        let source_name = name.strip_suffix(".liyi.jsonc")
            .ok_or("sidecar does not end in .liyi.jsonc")?;
        sidecar_path.parent().unwrap().join(source_name)
    };

    let source_content = std::fs::read_to_string(&source_path)
        .map_err(|e| format!("read source {}: {e}", source_path.display()))?;

    let mut count = 0usize;
    for spec in &mut sidecar.specs {
        match spec {
            liyi::sidecar::Spec::Item(item) => {
                if let Ok((h, _a)) = liyi::hashing::hash_span_t(&source_content, item.source_span) {
                    if item.source_hash.as_deref() != Some(&h) {
                        item.source_hash = Some(h);
                        count += 1;
                    }
                }
                // Rehash related requirement edges — these point to
                // requirement source, not rehashable here.  Leave as-is;
                // `liyi check --fix` will update them.
            }
            liyi::sidecar::Spec::Requirement(req) => {
                if let Ok((h, _a)) = liyi::hashing::hash_span_t(&source_content, req.source_span) {
                    if req.source_hash.as_deref() != Some(&h) {
                        req.source_hash = Some(h);
                        count += 1;
                    }
                }
            }
        }
    }

    if count > 0 {
        let output = liyi::sidecar::write_sidecar(&sidecar);
        std::fs::write(sidecar_path, output)
            .map_err(|e| format!("write sidecar: {e}"))?;
    }

    Ok(count)
}
