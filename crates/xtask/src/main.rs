mod build;
mod cli;
mod constants;
mod flash;
mod rtt;

use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Build the firmware
    Build {
        #[arg(long)]
        features: Option<String>,
        #[arg(long)]
        release: bool,
    },
    /// Flash the firmware
    Flash {
        #[arg(long)]
        features: Option<String>,
        #[arg(long)]
        release: bool,
        #[arg(long)]
        force: bool,
    },
    /// Build, flash and run the firmware with RTT
    Run {
        #[arg(long)]
        features: Option<String>,
        #[arg(long)]
        release: bool,
    },
    /// Attach RTT to a running target
    Attach {
        #[arg(long)]
        release: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Build { features, release } => {
            println!("Building firmware...");
            build::build_all_firmware(features.as_deref(), *release)?;
            println!("Build complete!");
        }
        Commands::Flash { features, release, force } => {
            flash::flash_firmware(features.as_deref(), *release, *force)?;
        }
        Commands::Run { features, release } => {
            // First flash with force=true to ensure clean state
            flash::flash_firmware(features.as_deref(), *release, false)?;

            // Then attach RTT
            println!("Attaching RTT...");
            rtt::run(if *release {
                "target/thumbv7em-none-eabihf/release/dc-mini-app"
            } else {
                "target/thumbv7em-none-eabihf/debug/dc-mini-app"
            })?;
        }
        Commands::Attach { release } => {
            rtt::run(if *release {
                "target/thumbv7em-none-eabihf/release/dc-mini-app"
            } else {
                "target/thumbv7em-none-eabihf/debug/dc-mini-app"
            })?;
        }
    }

    Ok(())
}
