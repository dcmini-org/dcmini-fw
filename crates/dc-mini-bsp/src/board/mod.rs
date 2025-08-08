const _ENABLED_FEATURES: u32 = 0
    + if cfg!(feature = "r6") { 1 } else { 0 }
    + if cfg!(feature = "sr1") { 1 } else { 0 }
    + if cfg!(feature = "sr2") { 1 } else { 0 }
    + if cfg!(feature = "sr3") { 1 } else { 0 };
const _: () = if _ENABLED_FEATURES > 1 {
    panic!("At most one hardware feature may be enabled.");
};

// Ensure only one feature is enabled
cfg_if::cfg_if! {
    if #[cfg(feature = "r6")] {
        pub mod r6;
        pub use r6::*;
    }
    else if #[cfg(feature = "sr1")] {
        pub mod sr1;
        pub use sr1::*;
    }
    else if #[cfg(feature = "sr2")] {
        pub mod sr2;
        pub use sr2::*;
    }
    else if #[cfg(feature = "sr3")] {
        pub mod sr3;
        pub use sr3::*;
    } else {
        // By default, let's use the rev4 board.
        pub mod r6;
        pub use r6::*;
    }
}
