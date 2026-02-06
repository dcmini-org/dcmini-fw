use super::*;
use crate::clock::CLOCK_SET;
use crate::prelude::*;
use crate::tasks::ads::ADS_MEAS_CH;
use crate::tasks::ads::ADS_WATCH;
use core::fmt::Write;
// use ads1299::AdsData;
use dc_mini_bsp::SdCardResources;
// use dc_mini_icd::AdsConfig;
use embassy_futures::select::{select3, Either3};
use embassy_time::Instant;
use embedded_sdmmc::{Mode, TimeSource, Timestamp, VolumeIdx, VolumeManager};
use heapless::String;
use portable_atomic::Ordering;
use prost::Message;

pub struct RealTimeSource;

impl TimeSource for RealTimeSource {
    fn get_timestamp(&self) -> Timestamp {
        let date = crate::CLOCK
            .get(time::Duration::seconds(Instant::now().as_secs() as i64));
        // Convert embassy-time to embedded-sdmmc timestamp
        // This is a placeholder - you'll need to implement proper time conversion
        Timestamp {
            year_since_1970: (date.year() - 1970) as u8,
            zero_indexed_month: date.month() as u8 - 1,
            zero_indexed_day: date.day() - 1,
            hours: date.hour(),
            minutes: date.minute(),
            seconds: date.second(),
        }
    }
}

#[embassy_executor::task]
pub async fn recording_task(
    sd: &'static Mutex<CriticalSectionRawMutex, SdCardResources>,
    id: Option<SessionId>,
) {
    SESSION_ACTIVE.store(true, Ordering::SeqCst);

    let mut sd_resources = sd.lock().await;

    let sd_card = sd_resources.get_card();

    // Initialize SD card
    info!(
        "SD card initialized, size: {} bytes",
        sd_card.num_bytes().unwrap()
    );

    // Create volume manager
    let volume_mgr = VolumeManager::new(sd_card, RealTimeSource);

    let mut ads_watcher =
        ADS_WATCH.receiver().expect("Failed to get ADS watch receiver");
    let mut ads_subscriber = ADS_MEAS_CH
        .subscriber()
        .expect("Failed to get ADS measurement subscriber");

    // Initialize recording
    let volume = volume_mgr
        .open_volume(VolumeIdx(0))
        .expect("Open volume failed.");
    let root_dir = volume.open_root_dir().expect("Failed to open root dir.");

    let mut filename: String<MAX_FILENAME_LEN> = String::new();
    if CLOCK_SET.load(Ordering::SeqCst) {
        let date = crate::CLOCK
            .get(time::Duration::seconds(Instant::now().as_secs() as i64));
        // Find next available sequence number for today
        let mut file_num = 0;
        loop {
            filename.clear();
            write!(
                filename,
                "{:04}{:02}{:02}_{:02}{:02}_{:03}",
                date.year(),
                date.month(),
                date.day(),
                date.hour(),
                date.minute(),
                file_num
            )
            .unwrap();
            // Add ID if present
            if let Some(recording_id) = &id {
                filename.push_str("_").unwrap();
                filename.push_str(recording_id.0.as_str()).unwrap();
                filename.push_str(".dat").unwrap();
            }

            // Check if file exists
            if root_dir.find_directory_entry(filename.as_str()).is_err()
            {
                break;
            }
            file_num += 1;
        }
    } else {
        // Find next available file number
        let mut file_num = 0;
        loop {
            filename.clear();

            write!(filename, "{:03}", file_num).unwrap();
            if let Some(recording_id) = &id {
                filename.push_str("_").unwrap();
                filename.push_str(recording_id.0.as_str()).unwrap();
            }

            filename.push_str(".dat").unwrap();

            if root_dir.find_directory_entry(filename.as_str()).is_err()
            {
                break;
            }
            file_num += 1;
        }
    }
    let file = root_dir
        .open_file_in_dir(filename.as_str(), Mode::ReadWriteCreateOrAppend)
        .expect("Failed to open file.");

    let batch_sz: usize = 100;
    let mut packet_counter = 0;
    let mut message = icd::proto::AdsDataFrame {
        packet_counter,
        ts: Instant::now().as_micros(),
        samples: alloc::vec::Vec::with_capacity(batch_sz),
    };
    let mut out_buffer = alloc::vec::Vec::new();

    loop {
        match select3(
            ads_subscriber.next_message_pure(),
            ads_watcher.changed(),
            SESSION_SIG.wait(),
        )
        .await
        {
            Either3::First(data) => {
                let ads_sample = convert_to_proto(data);

                message.samples.push(ads_sample);
                if message.samples.len() >= batch_sz {
                    out_buffer.clear();
                    message.encode(&mut out_buffer).unwrap();
                    let size = out_buffer.len() as u32;
                    file.write(&size.to_le_bytes()).unwrap();
                    file.write(out_buffer.as_slice()).unwrap();
                    message.samples.clear();
                    packet_counter += 1;
                    message.packet_counter = packet_counter;
                    message.ts = Instant::now().as_micros();
                }
            }
            Either3::Second(streaming) => {
                // If we have data in the buffer, we should probably write out here with
                // corresponding timestamp so that and gap in data has proper timestamping.
                if !streaming {
                    info!("While recording, ADS streaming has stopped!")
                }
            }
            Either3::Third(_) => {
                break;
            }
        }
    }
    // Probably need to also write any data that is still in the buffer out here.
    file.flush().unwrap();
    SESSION_ACTIVE.store(false, Ordering::SeqCst);
}
