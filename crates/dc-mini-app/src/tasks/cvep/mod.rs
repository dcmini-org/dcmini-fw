pub(crate) mod events;
mod model;
mod tasks;

pub use events::*;
use tasks::*;

use crate::prelude::*;
use embassy_sync::pubsub::PubSubChannel;
use embassy_sync::signal::Signal;
use embassy_sync::watch::Watch;
use portable_atomic::AtomicBool;

pub(self) static CVEP_ACTIVE: AtomicBool = AtomicBool::new(false);
pub(self) static CVEP_STOP_SIG: Signal<CriticalSectionRawMutex, ()> =
    Signal::new();

pub const CVEP_CAP: usize = 8;
pub const CVEP_SUBS: usize = 2;
pub type CvepCh<T> =
    PubSubChannel<CriticalSectionRawMutex, T, CVEP_CAP, CVEP_SUBS, 1>;

#[derive(Clone, Copy, Debug, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct CvepDecisionEvent {
    pub ts: u64,
    pub class_index: usize,
    pub raw_score: i64,
    pub normalized_score: f32,
    pub margin: f32,
}

pub static CVEP_WATCH: Watch<CriticalSectionRawMutex, bool, CVEP_SUBS> =
    Watch::new();
pub static CVEP_DECISION_CH: CvepCh<CvepDecisionEvent> = CvepCh::new();

pub(crate) fn current_config() -> icd::CvepConfig {
    model::config()
}
