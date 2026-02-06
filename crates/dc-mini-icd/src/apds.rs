use postcard_schema::Schema;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum LsGainRange {
    Gain1X,
    Gain3X,
    Gain6X,
    Gain9X,
    Gain18X,
}

impl From<u8> for LsGainRange {
    fn from(value: u8) -> Self {
        match value {
            x if x == Self::Gain1X as u8 => Self::Gain1X,
            x if x == Self::Gain3X as u8 => Self::Gain3X,
            x if x == Self::Gain6X as u8 => Self::Gain6X,
            x if x == Self::Gain9X as u8 => Self::Gain9X,
            x if x == Self::Gain18X as u8 => Self::Gain18X,
            _ => panic!("Invalid value for enum conversion"),
        }
    }
}

impl Into<u8> for LsGainRange {
    fn into(self) -> u8 {
        self as u8
    }
}

impl From<apds9253::LsGainRange> for LsGainRange {
    fn from(value: apds9253::LsGainRange) -> Self {
        match value {
            apds9253::LsGainRange::Gain1X => Self::Gain1X,
            apds9253::LsGainRange::Gain3X => Self::Gain3X,
            apds9253::LsGainRange::Gain6X => Self::Gain6X,
            apds9253::LsGainRange::Gain9X => Self::Gain9X,
            apds9253::LsGainRange::Gain18X => Self::Gain18X,
            _ => panic!("Reserved gain range value"),
        }
    }
}

impl From<LsGainRange> for apds9253::LsGainRange {
    fn from(value: LsGainRange) -> Self {
        match value {
            LsGainRange::Gain1X => apds9253::LsGainRange::Gain1X,
            LsGainRange::Gain3X => apds9253::LsGainRange::Gain3X,
            LsGainRange::Gain6X => apds9253::LsGainRange::Gain6X,
            LsGainRange::Gain9X => apds9253::LsGainRange::Gain9X,
            LsGainRange::Gain18X => apds9253::LsGainRange::Gain18X,
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum LsResolution {
    Bits20400Ms,
    Bits19200Ms,
    Bits18100Ms,
    Bits1750Ms,
    Bits1625Ms,
    Bits133125Ms,
}

impl From<u8> for LsResolution {
    fn from(value: u8) -> Self {
        match value {
            x if x == Self::Bits20400Ms as u8 => Self::Bits20400Ms,
            x if x == Self::Bits19200Ms as u8 => Self::Bits19200Ms,
            x if x == Self::Bits18100Ms as u8 => Self::Bits18100Ms,
            x if x == Self::Bits1750Ms as u8 => Self::Bits1750Ms,
            x if x == Self::Bits1625Ms as u8 => Self::Bits1625Ms,
            x if x == Self::Bits133125Ms as u8 => Self::Bits133125Ms,
            _ => panic!("Invalid value for enum conversion"),
        }
    }
}

impl Into<u8> for LsResolution {
    fn into(self) -> u8 {
        self as u8
    }
}

impl From<apds9253::LsResolution> for LsResolution {
    fn from(value: apds9253::LsResolution) -> Self {
        match value {
            apds9253::LsResolution::Bits20400Ms => Self::Bits20400Ms,
            apds9253::LsResolution::Bits19200Ms => Self::Bits19200Ms,
            apds9253::LsResolution::Bits18100Ms => Self::Bits18100Ms,
            apds9253::LsResolution::Bits1750Ms => Self::Bits1750Ms,
            apds9253::LsResolution::Bits1625Ms => Self::Bits1625Ms,
            apds9253::LsResolution::Bits133125Ms => Self::Bits133125Ms,
            _ => panic!("Reserved resolution value"),
        }
    }
}

impl From<LsResolution> for apds9253::LsResolution {
    fn from(value: LsResolution) -> Self {
        match value {
            LsResolution::Bits20400Ms => apds9253::LsResolution::Bits20400Ms,
            LsResolution::Bits19200Ms => apds9253::LsResolution::Bits19200Ms,
            LsResolution::Bits18100Ms => apds9253::LsResolution::Bits18100Ms,
            LsResolution::Bits1750Ms => apds9253::LsResolution::Bits1750Ms,
            LsResolution::Bits1625Ms => apds9253::LsResolution::Bits1625Ms,
            LsResolution::Bits133125Ms => apds9253::LsResolution::Bits133125Ms,
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum LsMeasurementRate {
    Ms25,
    Ms50,
    Ms100,
    Ms200,
    Ms500,
    Ms1000,
}

impl From<u8> for LsMeasurementRate {
    fn from(value: u8) -> Self {
        match value {
            x if x == Self::Ms25 as u8 => Self::Ms25,
            x if x == Self::Ms50 as u8 => Self::Ms50,
            x if x == Self::Ms100 as u8 => Self::Ms100,
            x if x == Self::Ms200 as u8 => Self::Ms200,
            x if x == Self::Ms500 as u8 => Self::Ms500,
            x if x == Self::Ms1000 as u8 => Self::Ms1000,
            _ => panic!("Invalid value for enum conversion"),
        }
    }
}

impl Into<u8> for LsMeasurementRate {
    fn into(self) -> u8 {
        self as u8
    }
}

impl From<apds9253::LsMeasurementRate> for LsMeasurementRate {
    fn from(value: apds9253::LsMeasurementRate) -> Self {
        match value {
            apds9253::LsMeasurementRate::Ms25 => Self::Ms25,
            apds9253::LsMeasurementRate::Ms50 => Self::Ms50,
            apds9253::LsMeasurementRate::Ms100 => Self::Ms100,
            apds9253::LsMeasurementRate::Ms200 => Self::Ms200,
            apds9253::LsMeasurementRate::Ms500 => Self::Ms500,
            apds9253::LsMeasurementRate::Ms1000 => Self::Ms1000,
            _ => panic!("Reserved measurement rate value"),
        }
    }
}

impl From<LsMeasurementRate> for apds9253::LsMeasurementRate {
    fn from(value: LsMeasurementRate) -> Self {
        match value {
            LsMeasurementRate::Ms25 => apds9253::LsMeasurementRate::Ms25,
            LsMeasurementRate::Ms50 => apds9253::LsMeasurementRate::Ms50,
            LsMeasurementRate::Ms100 => apds9253::LsMeasurementRate::Ms100,
            LsMeasurementRate::Ms200 => apds9253::LsMeasurementRate::Ms200,
            LsMeasurementRate::Ms500 => apds9253::LsMeasurementRate::Ms500,
            LsMeasurementRate::Ms1000 => apds9253::LsMeasurementRate::Ms1000,
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ApdsConfig {
    pub gain: LsGainRange,
    pub resolution: LsResolution,
    pub measurement_rate: LsMeasurementRate,
    pub rgb_mode: bool,
}

impl Default for ApdsConfig {
    fn default() -> Self {
        Self {
            gain: LsGainRange::Gain3X,
            resolution: LsResolution::Bits18100Ms,
            measurement_rate: LsMeasurementRate::Ms100,
            rgb_mode: true,
        }
    }
}

pub fn default_apds_settings() -> ApdsConfig {
    ApdsConfig::default()
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Schema)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ApdsDataFrame {
    pub red: u32,
    pub green: u32,
    pub blue: u32,
    pub ir: u32,
    pub lux: f32,
    pub cct: u16,
    pub cie_x: f32,
    pub cie_y: f32,
}
