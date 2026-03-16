use crate::prelude::*;
use embassy_futures::select::{select, Either};
use embassy_sync::signal::Signal;
use postcard_rpc::{header::VarHeader, server::Sender};

static CVEP_USB_STREAM: Signal<CriticalSectionRawMutex, ()> = Signal::new();

#[cfg(feature = "cvep")]
use crate::tasks::cvep::{current_config, CVEP_DECISION_CH, CVEP_WATCH};

#[embassy_executor::task]
pub async fn cvep_start_handler(
    context: SpawnCtx,
    header: VarHeader,
    _rqst: (),
    sender: Sender<super::AppTx>,
) {
    let config = cvep_config();

    #[cfg(feature = "cvep")]
    if config.model_enabled {
        let ctx = context.app.lock().await;
        ctx.event_sender.send(CvepEvent::Start.into()).await;
    }

    if sender.reply::<CvepStartEndpoint>(header.seq_no, &config).await.is_err()
    {
        error!("Failed to reply, stopping CVEP stream");
        return;
    }

    if !config.model_enabled {
        warn!("CVEP model is not configured; skipping USB decision stream.");
        return;
    }

    #[cfg(feature = "cvep")]
    select(cvep_stream_usb(sender), CVEP_USB_STREAM.wait()).await;
    CVEP_USB_STREAM.reset();
}

pub async fn cvep_stop_handler(
    context: &mut super::Context,
    _header: VarHeader,
    _rqst: (),
) -> bool {
    #[cfg(feature = "cvep")]
    {
        let ctx = context.app.lock().await;
        ctx.event_sender.send(CvepEvent::Stop.into()).await;
    }

    CVEP_USB_STREAM.signal(());
    true
}

pub async fn cvep_get_status(
    _context: &mut super::Context,
    _header: VarHeader,
    _rqst: (),
) -> bool {
    #[cfg(feature = "cvep")]
    {
        return CVEP_WATCH.try_get().unwrap_or(false);
    }

    #[cfg(not(feature = "cvep"))]
    {
        false
    }
}

pub async fn cvep_get_config(
    _context: &mut super::Context,
    _header: VarHeader,
    _rqst: (),
) -> dc_mini_icd::CvepConfig {
    cvep_config()
}

#[cfg(feature = "cvep")]
async fn cvep_stream_usb(sender: Sender<super::AppTx>) {
    let mut sub = CVEP_DECISION_CH
        .dyn_subscriber()
        .expect("Failed to create CVEP decision subscriber");
    let mut cvep_watcher =
        CVEP_WATCH.dyn_receiver().expect("Failed to create CVEP watcher");
    let mut packet_counter = 0u8;

    loop {
        match select(sub.next_message_pure(), cvep_watcher.changed()).await {
            Either::First(decision) => {
                let frame = dc_mini_icd::CvepDecision {
                    ts: decision.ts,
                    class_index: decision.class_index as u16,
                    raw_score: decision.raw_score,
                    normalized_score: decision.normalized_score,
                    margin: decision.margin,
                };
                if let Err(_e) = sender
                    .publish::<dc_mini_icd::CvepTopic>(
                        packet_counter.into(),
                        &frame,
                    )
                    .await
                {
                    #[cfg(feature = "defmt")]
                    warn!(
                        "Failed to publish CVEP decision: {:?}",
                        defmt::Debug2Format(&_e)
                    );
                }
                packet_counter = packet_counter.wrapping_add(1);
            }
            Either::Second(active) => {
                if !active {
                    loop {
                        if cvep_watcher.changed().await {
                            break;
                        }
                    }
                }
            }
        }
    }
}

fn cvep_config() -> dc_mini_icd::CvepConfig {
    #[cfg(feature = "cvep")]
    {
        return current_config();
    }

    #[cfg(not(feature = "cvep"))]
    {
        dc_mini_icd::CvepConfig {
            model_enabled: false,
            channels: 0,
            classes: 0,
            window_samples: 0,
            inference_stride_samples: 0,
            score_threshold: None,
            margin_threshold: None,
        }
    }
}
