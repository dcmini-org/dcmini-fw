use super::{
    gatt::{ServerWithDfu, ServerWithoutDfu},
    ATT_MTU,
};
use crate::prelude::*;
use heapless::Vec;
use trouble_host::prelude::*;

#[gatt_service(uuid = "32210000-af46-43af-a0ba-4dbeb457f51c")]
pub struct StatusService {
    #[characteristic(
        uuid = "32210001-af46-43af-a0ba-4dbeb457f51c",
        read
    )]
    pub snapshot: Vec<u8, ATT_MTU>,

    #[characteristic(
        uuid = "32210002-af46-43af-a0ba-4dbeb457f51c",
        notify
    )]
    pub event: Vec<u8, ATT_MTU>,
}

fn encode_status<T: serde::Serialize>(value: &T) -> Option<Vec<u8, ATT_MTU>> {
    let mut buf = [0u8; ATT_MTU];
    let encoded = postcard::to_slice(value, &mut buf).ok()?;
    Vec::from_slice(encoded).ok()
}

macro_rules! impl_status_support {
    ($server_ty:ident, $notify_fn:ident) => {
        impl<'d> $server_ty<'d> {
            pub async fn handle_status_read_event(
                &self,
                handle: u16,
                _app_context: &'static Mutex<
                    CriticalSectionRawMutex,
                    AppContext,
                >,
            ) {
                if handle != self.status.snapshot.handle {
                    return;
                }

                let snapshot = status_snapshot().await;
                let Some(payload) = encode_status(&snapshot) else {
                    report_status(
                        icd::SubsystemId::BleStream,
                        icd::SubsystemState::Degraded,
                        icd::FaultCode::EncodingOverflow,
                    )
                    .await;
                    return;
                };

                if let Err(_e) = self.set(&self.status.snapshot, &payload) {
                    report_status(
                        icd::SubsystemId::BleStream,
                        icd::SubsystemState::Degraded,
                        icd::FaultCode::EncodingOverflow,
                    )
                    .await;
                }
            }
        }

        pub async fn $notify_fn<P: PacketPool>(
            server: &$server_ty<'_>,
            conn: &GattConnection<'_, '_, P>,
        ) {
            loop {
                let mut receiver = match STATUS_WATCH.dyn_receiver() {
                    Some(receiver) => receiver,
                    None => {
                        warn!("Unable to subscribe to system status watch for BLE");
                        report_status(
                            icd::SubsystemId::BleStream,
                            icd::SubsystemState::Degraded,
                            icd::FaultCode::Busy,
                        )
                        .await;
                        Timer::after_millis(250).await;
                        continue;
                    }
                };

                loop {
                    let status = receiver.changed().await;
                    let Some(payload) = encode_status(&status) else {
                        report_status(
                            icd::SubsystemId::BleStream,
                            icd::SubsystemState::Degraded,
                            icd::FaultCode::EncodingOverflow,
                        )
                        .await;
                        continue;
                    };

                    if let Err(_e) = server.set(&server.status.event, &payload) {
                        report_status(
                            icd::SubsystemId::BleStream,
                            icd::SubsystemState::Degraded,
                            icd::FaultCode::EncodingOverflow,
                        )
                        .await;
                        continue;
                    }

                    if let Err(e) = server.status.event.notify(conn, &payload).await
                    {
                        warn!("Error notifying status update: {:?}", e);
                        report_status(
                            icd::SubsystemId::BleStream,
                            icd::SubsystemState::Degraded,
                            icd::FaultCode::Busy,
                        )
                        .await;
                        Timer::after_millis(100).await;
                    }
                }
            }
        }
    };
}

impl_status_support!(ServerWithDfu, status_notify_with_dfu);
impl_status_support!(ServerWithoutDfu, status_notify_without_dfu);
