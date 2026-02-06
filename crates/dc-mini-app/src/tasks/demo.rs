use crate::prelude::*;
use crate::tasks::neopix::{NeopixEvent, NEOPIX_CHAN};
use smart_leds::colors;

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
    Timer::after_secs(5).await;
    sender.send(AdsEvent::StopStream.into()).await;
    Timer::after_secs(1).await;

    // 3. IMU stream
    info!("[Demo] IMU stream — 5s");
    sender.send(ImuEvent::StartStream.into()).await;
    Timer::after_secs(5).await;
    sender.send(ImuEvent::StopStream.into()).await;
    Timer::after_secs(1).await;

    // 4. APDS stream
    info!("[Demo] APDS stream — 5s");
    sender.send(ApdsEvent::StartStream.into()).await;
    Timer::after_secs(5).await;
    sender.send(ApdsEvent::StopStream.into()).await;
    Timer::after_secs(1).await;

    // 5. Mic single sample
    info!("[Demo] Mic single sample");
    sender.send(MicEvent::SingleSample.into()).await;
    Timer::after_secs(2).await;

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
