use crate::prelude::*;
use dc_mini_icd::{
    FaultCode, SubsystemId, SubsystemState, SubsystemStatus,
    SystemStatusSnapshot, SUBSYSTEM_STATUS_COUNT,
};
use embassy_sync::mutex::Mutex;
use embassy_sync::watch::Watch;

const STATUS_SUBSCRIBERS: usize = 4;

const ALL_SUBSYSTEMS: [SubsystemId; SUBSYSTEM_STATUS_COUNT] = [
    SubsystemId::Power,
    SubsystemId::ExternalFlash,
    SubsystemId::Dfu,
    SubsystemId::Storage,
    SubsystemId::Ads,
    SubsystemId::Imu,
    SubsystemId::Apds,
    SubsystemId::Haptic,
    SubsystemId::Mic,
    SubsystemId::UsbStream,
    SubsystemId::BleStream,
];

const fn default_status(subsystem: SubsystemId) -> SubsystemStatus {
    SubsystemStatus {
        subsystem,
        state: SubsystemState::Unavailable,
        fault: FaultCode::None,
    }
}

pub struct SystemStatusTable {
    statuses: [SubsystemStatus; SUBSYSTEM_STATUS_COUNT],
}

impl SystemStatusTable {
    pub const fn new() -> Self {
        Self {
            statuses: [
                default_status(SubsystemId::Power),
                default_status(SubsystemId::ExternalFlash),
                default_status(SubsystemId::Dfu),
                default_status(SubsystemId::Storage),
                default_status(SubsystemId::Ads),
                default_status(SubsystemId::Imu),
                default_status(SubsystemId::Apds),
                default_status(SubsystemId::Haptic),
                default_status(SubsystemId::Mic),
                default_status(SubsystemId::UsbStream),
                default_status(SubsystemId::BleStream),
            ],
        }
    }

    fn index(subsystem: SubsystemId) -> usize {
        match subsystem {
            SubsystemId::Power => 0,
            SubsystemId::ExternalFlash => 1,
            SubsystemId::Dfu => 2,
            SubsystemId::Storage => 3,
            SubsystemId::Ads => 4,
            SubsystemId::Imu => 5,
            SubsystemId::Apds => 6,
            SubsystemId::Haptic => 7,
            SubsystemId::Mic => 8,
            SubsystemId::UsbStream => 9,
            SubsystemId::BleStream => 10,
        }
    }

    fn update(
        &mut self,
        subsystem: SubsystemId,
        state: SubsystemState,
        fault: FaultCode,
    ) -> Option<SubsystemStatus> {
        let idx = Self::index(subsystem);
        let next = SubsystemStatus { subsystem, state, fault };
        if self.statuses[idx] == next {
            None
        } else {
            self.statuses[idx] = next;
            Some(next)
        }
    }

    fn snapshot(&self) -> SystemStatusSnapshot {
        let mut statuses = heapless::Vec::new();
        for subsystem in ALL_SUBSYSTEMS {
            let _ = statuses.push(self.statuses[Self::index(subsystem)]);
        }
        SystemStatusSnapshot { statuses }
    }
}

pub static SYSTEM_STATUS: Mutex<CriticalSectionRawMutex, SystemStatusTable> =
    Mutex::new(SystemStatusTable::new());

pub static STATUS_WATCH: Watch<
    CriticalSectionRawMutex,
    SubsystemStatus,
    STATUS_SUBSCRIBERS,
> = Watch::new_with(default_status(SubsystemId::Power));

pub async fn report_status(
    subsystem: SubsystemId,
    state: SubsystemState,
    fault: FaultCode,
) {
    let changed = {
        let mut table = SYSTEM_STATUS.lock().await;
        table.update(subsystem, state, fault)
    };

    if let Some(status) = changed {
        STATUS_WATCH.sender().send(status);
    }
}

pub async fn snapshot() -> SystemStatusSnapshot {
    let table = SYSTEM_STATUS.lock().await;
    table.snapshot()
}
