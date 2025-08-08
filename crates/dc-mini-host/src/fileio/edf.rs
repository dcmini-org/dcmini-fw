use super::{
    ConversionConfig, EegDataRecord, EegMetadata, EegWriter, Error,
    PhysicalUnitConversion, Result,
};
use byteorder::{LittleEndian, WriteBytesExt};
use chrono::{Datelike, NaiveDate, Timelike};
use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};

// EDF constants
const EDF_VERSION: &str = "0"; // 8 chars with spaces
const DURATION_OF_RECORD: f32 = 1.0; // 1 second per record

// EDF digital range (16-bit signed)
const EDF_DIGITAL_MIN: i16 = -32768;
const EDF_DIGITAL_MAX: i16 = 32767;

// EDF+ annotation-related constants
const TAL_DURATION_CHAR: u8 = 0x14; // ASCII DC4 (20 decimal) - Annotation separator
const TAL_END_CHAR: u8 = 0x00; // NULL terminator (0 decimal)

// Standard EEG electrode positions according to 10/20 and 10/10% system
// static STANDARD_EEG_ELECTRODES: Lazy<HashSet<&'static str>> =
//     Lazy::new(|| {
//         let mut s = HashSet::new();
//         // 10/20 system
//         s.extend([
//             "Fp1", "Fp2", "F3", "F4", "C3", "C4", "P3", "P4", "O1", "O2",
//             "F7", "F8", "T3", "T4", "T5", "T6", "Fz", "Cz", "Pz",
//         ]);
//         // Additional 10/10% positions
//         s.extend([
//             "Fpz", "AFz", "FCz", "CPz", "POz", "Oz", "Iz", "F9", "F10", "AF7",
//             "AF3", "AF4", "AF8", "F5", "F1", "F2", "F6", "FT9", "FT7", "FC5",
//             "FC3", "FC1", "FC2", "FC4", "FC6", "FT8", "FT10", "T9", "T7",
//             "C5", "C1", "C2", "C6", "T8", "T10", "TP9", "TP7", "CP5", "CP3",
//             "CP1", "CP2", "CP4", "CP6", "TP8", "TP10", "P9", "P7", "P5", "P1",
//             "P2", "P6", "P8", "P10", "PO7", "PO3", "PO4", "PO8", "A1", "A2",
//         ]);
//         s
//     });

/// Validates an EEG channel label according to EDF+ specifications
// fn validate_eeg_label(label: &str) -> Result<()> {
//     // Split into signal type and specification
//     let parts: Vec<&str> = label.trim().split_whitespace().collect();
//     if parts.len() != 2 {
//         return Err(Error::InvalidInput(format!(
//             "Invalid EEG label format '{}'. Must be 'EEG <electrode>' or 'EEG <electrode>-<reference>'",
//             label
//         )));
//     }
//
//     if parts[0] != "EEG" {
//         return Err(Error::InvalidInput(format!(
//             "Invalid signal type '{}'. Must be 'EEG'",
//             parts[0]
//         )));
//     }
//
//     // Check electrode specification
//     let electrodes: Vec<&str> = parts[1].split('-').collect();
//     for electrode in electrodes {
//         if !STANDARD_EEG_ELECTRODES.contains(electrode) {
//             return Err(Error::InvalidInput(format!(
//                 "Invalid electrode position '{}'. Must be a standard 10/20 or 10/10% position",
//                 electrode
//             )));
//         }
//     }
//
//     Ok(())
// }

/// Represents an EDF+ annotation with onset time, optional duration, and text
#[derive(Debug, Clone)]
pub struct EdfAnnotation {
    /// Onset in seconds from start of recording
    pub onset: f64,
    /// Optional duration in seconds
    pub duration: Option<f64>,
    /// Annotation text
    pub text: String,
}

impl EdfAnnotation {
    /// Create a new annotation
    pub fn new(onset: f64, duration: Option<f64>, text: String) -> Self {
        Self { onset, duration, text }
    }

    /// Format a number for EDF+ annotations, removing unnecessary trailing zeros
    fn format_number(value: f64, include_plus: bool) -> String {
        if value.fract() == 0.0 {
            // For integer values, use simple format
            if include_plus {
                format!("+{}", value as i64)
            } else {
                format!("{}", value as i64)
            }
        } else {
            // For fractional values, format with up to 6 decimal places, no trailing zeros
            let mut s = if include_plus {
                format!("+{:.6}", value)
            } else {
                format!("{:.6}", value)
            };

            // Remove trailing zeros
            while s.ends_with('0') && s.contains('.') {
                s.pop();
            }

            // Remove decimal point if it's the last character
            if s.ends_with('.') {
                s.pop();
            }

            s
        }
    }

    /// Convert this annotation to a byte representation for EDF+ file
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        // Add onset timestamp with its marker
        bytes.extend_from_slice(
            Self::format_number(self.onset, true).as_bytes(),
        );

        // Add annotation text
        bytes.push(TAL_DURATION_CHAR);
        bytes.extend_from_slice(self.text.as_bytes());
        bytes.push(TAL_DURATION_CHAR);
        bytes.push(TAL_END_CHAR);

        bytes
    }
}

/// Configuration specific to EDF+ format
#[derive(Debug, Clone)]
pub struct EdfConfig {
    // Hospital information
    pub hospital_code: String,
    // Patient information
    pub patient_sex: char, // 'M' or 'F'
    pub patient_birthdate: NaiveDate,
    pub patient_name: String,
    // Recording information
    pub recording_technician: String,
    pub recording_equipment: String,
    pub recording_start_date: NaiveDate,
    // Channel configuration
    pub electrode_labels: Vec<String>,
    // EDF+ specific settings
    pub include_annotations: bool,
    pub annotations_samples_per_record: usize,
}

impl EdfConfig {
    pub fn new(
        hospital_code: String,
        patient_sex: char,
        patient_birthdate: NaiveDate,
        patient_name: String,
        recording_technician: String,
        recording_equipment: String,
        recording_start_date: NaiveDate,
        electrode_labels: Vec<String>,
    ) -> Result<Self> {
        // Validate sex
        if !matches!(patient_sex, 'M' | 'F') {
            return Err(Error::InvalidInput(
                "Sex must be either 'M' or 'F'".to_string(),
            ));
        }

        Ok(Self {
            hospital_code,
            patient_sex,
            patient_birthdate,
            patient_name,
            recording_technician,
            recording_equipment,
            recording_start_date,
            electrode_labels,
            include_annotations: true,
            annotations_samples_per_record: 60, // Default value - adjust as needed
        })
    }
}

pub struct EdfWriter {
    writer: BufWriter<File>,
    config: EdfConfig,
    metadata: Option<EegMetadata>,
    record_count: i64,
    annotations: Vec<EdfAnnotation>, // Add this field to store annotations
}

impl EdfWriter {
    pub fn new(config: &ConversionConfig) -> Result<Self> {
        match config {
            ConversionConfig::Edf {
                output_path, config: edf_config, ..
            } => Ok(Self {
                writer: BufWriter::new(File::create(output_path)?),
                config: edf_config.clone(),
                metadata: None,
                record_count: -1,
                annotations: Vec::new(),
            }),
            // _ => Err(Error::InvalidInput(
            //     "Expected EDF configuration".to_string(),
            // )),
        }
    }

    fn write_str(&mut self, s: &str, width: usize) -> Result<()> {
        // Ensure we write exactly width bytes, space-padded on the left
        let bytes = format!("{:<width$}", s, width = width).into_bytes();
        Ok(self.writer.write_all(&bytes[..width])?)
    }

    fn write_num<T: std::fmt::Display>(
        &mut self,
        num: T,
        width: usize,
    ) -> Result<()> {
        // Format number left-justified with space padding
        // This ensures numbers are properly aligned according to EDF spec
        let formatted = format!("{:<width$}", num, width = width);
        Ok(self.writer.write_all(formatted.as_bytes())?)
    }

    fn write_float(&mut self, num: f64, width: usize) -> Result<()> {
        // Format floating point numbers according to EDF spec:
        // - Left-justified
        // - One decimal place
        // - No scientific notation
        // - Space padded
        // - Minus sign immediately before first digit
        let formatted = format!("{:<.1}", num);
        let padded = format!("{:<width$}", formatted, width = width);
        Ok(self.writer.write_all(padded.as_bytes())?)
    }

    /// Scale a raw digital value to EDF's 16-bit range while preserving the relative magnitude
    fn scale_to_edf_digital(
        &self,
        raw_value: i32,
        metadata: &EegMetadata,
    ) -> i16 {
        // First convert to physical units
        let physical_value = metadata.to_physical_units(raw_value);

        // Then scale to EDF's digital range
        let scaled = (physical_value - metadata.physical_min)
            / (metadata.physical_max - metadata.physical_min)
            * (EDF_DIGITAL_MAX as f64 - EDF_DIGITAL_MIN as f64)
            + EDF_DIGITAL_MIN as f64;

        // Clamp to i16 range and convert
        scaled.round().clamp(EDF_DIGITAL_MIN as f64, EDF_DIGITAL_MAX as f64)
            as i16
    }

    /// Add an annotation to the EDF+ file
    pub fn add_annotation(&mut self, annotation: EdfAnnotation) {
        self.annotations.push(annotation);
    }

    /// Create a timekeeping TAL for a data record
    fn create_timekeeping_tal(record_index: usize) -> Vec<u8> {
        let mut bytes = Vec::new();

        // Calculate record time in seconds
        let record_time = record_index as f64 * DURATION_OF_RECORD as f64;

        // Create the timekeeping TAL
        bytes.extend_from_slice(
            EdfAnnotation::format_number(record_time, true).as_bytes(),
        );
        bytes.push(TAL_DURATION_CHAR);
        bytes.push(TAL_DURATION_CHAR);
        bytes.push(TAL_END_CHAR);

        bytes
    }

    /// Format all annotations for a specific data record
    fn format_annotations(&self, record_index: usize) -> Vec<u8> {
        let samples_per_record =
            self.config.annotations_samples_per_record * 2;
        let mut buffer = vec![0u8; samples_per_record];
        let mut position = 0;

        // Start with the required timekeeping TAL
        let timekeeping_tal = Self::create_timekeeping_tal(record_index);

        // Add timekeeping TAL to buffer
        let copy_len = timekeeping_tal.len().min(buffer.len());
        buffer[..copy_len].copy_from_slice(&timekeeping_tal[..copy_len]);
        position += copy_len;

        // Calculate time range for this record
        let record_time = record_index as f64 * DURATION_OF_RECORD as f64;
        let record_end_time = record_time + DURATION_OF_RECORD as f64;

        // Add any annotations that fall within this record's time range
        for annotation in &self.annotations {
            if annotation.onset >= record_time
                && annotation.onset < record_end_time
            {
                let annotation_bytes = annotation.to_bytes();
                let remaining_space = buffer.len() - position;

                if annotation_bytes.len() <= remaining_space {
                    // Copy the entire annotation
                    buffer[position..position + annotation_bytes.len()]
                        .copy_from_slice(&annotation_bytes);
                    position += annotation_bytes.len();
                } else {
                    // Copy as much as we can
                    if remaining_space > 0 {
                        buffer[position..].copy_from_slice(
                            &annotation_bytes[..remaining_space],
                        );
                        // position = buffer.len(); // buffer is now full
                    }

                    break;
                }
            }
        }

        buffer
    }

    /// Write the EDF Annotations signal to the file
    fn write_annotations_signal(&mut self, record_index: usize) -> Result<()> {
        let buffer = self.format_annotations(record_index);
        self.writer.write_all(&buffer)?;
        Ok(())
    }
}

impl EegWriter for EdfWriter {
    fn set_metadata(&mut self, mut metadata: EegMetadata) {
        // Override the start time with the one from our config
        if let Ok(dt) = self
            .config
            .recording_start_date
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| Error::InvalidInput("invalid date".to_string()))
        {
            metadata.start_time = Some(dt.and_utc());
        }
        self.metadata = Some(metadata);
    }

    fn write_header(&mut self) -> Result<()> {
        // Extract all metadata first to avoid borrow conflicts
        let metadata =
            self.metadata.clone().ok_or_else(|| Error::NoMetadataSet)?;

        let num_channels = metadata.num_channels;

        // Check electrode labels
        if num_channels != self.config.electrode_labels.len() {
            return Err(Error::InvalidInput(format!(
                "Number of electrode labels ({}) does not match number of channels ({})",
                self.config.electrode_labels.len(),
                num_channels
            )));
        }

        // Add 1 for annotations channel if enabled
        let total_channels = if self.config.include_annotations {
            num_channels + 1
        } else {
            num_channels
        };

        let _channel_labels = metadata.channel_labels.clone();
        let start_time = metadata.start_time;

        // Format patient identification according to EDF+ spec
        let patient_id = format!(
            "{} {} {} {}",
            self.config.hospital_code,
            self.config.patient_sex,
            self.config
                .patient_birthdate
                .format("%d-%b-%Y")
                .to_string()
                .to_uppercase(),
            self.config.patient_name
        );

        // Format recording identification according to EDF+ spec
        let recording_id = format!(
            "Startdate {} {} {} {}",
            self.config
                .recording_start_date
                .format("%d-%b-%Y")
                .to_string()
                .to_uppercase(),
            self.config.hospital_code,
            self.config.recording_technician,
            self.config.recording_equipment
        );

        let header_bytes = 256 + (total_channels * 256);
        let samples_per_record =
            (metadata.sample_rate * DURATION_OF_RECORD as f64) as u32;

        // Write version
        self.write_str(EDF_VERSION, 8)?;

        // Write patient and recording IDs
        self.write_str(&patient_id, 80)?;
        self.write_str(&recording_id, 80)?;

        // Format date and time according to EDF spec (using Y2K-compatible format)
        let (date_str, time_str) = if let Some(time) = start_time {
            let year = time.year();
            let yy = if year >= 1985 && year <= 1999 {
                year - 1900
            } else if year >= 2000 && year <= 2084 {
                year - 2000
            } else {
                return Err(Error::InvalidData(
                    "Year must be between 1985 and 2084".to_string(),
                ));
            };
            (
                format!("{:02}.{:02}.{:02}", time.day(), time.month(), yy),
                format!(
                    "{:02}.{:02}.{:02}",
                    time.hour(),
                    time.minute(),
                    time.second()
                ),
            )
        } else {
            ("01.01.85".to_string(), "00.00.00".to_string())
        };

        self.write_str(&date_str, 8)?;
        self.write_str(&time_str, 8)?;
        self.write_num(header_bytes, 8)?;
        self.write_str("EDF+C", 44)?; // Changed from EDF to EDF+C to indicate continuous recording
        self.write_num(self.record_count, 8)?;
        self.write_float(DURATION_OF_RECORD as f64, 8)?;
        self.write_num(total_channels, 4)?;

        // Write channel labels (16 chars each)
        let labels = self.config.electrode_labels.clone();
        for label in labels {
            self.write_str(&label, 16)?;
        }

        // Write EDF Annotations label if enabled
        if self.config.include_annotations {
            self.write_str("EDF Annotations", 16)?;
        }

        // Write transducer type (80 chars each)
        for _ in 0..num_channels {
            self.write_str("AgAgCl electrode", 80)?;
        }

        // Write empty transducer type for annotations
        if self.config.include_annotations {
            self.write_str("", 80)?;
        }

        // Write physical dimension (8 chars each)
        for _ in 0..num_channels {
            self.write_str("mV", 8)?;
        }

        // Write empty physical dimension for annotations
        if self.config.include_annotations {
            self.write_str("", 8)?;
        }

        // Write physical min values
        for _ in 0..num_channels {
            self.write_float(metadata.physical_min / 1000.0, 8)?;
        }

        // Write physical min for annotations (just needs to differ from max)
        if self.config.include_annotations {
            self.write_float(-1.0, 8)?;
        }

        // Write physical max values
        for _ in 0..num_channels {
            self.write_float(metadata.physical_max / 1000.0, 8)?;
        }

        // Write physical max for annotations (just needs to differ from min)
        if self.config.include_annotations {
            self.write_float(1.0, 8)?;
        }

        // Write digital min values
        for _ in 0..num_channels {
            self.write_num(EDF_DIGITAL_MIN, 8)?;
        }

        // Write digital min for annotations
        if self.config.include_annotations {
            self.write_num(EDF_DIGITAL_MIN, 8)?;
        }

        // Write digital max values
        for _ in 0..num_channels {
            self.write_num(EDF_DIGITAL_MAX, 8)?;
        }

        // Write digital max for annotations
        if self.config.include_annotations {
            self.write_num(EDF_DIGITAL_MAX, 8)?;
        }

        // Write prefiltering fields
        for _ in 0..num_channels {
            self.write_str("", 80)?;
        }

        // Write empty prefiltering for annotations
        if self.config.include_annotations {
            self.write_str("", 80)?;
        }

        // Write samples per record
        for _ in 0..num_channels {
            self.write_num(samples_per_record, 8)?;
        }

        // Write samples per record for annotations
        if self.config.include_annotations {
            self.write_num(self.config.annotations_samples_per_record, 8)?;
        }

        // Write reserved fields
        for _ in 0..num_channels {
            self.write_str("", 32)?;
        }

        // Write empty reserved field for annotations
        if self.config.include_annotations {
            self.write_str("", 32)?;
        }

        Ok(())
    }

    fn write_data(&mut self, records: Vec<EegDataRecord>) -> Result<()> {
        let metadata =
            self.metadata.clone().ok_or_else(|| Error::NoMetadataSet)?;
        let num_channels = metadata.num_channels;
        let samples_per_record =
            (metadata.sample_rate * DURATION_OF_RECORD as f64) as usize;

        let mut channel_buffers: Vec<Vec<i32>> =
            vec![Vec::new(); num_channels];
        let mut total_samples = 0;

        // First, reorganize samples by channel
        for record in records.iter() {
            for (ch_idx, channel_samples) in record.samples.iter().enumerate()
            {
                channel_buffers[ch_idx].extend(channel_samples);
            }
            total_samples += record.samples[0].len();
        }

        // Now write complete records
        let num_complete_records = total_samples / samples_per_record;
        for record_idx in 0..num_complete_records {
            // Write all channels for this record
            for ch_buffer in &channel_buffers {
                let start = record_idx * samples_per_record;
                let end = start + samples_per_record;
                // Write samples for this channel
                for &value in &ch_buffer[start..end] {
                    let edf_value =
                        self.scale_to_edf_digital(value, &metadata);
                    self.writer.write_i16::<LittleEndian>(edf_value)?;
                }
            }

            // Write annotations channel if enabled
            if self.config.include_annotations {
                self.write_annotations_signal(record_idx)?;
            }

            self.record_count += 1;
        }

        // Handle any remaining samples
        let remaining_samples = total_samples % samples_per_record;
        if remaining_samples > 0 {
            // Write remaining samples for each channel
            for ch_buffer in &channel_buffers {
                let start = num_complete_records * samples_per_record;
                // Write remaining samples
                for &value in &ch_buffer[start..start + remaining_samples] {
                    let edf_value =
                        self.scale_to_edf_digital(value, &metadata);
                    self.writer.write_i16::<LittleEndian>(edf_value)?;
                }
                // Pad with zeros to complete the record
                for _ in 0..(samples_per_record - remaining_samples) {
                    self.writer.write_i16::<LittleEndian>(0)?;
                }
            }

            // Write annotations for the last partial record
            if self.config.include_annotations {
                self.write_annotations_signal(num_complete_records)?;
            }

            self.record_count += 1;
        }

        Ok(())
    }

    fn finalize(&mut self) -> Result<()> {
        // Update record count in header
        self.writer.seek(SeekFrom::Start(236))?;
        self.write_num(self.record_count + 1, 8)?;
        Ok(self.writer.flush()?)
    }
}
