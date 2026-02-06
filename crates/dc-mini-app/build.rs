//! This build script copies the `memory.x` file from the crate root into
//! a directory where the linker can always find it at build time.
//! For many projects this is optional, as the linker always searches the
//! project root directory -- wherever `Cargo.toml` is. However, if you
//! are using a workspace or have a more complicated build setup, this
//! build script becomes required. Additionally, by requesting that
//! Cargo re-run the build script whenever `memory.x` is changed,
//! updating `memory.x` ensures a rebuild of the application with the
//! new memory settings.

use std::{env, fs::File, io::Write, path::PathBuf};

#[derive(Clone, Copy, PartialEq, PartialOrd)]
enum HwVersion {
    SR6,
}

impl HwVersion {
    fn as_str(self) -> &'static str {
        match self {
            Self::SR6 => "sr6",
        }
    }
}

impl Default for HwVersion {
    fn default() -> Self {
        Self::SR6
    }
}

fn linker_data() -> &'static [u8] {
    include_bytes!("memory.x")
}

fn main() {
    let hw_features = [(cfg!(feature = "sr6"), HwVersion::SR6)];

    let enabled_hw: Vec<HwVersion> = hw_features
        .into_iter()
        .filter(|(enabled, _)| *enabled)
        .map(|(_, version)| version)
        .collect();

    if enabled_hw.len() > 1 {
        panic!("At most one hardware feature may be enabled.");
    }

    let hw_ver = enabled_hw.first().cloned().unwrap_or_default();

    // Put `memory.x` in our output directory and ensure it's
    // on the linker search path.
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(linker_data())
        .unwrap();
    println!("cargo:rustc-link-search={}", out.display());

    // By default, Cargo will re-run a build script whenever
    // any file in the project changes. By specifying `memory.x`
    // here, we ensure the build script is only re-run when
    // `memory.x` is changed.
    println!("cargo:rerun-if-changed=memory.x");

    println!("cargo:rustc-link-arg-bins=--nmagic");
    println!("cargo:rustc-link-arg-bins=-Tlink.x");
    if env::var("CARGO_FEATURE_DEFMT").is_ok() {
        println!("cargo:rustc-link-arg-bins=-Tdefmt.x");
    }

    // Build info
    let pkg_version = env!("CARGO_PKG_VERSION");
    let git_hash_bytes = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .expect("Failed to execute git command")
        .stdout;

    let git_hash_str = std::str::from_utf8(&git_hash_bytes)
        .expect("Not a valid utf8 string")
        .trim();

    println!("cargo:rustc-env=COMMIT_HASH={git_hash_str}");
    println!("cargo:rustc-env=FW_VERSION={pkg_version}-{git_hash_str}");

    println!("cargo:rustc-env=HW_VERSION={}", hw_ver.as_str());
}
