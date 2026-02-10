use super::*;
use crate::prelude::*;
use derive_more::From;
use drv260x::{Effect, WaveformEntry};
use embassy_sync::mutex::Mutex;
use portable_atomic::Ordering;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum HapticCommand {
    PlayEffect(Effect),
    PlaySequence(heapless::Vec<WaveformEntry, 8>),
}

#[derive(Debug, From)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum HapticEvent {
    Play(HapticCommand),
    Stop,
    Init,
}

#[derive(Clone)]
pub struct HapticManager {
    bus_manager: &'static I2cBusManager,
    app: &'static Mutex<CriticalSectionRawMutex, AppContext>,
}

impl HapticManager {
    pub fn new(
        bus_manager: &'static I2cBusManager,
        app: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    ) -> Self {
        Self { bus_manager, app }
    }

    pub async fn handle_event(&self, event: HapticEvent) {
        info!("Received event {:?}", event);
        match event {
            HapticEvent::Init => {
                if HAPTIC_ACTIVE.load(Ordering::SeqCst) {
                    info!("Haptic task already running.");
                } else {
                    let app_ctx = self.app.lock().await;
                    app_ctx
                        .low_prio_spawner
                        .must_spawn(haptic_task(self.bus_manager));
                }
            }
            HapticEvent::Play(cmd) => {
                if !HAPTIC_ACTIVE.load(Ordering::SeqCst) {
                    // Auto-init: spawn the task first, then send command
                    let app_ctx = self.app.lock().await;
                    app_ctx
                        .low_prio_spawner
                        .must_spawn(haptic_task(self.bus_manager));
                }
                HAPTIC_CMD_SIG.signal(Some(cmd));
            }
            HapticEvent::Stop => {
                if !HAPTIC_ACTIVE.load(Ordering::SeqCst) {
                    info!("Haptic task not running, nothing to stop.");
                } else {
                    HAPTIC_CMD_SIG.signal(None);
                }
            }
        }
    }
}
