use crate::prelude::*;
use embassy_time::Instant;
use heapless::Vec;
use nrf_softdevice::ble::{gatt_client, Connection};

#[nrf_softdevice::gatt_client(uuid = "1805")]
struct CurrentTimeServiceClient {
    #[characteristic(uuid = "2a2b", write, read, notify)]
    current_time: Vec<u8, 10>,
}

pub async fn sync_time(conn: &Connection, clock: &crate::clock::Clock) {
    if let Ok(time_client) =
        gatt_client::discover::<CurrentTimeServiceClient>(&conn).await
    {
        info!("Found time server on peer, synchronizing time");
        match time_client.get_time().await {
            Ok(time) => {
                let time_of_boot = time
                    - time::Duration::microseconds(
                        Instant::now().as_micros() as i64
                    );
                clock.set(time_of_boot);
                info!("Time synced to {:?}", defmt::Debug2Format(&time));
            }
            Err(e) => {
                info!("Error retrieving time: {:?}", e);
            }
        }
    }
}

impl CurrentTimeServiceClient {
    pub async fn get_time(
        &self,
    ) -> Result<time::PrimitiveDateTime, gatt_client::ReadError> {
        let data = self.current_time_read().await?;
        if data.len() == 10 {
            let year = u16::from_le_bytes([data[0], data[1]]);
            let month = data[2];
            let day = data[3];
            let hour = data[4];
            let minute = data[5];
            let second = data[6];
            let _weekday = data[7];
            let secs_frac = data[8];

            if let Ok(month) = month.try_into() {
                let date =
                    time::Date::from_calendar_date(year as i32, month, day);
                let micros = secs_frac as u32 * 1000000 / 256;
                let time =
                    time::Time::from_hms_micro(hour, minute, second, micros);
                if let (Ok(time), Ok(date)) = (time, date) {
                    let dt = time::PrimitiveDateTime::new(date, time);
                    return Ok(dt);
                }
            }
        }
        Err(gatt_client::ReadError::Truncated)
    }
}
