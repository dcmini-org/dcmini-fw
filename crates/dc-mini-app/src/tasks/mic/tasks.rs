use super::*;
use crate::prelude::*;
use dc_mini_icd::MicConfig;
use embassy_nrf::pdm::SamplerState;
use embassy_sync::mutex::Mutex;
use portable_atomic::Ordering;

const MIC_STARTUP_SETTLE_MS: u64 = 10;

#[embassy_executor::task]
pub async fn mic_stream_task(
    mic: &'static Mutex<CriticalSectionRawMutex, MicResources>,
    config: MicConfig,
) {
    MIC_STREAMING.store(true, Ordering::SeqCst);

    let mut mic_resources = mic.lock().await;
    let publisher = MIC_STREAM_CH
        .publisher()
        .expect("This is the only expected publisher of MIC data.");

    let mut active_config = config;

    'stream: loop {
        let mut spk = mic_resources.configure(to_driver_config_with_channel(
            &active_config,
            DEFAULT_MIC_CHANNEL,
        ));
        let mut stop_requested = false;
        let mut next_config: Option<MicConfig> = None;
        let mut bufs = [[0i16; MIC_BUF_SAMPLES]; 2];

        info!("Mic streaming using {:?} edge", DEFAULT_MIC_CHANNEL);

        let run_result = spk
            .run_sampler(&mut bufs, |buf| {
                if publisher.try_publish(*buf).is_err() {
                    warn!("Failed to publish mic data! Subscriber back pressure!");
                }

                if let Some(sig) = MIC_STREAM_SIG.try_take() {
                    if let Some(new_config) = sig {
                        next_config = Some(new_config);
                    } else {
                        stop_requested = true;
                    }
                    return SamplerState::Stopped;
                }

                SamplerState::Sampled
            })
            .await;

        if let Err(e) = run_result {
            error!("Error sampling microphone: {:?}", e);
            break;
        }

        if let Some(new_config) = next_config {
            // Gain and sample-rate updates are applied by restarting the
            // continuous sampler with the updated configuration.
            active_config = new_config;
            continue 'stream;
        }

        if stop_requested {
            break;
        }

        // Should not happen in normal operation; avoid spinning forever.
        break;
    }

    info!("Mic stream stopped");

    // Drop any stale stop/reconfigure request when the stream exits.
    MIC_STREAM_SIG.reset();
    MIC_STREAMING.store(false, Ordering::SeqCst);
}

#[embassy_executor::task]
pub async fn mic_single_sample_task(
    mic: &'static Mutex<CriticalSectionRawMutex, MicResources>,
    config: MicConfig,
) {
    let mut mic_resources = mic.lock().await;
    let mut spk = mic_resources.configure(to_driver_config_with_channel(
        &config,
        DEFAULT_MIC_CHANNEL,
    ));

    // Start PDM clock and allow microphone startup time
    spk.start().await;
    Timer::after_millis(MIC_STARTUP_SETTLE_MS).await;

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
