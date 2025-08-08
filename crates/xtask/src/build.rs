use crate::constants::TARGET;
use anyhow::{Context, Result};
use std::process::Command;

pub fn build_all_firmware(
    features: Option<&str>,
    release: bool,
) -> Result<()> {
    // 1. Build bootloader
    println!("Building bootloader...");
    build_firmware("crates/dc-mini-boot/Cargo.toml", features, release)?;

    // 2. Build application
    println!("Building application...");
    build_firmware("crates/dc-mini-app/Cargo.toml", features, release)?;

    Ok(())
}

fn build_firmware(
    manifest_path: &str,
    features: Option<&str>,
    release: bool,
) -> Result<()> {
    let mut cargo_build = Command::new("cargo");
    cargo_build
        .arg("build")
        .arg("--no-default-features")
        .arg("--manifest-path")
        .arg(manifest_path)
        .arg("--target")
        .arg(TARGET);

    if release {
        cargo_build.arg("--release");
    }

    if let Some(features) = features {
        cargo_build.args(["--features", features]);
    }

    let status = cargo_build
        .status()
        .with_context(|| format!("Failed to build {}", manifest_path))?;

    if !status.success() {
        anyhow::bail!("Build failed for {}", manifest_path);
    }

    Ok(())
}
