extern crate alloc;

use crate::prelude::*;
use crate::tasks::mic::adpcm::AdpcmEncoder;
use crate::tasks::mic::{MIC_BUF_SAMPLES, MIC_STREAM_CH, MIC_WATCH};
use embassy_futures::select::{select, Either};
use embassy_time::Instant;
use heapless::Vec;
use prost::Message;

pub(crate) trait MicStreamNotifier {
    async fn notify_mic_data(
        &self,
        data: &Vec<u8, ATT_MTU>,
    ) -> Result<(), super::Error>;
}

pub(crate) async fn mic_stream_notify<T: MicStreamNotifier>(
    notifier: &T,
    _mtu: usize,
) {
    let mut mic_watcher =
        MIC_WATCH.dyn_receiver().expect("Failed to create mic watcher");
    let mut sub = MIC_STREAM_CH
        .dyn_subscriber()
        .expect("Failed to create mic subscriber");

    let mut encoder = AdpcmEncoder::new();
    let mut packet_counter: u64 = 0;
    let mut adpcm_buf = [0u8; MIC_BUF_SAMPLES / 2];
    let mut att_payload: Vec<u8, ATT_MTU> = Vec::new();

    loop {
        match select(sub.next_message_pure(), mic_watcher.changed()).await {
            Either::First(pcm_buf) => {
                let (predictor, step_index) = encoder.decoder_state();
                encoder.encode_block(&pcm_buf, &mut adpcm_buf);

                let frame = icd::mic_proto::MicDataFrame {
                    ts: Instant::now().as_micros(),
                    packet_counter,
                    sample_rate: 16000, // TODO: read from config
                    predictor,
                    step_index,
                    adpcm_data: adpcm_buf.to_vec(),
                };

                let mut out_buffer = alloc::vec::Vec::new();
                frame.encode(&mut out_buffer).unwrap();

                att_payload.clear();
                if att_payload.extend_from_slice(&out_buffer).is_err() {
                    warn!("Mic frame too large for ATT payload");
                    continue;
                }

                if let Err(_) = notifier.notify_mic_data(&att_payload).await {
                    warn!("Failed to notify mic data");
                }

                packet_counter = packet_counter.wrapping_add(1);
            }
            Either::Second(streaming) => {
                if !streaming {
                    // Streaming stopped — wait for restart
                    loop {
                        if mic_watcher.changed().await {
                            encoder.reset();
                            packet_counter = 0;
                            break;
                        }
                    }
                }
            }
        }
    }
}
