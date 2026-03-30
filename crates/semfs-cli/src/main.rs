use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

mod commands;
mod config;

#[derive(Parser)]
#[command(name = "semfs")]
#[command(about = "SemanticFS - Access files by meaning, not paths")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Mount a semantic filesystem
    Mount {
        /// Source directory to index
        source: PathBuf,
        /// Mount point for the FUSE filesystem
        mountpoint: PathBuf,
        /// Embedding model to use
        #[arg(long)]
        model: Option<String>,
        /// Mount as read-only
        #[arg(long)]
        read_only: bool,
    },

    /// Unmount a semantic filesystem
    Unmount {
        /// Mount point to unmount
        mountpoint: PathBuf,
    },

    /// Build or update the file index
    Index {
        /// Directory to index
        source: PathBuf,
        /// Force full reindex (ignore cache)
        #[arg(long)]
        full: bool,
    },

    /// Search files by semantic query
    Search {
        /// Natural language query
        query: String,
        /// Maximum number of results
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },

    /// Show index and system status
    Status,

    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Diagnose issues
    Diagnose {
        /// Subsystem to diagnose (query, index, cache)
        subsystem: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Force a full reindex (alias for `index --full`)
    Reindex {
        /// Directory to index (defaults to first configured source path)
        source: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Set a configuration value
    Set { key: String, value: String },
    /// Get a configuration value
    Get { key: String },
}

fn main() {
    let cli = Cli::parse();

    // Initialize logging
    let filter = if cli.verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("info")
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    let result = match cli.command {
        Commands::Mount {
            source,
            mountpoint,
            model,
            read_only,
        } => {
            #[cfg(feature = "fuse")]
            {
                commands::mount::execute(source, mountpoint, model, read_only)
            }
            #[cfg(not(feature = "fuse"))]
            {
                let _ = (source, mountpoint, model, read_only);
                Err(anyhow::anyhow!(
                    "FUSE support not compiled. Install macFUSE/libfuse3, then rebuild with:\n  \
                     cargo build --workspace --features semfs-cli/fuse\n\n  \
                     Use 'semfs search' for CLI-only mode."
                ))
            }
        }
        Commands::Unmount { mountpoint } => {
            #[cfg(feature = "fuse")]
            {
                commands::unmount::execute(mountpoint)
            }
            #[cfg(not(feature = "fuse"))]
            {
                let _ = mountpoint;
                Err(anyhow::anyhow!(
                    "FUSE support not compiled. Rebuild with --features semfs-cli/fuse"
                ))
            }
        }
        Commands::Index { source, full } => commands::index::execute(source, full),
        Commands::Search { query, limit } => commands::search::execute(query, limit),
        Commands::Status => commands::status::execute(),
        Commands::Config { action } => match action {
            ConfigAction::Set { key, value } => commands::config_cmd::execute_set(key, value),
            ConfigAction::Get { key } => commands::config_cmd::execute_get(key),
        },
        Commands::Diagnose { subsystem, json } => commands::diagnose::execute(subsystem, json),
        Commands::Reindex { source } => {
            let source = source.unwrap_or_else(|| {
                let cfg = config::AppConfig::load();
                cfg.source.paths.first().cloned().unwrap_or_else(|| {
                    eprintln!("Error: No source path provided and none configured.");
                    std::process::exit(1);
                })
            });
            commands::index::execute(source, true)
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
