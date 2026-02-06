pub(crate) mod adpcm;
pub(crate) mod config;
pub(crate) mod events;

mod tasks; // Tasks module is private

pub use config::*;
pub use events::*;
use tasks::*;

use crate::prelude::*;
use embassy_sync::pubsub::PubSubChannel;
use embassy_sync::signal::Signal;
use embassy_sync::watch::Watch;
use portable_atomic::AtomicBool;

pub(self) static MIC_STREAMING: AtomicBool = AtomicBool::new(false);

pub(self) static MIC_STREAM_SIG: Signal<
    CriticalSectionRawMutex,
    Option<MicConfig>,
> = Signal::new();

pub const MIC_CAP: usize = 10;
pub const MIC_SUBS: usize = 3;
pub const MIC_BUF_SAMPLES: usize = 256;

pub type MicCh<T> =
    PubSubChannel<CriticalSectionRawMutex, T, MIC_CAP, MIC_SUBS, 1>;
pub static MIC_STREAM_CH: MicCh<[i16; MIC_BUF_SAMPLES]> = MicCh::new();
pub static MIC_WATCH: Watch<CriticalSectionRawMutex, bool, MIC_SUBS> =
    Watch::new();
