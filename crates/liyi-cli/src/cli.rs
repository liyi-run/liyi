use clap::{ArgAction, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "liyi",
    version = "0.1.0",
    about = "立意 — establish intent before execution"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// Minimum severity level for displayed diagnostics.
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum DiagnosticLevel {
    /// Show everything — current, trivial, ignored, unreviewed, stale, errors
    #[default]
    All,
    /// Show warnings and errors only
    Warning,
    /// Show errors only
    Error,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Check specs for staleness, review status, and requirement tracking
    Check {
        /// Paths to check (default: CWD, recursive)
        paths: Vec<PathBuf>,

        /// Auto-correct shifted spans and fill missing hashes
        #[arg(long)]
        fix: bool,

        /// Preview --fix corrections without writing files
        #[arg(long, requires = "fix")]
        dry_run: bool,

        /// Fail if any reviewed spec is stale
        #[arg(long, default_value_t = true, action = ArgAction::Set)]
        fail_on_stale: bool,

        /// Fail if specs exist without review
        #[arg(long, default_value_t = false, action = ArgAction::Set)]
        fail_on_unreviewed: bool,

        /// Fail if a reviewed spec references a changed requirement
        #[arg(long, default_value_t = true, action = ArgAction::Set)]
        fail_on_req_changed: bool,

        /// Fail if requirements exist in source but not in sidecars
        #[arg(long, default_value_t = true, action = ArgAction::Set)]
        fail_on_untracked: bool,

        /// Override repo root (default: walk up to .git/)
        #[arg(long)]
        root: Option<PathBuf>,

        /// Show all diagnostics including "hash matches" (hidden by default)
        #[arg(short, long)]
        verbose: bool,

        /// Minimum severity level for displayed diagnostics
        #[arg(long, value_enum, default_value_t = DiagnosticLevel::All)]
        level: DiagnosticLevel,

        /// Emit agent-consumable JSON output for coverage gaps
        #[arg(long)]
        prompt: bool,
    },

    /// Migrate sidecar files to the current schema version
    Migrate {
        /// Sidecar files or directories to migrate (recursive)
        files: Vec<PathBuf>,
    },

    /// Scaffold AGENTS.md or skeleton .liyi.jsonc sidecar
    Init {
        /// Source file to create a skeleton sidecar for.
        /// If omitted, appends agent instruction to AGENTS.md.
        source_file: Option<PathBuf>,

        /// Overwrite existing files
        #[arg(long)]
        force: bool,
    },

    /// Mark specs as reviewed by a human
    Approve {
        /// Sidecar files, source files, or directories to approve
        paths: Vec<PathBuf>,

        /// Approve all without prompting
        #[arg(long)]
        yes: bool,

        /// Preview what would be approved without writing files
        #[arg(long)]
        dry_run: bool,

        /// Filter to a specific item by name
        #[arg(long)]
        item: Option<String>,
    },
}
