{
  description = "Toolchain setup for DC-Mini firmware development";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs = { nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
          config = {
            allowUnfree = true;
          };
        };
        toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        # Create a clean, maintainable PKG_CONFIG_PATH
        pkgConfigDirs = with pkgs; [
          "${dbus}/lib/pkgconfig"
        ];
      in
      with pkgs;
      {
        devShells.default = mkShell {
          buildInputs = [
            toolchain

            protobuf
            openssl
            pkg-config
            # Embedded/Cross-compiling dependencies
            picocom
            libusb1
            libftdi1
            elf2uf2-rs
            gcc-arm-embedded
            cargo-binstall
            cargo-watch
            wget

            # pkgconfig deps
            dbus
          ]
          ++ lib.optionals stdenv.hostPlatform.isDarwin [
            # apple-sdk_15
            # darwin.libiconv
          ]
          ++ lib.optionals stdenv.hostPlatform.isLinux [ systemd libiconv ];

          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";

          shellHook = ''
            export PATH=$PATH:''${CARGO_HOME:-~/.cargo}/bin

            installed_tools=$(cargo install --list)
            # List of tools to check
            tools=("probe-rs-tools" "cargo-bloat" "cargo-binutils" "cargo-expand" "bacon" "cargo-bundle" "cargo-update" "cargo-dist")

            # Iterate through each tool and check if it's installed
            for tool in "''${tools[@]}"; do
              tool=''${tool%@*}
              if ! echo "$installed_tools" | grep -q "^$tool " ; then
                cargo-binstall --no-confirm $tool
              fi
            done

            cargo install-update "''${tools[@]}"

            # Directory to check and create
            dir="softdevice"
            # Archive filename
            zipFile="s140_nrf52_7.3.0.zip"
            # URL of the zip archive
            url="https://nsscprodmedia.blob.core.windows.net/prod/software-and-other-downloads/softdevices/s140/$zipFile"
            # Check if the directory exists
            if [ ! -d "$dir" ]; then
                echo "$dir does not exist. Creating directory and downloading the file..."
                # Make directory
                mkdir -p $dir
                # Download the zip file
                wget $url -O $zipFile
                # Unzip only the .hex files to the specified directory
                unzip -j $zipFile "*.hex" -d $dir
                # Cleanup
                rm $zipFile
                echo "Done."
            fi
          '';

          RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";

          PKG_CONFIG_PATH = builtins.concatStringsSep ":" pkgConfigDirs;

        };
      }
    );
}
