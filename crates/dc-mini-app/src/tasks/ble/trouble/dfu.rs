use crate::prelude::*;
use embedded_storage_async::nor_flash::NorFlash;
use heapless::Vec;
use nrf_dfu_target::prelude::{
    DfuRequest, DfuStatus, DfuTarget, FirmwareInfo, FirmwareType, HardwareInfo,
};
use trouble_host::prelude::*;

use super::ATT_MTU;

pub type Target = DfuTarget<256>;

/// Nordic DFU GATT service (UUID FE59).
///
/// Implements the Nordic nRF DFU protocol for OTA firmware updates.
/// Compatible with nRF Connect app and nrfdfu CLI.
#[gatt_service(uuid = "FE59")]
pub struct NrfDfuService {
    /// DFU Control Point - receives DFU commands, sends notifications.
    #[characteristic(
        uuid = "8EC90001-F315-4F60-9FB8-838830DAEA50",
        write,
        notify
    )]
    pub control: Vec<u8, ATT_MTU>,

    /// DFU Packet - receives firmware data chunks.
    #[characteristic(
        uuid = "8EC90002-F315-4F60-9FB8-838830DAEA50",
        write_without_response,
        notify
    )]
    pub packet: Vec<u8, ATT_MTU>,
}

/// Handle a DFU control point write.
///
/// Decodes the DFU request, processes it via DfuTarget, encodes the response,
/// and notifies the client via the control characteristic.
pub async fn handle_dfu_control<'a, DFU: NorFlash, P: PacketPool>(
    server: &super::gatt::Server<'a>,
    target: &mut Target,
    dfu: &mut DFU,
    conn: &GattConnection<'_, '_, P>,
) -> Option<DfuStatus> {
    let data: Vec<u8, ATT_MTU> = unwrap!(server.dfu.control.get(server));
    if let Ok((request, _)) = DfuRequest::decode(&data) {
        let (response, status) = target.process(request, dfu).await;
        let mut buf = [0u8; 32];
        if let Ok(len) = response.encode(&mut buf[..]) {
            let response = Vec::from_slice(&buf[..len]).unwrap();
            if let Err(e) = server.dfu.control.notify(conn, &response).await {
                warn!("Error notifying DFU control: {:?}", e);
            }
        }
        Some(status)
    } else {
        warn!("Unable to decode DFU control request");
        None
    }
}

/// Handle a DFU packet (firmware data) write.
///
/// Wraps the raw data as a DfuRequest::Write, processes it, and notifies
/// on both control and packet characteristics per the Nordic DFU protocol.
pub async fn handle_dfu_packet<'a, DFU: NorFlash, P: PacketPool>(
    server: &super::gatt::Server<'a>,
    target: &mut Target,
    dfu: &mut DFU,
    conn: &GattConnection<'_, '_, P>,
) -> Option<DfuStatus> {
    let data: Vec<u8, ATT_MTU> = unwrap!(server.dfu.packet.get(server));
    let request = DfuRequest::Write { data: &data[..] };
    let (response, status) = target.process(request, dfu).await;
    let mut buf = [0u8; 32];
    if let Ok(len) = response.encode(&mut buf[..]) {
        let response = Vec::from_slice(&buf[..len]).unwrap();
        if let Err(e) = server.dfu.control.notify(conn, &response).await {
            warn!("Error notifying DFU control: {:?}", e);
        }
        if let Err(e) = server.dfu.packet.notify(conn, &response).await {
            warn!("Error notifying DFU packet: {:?}", e);
        }
    }
    Some(status)
}

/// Create hardware info from the nRF FICR registers.
pub fn hw_info() -> HardwareInfo {
    let ficr = embassy_nrf::pac::FICR;
    let part = ficr.info().part().read().part().to_bits();
    let variant = ficr.info().variant().read();

    HardwareInfo {
        part,
        variant,
        rom_size: 1024 * 1024, // 1MB internal flash
        ram_size: 256 * 1024,  // 256K RAM
        rom_page_size: 4096,   // 4K pages
    }
}

/// Create firmware info for the DFU target.
pub fn fw_info() -> FirmwareInfo {
    FirmwareInfo {
        ftype: FirmwareType::Application,
        version: 1,
        addr: 0,
        len: 0,
    }
}
