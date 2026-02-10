use super::{gatt::Server, ATT_MTU};
use crate::prelude::info;
use crate::tasks::ble::mic_stream::{self, MicStreamNotifier};
use heapless::Vec;
use trouble_host::prelude::*;

#[gatt_service(uuid = "33100000-af46-43af-a0ba-4dbeb457f51c")]
pub struct MicService {
    #[characteristic(
        uuid = "33000200-af46-43af-a0ba-4dbeb457f51c",
        read,
        notify
    )]
    pub data_stream: Vec<u8, ATT_MTU>,
    #[characteristic(
        uuid = "33000000-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub gain_db: i8,
    #[characteristic(
        uuid = "33000001-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub sample_rate: u8,
    #[characteristic(uuid = "33000300-af46-43af-a0ba-4dbeb457f51c", write)]
    pub command: u8,
}

struct TroubleNotifier<'a, 'b, 'c, P: PacketPool> {
    handle: Characteristic<Vec<u8, ATT_MTU>>,
    conn: &'a GattConnection<'b, 'c, P>,
}

impl<P: PacketPool> MicStreamNotifier for TroubleNotifier<'_, '_, '_, P> {
    async fn notify_mic_data(
        &self,
        data: &Vec<u8, ATT_MTU>,
    ) -> Result<(), super::Error> {
        self.handle.notify(self.conn, data).await?;
        Ok(())
    }
}

pub async fn mic_stream_notify<P: PacketPool>(
    server: &Server<'_>,
    conn: &GattConnection<'_, '_, P>,
) {
    let notifier =
        TroubleNotifier { handle: server.mic.data_stream.clone(), conn };

    // Wait for ATT MTU exchange to complete before querying the negotiated value.
    embassy_time::Timer::after_secs(1).await;

    let mtu = conn.raw().att_mtu();
    info!("Mic ATT mtu = {:?}", mtu);

    mic_stream::mic_stream_notify(&notifier, mtu as usize).await
}
