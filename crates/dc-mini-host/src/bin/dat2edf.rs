use byteorder::{LittleEndian, WriteBytesExt};
use chrono::DateTime;
use clap::Parser;
use dc_mini_icd::proto::AdsDataFrame;
use prost::Message;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about = "Convert DC-Mini .dat files to EDF format")]
struct Args {
    /// Input .dat file path
    #[arg(short, long)]
    input: PathBuf,

    /// Output file path (.edf)
    #[arg(short, long)]
    output: PathBuf,

    /// Patient ID (optional)
    #[arg(short, long)]
    patient_id: Option<String>,

    /// Recording ID (optional)
    #[arg(short, long)]
    recording_id: Option<String>,
}

// EDF constants
const EDF_VERSION: &str = "0       "; // 8 chars with spaces
const SAMPLE_RATE: u32 = 250; // ADS1299 sample rate
const DURATION_OF_RECORD: f32 = 1.0; // 1 second per record
const SAMPLES_PER_RECORD: u32 =
    (SAMPLE_RATE as f32 * DURATION_OF_RECORD) as u32;

// ADS1299 constants
const ADS_DIGITAL_MIN: i32 = -8388608; // 24-bit signed min
const ADS_DIGITAL_MAX: i32 = 8388607; // 24-bit signed max
const EDF_DIGITAL_MIN: i16 = -32768; // 16-bit signed min
const EDF_DIGITAL_MAX: i16 = 32767; // 16-bit signed max

struct EdfHeader {
    version: String,
    patient_id: String,
    recording_id: String,
    start_date: String,
    start_time: String,
    num_bytes_header: u64,
    reserved: String,
    num_data_records: i64,
    duration_data_records: f32,
    num_signals: u16,
    signal_labels: Vec<String>,
    transducer_types: Vec<String>,
    physical_dimensions: Vec<String>,
    physical_mins: Vec<f32>,
    physical_maxs: Vec<f32>,
    digital_mins: Vec<i16>, // Changed to i16 for standard EDF
    digital_maxs: Vec<i16>, // Changed to i16 for standard EDF
    prefiltering: Vec<String>,
    samples_per_record: Vec<u32>,
}

impl EdfHeader {
    fn new(num_signals: u16) -> Self {
        let mut header = EdfHeader {
            version: EDF_VERSION.to_string(),
            patient_id: "X X X X".to_string(),
            recording_id: String::new(),
            start_date: String::new(),
            start_time: String::new(),
            num_bytes_header: 256 + (num_signals as u64 * 256),
            reserved: String::new(), // Standard EDF
            num_data_records: -1,
            duration_data_records: DURATION_OF_RECORD,
            num_signals,
            signal_labels: vec![],
            transducer_types: vec![],
            physical_dimensions: vec![],
            physical_mins: vec![],
            physical_maxs: vec![],
            digital_mins: vec![],
            digital_maxs: vec![],
            prefiltering: vec![],
            samples_per_record: vec![],
        };

        // Initialize per-channel settings
        for i in 0..num_signals {
            header.signal_labels.push(format!("EEG-{}", i + 1));
            header.transducer_types.push("AgAgCl electrode".to_string());
            header.physical_dimensions.push("uV".to_string());
            header.physical_mins.push(-187500.0); // Based on ADS1299 gain and reference
            header.physical_maxs.push(187500.0);
            header.digital_mins.push(EDF_DIGITAL_MIN);
            header.digital_maxs.push(EDF_DIGITAL_MAX);
            header.prefiltering.push("HP:DC LP:None".to_string());
            header.samples_per_record.push(SAMPLES_PER_RECORD);
        }

        header
    }

    fn write_to(&self, writer: &mut impl Write) -> std::io::Result<()> {
        // Write fixed header
        write!(writer, "{:8}", self.version)?;
        write!(writer, "{:<80}", self.patient_id)?;
        write!(writer, "{:<80}", self.recording_id)?;
        write!(writer, "{:8}", self.start_date)?;
        write!(writer, "{:8}", self.start_time)?;
        write!(writer, "{:8}", self.num_bytes_header)?;
        write!(writer, "{:<44}", self.reserved)?;
        write!(writer, "{:8}", self.num_data_records)?;
        write!(writer, "{:8}", self.duration_data_records)?;
        write!(writer, "{:4}", self.num_signals)?;

        // Write signal-specific header fields in blocks
        for label in &self.signal_labels {
            write!(writer, "{:<16}", label)?;
        }
        for transducer in &self.transducer_types {
            write!(writer, "{:<80}", transducer)?;
        }
        for dimension in &self.physical_dimensions {
            write!(writer, "{:<8}", dimension)?;
        }
        for &min in &self.physical_mins {
            write!(writer, "{:<8}", min)?;
        }
        for &max in &self.physical_maxs {
            write!(writer, "{:<8}", max)?;
        }
        for &min in &self.digital_mins {
            write!(writer, "{:<8}", min)?;
        }
        for &max in &self.digital_maxs {
            write!(writer, "{:<8}", max)?;
        }
        for filter in &self.prefiltering {
            write!(writer, "{:<80}", filter)?;
        }
        for &samples in &self.samples_per_record {
            write!(writer, "{:<8}", samples)?;
        }
        // Write reserved area for each signal
        for _ in 0..self.num_signals {
            write!(writer, "{:<32}", "")?;
        }

        Ok(())
    }
}

/// Scale a 24-bit value to 16-bit while preserving the relative magnitude
fn scale_to_16bit(value: i32) -> i16 {
    // Convert to float for better precision in scaling
    let scaled = (value as f64 - ADS_DIGITAL_MIN as f64)
        / (ADS_DIGITAL_MAX as f64 - ADS_DIGITAL_MIN as f64)
        * (EDF_DIGITAL_MAX as f64 - EDF_DIGITAL_MIN as f64)
        + EDF_DIGITAL_MIN as f64;

    // Clamp to i16 range and convert back to integer
    scaled.round().clamp(EDF_DIGITAL_MIN as f64, EDF_DIGITAL_MAX as f64) as i16
}

fn process_dat_file(
    input_path: &PathBuf,
    output_path: &PathBuf,
    args: &Args,
) -> std::io::Result<()> {
    let mut input_file = BufReader::new(File::open(input_path)?);
    let mut output_file = BufWriter::new(File::create(output_path)?);

    // Read first message to get timestamp and determine number of channels
    let mut size_buf = [0u8; 4];
    input_file.read_exact(&mut size_buf)?;
    let msg_size = u32::from_le_bytes(size_buf);
    let mut msg_buf = vec![0u8; msg_size as usize];
    input_file.read_exact(&mut msg_buf)?;

    let first_frame = AdsDataFrame::decode(&msg_buf[..]).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, e)
    })?;

    // Get number of channels from first sample
    let num_channels = if let Some(first_sample) = first_frame.samples.first()
    {
        first_sample.data.len()
    } else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "No samples in first frame",
        ));
    };
    println!("Detected {} channels at {} Hz", num_channels, SAMPLE_RATE);

    // Create and initialize EDF header
    let mut header = EdfHeader::new(num_channels as u16);
    header.patient_id = args.patient_id.clone().unwrap_or_default();
    header.recording_id = args.recording_id.clone().unwrap_or_else(|| {
        input_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("UNKNOWN")
            .to_string()
    });

    // Convert timestamp to date/time strings
    let start_time = DateTime::from_timestamp_micros(first_frame.ts as i64)
        .expect("Invalid timestamp");
    header.start_date = start_time.format("%d.%m.%y").to_string();
    header.start_time = start_time.format("%H.%M.%S").to_string();

    // Write header placeholder (we'll update it later with correct record count)
    header.write_to(&mut output_file)?;

    // Process all data records
    let mut record_count = 0;
    let mut channel_buffers: Vec<Vec<i32>> =
        vec![Vec::with_capacity(SAMPLES_PER_RECORD as usize); num_channels];
    let mut total_samples = 0;

    input_file.seek(SeekFrom::Start(0))?;

    while let Ok(()) = input_file.read_exact(&mut size_buf) {
        let msg_size = u32::from_le_bytes(size_buf);
        let mut msg_buf = vec![0u8; msg_size as usize];
        input_file.read_exact(&mut msg_buf)?;

        let frame = AdsDataFrame::decode(&msg_buf[..]).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })?;

        // Process each sample in the frame
        for sample in frame.samples {
            // Validate channel count consistency
            if sample.data.len() != num_channels {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "Inconsistent channel count: expected {}, got {}",
                        num_channels,
                        sample.data.len()
                    ),
                ));
            }

            // Add each channel's sample to its buffer
            for (ch_idx, &value) in sample.data.iter().enumerate() {
                channel_buffers[ch_idx].push(value);
            }
            total_samples += 1;

            // If we have collected enough samples for a record
            if total_samples % SAMPLES_PER_RECORD as usize == 0 {
                // Write all channels for this record
                for ch_buffer in channel_buffers.iter_mut() {
                    // Write all available samples for this channel
                    for value in ch_buffer.drain(..) {
                        let scaled_value = scale_to_16bit(value);
                        output_file.write_i16::<LittleEndian>(scaled_value)?;
                    }
                }
                record_count += 1;
            }
        }
    }

    // Handle any remaining samples
    if !channel_buffers[0].is_empty() {
        let remaining = channel_buffers[0].len();
        // Pad each channel's buffer to full record size if needed
        for ch_buffer in channel_buffers.iter_mut() {
            // First write existing samples
            for value in ch_buffer.drain(..remaining) {
                let scaled_value = scale_to_16bit(value);
                output_file.write_i16::<LittleEndian>(scaled_value)?;
            }
            // Then pad with zeros
            for _ in 0..(SAMPLES_PER_RECORD as usize - remaining) {
                output_file.write_i16::<LittleEndian>(0)?;
            }
        }
        record_count += 1;
    }

    // Update header with correct record count
    output_file.seek(SeekFrom::Start(0))?;
    header.num_data_records = record_count;
    header.write_to(&mut output_file)?;

    output_file.flush()?;
    println!(
        "Successfully wrote {} data records ({:.1} seconds) from {} samples",
        record_count,
        record_count as f32 * DURATION_OF_RECORD,
        total_samples
    );
    Ok(())
}

fn main() -> std::io::Result<()> {
    let args = Args::parse();

    if !args.input.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Input file does not exist",
        ));
    }
    process_dat_file(&args.input, &args.output, &args)
}
