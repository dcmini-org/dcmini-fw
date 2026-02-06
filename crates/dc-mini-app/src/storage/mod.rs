pub mod data;
pub mod keys;
pub mod profile_manager;

// Re-export commonly used items for convenience
pub use data::{HapticConfig, NeopixelConfig, StorageData};
pub use keys::{Setting, StorageKey};
pub use profile_manager::ProfileManager;
