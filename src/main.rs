pub mod cli;
pub mod check;
pub mod discovery;
pub mod sidecar;
pub mod markers;
pub mod hashing;
pub mod shift;
pub mod reanchor;
pub mod diagnostics;
pub mod schema;

use clap::Parser;
use cli::{Cli, Commands};

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Check { paths, fix } => {
            println!("Running check... paths: {:?}, fix: {}", paths, fix);
            // TODO: dispatch to crate::check
        }
        Commands::Reanchor { sidecar, item, span, migrate } => {
            if *migrate {
                println!("Running migration... (scaffold)");
            } else if let Some(path) = sidecar {
                println!("Running reanchor on {:?}... item: {:?}, span: {:?}", path, item, span);
                // TODO: dispatch to crate::reanchor
            }
        }
    }
}
