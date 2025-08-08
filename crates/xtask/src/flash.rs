use anyhow::{Context, Result};
use std::process::Command;

use crate::constants::{CHIP, SOFTDEVICE_PATH};

pub fn flash_firmware(
    features: Option<&str>,
    release: bool,
    force: bool,
) -> Result<()> {
    // First build the firmware
    crate::build::build_all_firmware(features, release)?;

    let profile = if release { "release" } else { "debug" };
    let bootloader_path =
        format!("target/thumbv7em-none-eabihf/{}/dc-mini-boot", profile);
    let app_path =
        format!("target/thumbv7em-none-eabihf/{}/dc-mini-app", profile);

    if force {
        println!("Erasing chip...");
        let mut cmd = Command::new("probe-rs");
        cmd.args(["erase", "--chip", CHIP, "--allow-erase-all"]);
        let status = cmd.status().context("Failed to erase chip")?;
        if !status.success() {
            anyhow::bail!("Failed to erase chip");
        }
    }

    // If softdevice feature is enabled, flash it first
    if features.is_some_and(|f| f.contains("softdevice")) {
        println!("Checking/Flashing Softdevice...");
        let mut cmd = Command::new("probe-rs");
        cmd.args([
            "download",
            "--chip",
            CHIP,
            SOFTDEVICE_PATH,
            "--binary-format",
            "hex",
            "--preverify",
        ]);

        let status = cmd.status().context("Failed to flash softdevice")?;
        if !status.success() {
            anyhow::bail!("Failed to flash softdevice");
        }
    }

    // Flash bootloader
    println!("Checking/Flashing Bootloader...");
    let mut cmd = Command::new("probe-rs");
    cmd.args([
        "download",
        "--chip",
        CHIP,
        &bootloader_path,
        "--preverify",
        "--restore-unwritten",
    ]);

    let status = cmd.status().context("Failed to flash bootloader")?;
    if !status.success() {
        anyhow::bail!("Failed to flash bootloader");
    }

    // Flash application
    println!("Checking/Flashing App...");
    let mut cmd = Command::new("probe-rs");
    cmd.args([
        "download",
        "--chip",
        CHIP,
        &app_path,
        "--preverify",
        "--restore-unwritten",
    ]);

    let status = cmd.status().context("Failed to flash application")?;
    if !status.success() {
        anyhow::bail!("Failed to flash application");
    }

    Ok(())
}
