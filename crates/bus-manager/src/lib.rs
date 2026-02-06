#![no_std]
//! Generic bus lifecycle manager for shared peripheral access.
//!
//! Provides automatic configuration/deconfiguration of a bus based on usage,
//! with safe concurrent access across multiple tasks. The bus is only created
//! when first acquired and can be explicitly released when no longer needed.

mod error;
mod factory;
mod handle;
mod manager;

pub use error::BusError;
pub use factory::BusFactory;
pub use handle::BusHandle;
pub use manager::BusManager;
