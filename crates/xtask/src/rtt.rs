use anyhow::{Context, Result};
use std::process::Command;

pub fn run(elf_path: &str) -> Result<()> {
    let mut cmd = Command::new("probe-rs");
    cmd.args(["attach", elf_path]);

    let status = cmd.status().context("Failed to attach probe-rs")?;

    if !status.success() {
        anyhow::bail!("probe-rs attach failed");
    }

    Ok(())
}
