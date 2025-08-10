# DC-Mini Firmware

© 2025 The Johns Hopkins University Applied Physics Laboratory LLC

This repository contains the firmware for the DCMini board, a miniaturized biopotential amplifier and multisensor suite. The firmware is implemented using Rust and follows a modular approach with multiple crates for different components.

## Crates

### dc-mini-bsp

The `dc-mini-bsp` crate is the Board Support Package (BSP) for the DCMini board. It implements drivers for various peripherals available on the board, including GPIO pins, SPI interface for ADS1299 EEG amplifiers, I2C interface for the accelerometer and ambient light sensor, as well as control for the neopixel LED and speaker. The BSP provides a hardware abstraction layer to simplify interaction with the board's peripherals.

### dc-mini-boot

The `dc-mini-boot` crate is the boot manager for the DCMini firmware. It supports firmware upgrades and ensures automatic firmware fallback in case the watchdog timer expires. The boot manager handles the firmware update process and maintains a reliable firmware execution environment, enhancing the reliability and robustness of the device.

### dc-mini-app

The `dc-mini-app` crate contains the application logic for the DCMini firmware. It is responsible for polling and processing data from various sensors, including the ADS1299 EEG amplifiers, accelerometer, ambient light sensor, and microphone. The crate also includes logic to control the neopixel LED and speaker, allowing for customizable feedback and notification features.

## Toolchain Setup

To set up the development environment and build the firmware, follow these steps:

1. Make sure the Nix package manager is installed on your host machine. If Nix is not installed, you can follow the installation instructions for your operating system from the [official Nix website](https://nixos.org/guides/install-nix.html).

2. Open a terminal and navigate to the root directory of this project.

3. Run the following command to set up the toolchain environment using Nix:

   ```
   nix develop
   ```

   This command initializes the development environment with all the necessary dependencies and configurations required for building the firmware.

4. After running `nix develop`, you will be inside a shell environment with the toolchain set up. From here, you can execute build commands and interact with the firmware project.

By following these steps, the toolchain environment will be properly set up using the Nix package manager. This ensures a consistent and reproducible development environment across different machines.

Please note that if you encounter any issues during the toolchain setup process, refer to the documentation or support resources for the Nix package manager for further assistance.

## Development

### Building

To build the firmware with USB support:
```bash
cargo xbuild --features "defmt,usb"
```

To build with SoftDevice support:
```bash
cargo xbuild --features "defmt,softdevice"
```

To build with both USB and SoftDevice:
```bash
cargo xbuild --features "defmt,usb,softdevice"
```

Add `--release` flag for release builds.

### Flashing

To flash the firmware with USB support:
```bash
cargo xflash --features "defmt,usb"
```

To flash with SoftDevice support:
```bash
cargo xflash --features "defmt,softdevice"
```

To flash with both USB and SoftDevice:
```bash
cargo xflash --features "defmt,usb,softdevice"
```

Add `--release` flag for release builds.
Add `--force` flag to force flash even if the binary hasn't changed.
Note: `xflash` automatically builds the firmware before flashing.

### Running with RTT

To build, flash and run with RTT logging (USB support):
```bash
cargo xrun --features "defmt,usb"
```

To run with SoftDevice support:
```bash
cargo xrun --features "defmt,softdevice"
```

To run with both USB and SoftDevice:
```bash
cargo xrun --features "defmt,usb,softdevice"
```

Add `--release` flag for release builds.
Note: `xrun` automatically builds and flashes the firmware before attaching RTT.

### Attaching RTT

To attach RTT to a running target:
```bash
cargo xattach
```

Add `--release` flag if connecting to a release build.

### Initial Setup

If this is your first time flashing the board:

1. Erase the current settings:
```bash
probe-rs erase --chip nRF52840_xxAA
```
_NOTE_: You may need to add the `--allow-erase-all` flag to the above command if this is your first time flashing the board. This is due to Access Port Protection on the NRF MCU.

2. Flash the firmware with your desired features:
```bash
cargo xflash --features "defmt,usb,softdevice" --force  # Example with both USB and SoftDevice support
```

The xtask system will automatically handle flashing the bootloader, softdevice (if enabled), and application in the correct order.

## Acknowledgments
This work was supported in part by intramural research funding from [Johns Hopkins University Applied Physics Lab (JHU APL)](https://www.jhuapl.edu/).

* [Griffin Milsap](mailto:griffin.milsap@jhuapl.edu): Hardware design
* [Preston Peranich](mailto:preston.peranich@jhuapl.edu): Firmware implementation
* [Will Coon](mailto:will.coon@jhuapl.edu): Device validation

If you use this hardware in a project or publication, we’d love to hear about it!  Additionally, please consider citing: 

```
Coon, W. G., Peranich, P., & Milsap, G. (2025). StARS DCM: A Sleep Stage-Decoding Forehead EEG Patch for Real-time Modulation of Sleep Physiology. arXiv preprint arXiv:2506.03442.
```

## License

Dual licensed under your choice of either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  <http://opensource.org/licenses/MIT>)

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
