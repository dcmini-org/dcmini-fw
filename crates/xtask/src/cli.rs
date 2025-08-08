use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Build the firmware
    Build {
        #[arg(long)]
        features: Option<String>,

        #[arg(long)]
        release: bool,
    },
    /// Build and flash the firmware
    Flash {
        #[arg(long)]
        features: Option<String>,

        #[arg(long)]
        release: bool,

        #[arg(long)]
        force: bool,
    },
    /// Build, flash, and run with RTT logging
    Run {
        #[arg(long)]
        features: Option<String>,

        #[arg(long)]
        release: bool,
    },
    /// Attach to target and show RTT logs
    Attach {
        #[arg(long)]
        release: bool,
    },
}
