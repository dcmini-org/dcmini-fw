use clap::Parser;
use dc_mini_host::clients::usb::UsbClient;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "dfu", about = "DC-Mini USB DFU firmware updater")]
struct Args {
    /// Path to the firmware binary file
    firmware: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = Args::parse();

    let firmware = std::fs::read(&args.firmware)?;
    println!(
        "Loaded firmware: {} ({} bytes)",
        args.firmware.display(),
        firmware.len()
    );

    if firmware.is_empty() {
        return Err("Firmware file is empty".into());
    }

    if firmware.len() > 992 * 1024 {
        return Err(format!(
            "Firmware too large: {} bytes (max {} bytes)",
            firmware.len(),
            992 * 1024
        )
        .into());
    }

    println!("Connecting to DC-Mini via USB...");
    let client = UsbClient::try_new()?;
    println!("Connected.");

    client.dfu_upload(&firmware).await?;

    println!("DFU complete!");
    Ok(())
}
