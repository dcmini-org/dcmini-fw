use super::*;
use crate::prelude::*;
use drv260x::Drv260x;
use portable_atomic::Ordering;

#[embassy_executor::task]
pub async fn haptic_task(bus_manager: &'static I2cBusManager) {
    HAPTIC_ACTIVE.store(true, Ordering::SeqCst);

    // Acquire bus handle - configures bus if needed
    let handle = bus_manager.acquire().await.unwrap();
    let mut haptic = Drv260x::new(handle.device());

    // Initialize with retry loop
    for i in 0..5 {
        if haptic.init_async().await.is_ok() {
            break;
        } else {
            info!("Retry connection attempt {:?} to DRV2605L...", i);
            Timer::after_millis(1000).await;
        }
    }

    info!("DRV2605L haptic driver initialized.");

    loop {
        let cmd = HAPTIC_CMD_SIG.wait().await;

        match cmd {
            Some(HapticCommand::PlayEffect(effect)) => {
                if let Err(e) = haptic.set_single_effect_enum_async(effect).await
                {
                    error!("Failed to set haptic effect: {:?}", e);
                    continue;
                }
                if let Err(e) = haptic.go_async().await {
                    error!("Failed to trigger haptic: {:?}", e);
                }
            }
            Some(HapticCommand::PlaySequence(entries)) => {
                if let Err(e) =
                    haptic.set_waveform_sequence_async(&entries).await
                {
                    error!("Failed to set haptic sequence: {:?}", e);
                    continue;
                }
                if let Err(e) = haptic.go_async().await {
                    error!("Failed to trigger haptic sequence: {:?}", e);
                }
            }
            None => {
                // Stop signal received
                let _ = haptic.stop_async().await;
                break;
            }
        }
    }

    // Clean up - put driver into standby
    let _ = haptic.set_standby_async(true).await;

    HAPTIC_CMD_SIG.reset();
    HAPTIC_ACTIVE.store(false, Ordering::SeqCst);
}
