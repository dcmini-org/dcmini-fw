use crate::prelude::*;
use crate::tasks::ads::ADS_MEAS_CH;
use crate::tasks::ads::ADS_WATCH;
use ads1299::AdsData;
use dc_mini_icd::AdsConfig;
use dc_mini_icd::{AdsDataFrame, AdsSample};
use embassy_futures::select::{select, Either};
use embassy_sync::pubsub::DynSubscriber;
use embassy_sync::signal::Signal;
use embassy_sync::watch::DynReceiver;
use embassy_time::{Duration, Instant};
use heapless::Vec;
use postcard_rpc::{header::VarHeader, server::Sender};

const BATCH_INTERVAL: Duration = Duration::from_millis(33); // ~30Hz

static USB_STREAM: Signal<CriticalSectionRawMutex, ()> = Signal::new();

#[embassy_executor::task]
pub async fn ads_start_handler(
    context: SpawnCtx,
    header: VarHeader,
    _rqst: (),
    sender: Sender<super::AppTx>,
) {
    let config = {
        let mut ctx = context.app.lock().await;
        ctx.event_sender.send(AdsEvent::StartStream.into()).await;
        ctx.profile_manager
            .get_ads_config()
            .await
            .expect("Unable to get ADS config.")
            .clone()
    };

    if sender.reply::<AdsStartEndpoint>(header.seq_no, &config).await.is_err()
    {
        error!("Failed to reply, stopping ads");
        return;
    }

    select(ads_stream_usb(sender), USB_STREAM.wait()).await;
    USB_STREAM.reset();
}

pub async fn ads_stop_handler(
    context: &mut Context,
    _header: VarHeader,
    _rqst: (),
) -> () {
    let ctx = context.app.lock().await;
    let _res = ctx.event_sender.send(AdsEvent::StopStream.into()).await;
    USB_STREAM.signal(());
}

pub async fn ads_get_config(
    context: &mut Context,
    _header: VarHeader,
    _rqst: (),
) -> AdsConfig {
    let mut ctx = context.app.lock().await;
    ctx.profile_manager
        .get_ads_config()
        .await
        .expect("Unable to get ADS config.")
        .clone()
}

pub async fn ads_set_config(
    context: &mut Context,
    _header: VarHeader,
    rqst: AdsConfig,
) -> bool {
    let mut ctx = context.app.lock().await;
    ctx.save_ads_config(rqst).await;
    true
}

pub async fn ads_reset_config(
    context: &mut Context,
    _header: VarHeader,
    _rqst: (),
) -> bool {
    let ctx = context.app.lock().await;
    ctx.event_sender.send(AdsEvent::ResetConfig.into()).await;
    true
}

fn convert_sample(samples: alloc::sync::Arc<Vec<AdsData, 2>>) -> AdsSample {
    // Calculate the total number of channels across all ADS devices
    let total_channels: usize =
        samples.iter().map(|sample| sample.data.len()).sum();

    let mut data = alloc::vec::Vec::with_capacity(total_channels);
    let mut lead_off_positive: u32 = 0;
    let mut lead_off_negative: u32 = 0;
    let mut gpio: u32 = 0;

    let mut bit_shift = 0; // Tracks where to place the next lead-off bits
    let mut gpio_shift = 0; // Tracks where to place the next GPIO bits

    for sample in samples.iter() {
        let ch = sample.data.len(); // Number of channels in this ADS device

        // Append channel data
        data.extend(sample.data.iter());

        // Create a bitmask for the number of channels
        let mask = (1 << ch) - 1;

        // Encode lead-off status for positive and negative signals
        lead_off_positive |=
            (sample.lead_off_status_pos.bits() as u32 & mask) << bit_shift;
        lead_off_negative |=
            (sample.lead_off_status_neg.bits() as u32 & mask) << bit_shift;

        // Encode GPIO data (4 bits per ADS device)
        gpio |= (sample.gpio.bits() as u32 & 0x0F) << gpio_shift;

        // Increment shifts for the next ADS device
        bit_shift += ch; // Shift by the number of channels
        gpio_shift += 4; // Shift by 4 bits (1 nibble per GPIO)
    }

    // Return the constructed AdsSample
    AdsSample { lead_off_positive, lead_off_negative, gpio, data }
}

/// Collects samples until the batch interval is reached or streaming is stopped
async fn collect_batch(
    sub: &mut DynSubscriber<'_, alloc::sync::Arc<Vec<AdsData, 2>>>,
    ads_watcher: &mut DynReceiver<'_, bool>,
    next_batch_time: Instant,
) -> (alloc::vec::Vec<AdsSample>, bool) {
    let mut samples = alloc::vec::Vec::new();

    while Instant::now() < next_batch_time {
        match select(sub.next_message_pure(), ads_watcher.changed()).await {
            Either::First(data) => {
                samples.push(convert_sample(data));
            }
            Either::Second(streaming) => {
                if !streaming {
                    return (samples, true);
                }
            }
        }
    }

    (samples, false)
}

async fn ads_stream_usb(sender: Sender<super::AppTx>) {
    let mut sub =
        ADS_MEAS_CH.dyn_subscriber().expect("Failed to create subscriber");
    let mut ads_watcher =
        ADS_WATCH.dyn_receiver().expect("Failed to create watcher");

    let mut packet_counter = 0u8;
    let mut next_batch_time = Instant::now() + BATCH_INTERVAL;
    let mut needs_recalc = false;

    loop {
        // Wait for streaming to start if needed
        if needs_recalc {
            match ads_watcher.changed().await {
                true => {
                    next_batch_time = Instant::now() + BATCH_INTERVAL;
                }
                false => continue,
            }
        }

        // Collect samples until batch interval or streaming stops
        let (samples, should_recalc) =
            collect_batch(&mut sub, &mut ads_watcher, next_batch_time).await;
        needs_recalc = should_recalc;

        // Send collected samples if any
        if !samples.is_empty() {
            let frame =
                AdsDataFrame { ts: Instant::now().as_micros(), samples };

            if let Err(_e) = sender
                .publish::<dc_mini_icd::AdsTopic>(
                    packet_counter.into(),
                    &frame,
                )
                .await
            {
                #[cfg(feature = "defmt")]
                warn!(
                    "Failed to publish ADS data: {:?}",
                    defmt::Debug2Format(&_e)
                );
            }

            packet_counter = packet_counter.wrapping_add(1);
        }

        // Update next batch time if still streaming
        if !needs_recalc {
            next_batch_time = Instant::now() + BATCH_INTERVAL;
        }
    }
}
