use ads1299;
use alloc::vec::Vec;
use postcard_schema::Schema;
use serde::{Deserialize, Serialize};

pub const ADS_MAX_CHANNELS: usize = 16;

define_config_enum!(
    SampleRate,
    ads1299::SampleRate,
    {
        Sps250,
        Sps500,
        KSps1,
        KSps2,
        KSps4,
        KSps8,
        KSps16
    }
);

define_config_enum!(
    CompThreshPos,
    ads1299::CompThreshPos,
    {
        _95,
        _92_5,
        _90,
        _87_5,
        _85,
        _80,
        _75,
        _70,
    }
);

define_config_enum!(
    CalFreq,
    ads1299::CalFreq,
    {
        FclkBy21,
        FclkBy20,
        DoNotUse,
        DC,
    }
);

define_config_enum!(
    Gain,
    ads1299::Gain,
    {
        X1,
        X2,
        X4,
        X6,
        X8,
        X12,
        X24,
    }
);

define_config_enum!(
    Mux,
    ads1299::Mux,
    {
        NormalElectrodeInput,
        InputShorted,
        RldMeasure,
        MVDD,
        TemperatureSensor,
        TestSignal,
        RldDrp,
        RldDrn,
    }
);

define_config_enum!(
    ILeadOff,
    ads1299::ILeadOff,
    {
        _6nA,
        _24nA,
        _6uA,
        _24uA,
    }
);

define_config_enum!(
    FLeadOff,
    ads1299::FLeadOff,
    {
        Dc,
        Ac7_8,
        Ac31_2,
        AcFdrBy4,
    }
);

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ChannelConfig {
    pub power_down: bool,
    pub gain: Gain,
    pub srb2: bool,
    pub mux: Mux,
    pub bias_sensp: bool,
    pub bias_sensn: bool,
    pub lead_off_sensp: bool,
    pub lead_off_sensn: bool,
    pub lead_off_flip: bool,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct AdsConfig {
    pub daisy_en: bool, // Active low!
    pub clk_en: bool,
    pub sample_rate: SampleRate,
    pub internal_calibration: bool,
    pub calibration_amplitude: bool,
    pub calibration_frequency: CalFreq,
    pub pd_refbuf: bool, // Active low!
    pub bias_meas: bool,
    pub biasref_int: bool,
    pub pd_bias: bool, // Active low!
    pub bias_loff_sens: bool,
    pub bias_stat: bool,
    pub comparator_threshold_pos: CompThreshPos,
    pub lead_off_current: ILeadOff,
    pub lead_off_frequency: FLeadOff,
    pub gpioc: [bool; 4],
    pub srb1: bool,
    pub single_shot: bool,
    pub pd_loff_comp: bool, // Active low!
    pub channels: heapless::Vec<ChannelConfig, ADS_MAX_CHANNELS>,
}

#[derive(Serialize, Deserialize, Schema, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct AdsSample {
    pub lead_off_positive: u32,
    pub lead_off_negative: u32,
    pub gpio: u32,
    pub data: Vec<i32>,
}

#[derive(Serialize, Deserialize, Schema, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct AdsDataFrame {
    pub ts: u64,
    pub samples: Vec<AdsSample>,
}

impl Default for AdsConfig {
    fn default() -> Self {
        Self {
            daisy_en: false,
            clk_en: false,
            sample_rate: SampleRate::Sps250,
            internal_calibration: false,
            calibration_amplitude: false,
            calibration_frequency: CalFreq::FclkBy21,
            pd_refbuf: false,
            bias_meas: false,
            biasref_int: false,
            pd_bias: false,
            bias_loff_sens: false,
            bias_stat: false,
            comparator_threshold_pos: CompThreshPos::_95,
            lead_off_current: ILeadOff::_6nA,
            lead_off_frequency: FLeadOff::Dc,
            gpioc: [false; 4],
            srb1: false,
            single_shot: false,
            pd_loff_comp: false,
            channels: heapless::Vec::new(),
        }
    }
}
