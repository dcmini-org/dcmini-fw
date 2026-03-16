use super::*;
use crate::prelude::*;
use crate::tasks::ads::{ADS_MEAS_CH, ADS_WATCH};
use cvep_decoder::CvepDecoder;
use embassy_futures::select::{select3, Either3};
use embassy_time::Instant;
use portable_atomic::Ordering;

#[embassy_executor::task]
pub async fn cvep_decode_task() {
    use model::{
        bank, preprocessor, to_decision, ADC_SCALE, CHANNELS,
        INFERENCE_STRIDE, WINDOW,
    };

    CVEP_ACTIVE.store(true, Ordering::SeqCst);
    CVEP_WATCH.sender().send(true);

    let Some(bank) = bank() else {
        warn!(
            "CVEP feature enabled without a configured bank artifact; decoder task exiting."
        );
        CVEP_WATCH.sender().send(false);
        CVEP_ACTIVE.store(false, Ordering::SeqCst);
        return;
    };

    let mut ads_subscriber = ADS_MEAS_CH
        .subscriber()
        .expect("Failed to create CVEP ADS subscriber.");
    let mut ads_watcher =
        ADS_WATCH.receiver().expect("Failed to create CVEP ADS watcher.");
    let publisher = CVEP_DECISION_CH
        .publisher()
        .expect("This is the only expected publisher of CVEP decisions.");

    let mut decoder = CvepDecoder::<CHANNELS, WINDOW>::new();
    let mut pre = preprocessor();
    let mut stride_count = 0usize;
    let mut warned_short_frame = false;
    let mut warned_extra_channels = false;

    loop {
        match select3(
            ads_subscriber.next_message_pure(),
            ads_watcher.changed(),
            CVEP_STOP_SIG.wait(),
        )
        .await
        {
            Either3::First(data) => {
                let Some(frame) = extract_frame::<CHANNELS>(
                    &data,
                    &mut warned_short_frame,
                    &mut warned_extra_channels,
                ) else {
                    continue;
                };

                let filtered = pre.process_frame_i32(frame, ADC_SCALE);
                let mut quantized = [0i32; CHANNELS];
                let mut idx = 0;
                while idx < CHANNELS {
                    quantized[idx] = filtered[idx] as i32;
                    idx += 1;
                }

                decoder.push(quantized);
                if !decoder.is_ready() {
                    continue;
                }

                stride_count += 1;
                if stride_count < INFERENCE_STRIDE {
                    continue;
                }
                stride_count = 0;

                match decoder.predict_etrca(bank) {
                    Ok(decision) => {
                        let decision =
                            to_decision(decision, Instant::now().as_micros());
                        if publisher.try_publish(decision).is_err() {
                            warn!("Failed to publish CVEP decision.");
                        }
                        info!("CVEP decision: {:?}", decision);
                    }
                    Err(_) => {
                        warn!("CVEP prediction failed.");
                    }
                }
            }
            Either3::Second(streaming) => {
                if !streaming {
                    decoder.clear();
                    pre.reset();
                    stride_count = 0;
                    info!("ADS stream stopped; cleared CVEP decoder state.");
                }
            }
            Either3::Third(_) => {
                break;
            }
        }
    }

    CVEP_STOP_SIG.reset();
    CVEP_WATCH.sender().send(false);
    CVEP_ACTIVE.store(false, Ordering::SeqCst);
}

fn extract_frame<const CHANNELS: usize>(
    data: &alloc::sync::Arc<heapless::Vec<ads1299::AdsData, 2>>,
    warned_short_frame: &mut bool,
    warned_extra_channels: &mut bool,
) -> Option<[i32; CHANNELS]> {
    let mut frame = [0i32; CHANNELS];
    let mut copied = 0usize;
    let mut total = 0usize;

    for device in data.iter() {
        total += device.data.len();
        for sample in device.data.iter() {
            if copied < CHANNELS {
                frame[copied] = *sample;
                copied += 1;
            }
        }
    }

    if copied < CHANNELS {
        if !*warned_short_frame {
            warn!(
                "CVEP decoder expected {} channels but ADS stream only provided {}.",
                CHANNELS,
                copied
            );
            *warned_short_frame = true;
        }
        return None;
    }

    if total > CHANNELS && !*warned_extra_channels {
        warn!(
            "CVEP decoder configured for {} channels; ignoring {} extra ADS channels.",
            CHANNELS,
            total - CHANNELS
        );
        *warned_extra_channels = true;
    }

    Some(frame)
}
