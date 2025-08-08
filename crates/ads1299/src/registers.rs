use super::errors::ADS1299RegisterError;
use bitflags::bitflags;

#[derive(Debug, Copy, Clone)]
pub enum Command {
    WAKEUP,
    STANDBY,
    RESET,
    START,
    STOP,
    RDATAC,
    SDATAC,
    RDATA,
    RREG(u8, u8),
    WREG(u8, u8),
}

impl From<Command> for ([u8; 2], usize) {
    fn from(val: Command) -> Self {
        match val {
            Command::WAKEUP => ([0x02, 0], 1),
            Command::STANDBY => ([0x04, 0], 1),
            Command::RESET => ([0x06, 0], 1),
            Command::START => ([0x08, 0], 1),
            Command::STOP => ([0x0A, 0], 1),
            Command::RDATAC => ([0x10, 0], 1),
            Command::SDATAC => ([0x11, 0], 1),
            Command::RDATA => ([0x12, 0], 1),
            Command::RREG(reg, len) => ([0x20 | reg, len - 1], 2),
            Command::WREG(reg, len) => ([0x40 | reg, len - 1], 2),
        }
    }
}

/// Configuration enums
#[derive(Debug, Copy, Clone, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum SampleRate {
    #[default]
    Sps250,
    Sps500,
    KSps1,
    KSps2,
    KSps4,
    KSps8,
    KSps16,
}

#[derive(Debug, Copy, Clone, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum CalFreq {
    #[default]
    FclkBy21,
    FclkBy20,
    DoNotUse,
    DC,
}

#[derive(Debug, Copy, Clone, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum CompThreshPos {
    #[default]
    _95,
    _92_5,
    _90,
    _87_5,
    _85,
    _80,
    _75,
    _70,
}

#[derive(Debug, Copy, Clone, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ILeadOff {
    #[default]
    _6nA,
    _24nA,
    _6uA,
    _24uA,
}

#[derive(Debug, Copy, Clone, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum FLeadOff {
    #[default]
    Dc,
    Ac7_8,
    Ac31_2,
    AcFdrBy4,
}

#[derive(Debug, Copy, Clone, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Gain {
    #[default]
    X1,
    X2,
    X4,
    X6,
    X8,
    X12,
    X24,
}

#[derive(Debug, Copy, Clone, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Mux {
    #[default]
    NormalElectrodeInput,
    InputShorted,
    RldMeasure,
    MVDD,
    TemperatureSensor,
    TestSignal,
    RldDrp,
    RldDrn,
}

///
/// Read / write-able registers
///
#[allow(non_camel_case_types)]
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum Register {
    /// ID Control Register (Factory-Programmed, Read-Only)
    ID = 0x00,
    /// Configuration Register 1
    CONFIG1 = 0x01,
    /// Configuration Register 2
    CONFIG2 = 0x02,
    /// Configuration Register 3
    CONFIG3 = 0x03,
    /// Lead-Off Control Register
    LOFF = 0x04,
    /// Channel 1 Settings
    CH1SET = 0x05,
    /// Channel 2 Settings
    CH2SET = 0x06,
    /// Channel 3 Settings
    CH3SET = 0x07,
    /// Channel 4 Settings
    CH4SET = 0x08,
    /// Channel 5 Settings
    CH5SET = 0x09,
    /// Channel 6 Settings
    CH6SET = 0x0A,
    /// Channel 7 Settings
    CH7SET = 0x0B,
    /// Channel 8 Settings
    CH8SET = 0x0C,
    /// Bias Sense Positive
    BIAS_SENSP = 0x0D,
    /// Bias Sense Negative
    BIAS_SENSN = 0x0E,
    /// Lead-Off Sense Positive
    LOFF_SENSP = 0x0F,
    /// Lead-Off Sense Negative
    LOFF_SENSN = 0x10,
    /// Lead-Off Flip
    LOFF_FLIP = 0x11,
    /// Lead-Off Stat Positive
    LOFF_STATP = 0x12,
    /// Lead-Off Stat Negative
    LOFF_STATN = 0x13,
    /// General-Purpose I/O Register
    GPIO = 0x14,
    /// Miscellaneous 1 Register
    MISC1 = 0x15,
    /// Miscellaneous 2 Register
    MISC2 = 0x16,
    /// Configuration Register 4
    CONFIG4 = 0x17,
}

impl Register {
    pub fn from_channel_number(ch: u8) -> Self {
        match ch {
            0 => Self::CH1SET,
            1 => Self::CH2SET,
            2 => Self::CH3SET,
            3 => Self::CH4SET,
            4 => Self::CH5SET,
            5 => Self::CH6SET,
            6 => Self::CH7SET,
            7..=u8::MAX => Self::CH8SET,
        }
    }
}

bitflags! {
    /// ID
    #[derive(Debug, Copy, Clone)]
    pub struct Id: u8 {
        const REV_ID2 = 0b1000_0000;
        const REV_ID1 = 0b0100_0000;
        const REV_ID0 = 0b0010_0000;
        const DEV_ID1 = 0b0000_1000;
        const DEV_ID0 = 0b0000_0100;
        const NU_CH1  = 0b0000_0010;
        const NU_CH0  = 0b0000_0001;

        const REV_ID = Self::REV_ID2.bits() | Self::REV_ID1.bits() | Self::REV_ID0.bits();
        const DEV_ID = Self::DEV_ID1.bits() | Self::DEV_ID0.bits();
        const NU_CH = Self::NU_CH1.bits() | Self::NU_CH0.bits();
    }
}

impl Id {
    pub const fn num_chs(&self) -> Result<u8, ADS1299RegisterError> {
        let channel_count = match self.intersection(Self::NU_CH).bits() {
            0b00 => 4,
            0b01 => 6,
            0b10 => 8,
            e => return Err(ADS1299RegisterError::InvalidChannelCount(e)),
        };
        Ok(channel_count)
    }

    pub const fn smell(&self) -> Result<(), ADS1299RegisterError> {
        // First, check if channel count is valid.
        match self.num_chs() {
            Ok(_) => {}
            Err(_) => return Err(ADS1299RegisterError::AdsNotDetected),
        }
        // If Ok, make sure device ID bits match as well.
        match self.intersection(Self::DEV_ID).bits() >> 2 {
            0b11 => Ok(()),
            _ => Err(ADS1299RegisterError::AdsNotDetected),
        }
    }
}

bitflags! {
    /// CONFIG1
    #[derive(Debug, Copy, Clone)]
    pub struct Config1: u8 {
        const DAISY_EN = 0b0100_0000;
        const CLK_EN   = 0b0010_0000;
        const DR2      = 0b0000_0100;
        const DR1      = 0b0000_0010;
        const DR0      = 0b0000_0001;

        const DR = Self::DR2.bits() | Self::DR1.bits() | Self::DR0.bits();
    }
}

impl Default for Config1 {
    fn default() -> Config1 {
        Self::from_bits_retain(0x96)
    }
}

impl Config1 {
    pub const fn odr(&self) -> Result<SampleRate, ADS1299RegisterError> {
        let sample_rate = match self.intersection(Self::DR).bits() {
            0b000 => SampleRate::KSps16,
            0b001 => SampleRate::KSps8,
            0b010 => SampleRate::KSps4,
            0b011 => SampleRate::KSps2,
            0b100 => SampleRate::KSps1,
            0b101 => SampleRate::Sps500,
            0b110 => SampleRate::Sps250,
            e => return Err(ADS1299RegisterError::InvalidSamplingRate(e)),
        };
        Ok(sample_rate)
    }

    pub const fn with_odr(self, sample_rate: SampleRate) -> Self {
        let reg = self.difference(Self::DR);
        match sample_rate {
            SampleRate::KSps16 => reg,
            SampleRate::KSps8 => reg.union(Self::DR0),
            SampleRate::KSps4 => reg.union(Self::DR1),
            SampleRate::KSps2 => reg.union(Self::DR1).union(Self::DR0),
            SampleRate::KSps1 => reg.union(Self::DR2),
            SampleRate::Sps500 => reg.union(Self::DR2).union(Self::DR0),
            SampleRate::Sps250 => reg.union(Self::DR2).union(Self::DR1),
        }
    }

    pub const fn clk_en(&self) -> bool {
        self.contains(Self::CLK_EN)
    }

    pub const fn with_clk_en(self, en: bool) -> Self {
        let reg = self.difference(Self::CLK_EN);
        match en {
            false => reg,
            true => reg.union(Self::CLK_EN),
        }
    }

    pub const fn daisy_en(&self) -> bool {
        self.contains(Self::DAISY_EN)
    }

    pub const fn with_daisy_en(self, en: bool) -> Self {
        let reg = self.difference(Self::DAISY_EN);
        match en {
            false => reg,
            true => reg.union(Self::DAISY_EN),
        }
    }
}

bitflags! {
    /// CONFIG2
    #[derive(Debug, Copy, Clone)]
    pub struct Config2: u8 {
        const INT_CAL   = 0b0001_0000;
        const CAL_AMP   = 0b0000_0100;
        const CAL_FREQ1 = 0b0000_0010;
        const CAL_FREQ0 = 0b0000_0001;

        const CAL_FREQ = Self::CAL_FREQ1.bits() | Self::CAL_FREQ0.bits();
    }
}

impl Default for Config2 {
    fn default() -> Self {
        Self::from_bits_retain(0xC0)
    }
}

impl Config2 {
    pub const fn int_cal(&self) -> bool {
        self.contains(Self::INT_CAL)
    }

    pub const fn with_int_cal(self, int_cal: bool) -> Self {
        let reg = self.difference(Self::INT_CAL);
        match int_cal {
            false => reg,
            true => reg.union(Self::INT_CAL),
        }
    }

    pub const fn cal_amp(&self) -> bool {
        self.contains(Self::CAL_AMP)
    }

    pub const fn with_cal_amp(self, cal_amp: bool) -> Self {
        let reg = self.difference(Self::CAL_AMP);
        match cal_amp {
            false => reg,
            true => reg.union(Self::CAL_AMP),
        }
    }
    pub const fn cal_freq(&self) -> Result<CalFreq, ADS1299RegisterError> {
        let cal_freq = match self.intersection(Self::CAL_FREQ).bits() {
            0b00 => CalFreq::FclkBy21,
            0b01 => CalFreq::FclkBy20,
            0b10 => CalFreq::DoNotUse,
            0b11 => CalFreq::DC,
            e => {
                return Err(ADS1299RegisterError::InvalidCalibrationFrequency(
                    e,
                ))
            }
        };

        Ok(cal_freq)
    }

    pub const fn with_cal_freq(self, cal_freq: CalFreq) -> Self {
        let reg = self.difference(Self::CAL_FREQ);
        match cal_freq {
            CalFreq::FclkBy21 => reg,
            CalFreq::FclkBy20 => reg.union(Self::CAL_FREQ0),
            CalFreq::DoNotUse => reg.union(Self::CAL_FREQ1),
            CalFreq::DC => reg.union(Self::CAL_FREQ),
        };
        reg
    }
}

bitflags! {
    /// CONFIG3
    #[derive(Debug, Copy, Clone)]
    pub struct Config3: u8 {
        const PD_REFBUF      = 0b1000_0000;
        const BIAS_MEAS      = 0b0001_0000;
        const BIASREF_INT    = 0b0000_1000;
        const PD_BIAS        = 0b0000_0100;
        const BIAS_LOFF_SENS = 0b0000_0010;
        const BIAS_STAT      = 0b0000_0001;
    }
}

impl Default for Config3 {
    fn default() -> Self {
        Self::from_bits_retain(0x60)
    }
}

impl Config3 {
    /// Check if the reference buffer is powered down
    pub const fn pd_refbuf(&self) -> bool {
        self.contains(Self::PD_REFBUF)
    }

    /// Set the power-down state of the reference buffer
    pub const fn with_pd_refbuf(self, pd_refbuf: bool) -> Self {
        let reg = self.difference(Self::PD_REFBUF);
        match pd_refbuf {
            false => reg,
            true => reg.union(Self::PD_REFBUF),
        }
    }

    /// Check if bias measurement is enabled
    pub const fn bias_meas(&self) -> bool {
        self.contains(Self::BIAS_MEAS)
    }

    /// Enable or disable bias measurement
    pub const fn with_bias_meas(self, bias_meas: bool) -> Self {
        let reg = self.difference(Self::BIAS_MEAS);
        match bias_meas {
            false => reg,
            true => reg.union(Self::BIAS_MEAS),
        }
    }

    /// Check if internal bias reference is used
    pub const fn biasref_int(&self) -> bool {
        self.contains(Self::BIASREF_INT)
    }

    /// Set the internal bias reference usage
    pub const fn with_biasref_int(self, biasref_int: bool) -> Self {
        let reg = self.difference(Self::BIASREF_INT);
        match biasref_int {
            false => reg,
            true => reg.union(Self::BIASREF_INT),
        }
    }

    /// Check if bias drive is powered down
    pub const fn pd_bias(&self) -> bool {
        self.contains(Self::PD_BIAS)
    }

    /// Set the power-down state of the bias drive
    pub const fn with_pd_bias(self, pd_bias: bool) -> Self {
        let reg = self.difference(Self::PD_BIAS);
        match pd_bias {
            false => reg,
            true => reg.union(Self::PD_BIAS),
        }
    }

    /// Check if bias lead-off sensing is enabled
    pub const fn bias_loff_sens(&self) -> bool {
        self.contains(Self::BIAS_LOFF_SENS)
    }

    /// Enable or disable bias lead-off sensing
    pub const fn with_bias_loff_sens(self, bias_loff_sens: bool) -> Self {
        let reg = self.difference(Self::BIAS_LOFF_SENS);
        match bias_loff_sens {
            false => reg,
            true => reg.union(Self::BIAS_LOFF_SENS),
        }
    }

    /// Check if bias status is asserted
    pub const fn bias_stat(&self) -> bool {
        self.contains(Self::BIAS_STAT)
    }

    /// Set the bias status
    pub const fn with_bias_stat(self, bias_stat: bool) -> Self {
        let reg = self.difference(Self::BIAS_STAT);
        match bias_stat {
            false => reg,
            true => reg.union(Self::BIAS_STAT),
        }
    }
}

bitflags! {
    /// LOFF
    #[derive(Debug, Copy, Clone)]
    pub struct Loff: u8 {
        const COMP_TH2   = 0b1000_0000;
        const COMP_TH1   = 0b0100_0000;
        const COMP_TH0   = 0b0010_0000;
        const ILEAD_OFF1 = 0b0000_1000;
        const ILEAD_OFF0 = 0b0000_0100;
        const FLEAD_OFF1 = 0b0000_0010;
        const FLEAD_OFF0 = 0b0000_0001;

        const COMP_TH = Self::COMP_TH2.bits() | Self::COMP_TH1.bits() | Self::COMP_TH0.bits();
        const ILEAD_OFF = Self::ILEAD_OFF1.bits() | Self::ILEAD_OFF0.bits();
        const FLEAD_OFF = Self::FLEAD_OFF1.bits() | Self::FLEAD_OFF0.bits();
    }
}

impl Loff {
    pub const fn comp_th(
        &self,
    ) -> Result<CompThreshPos, ADS1299RegisterError> {
        let comp_th = match self.intersection(Self::COMP_TH).bits() {
            0b000 => CompThreshPos::_95,
            0b001 => CompThreshPos::_92_5,
            0b010 => CompThreshPos::_90,
            0b011 => CompThreshPos::_87_5,
            0b100 => CompThreshPos::_85,
            0b101 => CompThreshPos::_80,
            0b110 => CompThreshPos::_75,
            0b111 => CompThreshPos::_70,
            e => {
                return Err(ADS1299RegisterError::InvalidComparatorThreshold(
                    e,
                ))
            }
        };

        Ok(comp_th)
    }

    pub const fn with_comp_th(self, comp_th: CompThreshPos) -> Self {
        let reg = self.difference(Self::COMP_TH);
        match comp_th {
            CompThreshPos::_95 => reg,
            CompThreshPos::_92_5 => reg.union(Self::COMP_TH0),
            CompThreshPos::_90 => reg.union(Self::COMP_TH1),
            CompThreshPos::_87_5 => {
                reg.union(Self::COMP_TH1).union(Self::COMP_TH0)
            }
            CompThreshPos::_85 => reg.union(Self::COMP_TH2),
            CompThreshPos::_80 => {
                reg.union(Self::COMP_TH2).union(Self::COMP_TH0)
            }
            CompThreshPos::_75 => {
                reg.union(Self::COMP_TH2).union(Self::COMP_TH1)
            }
            CompThreshPos::_70 => reg
                .union(Self::COMP_TH2)
                .union(Self::COMP_TH1)
                .union(Self::COMP_TH0),
        }
    }

    pub const fn ilead_off(&self) -> Result<ILeadOff, ADS1299RegisterError> {
        let ilead_off = match self.intersection(Self::ILEAD_OFF).bits() {
            0b00 => ILeadOff::_6nA,
            0b01 => ILeadOff::_24nA,
            0b10 => ILeadOff::_6uA,
            0b11 => ILeadOff::_24uA,
            e => return Err(ADS1299RegisterError::InvalidLeadOffCurrent(e)),
        };

        Ok(ilead_off)
    }

    pub const fn with_ilead_off(self, ilead_off: ILeadOff) -> Self {
        let reg = self.difference(Self::ILEAD_OFF);
        match ilead_off {
            ILeadOff::_6nA => reg,
            ILeadOff::_24nA => reg.union(Self::ILEAD_OFF0),
            ILeadOff::_6uA => reg.union(Self::ILEAD_OFF1),
            ILeadOff::_24uA => {
                reg.union(Self::ILEAD_OFF1).union(Self::ILEAD_OFF0)
            }
        }
    }

    pub const fn flead_off(&self) -> Result<FLeadOff, ADS1299RegisterError> {
        let flead_off = match self.intersection(Self::FLEAD_OFF).bits() {
            0b00 => FLeadOff::Dc,
            0b01 => FLeadOff::Ac7_8,
            0b10 => FLeadOff::Ac31_2,
            0b11 => FLeadOff::AcFdrBy4,
            e => return Err(ADS1299RegisterError::InvalidLeadOffFrequency(e)),
        };

        Ok(flead_off)
    }

    pub const fn with_flead_off(self, flead_off: FLeadOff) -> Self {
        let reg = self.difference(Self::FLEAD_OFF);
        match flead_off {
            FLeadOff::Dc => reg,
            FLeadOff::Ac7_8 => reg.union(Self::FLEAD_OFF0),
            FLeadOff::Ac31_2 => reg.union(Self::FLEAD_OFF1),
            FLeadOff::AcFdrBy4 => {
                reg.union(Self::FLEAD_OFF1).union(Self::FLEAD_OFF0)
            }
        }
    }
}

impl Default for Loff {
    fn default() -> Self {
        Self::from_bits_retain(0x00)
    }
}

bitflags! {
    /// ChSet
    #[derive(Debug, Copy, Clone)]
    pub struct ChSet: u8 {
        const PD    = 0b1000_0000;
        const GAIN2 = 0b0100_0000;
        const GAIN1 = 0b0010_0000;
        const GAIN0 = 0b0001_0000;
        const SRB2  = 0b0000_1000;
        const MUX2  = 0b0000_0100;
        const MUX1  = 0b0000_0010;
        const MUX0  = 0b0000_0001;

        const GAIN = Self::GAIN2.bits() | Self::GAIN1.bits() | Self::GAIN0.bits();
        const MUX = Self::MUX2.bits() | Self::MUX1.bits() | Self::MUX0.bits();
    }
}

impl Default for ChSet {
    fn default() -> Self {
        Self::from_bits_retain(0x61)
    }
}

impl ChSet {
    pub const fn pd(&self) -> bool {
        self.contains(Self::PD)
    }

    pub const fn with_pd(self, en: bool) -> Self {
        let reg = self.difference(Self::PD);
        match en {
            false => reg,
            true => reg.union(Self::PD),
        }
    }

    pub const fn srb2(&self) -> bool {
        self.contains(Self::SRB2)
    }

    pub const fn with_srb2(self, srb2: bool) -> Self {
        let reg = self.difference(Self::SRB2);
        match srb2 {
            false => reg,
            true => reg.union(Self::SRB2),
        }
    }

    pub const fn mux(&self) -> Result<Mux, ADS1299RegisterError> {
        let mux = match self.intersection(Self::MUX).bits() {
            0b000 => Mux::NormalElectrodeInput,
            0b001 => Mux::InputShorted,
            0b010 => Mux::RldMeasure,
            0b011 => Mux::MVDD,
            0b100 => Mux::TemperatureSensor,
            0b101 => Mux::TestSignal,
            0b110 => Mux::RldDrp,
            0b111 => Mux::RldDrn,
            e => return Err(ADS1299RegisterError::InvalidSamplingRate(e)),
        };
        Ok(mux)
    }

    pub const fn with_mux(self, mux: Mux) -> Self {
        let reg = self.difference(Self::MUX);
        match mux {
            Mux::NormalElectrodeInput => reg,
            Mux::InputShorted => reg.union(Self::MUX0),
            Mux::RldMeasure => reg.union(Self::MUX1),
            Mux::MVDD => reg.union(Self::MUX1).union(Self::MUX0),
            Mux::TemperatureSensor => reg.union(Self::MUX2),
            Mux::TestSignal => reg.union(Self::MUX2).union(Self::MUX0),
            Mux::RldDrp => reg.union(Self::MUX2).union(Self::MUX1),
            Mux::RldDrn => reg.union(Self::MUX),
        }
    }

    pub const fn gain(&self) -> Result<Gain, ADS1299RegisterError> {
        let gain = match self.intersection(Self::GAIN).bits() >> 4 {
            0b000 => Gain::X1,
            0b001 => Gain::X2,
            0b010 => Gain::X4,
            0b011 => Gain::X6,
            0b100 => Gain::X8,
            0b101 => Gain::X12,
            0b110 => Gain::X24,
            e => return Err(ADS1299RegisterError::InvalidSamplingRate(e)),
        };
        Ok(gain)
    }

    pub const fn with_gain(self, gain: Gain) -> Self {
        let reg = self.difference(Self::GAIN);
        match gain {
            Gain::X1 => reg,
            Gain::X2 => reg.union(Self::GAIN0),
            Gain::X4 => reg.union(Self::GAIN1),
            Gain::X6 => reg.union(Self::GAIN1).union(Self::GAIN0),
            Gain::X8 => reg.union(Self::GAIN2),
            Gain::X12 => reg.union(Self::GAIN2).union(Self::GAIN0),
            Gain::X24 => reg.union(Self::GAIN2).union(Self::GAIN1),
        }
    }
}

bitflags! {
    /// BIASSENSP
    #[derive(Debug, Copy, Clone)]
    pub struct BiasSensP: u8 {
        const BIASP8 = 0b1000_0000;
        const BIASP7 = 0b0100_0000;
        const BIASP6 = 0b0010_0000;
        const BIASP5 = 0b0001_0000;
        const BIASP4 = 0b0000_1000;
        const BIASP3 = 0b0000_0100;
        const BIASP2 = 0b0000_0010;
        const BIASP1 = 0b0000_0001;
    }
}

impl Default for BiasSensP {
    fn default() -> Self {
        Self::from_bits_retain(0x00)
    }
}

bitflags! {
    /// BIASSENSN
    #[derive(Debug, Copy, Clone)]
    pub struct BiasSensN: u8 {
        const BIASN8 = 0b1000_0000;
        const BIASN7 = 0b0100_0000;
        const BIASN6 = 0b0010_0000;
        const BIASN5 = 0b0001_0000;
        const BIASN4 = 0b0000_1000;
        const BIASN3 = 0b0000_0100;
        const BIASN2 = 0b0000_0010;
        const BIASN1 = 0b0000_0001;
    }
}

impl Default for BiasSensN {
    fn default() -> Self {
        Self::from_bits_retain(0x00)
    }
}

bitflags! {
    /// LOFFSENSP
    #[derive(Debug, Copy, Clone)]
    pub struct LoffSensP: u8 {
        const LOFFP8 = 0b1000_0000;
        const LOFFP7 = 0b0100_0000;
        const LOFFP6 = 0b0010_0000;
        const LOFFP5 = 0b0001_0000;
        const LOFFP4 = 0b0000_1000;
        const LOFFP3 = 0b0000_0100;
        const LOFFP2 = 0b0000_0010;
        const LOFFP1 = 0b0000_0001;
    }
}

impl Default for LoffSensP {
    fn default() -> Self {
        Self::from_bits_retain(0x00)
    }
}

bitflags! {
    /// LOFFSENSN
    #[derive(Debug, Copy, Clone)]
    pub struct LoffSensN: u8 {
        const LOFFN8 = 0b1000_0000;
        const LOFFN7 = 0b0100_0000;
        const LOFFN6 = 0b0010_0000;
        const LOFFN5 = 0b0001_0000;
        const LOFFN4 = 0b0000_1000;
        const LOFFN3 = 0b0000_0100;
        const LOFFN2 = 0b0000_0010;
        const LOFFN1 = 0b0000_0001;
    }
}

impl Default for LoffSensN {
    fn default() -> Self {
        Self::from_bits_retain(0x00)
    }
}

bitflags! {
    /// LOFFFLIP
    #[derive(Debug, Copy, Clone)]
    pub struct LoffFlip: u8 {
        const LOFF_FLIP8 = 0b1000_0000;
        const LOFF_FLIP7 = 0b0100_0000;
        const LOFF_FLIP6 = 0b0010_0000;
        const LOFF_FLIP5 = 0b0001_0000;
        const LOFF_FLIP4 = 0b0000_1000;
        const LOFF_FLIP3 = 0b0000_0100;
        const LOFF_FLIP2 = 0b0000_0010;
        const LOFF_FLIP1 = 0b0000_0001;
    }
}

impl Default for LoffFlip {
    fn default() -> Self {
        Self::from_bits_retain(0x00)
    }
}

bitflags! {
    /// LOFFSTATP
    #[derive(Debug, Copy, Clone)]
    pub struct LoffStatP: u8 {
        const IN8P_OFF = 0b1000_0000;
        const IN7P_OFF = 0b0100_0000;
        const IN6P_OFF = 0b0010_0000;
        const IN5P_OFF = 0b0001_0000;
        const IN4P_OFF = 0b0000_1000;
        const IN3P_OFF = 0b0000_0100;
        const IN2P_OFF = 0b0000_0010;
        const IN1P_OFF = 0b0000_0001;
    }
}
impl Default for LoffStatP {
    fn default() -> Self {
        Self::from_bits_retain(0x00)
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for LoffStatP {
    fn format(&self, f: defmt::Formatter) {
        // format the bitfields of the register as struct fields
        defmt::write!(f, "LoffStatP {0}", self.bits(),)
    }
}

bitflags! {
    /// LOFFSTATN
    #[derive(Debug, Copy, Clone)]
    pub struct LoffStatN: u8 {
        const IN8N_OFF = 0b1000_0000;
        const IN7N_OFF = 0b0100_0000;
        const IN6N_OFF = 0b0010_0000;
        const IN5N_OFF = 0b0001_0000;
        const IN4N_OFF = 0b0000_1000;
        const IN3N_OFF = 0b0000_0100;
        const IN2N_OFF = 0b0000_0010;
        const IN1N_OFF = 0b0000_0001;
    }
}

impl Default for LoffStatN {
    fn default() -> Self {
        Self::from_bits_retain(0x00)
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for LoffStatN {
    fn format(&self, f: defmt::Formatter) {
        // format the bitfields of the register as struct fields
        defmt::write!(f, "LoffStatN {0}", self.bits(),)
    }
}

bitflags! {
    /// GPIO
    #[derive(Debug, Copy, Clone)]
    pub struct Gpio: u8 {
        const GPIOD4 = 0b1000_0000;
        const GPIOD3 = 0b0100_0000;
        const GPIOD2 = 0b0010_0000;
        const GPIOD1 = 0b0001_0000;
        const GPIOC4 = 0b0000_1000;
        const GPIOC3 = 0b0000_0100;
        const GPIOC2 = 0b0000_0010;
        const GPIOC1 = 0b0000_0001;

        const GPIOD = Self::GPIOD4.bits() | Self::GPIOD3.bits() | Self::GPIOD2.bits() | Self::GPIOD1.bits();
        const GPIOC = Self::GPIOC4.bits() | Self::GPIOC3.bits() | Self::GPIOC2.bits() | Self::GPIOC1.bits();
    }
}

impl Default for Gpio {
    fn default() -> Self {
        Self::from_bits_retain(0x0F)
    }
}

impl Gpio {
    // *** GPIOD (Data Register) Methods ***

    /// Check the state of a specific GPIOD (data) pin
    pub const fn gpiod(&self, pin: usize) -> bool {
        match pin {
            1 => self.contains(Self::GPIOD1),
            2 => self.contains(Self::GPIOD2),
            3 => self.contains(Self::GPIOD3),
            4 => self.contains(Self::GPIOD4),
            _ => false, // Invalid pin
        }
    }

    /// Set or clear a specific GPIOD (data) pin
    pub const fn with_gpiod(self, pin: usize, state: bool) -> Self {
        let reg = match pin {
            1 => self.difference(Self::GPIOD1),
            2 => self.difference(Self::GPIOD2),
            3 => self.difference(Self::GPIOD3),
            4 => self.difference(Self::GPIOD4),
            _ => return self, // Invalid pin, return unchanged
        };
        match state {
            true => reg.union(match pin {
                1 => Self::GPIOD1,
                2 => Self::GPIOD2,
                3 => Self::GPIOD3,
                4 => Self::GPIOD4,
                _ => unreachable!(),
            }),
            false => reg,
        }
    }

    // *** GPIOC (Control Register) Methods ***

    /// Check if a GPIOC (control) pin is set as input or output
    /// Returns true for input, false for output
    pub const fn gpioc(&self, pin: usize) -> bool {
        match pin {
            1 => self.contains(Self::GPIOC1),
            2 => self.contains(Self::GPIOC2),
            3 => self.contains(Self::GPIOC3),
            4 => self.contains(Self::GPIOC4),
            _ => false, // Invalid pin
        }
    }

    /// Configure a GPIOC (control) pin as input or output
    /// `state` = true for input, false for output
    pub const fn with_gpioc(self, pin: usize, state: bool) -> Self {
        let reg = match pin {
            1 => self.difference(Self::GPIOC1),
            2 => self.difference(Self::GPIOC2),
            3 => self.difference(Self::GPIOC3),
            4 => self.difference(Self::GPIOC4),
            _ => return self, // Invalid pin, return unchanged
        };
        match state {
            true => reg.union(match pin {
                1 => Self::GPIOC1,
                2 => Self::GPIOC2,
                3 => Self::GPIOC3,
                4 => Self::GPIOC4,
                _ => unreachable!(),
            }),
            false => reg,
        }
    }

    // *** Group Methods ***

    /// Check if all GPIOD (data) pins are set
    pub const fn gpiod_group(&self) -> bool {
        self.contains(Self::GPIOD)
    }

    /// Set or clear all GPIOD (data) pins
    pub const fn with_gpiod_group(self, state: bool) -> Self {
        let reg = self.difference(Self::GPIOD);
        match state {
            true => reg.union(Self::GPIOD),
            false => reg,
        }
    }

    /// Check if all GPIOC (control) pins are configured as inputs
    pub const fn gpioc_group(&self) -> bool {
        self.contains(Self::GPIOC)
    }

    /// Configure all GPIOC (control) pins as input or output
    /// `state` = true for input, false for output
    pub const fn with_gpioc_group(self, state: bool) -> Self {
        let reg = self.difference(Self::GPIOC);
        match state {
            true => reg.union(Self::GPIOC),
            false => reg,
        }
    }
}

bitflags! {
    /// MISC1
    #[derive(Debug, Copy, Clone)]
    pub struct Misc1: u8 {
        const SRB1 = 0b0010_0000;
    }
}

impl Default for Misc1 {
    fn default() -> Self {
        Self::from_bits_retain(0x00)
    }
}

impl Misc1 {
    /// Check if SRB1 is enabled
    pub const fn srb1(&self) -> bool {
        self.contains(Self::SRB1)
    }

    /// Enable or disable SRB1
    pub const fn with_srb1(self, srb1: bool) -> Self {
        let reg = self.difference(Self::SRB1);
        match srb1 {
            false => reg,
            true => reg.union(Self::SRB1),
        }
    }
}

bitflags! {
    /// MISC2
    #[derive(Debug, Copy, Clone)]
    pub struct Misc2: u8 {}
}

impl Default for Misc2 {
    fn default() -> Self {
        Self::from_bits_retain(0x00)
    }
}

bitflags! {
    /// CONFIG4
    #[derive(Debug, Copy, Clone)]
    pub struct Config4: u8 {
        const SINGLE_SHOT  = 0b0000_1000;
        const PD_LOFF_COMP = 0b0000_0010;
    }
}

impl Default for Config4 {
    fn default() -> Self {
        Self::from_bits_retain(0x00)
    }
}

impl Config4 {
    /// Check if single-shot mode is enabled
    pub const fn single_shot(&self) -> bool {
        self.contains(Self::SINGLE_SHOT)
    }

    /// Enable or disable single-shot mode
    pub const fn with_single_shot(self, single_shot: bool) -> Self {
        let reg = self.difference(Self::SINGLE_SHOT);
        match single_shot {
            false => reg,
            true => reg.union(Self::SINGLE_SHOT),
        }
    }

    /// Check if lead-off comparator is powered down
    pub const fn pd_loff_comp(&self) -> bool {
        self.contains(Self::PD_LOFF_COMP)
    }

    /// Enable or disable power-down of lead-off comparator
    pub const fn with_pd_loff_comp(self, pd_loff_comp: bool) -> Self {
        let reg = self.difference(Self::PD_LOFF_COMP);
        match pd_loff_comp {
            false => reg,
            true => reg.union(Self::PD_LOFF_COMP),
        }
    }
}
