# Board Revision Pattern

## Overview

The BSP crate supports multiple hardware revisions via Cargo features and
`cfg_if` routing. Only one board feature may be enabled at a time; this is
enforced by a compile-time constant check in `board/mod.rs`.

## Feature system

Each board revision has a corresponding feature in `Cargo.toml` (e.g. `sr6 = []`).
Dependent crates re-export the feature so it propagates:

```toml
# dc-mini-app/Cargo.toml
sr6 = ["dc-mini-bsp/sr6"]
```

`board/mod.rs` uses `cfg_if!` to select the correct module:

```rust
cfg_if::cfg_if! {
    if #[cfg(feature = "sr6")] {
        pub mod sr6;
        pub use sr6::*;
    } else {
        // Default fallback
        pub mod sr6;
        pub use sr6::*;
    }
}
```

## What each board file must export

Every board file (e.g. `sr6.rs`) must define and export:

### Resource structs

| Struct | Purpose |
|--------|---------|
| `ImuResources` | IMU interrupt + sync pins |
| `Twim1BusResources` | I2C bus instance + SDA/SCL |
| `AdsResources` | ADS1299 control pins (pwdn, reset, start, cs1, cs2, drdy) |
| `Spi3BusResources` | SPI3 bus instance + clock/data pins |
| `SdCardResources` | SD card SPI bus + CS + SDIO pins |
| `ExternalFlashResources` | QSPI instance + 4 data lines + SCK + CSN |

### `DCMini` struct

Top-level struct containing:
- Individual pins (pwrbtn, neopix, en5v, usbsel, haptrig, vbus_src, mic, gpio, etc.)
- All resource sub-structs above
- Standard nRF peripherals (timers, UARTs, PWMs, RTC2, WDT, NVMC, etc.)
- `#[cfg(feature = "trouble")] ble: ble::BleControllerBuilder<'static>`
- `#[cfg(feature = "usb")] usb: usb::UsbDriverBuilder`

**Important:** Do NOT include `RTC1` — it is used by the time driver and gated
behind `not(time-driver-rtc1)` in embassy-nrf 0.9.

### `Default` impl and `new()` constructor

```rust
impl Default for DCMini {
    fn default() -> Self {
        let mut config = embassy_nrf::config::Config::default();
        config.gpiote_interrupt_priority = Priority::P2;
        config.time_interrupt_priority = Priority::P2;
        Self::new(config)
    }
}

impl DCMini {
    pub fn new(config: embassy_nrf::config::Config) -> Self {
        let p = embassy_nrf::init(config);
        Self { /* assign all pins from `p` */ }
    }
}
```

## Adding a new board revision (e.g. SR7)

1. Create `crates/dc-mini-bsp/src/board/sr7.rs` following the pattern above.
2. Add `sr7 = []` to `crates/dc-mini-bsp/Cargo.toml` features.
3. Add `sr7 = ["dc-mini-bsp/sr7"]` to `crates/dc-mini-app/Cargo.toml` and
   `crates/dc-mini-boot/Cargo.toml`.
4. Update `board/mod.rs` — add to the feature count check and `cfg_if!` chain.
5. Update `crates/dc-mini-app/build.rs` — add the `HwVersion` variant and
   feature entry.
6. If the new board has different power control behavior, sensor support, or
   pin mappings, add appropriate `#[cfg(feature = "sr7")]` gates in the app
   crate where needed.
