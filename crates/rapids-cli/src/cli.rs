use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// RAPIDS command line interface.
#[derive(Debug, Parser)]
#[command(name = "rapids", version, about = "Fast private downloads from the command line")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Download a file over HTTP or HTTPS.
    Get(GetArgs),
    /// Remove the resume workspace of a download that is not running.
    Cancel(CancelArgs),
}

#[derive(Debug, Args)]
pub struct GetArgs {
    /// URL of the file to download.
    pub url: String,

    /// Destination file. Without it the server's filename (or the URL's
    /// last segment) is used in the current directory.
    #[arg(short = 'o', long)]
    pub output: Option<PathBuf>,

    /// Expected SHA-256 as 64 hex characters; checked before publishing.
    #[arg(long)]
    pub sha256: Option<String>,

    /// Fixed connection count (1-32). Without it the engine scales 8→32.
    #[arg(long)]
    pub connections: Option<u8>,

    /// Replace the destination when it already exists.
    #[arg(long)]
    pub overwrite: bool,
}

#[derive(Debug, Args)]
pub struct CancelArgs {
    /// Destination file of the download, or its `.rapids` sidecar
    /// directory.
    pub target: PathBuf,
}
