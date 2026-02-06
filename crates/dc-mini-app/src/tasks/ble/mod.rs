use derive_more::From;

#[cfg(all(feature = "trouble", feature = "softdevice"))]
compile_error!("You may not enable both `trouble` and `softdevice` features.");

pub mod ads_stream;
// pub use ads_stream::*;

cfg_if::cfg_if! {
    if #[cfg(feature = "softdevice")] {
        pub mod softdevice;
        pub use softdevice::*;
    } else if #[cfg(feature = "trouble")] {
        pub mod trouble;
        pub use trouble::*;
    }
}

#[derive(Debug, From)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub(crate) enum Error {
    HeaplessExtendFromSlice,

    #[cfg(feature = "trouble")]
    #[from]
    #[allow(dead_code)]
    TroubleError(trouble_host::Error),
    #[cfg(feature = "softdevice")]
    #[from]
    SoftdeviceSetValueError(nrf_softdevice::ble::gatt_server::SetValueError),
}
