use core;

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error<SpiE> {
    SpiError(SpiE),
    RegisterError(ADS1299RegisterError),
}

impl<E: core::fmt::Display> core::fmt::Display for Error<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::SpiError(err) => {
                write!(f, "SPI communication error: {}", err)
            }
            Error::RegisterError(value) => {
                write!(f, "Register Error: {}", value)
            }
        }
    }
}

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ADS1299RegisterError {
    InvalidSamplingRate(u8),
    InvalidCalibrationFrequency(u8),
    InvalidChannelCount(u8),
    InvalidComparatorThreshold(u8),
    InvalidLeadOffCurrent(u8),
    InvalidLeadOffFrequency(u8),
    AdsNotDetected,
}
impl core::fmt::Display for ADS1299RegisterError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ADS1299RegisterError::InvalidSamplingRate(value) => {
                write!(f, "Invalid sampling rate value: {}", value)
            }
            ADS1299RegisterError::InvalidCalibrationFrequency(value) => {
                write!(f, "Invalid calibration frequency value: {}", value)
            }
            ADS1299RegisterError::InvalidChannelCount(value) => {
                write!(f, "Invalid channel count value: {}", value)
            }
            ADS1299RegisterError::InvalidComparatorThreshold(value) => {
                write!(f, "Invalid comparator threshold value: {}", value)
            }
            ADS1299RegisterError::InvalidLeadOffCurrent(value) => {
                write!(f, "Invalid lead off current value: {}", value)
            }
            ADS1299RegisterError::InvalidLeadOffFrequency(value) => {
                write!(f, "Invalid lead off frequency value: {}", value)
            }
            ADS1299RegisterError::AdsNotDetected => {
                write!(f, "Ads not detected!")
            }
        }
    }
}

impl<SpiE> From<ADS1299RegisterError> for Error<SpiE> {
    fn from(e: ADS1299RegisterError) -> Self {
        Error::RegisterError(e)
    }
}
