use super::{EegDataRecord, EegMetadata, EegReader, Error, Result};
use crate::icd::proto::AdsDataFrame;
use chrono::DateTime;
use prost::Message;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::PathBuf;

// Eventually, this metadata will be contained in the files we write out.
const SAMPLE_RATE: f64 = 250.0; // ADS1299 sample rate
const BIT_DEPTH: u8 = 24; // ADS1299 bit depth
const VREF: f64 = 4.5; // Reference voltage in volts
const GAIN: f64 = 24.0; // PGA gain

// Conversion factor from digital values to microvolts
const CONVERSION_FACTOR: f64 = (VREF / GAIN)
    / (i32::pow(2, BIT_DEPTH as u32 - 1) as f64 - 1.0)
    * 1_000_000.0;

pub struct DatReader {
    reader: BufReader<File>,
    path: PathBuf,
    first_frame: Option<AdsDataFrame>,
    metadata: Option<EegMetadata>,
}

impl DatReader {
    pub fn new(path: &PathBuf) -> Result<Self> {
        Ok(Self {
            reader: BufReader::new(File::open(path)?),
            path: path.clone(),
            first_frame: None,
            metadata: None,
        })
    }

    fn read_frame(&mut self) -> Result<Option<AdsDataFrame>> {
        let mut size_buf = [0u8; 4];
        match self.reader.read_exact(&mut size_buf) {
            Ok(()) => {
                let msg_size = u32::from_le_bytes(size_buf);
                let mut msg_buf = vec![0u8; msg_size as usize];
                self.reader.read_exact(&mut msg_buf)?;

                Ok(Some(AdsDataFrame::decode(&msg_buf[..])?))
            }
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn read_first_frame(&mut self) -> Result<&AdsDataFrame> {
        if self.first_frame.is_none() {
            let frame = self.read_frame()?.ok_or_else(|| {
                Error::InvalidData("Empty DAT file".to_string())
            })?;
            self.first_frame = Some(frame);
        }
        Ok(self.first_frame.as_ref().unwrap())
    }

    /// Scan the file to find the physical min/max values by converting all samples
    fn find_physical_range(&mut self) -> Result<(f64, f64)> {
        // Save current position
        let current_pos = self.reader.stream_position()?;

        // Seek to start
        self.reader.seek(SeekFrom::Start(0))?;

        let mut min_value = f64::MAX;
        let mut max_value = f64::MIN;

        // Read all frames and find min/max values
        while let Some(frame) = self.read_frame()? {
            for sample in frame.samples {
                for value in sample.data {
                    let physical_value = value as f64 * CONVERSION_FACTOR;
                    min_value = min_value.min(physical_value);
                    max_value = max_value.max(physical_value);
                }
            }
        }

        // Restore original position
        self.reader.seek(SeekFrom::Start(current_pos))?;

        // If we didn't find any values, use theoretical limits
        if min_value == f64::MAX || max_value == f64::MIN {
            let max_digital = (1i32 << (BIT_DEPTH - 1)) - 1;
            let min_digital = -(1i32 << (BIT_DEPTH - 1));
            min_value = min_digital as f64 * CONVERSION_FACTOR;
            max_value = max_digital as f64 * CONVERSION_FACTOR;
        }

        Ok((min_value, max_value))
    }
}

impl EegReader for DatReader {
    fn read_header(&mut self) -> Result<EegMetadata> {
        let first_frame = self.read_first_frame()?;

        let num_channels = first_frame
            .samples
            .first()
            .map(|sample| sample.data.len())
            .ok_or_else(|| {
                Error::InvalidData("No samples in first frame".to_string())
            })?;

        let start_time =
            DateTime::from_timestamp_micros(first_frame.ts as i64)
                .ok_or_else(|| {
                    Error::InvalidData("Invalid timestamp".to_string())
                })?;

        // Find actual physical min/max values from the data
        let (physical_min, physical_max) = self.find_physical_range()?;

        let metadata = EegMetadata {
            num_channels,
            sample_rate: SAMPLE_RATE,
            channel_labels: (1..=num_channels)
                .map(|i| format!("EEG-{}", i))
                .collect(),
            start_time: Some(start_time),
            patient_id: None,
            recording_id: self
                .path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(String::from),
            bit_depth: BIT_DEPTH,
            physical_min,
            physical_max,
            conversion_factor: CONVERSION_FACTOR,
        };

        self.metadata = Some(metadata.clone());
        Ok(metadata)
    }

    fn read_data(&mut self) -> Result<Vec<EegDataRecord>> {
        let mut records = Vec::new();
        let num_channels = self.metadata.as_ref().unwrap().num_channels;

        // Seek to start if we haven't read any data yet
        if self.first_frame.is_none() {
            self.reader.seek(SeekFrom::Start(0))?;
        }

        while let Some(frame) = self.read_frame()? {
            for sample in frame.samples {
                // Initialize a vector for each channel
                let mut channel_samples = vec![Vec::new(); num_channels];

                // Store raw digital values
                for (ch_idx, &value) in sample.data.iter().enumerate() {
                    channel_samples[ch_idx].push(value);
                }

                records.push(EegDataRecord {
                    timestamp: Some(frame.ts as f64 / 1_000_000.0),
                    samples: channel_samples,
                });
            }
        }

        Ok(records)
    }
}
