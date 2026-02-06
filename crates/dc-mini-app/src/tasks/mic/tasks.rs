use super::*;
use crate::prelude::*;
use dc_mini_icd::MicConfig;
use embassy_futures::select::{select, Either};
use embassy_sync::mutex::Mutex;
use fixed::types::I7F1;
use portable_atomic::Ordering;

#[embassy_executor::task]
pub async fn mic_stream_task(
    mic: &'static Mutex<CriticalSectionRawMutex, MicResources>,
    config: MicConfig,
) {
    MIC_STREAMING.store(true, Ordering::SeqCst);

    let mut mic_resources = mic.lock().await;
    let driver_config = to_driver_config(&config);
    let mut spk = mic_resources.configure(driver_config);

    // Start PDM clock and allow microphone startup time
    spk.start().await;
    Timer::after_millis(10).await;

    let publisher = MIC_STREAM_CH
        .publisher()
        .expect("This is the only expected publisher of MIC data.");

    let mut buf = [0i16; MIC_BUF_SAMPLES];

    loop {
        match select(MIC_STREAM_SIG.wait(), spk.sample(&mut buf)).await {
            Either::First(sig) => {
                if let Some(new_config) = sig {
                    // Reconfigure gain at runtime
                    let gain = I7F1::from_num(new_config.gain_db);
                    spk.set_gain(gain);
                    MIC_STREAM_SIG.reset();
                } else {
                    // None means stop
                    break;
                }
            }
            Either::Second(Ok(())) => {
                if let Err(_) = publisher.try_publish(buf) {
                    warn!("Failed to publish mic data! Subscriber back pressure!");
                }
            }
            Either::Second(Err(e)) => {
                error!("Error sampling microphone: {:?}", e);
                break;
            }
        }
    }

    spk.stop().await;
    MIC_STREAM_SIG.reset();
    MIC_STREAMING.store(false, Ordering::SeqCst);
}

#[embassy_executor::task]
pub async fn mic_single_sample_task(
    mic: &'static Mutex<CriticalSectionRawMutex, MicResources>,
    config: MicConfig,
) {
    let mut mic_resources = mic.lock().await;
    let driver_config = to_driver_config(&config);
    let mut spk = mic_resources.configure(driver_config);

    // Start PDM clock and allow microphone startup time
    spk.start().await;
    Timer::after_millis(10).await;

    let mut buf = [0i16; MIC_BUF_SAMPLES];
    match spk.sample(&mut buf).await {
        Ok(()) => {
            let publisher = MIC_STREAM_CH
                .publisher()
                .expect("This is the only expected publisher of MIC data.");
            if let Err(_) = publisher.try_publish(buf) {
                warn!("Failed to publish single mic sample!");
            }
        }
        Err(e) => {
            error!("Error during single mic sample: {:?}", e);
        }
    }

    spk.stop().await;
    // Spk0838 drops, PDM disabled
}
