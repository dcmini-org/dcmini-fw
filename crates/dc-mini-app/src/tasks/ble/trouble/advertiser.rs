use super::gatt::{ServerWithDfu, ServerWithoutDfu};
use crate::prelude::*;
use trouble_host::prelude::*;

pub async fn advertise_with_dfu<'values, 'server, C: Controller>(
    name: &'values str,
    peripheral: &mut Peripheral<'values, C, DefaultPacketPool>,
    server: &'server ServerWithDfu<'values>,
) -> Result<
    GattConnection<'values, 'server, DefaultPacketPool>,
    BleHostError<C::Error>,
> {
    let mut adv_data = [0; 31];
    let adv_len = AdStructure::encode_slice(
        &[
            AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
            AdStructure::ServiceUuids16(&[
                [0x0f, 0x18],
                [0x0a, 0x18],
                [0x59, 0xFE],
            ]),
            AdStructure::CompleteLocalName(name.as_bytes()),
        ],
        &mut adv_data[..],
    )?;

    let mut scan_data = [0; 31];
    let scan_len = AdStructure::encode_slice(
        &[AdStructure::ServiceUuids128(&[[
            0x1c, 0xf5, 0x57, 0xb4, 0xbe, 0x4d, 0xba, 0xa0, 0xaf, 0x43,
            0x46, 0xaf, 0x00, 0x00, 0x10, 0x32,
        ]])],
        &mut scan_data[..],
    )?;

    let advertiser = peripheral
        .advertise(
            &Default::default(),
            Advertisement::ConnectableScannableUndirected {
                adv_data: &adv_data[..adv_len],
                scan_data: &scan_data[..scan_len],
            },
        )
        .await?;
    info!("[adv] advertising");
    let conn = advertiser.accept().await?.with_attribute_server(server)?;
    info!("[adv] connection established");
    Ok(conn)
}

pub async fn advertise_without_dfu<'values, 'server, C: Controller>(
    name: &'values str,
    peripheral: &mut Peripheral<'values, C, DefaultPacketPool>,
    server: &'server ServerWithoutDfu<'values>,
) -> Result<
    GattConnection<'values, 'server, DefaultPacketPool>,
    BleHostError<C::Error>,
> {
    let mut adv_data = [0; 31];
    let adv_len = AdStructure::encode_slice(
        &[
            AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
            AdStructure::ServiceUuids16(&[[0x0f, 0x18], [0x0a, 0x18]]),
            AdStructure::CompleteLocalName(name.as_bytes()),
        ],
        &mut adv_data[..],
    )?;

    let mut scan_data = [0; 31];
    let scan_len = AdStructure::encode_slice(
        &[AdStructure::ServiceUuids128(&[[
            0x1c, 0xf5, 0x57, 0xb4, 0xbe, 0x4d, 0xba, 0xa0, 0xaf, 0x43,
            0x46, 0xaf, 0x00, 0x00, 0x10, 0x32,
        ]])],
        &mut scan_data[..],
    )?;

    let advertiser = peripheral
        .advertise(
            &Default::default(),
            Advertisement::ConnectableScannableUndirected {
                adv_data: &adv_data[..adv_len],
                scan_data: &scan_data[..scan_len],
            },
        )
        .await?;
    info!("[adv] advertising");
    let conn = advertiser.accept().await?.with_attribute_server(server)?;
    info!("[adv] connection established");
    Ok(conn)
}
