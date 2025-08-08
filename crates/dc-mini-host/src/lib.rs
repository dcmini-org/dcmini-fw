pub mod clients;
pub mod ui;

pub use clients::DeviceConnection;
pub use dc_mini_icd as icd;

pub mod fileio;

pub enum AdsDataFrames {
    Proto(icd::proto::AdsDataFrame),
    Icd(icd::AdsDataFrame),
}

pub async fn read_line() -> String {
    tokio::task::spawn_blocking(|| {
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap();
        line
    })
    .await
    .unwrap()
}

// Re-export clients and UI components for convenience
pub use clients::*;
pub use ui::*;

pub fn log_ads_frame(
    rec: rerun::RecordingStream,
) -> Box<dyn Fn(icd::SampleRate, AdsDataFrames) + Send> {
    let fp = move |sample_rate, data_frame| {
        let sample_period_us = get_sample_period_us(sample_rate);
        match data_frame {
            AdsDataFrames::Icd(frame) => {
                let num_samples = frame.samples.len();
                if num_samples == 0 {
                    return;
                }

                // For each sample in the frame
                for (i, sample) in frame.samples.iter().enumerate() {
                    // Calculate timestamp for this sample
                    let timestamp = (frame.ts as f64
                        - ((num_samples - 1 - i) as f64 * sample_period_us))
                        / 1_000_000.0;
                    rec.set_time_seconds("time", timestamp);

                    // Log each channel's data
                    for (ch, &value) in sample.data.iter().enumerate() {
                        rec.log(
                            format!("ads/channel_{}", ch),
                            &rerun::Scalar::new(value as f64),
                        )
                        .unwrap();
                    }
                }
            }
            AdsDataFrames::Proto(frame) => {
                let num_samples = frame.samples.len();
                if num_samples == 0 {
                    return;
                }

                // For each sample in the frame
                for (i, sample) in frame.samples.iter().enumerate() {
                    // Calculate timestamp for this sample
                    let timestamp = (frame.ts as f64
                        - ((num_samples - 1 - i) as f64 * sample_period_us))
                        / 1_000_000.0;
                    rec.set_time_seconds("time", timestamp);

                    // Log each channel's data
                    for (ch, &value) in sample.data.iter().enumerate() {
                        rec.log(
                            format!("ads/channel_{}", ch),
                            &rerun::Scalar::new(value as f64),
                        )
                        .unwrap();
                    }
                }
            }
        }
    };
    Box::new(fp)
}

/// Calculate sample period in microseconds from sample rate
pub fn get_sample_period_us(sample_rate: icd::SampleRate) -> f64 {
    let rate_hz = match sample_rate {
        icd::SampleRate::Sps250 => 250.0,
        icd::SampleRate::Sps500 => 500.0,
        icd::SampleRate::KSps1 => 1_000.0,
        icd::SampleRate::KSps2 => 2_000.0,
        icd::SampleRate::KSps4 => 4_000.0,
        icd::SampleRate::KSps8 => 8_000.0,
        icd::SampleRate::KSps16 => 16_000.0,
    };
    1_000_000.0 / rate_hz
}
