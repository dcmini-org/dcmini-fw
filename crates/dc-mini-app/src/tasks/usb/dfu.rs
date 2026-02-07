use crate::events::DfuEvent;
use crate::prelude::*;
use dc_mini_icd::{
    DfuBegin, DfuProgress, DfuProgressState, DfuResult, DfuWriteChunk,
};
use embedded_storage_async::nor_flash::{NorFlash, ReadNorFlash};
use postcard_rpc::header::VarHeader;

/// Maximum firmware size (992K DFU partition).
const MAX_FIRMWARE_SIZE: u32 = 992 * 1024;

pub async fn dfu_begin(
    context: &mut super::Context,
    _header: VarHeader,
    req: DfuBegin,
) -> DfuResult {
    if req.firmware_size == 0 || req.firmware_size > MAX_FIRMWARE_SIZE {
        return DfuResult {
            success: false,
            message: heapless::String::try_from("Invalid firmware size")
                .unwrap(),
        };
    }

    // Check if recording is active
    {
        let app_ctx = context.app.lock().await;
        if app_ctx.state.recording_status {
            return DfuResult {
                success: false,
                message: heapless::String::try_from("Recording active")
                    .unwrap(),
            };
        }
    }

    // Try to claim DFU lock
    if !context.dfu.try_start() {
        return DfuResult {
            success: false,
            message: heapless::String::try_from("DFU already active").unwrap(),
        };
    }

    // Erase the DFU partition
    info!(
        "[usb-dfu] Begin: erasing DFU partition for {}B firmware",
        req.firmware_size
    );
    {
        let mut partition = context.dfu.dfu_partition();
        let capacity = partition.capacity();
        if let Err(_e) = partition.erase(0, capacity as u32).await {
            context.dfu.finish();
            #[cfg(feature = "defmt")]
            warn!("[usb-dfu] Erase failed: {:?}", defmt::Debug2Format(&_e));
            return DfuResult {
                success: false,
                message: heapless::String::try_from("Flash erase failed")
                    .unwrap(),
            };
        }
    }

    context.dfu.set_total_size(req.firmware_size);

    {
        let app_ctx = context.app.lock().await;
        app_ctx.event_sender.send(DfuEvent::Started.into()).await;
    }

    DfuResult {
        success: true,
        message: heapless::String::try_from("DFU started").unwrap(),
    }
}

pub async fn dfu_write(
    context: &mut super::Context,
    _header: VarHeader,
    req: DfuWriteChunk,
) -> DfuResult {
    if !context.dfu.is_active() {
        return DfuResult {
            success: false,
            message: heapless::String::try_from("No DFU in progress").unwrap(),
        };
    }

    // Pad data to 4-byte alignment for QSPI WRITE_SIZE requirement
    let data = &req.data;
    let aligned_len = (data.len() + 3) & !3;
    let mut buf = [0u8; 516]; // 512 max data + 3 max padding + 1
    buf[..data.len()].copy_from_slice(data);

    let mut partition = context.dfu.dfu_partition();
    if let Err(_e) = partition.write(req.offset, &buf[..aligned_len]).await {
        context.dfu.finish();
        #[cfg(feature = "defmt")]
        warn!(
            "[usb-dfu] Write failed at offset {}: {:?}",
            req.offset,
            defmt::Debug2Format(&_e)
        );
        {
            let app_ctx = context.app.lock().await;
            app_ctx.event_sender.send(DfuEvent::Failed.into()).await;
        }
        return DfuResult {
            success: false,
            message: heapless::String::try_from("Flash write failed").unwrap(),
        };
    }

    // Track progress and emit events at 10% boundaries
    let prev_offset = context.dfu.progress().0;
    context.dfu.add_offset(data.len() as u32);
    let (new_offset, total) = context.dfu.progress();
    if total > 0 {
        let prev_pct = (prev_offset as u64 * 100 / total as u64) / 10;
        let new_pct = (new_offset as u64 * 100 / total as u64) / 10;
        if new_pct > prev_pct {
            let pct = (new_offset as u64 * 100 / total as u64) as u8;
            let app_ctx = context.app.lock().await;
            app_ctx.event_sender.send(DfuEvent::Progress(pct).into()).await;
        }
    }

    DfuResult {
        success: true,
        message: heapless::String::try_from("Chunk written").unwrap(),
    }
}

pub async fn dfu_finish(
    context: &mut super::Context,
    _header: VarHeader,
    _req: (),
) -> DfuResult {
    if !context.dfu.is_active() {
        return DfuResult {
            success: false,
            message: heapless::String::try_from("No DFU in progress").unwrap(),
        };
    }

    info!("[usb-dfu] Finish: marking updated");
    match context.dfu.mark_updated() {
        Ok(()) => {
            context.dfu.finish();
            {
                let app_ctx = context.app.lock().await;
                app_ctx.event_sender.send(DfuEvent::Complete.into()).await;
            }
            info!("[usb-dfu] Marked updated, resetting in 4s");
            embassy_time::Timer::after_secs(4).await;
            cortex_m::peripheral::SCB::sys_reset();
            // Unreachable, but satisfies the compiler
            #[allow(unreachable_code)]
            DfuResult {
                success: true,
                message: heapless::String::try_from("DFU complete, resetting")
                    .unwrap(),
            }
        }
        Err(_e) => {
            context.dfu.finish();
            {
                let app_ctx = context.app.lock().await;
                app_ctx.event_sender.send(DfuEvent::Failed.into()).await;
            }
            warn!("[usb-dfu] mark_updated failed");
            DfuResult {
                success: false,
                message: heapless::String::try_from("mark_updated failed")
                    .unwrap(),
            }
        }
    }
}

pub async fn dfu_abort(
    context: &mut super::Context,
    _header: VarHeader,
    _req: (),
) -> DfuResult {
    if context.dfu.is_active() {
        context.dfu.finish();
        {
            let app_ctx = context.app.lock().await;
            app_ctx.event_sender.send(DfuEvent::Aborted.into()).await;
        }
        info!("[usb-dfu] DFU aborted");
        DfuResult {
            success: true,
            message: heapless::String::try_from("DFU aborted").unwrap(),
        }
    } else {
        DfuResult {
            success: false,
            message: heapless::String::try_from("No DFU in progress").unwrap(),
        }
    }
}

pub async fn dfu_status(
    context: &mut super::Context,
    _header: VarHeader,
    _req: (),
) -> DfuProgress {
    let (offset, total_size) = context.dfu.progress();
    if context.dfu.is_active() {
        DfuProgress { state: DfuProgressState::Receiving, offset, total_size }
    } else {
        DfuProgress { state: DfuProgressState::Idle, offset, total_size }
    }
}
