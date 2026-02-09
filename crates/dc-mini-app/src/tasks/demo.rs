use crate::prelude::*;
use crate::tasks::ads::ADS_MEAS_CH;
use crate::tasks::apds::APDS_DATA_WATCH;
use crate::tasks::imu::IMU_DATA_WATCH;
use crate::tasks::mic::{MIC_BUF_SAMPLES, MIC_STREAM_CH};
use crate::tasks::neopix::{NeopixEvent, NEOPIX_CHAN};
use embassy_futures::select::{select, Either};
use futures::pin_mut;
use smart_leds::colors;

async fn log_ads_for_seconds(seconds: u64) {
    let mut sub =
        ADS_MEAS_CH.subscriber().expect("Failed to create ADS demo subscriber");

    for _ in 0..seconds {
        let tick = Timer::after_secs(1);
        pin_mut!(tick);
        let mut latest = None;

        loop {
            match select(sub.next_message_pure(), tick.as_mut()).await {
                Either::First(data) => latest = Some(data),
                Either::Second(_) => break,
            }
        }

        if let Some(data) = latest {
            let total_channels: usize =
                data.iter().map(|dev| dev.data.len()).sum();
            let first_sample = data
                .iter()
                .next()
                .and_then(|dev| dev.data.first())
                .copied()
                .unwrap_or(0);
            info!(
                "[Demo][ADS] devs={}, total_channels={}, first_sample={}",
                data.len(),
                total_channels,
                first_sample
            );
        } else {
            warn!("[Demo][ADS] no data in last second");
        }
    }
}

async fn log_imu_for_seconds(seconds: u64) {
    for _ in 0..seconds {
        Timer::after_secs(1).await;
        if let Some(data) = IMU_DATA_WATCH.try_get() {
            let ax_mg = (data.accel_x * 1000.0) as i32;
            let ay_mg = (data.accel_y * 1000.0) as i32;
            let az_mg = (data.accel_z * 1000.0) as i32;
            let gx_mdps = (data.gyro_x * 1000.0) as i32;
            let gy_mdps = (data.gyro_y * 1000.0) as i32;
            let gz_mdps = (data.gyro_z * 1000.0) as i32;
            info!(
                "[Demo][IMU] accel_mg=({},{},{}), gyro_mdps=({},{},{})",
                ax_mg,
                ay_mg,
                az_mg,
                gx_mdps,
                gy_mdps,
                gz_mdps
            );
        } else {
            warn!("[Demo][IMU] no data in last second");
        }
    }
}

async fn log_apds_for_seconds(seconds: u64) {
    for _ in 0..seconds {
        Timer::after_secs(1).await;
        if let Some(data) = APDS_DATA_WATCH.try_get() {
            let lux_milli = (data.lux * 1000.0) as i32;
            info!(
                "[Demo][APDS] r={}, g={}, b={}, ir={}, cct={}, lux_milli={}",
                data.red,
                data.green,
                data.blue,
                data.ir,
                data.cct,
                lux_milli
            );
        } else {
            warn!("[Demo][APDS] no data in last second");
        }
    }
}

async fn log_mic_for_seconds(seconds: u64) {
    let mut sub =
        MIC_STREAM_CH.subscriber().expect("Failed to create mic demo subscriber");

    for _ in 0..seconds {
        let tick = Timer::after_secs(1);
        pin_mut!(tick);
        let mut latest = None;

        loop {
            match select(sub.next_message_pure(), tick.as_mut()).await {
                Either::First(buf) => latest = Some(buf),
                Either::Second(_) => break,
            }
        }

        if let Some(buf) = latest {
            let mut min = i16::MAX;
            let mut max = i16::MIN;
            for &sample in buf.iter() {
                min = min.min(sample);
                max = max.max(sample);
            }
            info!(
                "[Demo][MIC] first={}, last={}, min={}, max={}",
                buf[0],
                buf[MIC_BUF_SAMPLES - 1],
                min,
                max
            );
        } else {
            warn!("[Demo][MIC] no data in last second");
        }
    }
}

#[embassy_executor::task]
pub async fn demo_task(sender: EventSender) {
    info!("--- Demo starting ---");

    // 1. NeoPixel color cycle
    info!("[Demo] NeoPixel color cycle");
    for color in [
        colors::RED,
        colors::GREEN,
        colors::BLUE,
        colors::YELLOW,
        colors::CYAN,
        colors::MAGENTA,
    ] {
        NEOPIX_CHAN.send(NeopixEvent::Color(color)).await;
        Timer::after_secs(1).await;
    }

    // 2. ADS stream
    info!("[Demo] ADS stream — 5s");
    sender.send(AdsEvent::StartStream.into()).await;
    log_ads_for_seconds(5).await;
    sender.send(AdsEvent::StopStream.into()).await;
    Timer::after_secs(1).await;

    // 3. IMU stream
    info!("[Demo] IMU stream — 5s");
    sender.send(ImuEvent::StartStream.into()).await;
    log_imu_for_seconds(5).await;
    sender.send(ImuEvent::StopStream.into()).await;
    Timer::after_secs(1).await;

    // 4. APDS stream
    info!("[Demo] APDS stream — 5s");
    sender.send(ApdsEvent::StartStream.into()).await;
    log_apds_for_seconds(5).await;
    sender.send(ApdsEvent::StopStream.into()).await;
    Timer::after_secs(1).await;

    // 5. Mic stream
    info!("[Demo] Mic stream — 5s");
    sender.send(MicEvent::StartStream.into()).await;
    log_mic_for_seconds(5).await;
    sender.send(MicEvent::StopStream.into()).await;
    Timer::after_secs(1).await;

    // 6. Done — green flash
    info!("--- Demo complete ---");
    NEOPIX_CHAN
        .send(NeopixEvent::FlashFor(
            colors::GREEN,
            Duration::from_millis(500),
            6,
            None,
        ))
        .await;
}
