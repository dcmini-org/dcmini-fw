use prost_build::Config;
use std::{env, path::PathBuf, process::Command};

fn main() {
    // Compile our protos
    let mut config = Config::new();

    #[cfg(feature = "defmt")]
    {
        config.message_attribute(".", "#[derive(defmt::Format)]");
        config.enum_attribute(".", "#[derive(defmt::Format)]");
    }

    config.btree_map(&["."]);
    config.compile_protos(&["protos/ads.proto"], &["protos"]).unwrap();
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());

    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=protos/");

    // Generate Python protobuf files
    Command::new("protoc")
        .args(&[
            "--proto_path=protos",
            "--python_out=protos/",
            "--pyi_out=protos/",
            "protos/ads.proto",
        ])
        .status()
        .expect("Failed to run protoc for Python files");
}
