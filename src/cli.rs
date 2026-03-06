use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "liyi", version = "0.1.0", about = "Lìyì - Intent linter and toolkit")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Lint: staleness, review status, requirement tracking
    Check {
        /// Optional paths to check (defaults to current directory)
        paths: Vec<PathBuf>,

        /// Auto-correct shifted spans, fill missing hashes
        #[arg(long)]
        fix: bool,
    },
    /// Manual span re-hashing for targeted fixes
    Reanchor {
        /// Path to the sidecar file
        #[arg(required_unless_present = "migrate")]
        sidecar: Option<PathBuf>,

        /// Specific item name to reanchor
        #[arg(long, requires = "span")]
        item: Option<String>,

        /// Specific span to reanchor, format: start,end (e.g., 10,20)
        #[arg(long, requires = "item")]
        span: Option<String>,

        /// Schema version migration (no-op in 0.1, scaffolded)
        #[arg(long, conflicts_with_all = ["sidecar", "item", "span"])]
        migrate: bool,
    },
}