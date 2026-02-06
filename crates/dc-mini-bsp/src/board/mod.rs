const _ENABLED_FEATURES: u32 = 0 + if cfg!(feature = "sr6") { 1 } else { 0 };
const _: () = if _ENABLED_FEATURES > 1 {
    panic!("At most one hardware feature may be enabled.");
};

cfg_if::cfg_if! {
    if #[cfg(feature = "sr6")] {
        pub mod sr6;
        pub use sr6::*;
    } else {
        // Default fallback to sr6.
        pub mod sr6;
        pub use sr6::*;
    }
}
