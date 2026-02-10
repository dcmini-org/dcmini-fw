extern crate alloc;

use crate::prelude::*;
use crate::tasks::ads::ADS_MEAS_CH;
use ads1299::AdsData;
use embassy_futures::select::{select, Either};
use embassy_sync::pubsub::DynSubscriber;
use embassy_sync::watch::DynReceiver;
use embassy_time::Instant;
use heapless::Vec;
use prost::Message;

/// Find the initial maximum number of samples that can fit in the agreed upon mtu.
pub(crate) async fn find_initial_max_samples(
    att_mtu: usize,
    sub: &mut DynSubscriber<'_, alloc::sync::Arc<Vec<AdsData, 2>>>,
) -> (usize, alloc::vec::Vec<u8>, Option<alloc::vec::Vec<icd::proto::AdsSample>>)
{
    let mut max_samples = 0;
    let mut out_buffer = alloc::vec::Vec::new();

    let mut message = icd::proto::AdsDataFrame {
        packet_counter: 0,
        ts: Instant::now().as_micros(),
        samples: alloc::vec::Vec::with_capacity(16),
    };

    loop {
        out_buffer.clear();

        let data = sub.next_message_pure().await;
        let ads_sample = convert_to_proto(data);

        message.samples.push(ads_sample);
        max_samples += 1;

        message.encode(&mut out_buffer).unwrap();

        // Check if the encoded frame fits within att_mtu
        if out_buffer.len() > att_mtu {
            if max_samples <= 1 {
                // Special case where we should send anyway. This means our MTU is probably
                // ~23bytes.
                return (max_samples, out_buffer, None);
            }
            out_buffer.clear();
            let carry_over_samples = if let Some(carry) = message.samples.pop()
            {
                Some(alloc::vec![carry])
            } else {
                None
            };
            message.encode(&mut out_buffer).unwrap();
            return (max_samples - 1, out_buffer, carry_over_samples);
        }
    }
}

/// Trait defining the interface for ADS stream notification
pub(crate) trait AdsStreamNotifier {
    async fn notify_data_stream(
        &self,
        data: &Vec<u8, ATT_MTU>,
    ) -> Result<(), super::Error>;
}

/// Encodes and sends a message frame
async fn encode_and_send<T: AdsStreamNotifier>(
    message: icd::proto::AdsDataFrame,
    att_payload: &mut Vec<u8, ATT_MTU>,
    notifier: &T,
) -> Result<(), super::Error> {
    let mut out_buffer = alloc::vec::Vec::new();
    message.encode(&mut out_buffer).unwrap();
    att_payload
        .extend_from_slice(&out_buffer)
        .map_err(|_| super::Error::HeaplessExtendFromSlice)?;
    notifier.notify_data_stream(att_payload).await.map_err(|err| err.into())
}

/// Collects samples up to max_samples, handling watcher interruptions
async fn collect_samples(
    sub: &mut DynSubscriber<'_, alloc::sync::Arc<Vec<AdsData, 2>>>,
    ads_watcher: &mut DynReceiver<'_, bool>,
    max_samples: usize,
    carry_over_samples: Option<alloc::vec::Vec<icd::proto::AdsSample>>,
) -> (alloc::vec::Vec<icd::proto::AdsSample>, bool) {
    let mut samples = alloc::vec::Vec::with_capacity(max_samples.max(1));

    // Add carry-over samples first
    if let Some(mut carry_samples) = carry_over_samples {
        samples.append(&mut carry_samples);
    }

    while samples.len() < max_samples.max(1) {
        match select(sub.next_message_pure(), ads_watcher.changed()).await {
            Either::First(data) => {
                samples.push(convert_to_proto(data));
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

/// Ensures the message fits within MTU size, adjusting max_samples if needed
fn ensure_mtu_fit(
    message: &mut icd::proto::AdsDataFrame,
    mtu: usize,
    max_samples: usize,
) -> (usize, Option<alloc::vec::Vec<icd::proto::AdsSample>>) {
    let mut out_buffer = alloc::vec::Vec::new();
    let mut current_max_samples = max_samples;
    let mut carry_over_samples = alloc::vec::Vec::new();

    message.encode(&mut out_buffer).unwrap();

    while out_buffer.len() > mtu {
        out_buffer.clear();
        current_max_samples = current_max_samples.saturating_sub(1);
        carry_over_samples.push(message.samples.pop().unwrap());
        message.encode(&mut out_buffer).unwrap();
        warn!("Reduced max_samples to {}", current_max_samples);
    }

    let carry_over = (!carry_over_samples.is_empty()).then(|| {
        carry_over_samples.reverse();
        carry_over_samples
    });

    (current_max_samples, carry_over)
}

/// Generic implementation of ADS stream notification
pub(crate) async fn ads_stream_notify<T: AdsStreamNotifier>(
    notifier: &T,
    mtu: usize,
) {
    let mut ads_watcher =
        ADS_WATCH.dyn_receiver().expect("fixme: better error message.");
    let mut sub =
        ADS_MEAS_CH.dyn_subscriber().expect("Failed to create subscriber.");

    let mut packet_counter = 0;
    let mut max_samples = 0;
    let mut needs_recalc = true;
    let mut carry_over_samples = None;
    let mut att_payload: heapless::Vec<u8, ATT_MTU> = heapless::Vec::new();

    loop {
        // Initialize or reinitialize max_samples if needed
        if needs_recalc {
            match select(
                find_initial_max_samples(mtu, &mut sub),
                ads_watcher.changed(),
            )
            .await
            {
                Either::First((samples, buffer, carry)) => {
                    max_samples = samples;
                    carry_over_samples = carry;
                    needs_recalc = false;
                    if let Err(_) = att_payload.extend_from_slice(&buffer) {
                        error!("Failed to extend payload buffer");
                        continue;
                    }
                    if let Err(_) =
                        notifier.notify_data_stream(&att_payload).await
                    {
                        warn!("Failed to notify data stream");
                    }
                    packet_counter += 1;
                    att_payload.clear();
                }
                Either::Second(streaming) => {
                    if !streaming {
                        needs_recalc = true;
                    }
                    continue;
                }
            }
        }

        // Collect samples and handle any interruptions
        let (samples, should_recalc) = collect_samples(
            &mut sub,
            &mut ads_watcher,
            max_samples,
            carry_over_samples.take(),
        )
        .await;

        needs_recalc = should_recalc;

        // Only proceed with encoding and sending if we have samples
        if !samples.is_empty() {
            // Prepare and encode message
            let mut message = icd::proto::AdsDataFrame {
                ts: Instant::now().as_micros(),
                packet_counter,
                samples,
            };

            // Ensure message fits within MTU and update state
            let (new_max_samples, new_carry_over) =
                ensure_mtu_fit(&mut message, mtu, max_samples);
            max_samples = new_max_samples;
            carry_over_samples = new_carry_over;

            if let Err(_) =
                encode_and_send(message, &mut att_payload, notifier).await
            {
                error!("Failed to encode and send message");
            }
            packet_counter += 1;
            att_payload.clear();
        }
    }
}
