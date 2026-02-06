//! I2C Bus Manager for power-efficient shared bus access
//!
//! Thin type aliases over the generic `bus_manager` crate, specialized for
//! the TWIM1 peripheral on nRF52840.

use bus_manager::{BusHandle, BusManager};
use dc_mini_bsp::Twim1Factory;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;

/// I2C bus manager for the TWIM1 peripheral.
pub type I2cBusManager = BusManager<CriticalSectionRawMutex, Twim1Factory>;

/// RAII handle for accessing the shared I2C bus.
pub type I2cBusHandle<'a> =
    BusHandle<'a, CriticalSectionRawMutex, Twim1Factory>;
