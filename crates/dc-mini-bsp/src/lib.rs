#![no_std]
#![doc = include_str!("../README.md")]
#[macro_use]

// Modules
mod board;
mod resources;

// Flatten
pub use board::*;
pub use resources::*;

#[cfg(feature = "trouble")]
pub mod ble;
#[cfg(feature = "usb")]
pub mod usb;
