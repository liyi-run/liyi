use std::env;
use std::io::{self, IsTerminal};
use std::path::PathBuf;
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

            let agents_files = match liyi::discovery::resolve_agents_md_targets(&files) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(2);
                }
            };
            // Explicit agent-instruction files are not source files, so keep
            // them out of sidecar resolution (which would otherwise demand a
            // co-located `.liyi.jsonc`).
            let sidecar_inputs: Vec<PathBuf> = files
                .iter()
                .filter(|p| !agents_files.contains(p))
                .cloned()
                .collect();
            let sidecar_targets = if sidecar_inputs.is_empty() {
                Vec::new()
            } else {
                match liyi::discovery::resolve_sidecar_targets(&sidecar_inputs) {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("Error: {e}");
                        process::exit(2);
                    }
                }
            };

            if sidecar_targets.is_empty() && agents_files.is_empty() {
                eprintln!(
                    "no .liyi.jsonc sidecars or agent-instruction blocks found in the given paths"
                );
                process::exit(2);
            }

            let mut errors = 0;
            for sidecar_path in &sidecar_targets {
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
            for agents_path in &agents_files {
                match liyi::init::migrate_agents_md(agents_path) {
                    Ok(_) => {
                        println!("Migrated: {}", agents_path.display());
                    }
                    Err(e) => {
                        eprintln!("Error ({}): {e}", agents_path.display());
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
                    Ok(path) => {
                        println!("Created: {}", path.display());
                        // When discovery was requested but the file's extension
                        // maps to no supported grammar, item discovery silently
                        // yields nothing. Surface that so an empty scaffold is not
                        // mistaken for a fully-covered file.
                        if !no_discover && liyi::tree_path::detect_language(&src).is_none() {
                            let ext = src
                                .extension()
                                .and_then(|e| e.to_str())
                                .map(|e| format!(" (.{e})"))
                                .unwrap_or_default();
                            println!(
                                "note: unsupported file format{ext}; no items discovered \
                                 — wrote an empty scaffold for manual authoring"
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: {e}");
                        process::exit(1);
                    }
                }
            }
            None => {
                let root = env::current_dir().unwrap_or_default();
                match liyi::init::init_agents_md(&root, force) {
                    Ok(path) => {
                        println!("Initialized: {}", path.display());
                        println!(
                            "note: these instructions follow the AGENTS.md convention \
                             (https://agents.md); if your harness does not read AGENTS.md, it is \
                             your responsibility to place the block where your harness will read it."
                        );
                    }
                    Err(e) => {
                        eprintln!("Error: {e}");
                        process::exit(1);
                    }
                }
            }
        },
        Commands::Context { target, root } => {
            run_context(target, root);
        }
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

/// Resolve and print the context notes applicable to a `<path>` or
/// `<path>:<line>` location. The line component is accepted for forward
/// compatibility; the MVP resolves notes file-scoped.
///
/// <!-- @liyi:intent Parse the target into a path (and optional trailing
/// :line), locate the repo root, resolve applicable notes via the live note
/// scan, print the rendered report to stdout, and exit 0. Exit 2 on usage or
/// resolution errors (bad line number, path outside the repo, missing root). -->
fn run_context(target: String, root: Option<std::path::PathBuf>) {
    // Split an optional trailing `:<line>` (line must be all digits so that
    // Windows-style drive letters or paths containing ':' are not misparsed).
    let (path_str, _line): (&str, Option<usize>) = match target.rsplit_once(':') {
        Some((p, n)) if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => {
            match n.parse::<usize>() {
                Ok(parsed) => (p, Some(parsed)),
                Err(_) => {
                    eprintln!("Error: invalid line number in '{target}'");
                    process::exit(2);
                }
            }
        }
        _ => (target.as_str(), None),
    };

    let path = std::path::Path::new(path_str);
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir().unwrap_or_default().join(path)
    };

    let root = match root.or_else(|| liyi::discovery::find_repo_root(&abs)) {
        Some(r) => r,
        None => {
            eprintln!("Error: could not locate repo root (no .git/ found); pass --root");
            process::exit(2);
        }
    };
    // Normalize both paths to a common absolute, symlink-resolved form so a
    // relative `--root .` (or `.`/`..` components in the target) still strips
    // correctly. Fall back to the un-normalized path when canonicalization
    // fails (e.g. the target does not exist yet).
    let root = std::fs::canonicalize(&root).unwrap_or(root);
    let abs = std::fs::canonicalize(&abs).unwrap_or(abs);

    let rel = match abs.strip_prefix(&root) {
        Ok(r) => r.to_string_lossy().replace('\\', "/"),
        Err(_) => {
            eprintln!(
                "Error: target '{}' is not inside repo root '{}'",
                abs.display(),
                root.display()
            );
            process::exit(2);
        }
    };

    let notes = liyi::context::resolve(&root, &rel);
    print!("{}", liyi::context::render(&notes));
}

/// Check if stderr is a TTY (for interactive mode detection).
/// Uses stderr since the TUI renders there.
///
/// <!-- @liyi:intent=doc -->
fn is_tty() -> bool {
    io::stderr().is_terminal()
}
