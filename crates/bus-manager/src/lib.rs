#![no_std]
//! Generic bus lifecycle manager for shared peripheral access.
//!
//! # Problem
//!
//! Embedded peripherals like I2C or SPI buses are often shared across multiple
//! tasks, but must be configured before use and ideally deconfigured when idle
//! (e.g., to save power). Naively sharing a bus with `'static` references leads
//! to unsound patterns like `Box::leak` for fake static lifetimes, `static mut`
//! DMA buffers, and manual reference counting with fallible `try_lock` in drop
//! handlers.
//!
//! # Solution
//!
//! `bus-manager` provides a generic `BusManager<M, F>` that:
//!
//! - **Lazily creates** the bus on first [`acquire()`](BusManager::acquire) via
//!   a user-defined [`BusFactory`]
//! - **Shares** the bus through [`BusHandle`] RAII handles with automatic
//!   reference counting
//! - **Explicitly releases** the bus via [`try_release()`](BusManager::try_release)
//!   when all handles are dropped, recovering the original peripheral resources
//!
//! The design avoids heap allocation (`#![no_std]`, no `alloc`), uses
//! [`GroundedCell`](grounded::uninit::GroundedCell) for sound in-place storage,
//! and keeps all unsafe confined to four well-documented operations.
//!
//! # Usage
//!
//! ```rust,ignore
//! // 1. Implement BusFactory for your peripheral
//! struct MyBusFactory;
//! impl BusFactory for MyBusFactory {
//!     type Bus = Mutex<CriticalSectionRawMutex, Twim<'static>>;
//!     type Resources = MyPeripheralPins;
//!     type Destructor = MyDestructor;
//!     type Error = core::convert::Infallible;
//!
//!     fn create(res: Self::Resources) -> Result<(Self::Bus, Self::Destructor), (Self::Error, Self::Resources)> {
//!         // Build the bus from peripheral resources
//!     }
//!     fn recover(d: Self::Destructor) -> Self::Resources {
//!         // Reconstruct peripheral resources for reuse
//!     }
//! }
//!
//! // 2. Create a manager (typically in a StaticCell)
//! let manager: BusManager<CriticalSectionRawMutex, MyBusFactory> =
//!     BusManager::new(my_resources);
//!
//! // 3. Acquire handles from any async task
//! let handle = manager.acquire().await?;
//! let device = I2cDevice::new(handle.bus());
//!
//! // 4. Handles drop automatically; optionally release when idle
//! drop(handle);
//! manager.try_release().await?; // recovers resources, bus powers down
//! ```
//!
//! # Safety invariants
//!
//! - The bus is written to `GroundedCell` only while the mutex is held (Idle -> Active)
//! - The bus is dropped only while the mutex is held **and** `users == 0` (Active -> Idle)
//! - `BusHandle::Deref` is safe because a live handle implies `users > 0`,
//!   which prevents `try_release` from dropping the bus
//! - `BusHandle` is `Send`/`Sync` only when `F::Bus: Sync`, mirroring `&T`

mod error;
mod factory;
mod handle;
mod manager;

pub use error::BusError;
pub use factory::BusFactory;
pub use handle::BusHandle;
pub use manager::BusManager;
