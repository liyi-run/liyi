use clap::{ArgAction, Parser, Subcommand};
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

#[derive(Subcommand)]
pub enum Commands {
    /// Check specs for staleness, review status, and requirement tracking
    Check {
        /// Paths to check (default: CWD, recursive)
        paths: Vec<PathBuf>,

        /// Auto-correct shifted spans and fill missing hashes
        #[arg(long)]
        fix: bool,

        /// Fail if any reviewed spec is stale
        #[arg(long, default_value_t = true, action = ArgAction::Set)]
        fail_on_stale: bool,

        /// Fail if specs exist without review
        #[arg(long, default_value_t = false, action = ArgAction::Set)]
        fail_on_unreviewed: bool,

        /// Fail if a reviewed spec references a changed requirement
        #[arg(long, default_value_t = true, action = ArgAction::Set)]
        fail_on_req_changed: bool,

        /// Override repo root (default: walk up to .git/)
        #[arg(long)]
        root: Option<PathBuf>,

        /// Show all diagnostics including "hash matches" (hidden by default)
        #[arg(short, long)]
        verbose: bool,
    },

    /// Re-hash source spans in sidecar files
    Reanchor {
        /// Sidecar files or directories to reanchor (recursive)
        #[arg(required_unless_present = "migrate")]
        files: Vec<PathBuf>,

        /// Target a specific item by name
        #[arg(long, requires = "span")]
        item: Option<String>,

        /// Override span (start,end)
        #[arg(long, requires = "item", value_parser = parse_span)]
        span: Option<[usize; 2]>,

        /// Migrate sidecar to current schema version
        #[arg(long)]
        migrate: bool,
    },
}

/// Parse a "start,end" string into a [usize; 2] span.
fn parse_span(s: &str) -> Result<[usize; 2], String> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 2 {
        return Err(format!("expected format 'start,end', got '{s}'"));
    }
    let start: usize = parts[0]
        .trim()
        .parse()
        .map_err(|_| format!("invalid start: '{}'", parts[0].trim()))?;
    let end: usize = parts[1]
        .trim()
        .parse()
        .map_err(|_| format!("invalid end: '{}'", parts[1].trim()))?;
    Ok([start, end])
}
