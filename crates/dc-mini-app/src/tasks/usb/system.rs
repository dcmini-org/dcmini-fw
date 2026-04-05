use crate::prelude::*;
use postcard_rpc::header::VarHeader;
use postcard_rpc::server::Sender;

pub async fn system_status_get(
    _context: &mut super::Context,
    _header: VarHeader,
    _rqst: (),
) -> icd::SystemStatusSnapshot {
    status_snapshot().await
}

pub async fn system_status_topic_task(sender: Sender<super::AppTx>) {
    let mut receiver = match STATUS_WATCH.dyn_receiver() {
        Some(receiver) => receiver,
        None => {
            warn!("Unable to subscribe to system status watch for USB");
            return;
        }
    };

    let mut seq_no: u8 = 0;
    loop {
        let status = receiver.changed().await;
        if let Err(_e) = sender
            .publish::<dc_mini_icd::SystemStatusTopic>(seq_no.into(), &status)
            .await
        {
            #[cfg(feature = "defmt")]
            warn!(
                "Failed to publish system status: {:?}",
                defmt::Debug2Format(&_e)
            );
        }
        seq_no = seq_no.wrapping_add(1);
    }
}
