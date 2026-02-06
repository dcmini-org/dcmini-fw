use derive_more::From;

pub mod ads_stream;
pub mod mic_stream;
// pub use ads_stream::*;

#[cfg(feature = "trouble")]
pub mod trouble;
#[cfg(feature = "trouble")]
pub use trouble::*;

#[derive(Debug, From)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub(crate) enum Error {
    HeaplessExtendFromSlice,

    #[cfg(feature = "trouble")]
    #[from]
    #[allow(dead_code)]
    TroubleError(trouble_host::Error),
}
