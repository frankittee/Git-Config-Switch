use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "gcs",
    version,
    about = "Switch Git identity profiles in the current repository",
    after_help = "Examples:\n  gcs add work\n  gcs edit work\n  gcs edit work --email new@example.com\n  gcs add personal --name \"Ada Lovelace\" --email ada@example.com\n  gcs use work\n  gcs info"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Add a new profile
    Add {
        /// Unique profile name
        profile: String,
        /// Git author name
        #[arg(long)]
        name: Option<String>,
        /// Git author email
        #[arg(long)]
        email: Option<String>,
        /// Signing key; also enables commit.gpgsign
        #[arg(long)]
        signing_key: Option<String>,
    },
    /// Edit an existing profile
    Edit {
        /// Saved profile name
        profile: String,
        /// Replace the Git author name
        #[arg(long)]
        name: Option<String>,
        /// Replace the Git author email
        #[arg(long)]
        email: Option<String>,
        /// Set the signing key and enable commit signing
        #[arg(long, conflicts_with = "no_signing")]
        signing_key: Option<String>,
        /// Remove the signing key and disable commit signing
        #[arg(long)]
        no_signing: bool,
    },
    /// List saved profile names
    List,
    /// Show a saved profile
    Show {
        /// Saved profile name
        profile: String,
    },
    /// Remove a saved profile
    Remove {
        /// Saved profile name
        profile: String,
    },
    /// Apply a profile to the current Git repository
    Use {
        /// Saved profile name
        profile: String,
    },
    /// Show the profile matching the current repository configuration
    Info,
}
