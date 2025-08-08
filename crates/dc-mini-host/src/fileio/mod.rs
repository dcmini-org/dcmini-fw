use derive_more::{Display, From};
use std::io;
use std::path::PathBuf;

pub mod dat;
pub mod edf;

use edf::EdfConfig;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, From, Display)]
pub enum Error {
    NoMetadataSet,
    #[from(skip)]
    InvalidData(String),
    #[from(skip)]
    InvalidInput(String),
    NotFound(&'static str),

    // External
    IoError(io::Error),
    ProstError(prost::DecodeError),
    Egui(eframe::Error),
    SerdeJson(serde_json::Error),
}

/// Configuration for file conversion
#[derive(Debug, Clone)]
pub enum ConversionConfig {
    Edf { input_path: PathBuf, output_path: PathBuf, config: EdfConfig },
}

impl ConversionConfig {
    pub fn input_path(&self) -> &PathBuf {
        match self {
            ConversionConfig::Edf { input_path, .. } => input_path,
            // Add arms for other formats
        }
    }

    pub fn output_path(&self) -> &PathBuf {
        match self {
            ConversionConfig::Edf { output_path, .. } => output_path,
            // Add arms for other formats
        }
    }
}

/// Common trait for all file writers that can write EEG data
pub trait EegWriter {
    fn set_metadata(&mut self, metadata: EegMetadata);
    fn write_header(&mut self) -> Result<()>;
    fn write_data(&mut self, records: Vec<EegDataRecord>) -> Result<()>;
    fn finalize(&mut self) -> Result<()>;
}

/// Common trait for all file readers that can read EEG data
pub trait EegReader {
    fn read_header(&mut self) -> Result<EegMetadata>;
    fn read_data(&mut self) -> Result<Vec<EegDataRecord>>;
}

/// Metadata common to all EEG file formats
#[derive(Debug, Clone)]
pub struct EegMetadata {
    pub num_channels: usize,
    pub sample_rate: f64,
    pub channel_labels: Vec<String>,
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub patient_id: Option<String>,
    pub recording_id: Option<String>,
    pub bit_depth: u8, // Bit depth of the raw data (e.g. 24 for ADS1299)
    pub physical_min: f64, // Physical minimum value in microvolts
    pub physical_max: f64, // Physical maximum value in microvolts
    pub conversion_factor: f64, // Factor to convert from digital to physical units (microvolts)
}

/// Single data record containing samples for all channels
#[derive(Debug, Clone)]
pub struct EegDataRecord {
    pub timestamp: Option<f64>,
    pub samples: Vec<Vec<i32>>, // Raw digital samples for each channel (signed)
}

/// Trait for converting between digital and physical units
pub trait PhysicalUnitConversion {
    fn to_physical_units(&self, digital_value: i32) -> f64;
    fn from_physical_units(&self, physical_value: f64) -> i32;
}

impl PhysicalUnitConversion for EegMetadata {
    fn to_physical_units(&self, digital_value: i32) -> f64 {
        digital_value as f64 * self.conversion_factor
    }

    fn from_physical_units(&self, physical_value: f64) -> i32 {
        (physical_value / self.conversion_factor) as i32
    }
}

/// Factory function to create appropriate writer based on file extension
pub fn create_writer(config: &ConversionConfig) -> Result<Box<dyn EegWriter>> {
    match config {
        ConversionConfig::Edf { .. } => {
            Ok(Box::new(edf::EdfWriter::new(config)?))
        } // Add arms for other formats
    }
}

/// Factory function to create appropriate reader based on file extension
pub fn create_reader(path: &PathBuf) -> Result<Box<dyn EegReader>> {
    // If there's no extension, treat it as a .dat file
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("dat");

    match ext.to_lowercase().as_str() {
        "dat" => Ok(Box::new(dat::DatReader::new(path)?)),
        _ => Err(Error::InvalidInput(format!(
            "Unsupported input format: {}. Only DAT format is supported.",
            ext
        ))),
    }
}
