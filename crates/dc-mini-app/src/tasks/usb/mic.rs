use crate::prelude::*;
use crate::tasks::mic::adpcm::AdpcmEncoder;
use crate::tasks::mic::{MIC_BUF_SAMPLES, MIC_STREAM_CH, MIC_WATCH};
use dc_mini_icd::MicConfig;
use embassy_futures::select::{select, Either};
use embassy_sync::signal::Signal;
use embassy_time::Instant;
use postcard_rpc::{header::VarHeader, server::Sender};

static MIC_USB_STREAM: Signal<CriticalSectionRawMutex, ()> = Signal::new();

#[embassy_executor::task]
pub async fn mic_start_handler(
    context: SpawnCtx,
    header: VarHeader,
    _rqst: (),
    sender: Sender<super::AppTx>,
) {
    let config = {
        let mut ctx = context.app.lock().await;
        ctx.event_sender.send(MicEvent::StartStream.into()).await;
        ctx.profile_manager
            .get_mic_config()
            .await
            .cloned()
            .unwrap_or_default()
    };

    if sender
        .reply::<MicStartEndpoint>(header.seq_no, &config)
        .await
        .is_err()
    {
        error!("Failed to reply, stopping mic");
        return;
    }

    select(mic_stream_usb(sender, &config), MIC_USB_STREAM.wait()).await;
    MIC_USB_STREAM.reset();
}

pub async fn mic_stop_handler(
    context: &mut super::Context,
    _header: VarHeader,
    _rqst: (),
) -> () {
    let ctx = context.app.lock().await;
    let _res = ctx.event_sender.send(MicEvent::StopStream.into()).await;
    MIC_USB_STREAM.signal(());
}

pub async fn mic_get_config(
    context: &mut super::Context,
    _header: VarHeader,
    _rqst: (),
) -> MicConfig {
    let mut ctx = context.app.lock().await;
    ctx.profile_manager
        .get_mic_config()
        .await
        .cloned()
        .unwrap_or_default()
}

pub async fn mic_set_config(
    context: &mut super::Context,
    _header: VarHeader,
    rqst: MicConfig,
) -> bool {
    let mut ctx = context.app.lock().await;
    ctx.save_mic_config(rqst).await;
    true
}

async fn mic_stream_usb(sender: Sender<super::AppTx>, config: &MicConfig) {
    let mut sub =
        MIC_STREAM_CH.dyn_subscriber().expect("Failed to create mic subscriber");
    let mut mic_watcher =
        MIC_WATCH.dyn_receiver().expect("Failed to create mic watcher");

    let sample_rate = config.sample_rate.as_hz();
    let mut encoder = AdpcmEncoder::new();
    let mut packet_counter: u64 = 0;
    let mut adpcm_buf = [0u8; MIC_BUF_SAMPLES / 2];

    loop {
        match select(sub.next_message_pure(), mic_watcher.changed()).await {
            Either::First(pcm_buf) => {
                let (predictor, step_index) = encoder.decoder_state();
                encoder.encode_block(&pcm_buf, &mut adpcm_buf);

                let frame = dc_mini_icd::MicDataFrame {
                    ts: Instant::now().as_micros(),
                    packet_counter,
                    sample_rate,
                    predictor,
                    step_index,
                    adpcm_data: adpcm_buf.to_vec(),
                };

                let seq: u8 = (packet_counter & 0xFF) as u8;
                if let Err(_e) = sender
                    .publish::<dc_mini_icd::MicTopic>(
                        seq.into(),
                        &frame,
                    )
                    .await
                {
                    #[cfg(feature = "defmt")]
                    warn!(
                        "Failed to publish mic data: {:?}",
                        defmt::Debug2Format(&_e)
                    );
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
