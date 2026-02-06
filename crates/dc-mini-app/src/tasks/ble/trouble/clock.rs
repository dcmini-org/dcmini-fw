use crate::prelude::*;
use embassy_futures::select::select;
use embassy_futures::select::Either;
use embassy_time::Instant;
use trouble_host::prelude::*;

use super::BleController;

pub async fn sync_time<'a>(
    stack: &'a Stack<'a, BleController, DefaultPacketPool>,
    conn: &Connection<'a, DefaultPacketPool>,
) {
    info!("[ble] synchronizing time");
    let client =
        unwrap!(GattClient::<_, _, 10>::new(stack, conn).await);
    match select(client.task(), async {
        let services =
            client.services_by_uuid(&Uuid::new_short(0x1805)).await?;
        if let Some(service) = services.first() {
            let c: Characteristic<u8> = client
                .characteristic_by_uuid(&service, &Uuid::new_short(0x2a2b))
                .await?;

            let mut data = [0; 10];
            client.read_characteristic(&c, &mut data[..]).await?;

            if let Some(time) = parse_time(data) {
                let time_of_boot = time
                    - time::Duration::microseconds(
                        Instant::now().as_micros() as i64
                    );
                crate::CLOCK.set(time_of_boot);
                #[cfg(feature = "defmt")]
                info!("Time synced to {:?}", ::defmt::Debug2Format(&time));
                #[cfg(not(feature = "defmt"))]
                info!("Time synced");
            }
        }
        Ok::<(), BleHostError<_>>(())
    })
    .await
    {
        Either::First(_) => panic!("[ble] gatt client exited prematurely"),
        Either::Second(Ok(_)) => {
            info!("[ble] time sync completed");
        }
        Either::Second(Err(e)) => {
            warn!("[ble] time sync error: {:?}", e);
        }
    }
}

fn parse_time(data: [u8; 10]) -> Option<time::PrimitiveDateTime> {
    let year = u16::from_le_bytes([data[0], data[1]]);
    let month = data[2];
    let day = data[3];
    let hour = data[4];
    let minute = data[5];
    let second = data[6];
    let _weekday = data[7];
    let secs_frac = data[8];

    if let Ok(month) = month.try_into() {
        let date = time::Date::from_calendar_date(year as i32, month, day);
        let micros = secs_frac as u32 * 1000000 / 256;
        let time = time::Time::from_hms_micro(hour, minute, second, micros);
        if let (Ok(time), Ok(date)) = (time, date) {
            let dt = time::PrimitiveDateTime::new(date, time);
            return Some(dt);
        }
    }
    None
}
