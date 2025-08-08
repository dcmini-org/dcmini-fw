#![no_std]

use bitflags::bitflags;

pub mod ll;
pub use ll::{
    AccelFsr, AccelMode, AccelOdr, FifoDepth, FifoMode, GyroFsr, GyroMode,
    GyroOdr, Int1Drive, Int1Mode, Int1Polarity,
};

// VQF for quaternions

use embedded_hal_async::{delay, i2c};
use heapless::Vec;
pub use micromath::Quaternion;

/// Raw sensor data structure
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct SensorData {
    pub accel_x: i16,
    pub accel_y: i16,
    pub accel_z: i16,
    pub gyro_x: i16,
    pub gyro_y: i16,
    pub gyro_z: i16,
    pub temp: i16,
}

/// Sensor data with real units
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct CalibSensorData {
    /// Accelerometer data in g
    pub accel_x: f32,
    pub accel_y: f32,
    pub accel_z: f32,
    /// Gyroscope data in degrees per second
    pub gyro_x: f32,
    pub gyro_y: f32,
    pub gyro_z: f32,
    /// Temperature in degrees Celsius
    pub temp: f32,
}

/// Unit of accelerometer readings
#[derive(Copy, Clone, Debug)]
pub enum AccUnit {
    /// Meters per second squared (m/s^2)
    Mpss,
    /// Number of times of normal gravity
    Gs,
}

impl AccUnit {
    pub fn scalar(self) -> f32 {
        match self {
            Self::Mpss => 9.82,
            Self::Gs => 1.0,
        }
    }
}

/// Unit of gyroscope readings
#[derive(Copy, Clone, Debug)]
pub enum GyrUnit {
    /// Radians per second
    Rps,
    /// Degrees per second
    Dps,
}

impl GyrUnit {
    pub fn scalar(self) -> f32 {
        match self {
            Self::Rps => 0.017453293,
            Self::Dps => 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FifoConfig {
    pub accel_en: bool,
    pub gyro_en: bool,
    pub temp_en: bool,
    pub hires_en: bool,
    pub watermark: u16,
    pub mode: FifoMode,
}

impl Default for FifoConfig {
    fn default() -> Self {
        Self {
            accel_en: true,
            gyro_en: true,
            temp_en: false,
            hires_en: false,
            watermark: 32,
            mode: FifoMode::Stream,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ApexFeature {
    Pedometer,
    Tilt,
    Tap,
    RaiseToWake,
    WakeOnMotion,
}

#[derive(Debug, Clone, Copy)]
pub struct PedometerData {
    pub step_count: u32,
    pub step_cadence: f32,
    pub activity: PedometerActivity,
}

#[derive(Debug, Clone, Copy)]
pub enum PedometerActivity {
    Unknown,
    Walk,
    Run,
}

#[derive(Debug, Clone, Copy)]
pub struct TapData {
    pub count: u8,
    pub axis: u8,
    pub direction: u8,
}

#[derive(derive_more::From, Debug, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error<I2cError> {
    I2c(I2cError),
    DeviceInterfaceError(ll::DeviceInterfaceError<I2cError>),
    InvalidWhoAmI,
    InvalidConfiguration,
    FifoError,
    ApexError,
    FailedToPushData,
}

bitflags! {
    /// FIFO packet header byte flags
    #[derive(Debug, Copy, Clone)]
    pub struct FifoHeader: u8 {
        /// 1: FIFO header length is extended to 2 bytes. The second byte is used for
        /// compressed frame decoding fields or external sensors information
        /// 0: FIFO header length is 1 byte
        const EXT_HEADER    = 0b1000_0000;
        /// 1: Accel is enabled or high resolution is enabled
        /// 0: Accel is not enabled and high resolution is not enabled
        const ACCEL_EN      = 0b0100_0000;
        /// 1: Gyro is enabled or high resolution is enabled
        /// 0: Gyro is not enabled and high resolution is not enabled
        const GYRO_EN       = 0b0010_0000;
        /// 1: High-resolution is enabled (20-bytes format)
        /// 0: High-resolution is not enabled
        const HIRES_EN      = 0b0001_0000;
        /// 1: Timestamp field is included in the packet
        /// 0: Timestamp field is not included in the packet
        const TMST_FIELD_EN = 0b0000_1000;
        /// 1: FSYNC is triggered and the Timestamp field contains the FSYNC-ODR delay
        /// 0: FSYNC is not triggered and the Timestamp field does not contain the FSYNC-ODR delay
        const FSYNC_TAG_EN  = 0b0000_0100;
        /// 1: The ODR for accel is different for this accel data packet compared to the previous accel packet
        /// 0: The ODR for accel is the same as the previous packet with accel
        const ACCEL_ODR     = 0b0000_0010;
        /// 1: The ODR for gyro is different for this gyro data packet compared to the previous gyro packet
        /// 0: The ODR for gyro is the same as the previous packet with gyro
        const GYRO_ODR      = 0b0000_0001;
    }
}

impl FifoHeader {
    /// 1: FIFO header length is extended to 2 bytes. The second byte is used for
    /// compressed frame decoding fields or external sensors information
    /// 0: FIFO header length is 1 byte
    pub const fn ext_header(&self) -> bool {
        self.contains(Self::EXT_HEADER)
    }

    /// 1: Accel is enabled or high resolution is enabled
    /// 0: Accel is not enabled and high resolution is not enabled
    pub const fn accel_en(&self) -> bool {
        self.contains(Self::ACCEL_EN)
    }

    /// 1: Gyro is enabled or high resolution is enabled
    /// 0: Gyro is not enabled and high resolution is not enabled
    pub const fn gyro_en(&self) -> bool {
        self.contains(Self::GYRO_EN)
    }

    /// 1: High-resolution is enabled (20-bytes format)
    /// 0: High-resolution is not enabled
    pub const fn hires_en(&self) -> bool {
        self.contains(Self::HIRES_EN)
    }

    /// 1: Timestamp field is included in the packet. This requires that:
    /// a) high-resolution is enabled, or
    /// b) both Accel and Gyro are enabled, or
    /// c) either Accel or Gyro are enabled, and either ES0 or ES1 are enabled
    ///
    /// The timestamp field contains the timestamp value or FSYNC-ODR delay depending on configuration
    ///
    /// 0: Timestamp field is not included in the packet
    pub const fn tmst_field_en(&self) -> bool {
        self.contains(Self::TMST_FIELD_EN)
    }

    /// 1: FSYNC is triggered and the Timestamp field contains the FSYNC-ODR delay
    /// 0: FSYNC is not triggered and the Timestamp field does not contain the FSYNC-ODR delay
    pub const fn fsync_tag_en(&self) -> bool {
        self.contains(Self::FSYNC_TAG_EN)
    }

    /// 1: The ODR for accel is different for this accel data packet compared to the previous accel packet
    /// 0: The ODR for accel is the same as the previous packet with accel
    pub const fn accel_odr(&self) -> bool {
        self.contains(Self::ACCEL_ODR)
    }

    /// 1: The ODR for gyro is different for this gyro data packet compared to the previous gyro packet
    /// 0: The ODR for gyro is the same as the previous packet with gyro
    pub const fn gyro_odr(&self) -> bool {
        self.contains(Self::GYRO_ODR)
    }
}

bitflags! {
    /// FIFO extended header byte flags (present when EXT_HEADER is set)
    #[derive(Debug, Copy, Clone)]
    pub struct FifoExtHeader: u8 {
        /// Indicates how many bytes sensor ES0 provides
        /// 1: Sensor ES0 provides 9 bytes data
        /// 0: Sensor ES0 provides 6 bytes data
        const ES0_6B_9B = 0b0001_0000;
        /// 1: ES1 data is valid
        /// 0: ES1 data is not valid
        const ES1_VLD   = 0b0000_1000;
        /// 1: ES0 data is valid
        /// 0: ES0 data is not valid
        const ES0_VLD   = 0b0000_0100;
        /// 1: Sensor ES1 is enabled
        /// 0: Sensor ES1 is not enabled
        const ES1_EN    = 0b0000_0010;
        /// 1: Sensor ES0 is enabled
        /// 0: Sensor ES0 is not enabled
        const ES0_EN    = 0b0000_0001;
    }
}

impl FifoExtHeader {
    /// Indicates how many bytes sensor ES0 provides
    /// 1: Sensor ES0 provides 9 bytes data
    /// 0: Sensor ES0 provides 6 bytes data
    pub const fn es0_6b_9b(&self) -> bool {
        self.contains(Self::ES0_6B_9B)
    }

    /// 1: ES1 data is valid
    /// 0: ES1 data is not valid
    pub const fn es1_vld(&self) -> bool {
        self.contains(Self::ES1_VLD)
    }

    /// 1: ES0 data is valid
    /// 0: ES0 data is not valid
    pub const fn es0_vld(&self) -> bool {
        self.contains(Self::ES0_VLD)
    }

    /// 1: Sensor ES1 is enabled
    /// 0: Sensor ES1 is not enabled
    pub const fn es1_en(&self) -> bool {
        self.contains(Self::ES1_EN)
    }

    /// 1: Sensor ES0 is enabled
    /// 0: Sensor ES0 is not enabled
    pub const fn es0_en(&self) -> bool {
        self.contains(Self::ES0_EN)
    }
}

pub struct Icm45605<I2c: i2c::I2c, D: delay::DelayNs> {
    pub device: ll::Device<ll::DeviceInterface<I2c, D>>,
    config: DeviceConfig,
}

#[derive(Debug, Clone, Copy)]
pub struct DeviceConfig {
    pub acc_unit: AccUnit,
    pub gyr_unit: GyrUnit,
    pub acc_fsr: AccelFsr,
    pub gyr_fsr: GyroFsr,
    pub acc_odr: AccelOdr,
    pub gyr_odr: GyroOdr,
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            acc_unit: AccUnit::Gs,
            gyr_unit: GyrUnit::Dps,
            acc_fsr: AccelFsr::Fs4G,
            gyr_fsr: GyroFsr::Fs2000Dps,
            acc_odr: AccelOdr::Odr100Hz,
            gyr_odr: GyroOdr::Odr100Hz,
        }
    }
}

impl<
        I2c: embedded_hal_async::i2c::I2c,
        D: embedded_hal_async::delay::DelayNs,
    > Icm45605<I2c, D>
{
    pub fn new(i2c: I2c, delay: D) -> Self {
        Self {
            device: ll::Device::new(ll::DeviceInterface { i2c, delay }),
            config: DeviceConfig::default(),
        }
    }

    /// Initialize the IMU
    pub async fn init(&mut self) -> Result<(), Error<I2c::Error>> {
        // Wait for power-up
        self.device.interface.delay.delay_ms(3).await;

        // Check WHO_AM_I register
        let who_am_i = self.device.who_am_i().read_async().await?;
        if who_am_i.whoami() != 0xE5 {
            // Replace with actual WHO_AM_I value
            return Err(Error::InvalidWhoAmI);
        }

        self.device
            .ipreg_top_1()
            .sreg_ctrl()
            .modify_async(|w| w.set_sreg_data_endian_sel(true))
            .await?;

        // Disable APEX features initially
        self.device
            .edmp_apex_en_0()
            .modify_async(|w| {
                w.set_tap_en(false);
                w.set_tilt_en(false);
                w.set_pedo_en(false);
                w.set_ff_en(false);
                w.set_r_2_w_en(false);
                w.set_smd_en(false);
            })
            .await?;

        Ok(())
    }

    /// Start accelerometer with specified ODR and FSR
    pub async fn start_accel(
        &mut self,
        odr: AccelOdr,
        fsr: AccelFsr,
    ) -> Result<(), Error<I2c::Error>> {
        // Set accelerometer FSR and ODR
        self.device
            .accel_config_0()
            .modify_async(|w| {
                w.set_accel_ui_fs_sel(fsr);
                w.set_accel_odr(odr);
            })
            .await?;

        // Set accelerometer to low noise mode
        self.device
            .pwr_mgmt_0()
            .modify_async(|w| w.set_accel_mode(AccelMode::LowNoise))
            .await?;

        self.device
            .int_1_config_0()
            .modify_async(|w| {
                w.set_int_1_status_en_drdy(true);
            })
            .await?;

        // Update configuration state
        self.config.acc_fsr = fsr;
        self.config.acc_odr = odr;

        Ok(())
    }

    /// Start gyroscope with specified ODR and FSR
    pub async fn start_gyro(
        &mut self,
        odr: GyroOdr,
        fsr: GyroFsr,
    ) -> Result<(), Error<I2c::Error>> {
        // Set gyroscope FSR and ODR
        self.device
            .gyro_config_0()
            .modify_async(|w| {
                w.set_gyro_ui_fs_sel(fsr);
                w.set_gyro_odr(odr);
            })
            .await?;

        // Set gyroscope to low noise mode
        self.device
            .pwr_mgmt_0()
            .modify_async(|w| w.set_gyro_mode(GyroMode::LowNoise))
            .await?;

        // Update configuration state
        self.config.gyr_fsr = fsr;
        self.config.gyr_odr = odr;

        Ok(())
    }

    /// Stop accelerometer
    pub async fn stop_accel(&mut self) -> Result<(), Error<I2c::Error>> {
        Ok(self
            .device
            .pwr_mgmt_0()
            .modify_async(|w| w.set_accel_mode(AccelMode::Off))
            .await?)
    }

    /// Stop gyroscope
    pub async fn stop_gyro(&mut self) -> Result<(), Error<I2c::Error>> {
        Ok(self
            .device
            .pwr_mgmt_0()
            .modify_async(|w| w.set_gyro_mode(GyroMode::Off))
            .await?)
    }

    /// Read raw sensor data from registers
    pub async fn read_raw_data(
        &mut self,
    ) -> Result<SensorData, Error<I2c::Error>> {
        let accel_x = self.device.accel_data_x_ui().read_async().await?.data();
        let accel_y = self.device.accel_data_y_ui().read_async().await?.data();
        let accel_z = self.device.accel_data_z_ui().read_async().await?.data();
        let gyro_x = self.device.gyro_data_x_ui().read_async().await?.data();
        let gyro_y = self.device.gyro_data_y_ui().read_async().await?.data();
        let gyro_z = self.device.gyro_data_z_ui().read_async().await?.data();
        let temp = self.device.temp_data_ui().read_async().await?.data();

        Ok(SensorData {
            accel_x: accel_x as i16,
            accel_y: accel_y as i16,
            accel_z: accel_z as i16,
            gyro_x: gyro_x as i16,
            gyro_y: gyro_y as i16,
            gyro_z: gyro_z as i16,
            temp: temp as i16,
        })
    }

    /// Configure and enable FIFO
    pub async fn configure_fifo(
        &mut self,
        config: FifoConfig,
    ) -> Result<(), Error<I2c::Error>> {
        // Configure FIFO mode and depth
        self.device
            .fifo_config_0()
            .modify_async(|w| {
                w.set_fifo_mode(match config.mode {
                    FifoMode::Bypass => FifoMode::Bypass,
                    FifoMode::Stream => FifoMode::Stream,
                    FifoMode::StopOnFull => FifoMode::StopOnFull,
                });
                w.set_fifo_depth(FifoDepth::Depth2K);
            })
            .await?;

        // Set watermark threshold
        self.device
            .fifo_config_1()
            .modify_async(|w| w.set_fifo_wm_th(config.watermark))
            .await?;

        // Configure FIFO data sources
        self.device
            .fifo_config_3()
            .modify_async(|w| {
                w.set_fifo_if_en(true);
                w.set_fifo_accel_en(config.accel_en);
                w.set_fifo_gyro_en(config.gyro_en);
                w.set_fifo_hires_en(config.hires_en);
            })
            .await?;

        Ok(())
    }

    /// Read raw data from FIFO
    pub async fn read_fifo_data(
        &mut self,
    ) -> Result<Vec<SensorData, 32>, Error<I2c::Error>> {
        let mut data = Vec::new();

        // Read FIFO count
        let count = self.device.fifo_data_cnt().read_async().await?.data();
        if count == 0 {
            return Ok(data);
        }

        // Constants for invalid values
        const INVALID_VALUE_FIFO: i16 = -32768;
        const INVALID_VALUE_FIFO_1B: i8 = -128;

        while data.len() < 32 {
            let mut frame_idx = 0;
            let mut packet = [0u8; 32]; // Support up to 32 bytes per frame

            // Read header byte first
            packet[frame_idx] =
                self.device.fifo_data().read_async().await?.data();
            let header = FifoHeader::from_bits_truncate(packet[frame_idx]);
            frame_idx += 1;

            // Read extended header if present
            let ext_header = if header.ext_header() {
                packet[frame_idx] =
                    self.device.fifo_data().read_async().await?.data();
                frame_idx += 1;
                Some(FifoExtHeader::from_bits_truncate(packet[frame_idx - 1]))
            } else {
                None
            };

            let mut sensor_data = SensorData {
                accel_x: 0,
                accel_y: 0,
                accel_z: 0,
                gyro_x: 0,
                gyro_y: 0,
                gyro_z: 0,
                temp: 0,
            };

            // Determine if we're in 32-byte frame mode
            let frame_32bytes = count == 32;
            let should_read_accel = header.accel_en() || frame_32bytes;
            let should_read_gyro = header.gyro_en() || frame_32bytes;

            // Read accelerometer data
            if should_read_accel {
                for i in 0..6 {
                    packet[frame_idx + i] =
                        self.device.fifo_data().read_async().await?.data();
                }
                sensor_data.accel_x = i16::from_be_bytes([
                    packet[frame_idx],
                    packet[frame_idx + 1],
                ]);
                sensor_data.accel_y = i16::from_be_bytes([
                    packet[frame_idx + 2],
                    packet[frame_idx + 3],
                ]);
                sensor_data.accel_z = i16::from_be_bytes([
                    packet[frame_idx + 4],
                    packet[frame_idx + 5],
                ]);
                frame_idx += 6;
            }

            // Read gyroscope data
            if should_read_gyro {
                for i in 0..6 {
                    packet[frame_idx + i] =
                        self.device.fifo_data().read_async().await?.data();
                }
                sensor_data.gyro_x = i16::from_be_bytes([
                    packet[frame_idx],
                    packet[frame_idx + 1],
                ]);
                sensor_data.gyro_y = i16::from_be_bytes([
                    packet[frame_idx + 2],
                    packet[frame_idx + 3],
                ]);
                sensor_data.gyro_z = i16::from_be_bytes([
                    packet[frame_idx + 4],
                    packet[frame_idx + 5],
                ]);
                frame_idx += 6;
            }

            // Handle external sensors if present in extended header
            if let Some(ext_header) = ext_header {
                // Handle ES0
                if ext_header.es0_en() || frame_32bytes {
                    // let es0_size = if ext_header.es0_6b_9b() { 9 } else { 6 };
                    // Always skip 9 bytes as per reference implementation
                    for _ in 0..9 {
                        let _ =
                            self.device.fifo_data().read_async().await?.data();
                    }
                    frame_idx += 9;
                }

                // Handle ES1
                if ext_header.es1_en() || frame_32bytes {
                    // ES1 is always 6 bytes
                    for _ in 0..6 {
                        let _ =
                            self.device.fifo_data().read_async().await?.data();
                    }
                    frame_idx += 6;
                }
            }

            // Read temperature
            if (should_read_accel || should_read_gyro) && !frame_32bytes {
                if header.hires_en() {
                    // High resolution temperature (2 bytes + high res byte)
                    for i in 0..3 {
                        packet[frame_idx + i] =
                            self.device.fifo_data().read_async().await?.data();
                    }
                    sensor_data.temp = i16::from_be_bytes([
                        packet[frame_idx],
                        packet[frame_idx + 1],
                    ]);
                    frame_idx += 3;
                } else {
                    // Single byte temperature
                    packet[frame_idx] =
                        self.device.fifo_data().read_async().await?.data();
                    sensor_data.temp = i16::from(packet[frame_idx] as i8);
                    frame_idx += 1;
                }
            }

            // Read timestamp/FSYNC if present
            if header.tmst_field_en() || header.fsync_tag_en() || frame_32bytes
            {
                for _ in 0..2 {
                    let _ = self.device.fifo_data().read_async().await?.data();
                }
                frame_idx += 2;
            }

            // Read high resolution bits if enabled
            if header.hires_en() && !frame_32bytes {
                // Read high resolution data for accel and gyro
                if should_read_accel {
                    packet[frame_idx] =
                        self.device.fifo_data().read_async().await?.data();
                    sensor_data.accel_x = (sensor_data.accel_x << 4)
                        | (((packet[frame_idx] >> 4) & 0x0F) as i16);
                    sensor_data.accel_y = (sensor_data.accel_y << 4)
                        | (((packet[frame_idx] >> 2) & 0x0F) as i16);
                    sensor_data.accel_z = (sensor_data.accel_z << 4)
                        | ((packet[frame_idx] & 0x0F) as i16);
                    frame_idx += 1;
                }

                if should_read_gyro {
                    packet[frame_idx] =
                        self.device.fifo_data().read_async().await?.data();
                    sensor_data.gyro_x = (sensor_data.gyro_x << 4)
                        | ((packet[frame_idx] >> 4 & 0x0F) as i16);
                    sensor_data.gyro_y = (sensor_data.gyro_y << 4)
                        | ((packet[frame_idx] >> 2 & 0x0F) as i16);
                    sensor_data.gyro_z = (sensor_data.gyro_z << 4)
                        | ((packet[frame_idx] & 0x0F) as i16);
                }
            }

            // Validate data before adding to vector
            let valid_accel = !should_read_accel
                || (sensor_data.accel_x != INVALID_VALUE_FIFO
                    && sensor_data.accel_y != INVALID_VALUE_FIFO
                    && sensor_data.accel_z != INVALID_VALUE_FIFO);

            let valid_gyro = !should_read_gyro
                || (sensor_data.gyro_x != INVALID_VALUE_FIFO
                    && sensor_data.gyro_y != INVALID_VALUE_FIFO
                    && sensor_data.gyro_z != INVALID_VALUE_FIFO);

            let valid_temp = sensor_data.temp as i8 != INVALID_VALUE_FIFO_1B;

            if valid_accel && valid_gyro && valid_temp {
                data.push(sensor_data)
                    .map_err(|_| Error::<I2c::Error>::FailedToPushData)?;
            }
        }

        Ok(data)
    }

    /// Read calibrated data from FIFO
    pub async fn read_fifo_data_calibrated(
        &mut self,
    ) -> Result<Vec<CalibSensorData, 32>, Error<I2c::Error>> {
        let raw_data = self.read_fifo_data().await?;
        let mut calib_data = Vec::new();

        for raw in raw_data {
            let calib = CalibSensorData {
                accel_x: f32::from(raw.accel_x) * self.acc_scalar(),
                accel_y: f32::from(raw.accel_y) * self.acc_scalar(),
                accel_z: f32::from(raw.accel_z) * self.acc_scalar(),
                gyro_x: f32::from(raw.gyro_x) * self.gyr_scalar(),
                gyro_y: f32::from(raw.gyro_y) * self.gyr_scalar(),
                gyro_z: f32::from(raw.gyro_z) * self.gyr_scalar(),
                temp: self.scaled_tmp_from_bytes(raw.temp.to_be_bytes()), // Temperature not included in FIFO
            };
            calib_data
                .push(calib)
                .map_err(|_| Error::<I2c::Error>::FailedToPushData)?;
        }

        Ok(calib_data)
    }

    /// Configure FIFO watermark interrupt
    pub async fn configure_fifo_interrupt(
        &mut self,
        enable: bool,
    ) -> Result<(), Error<I2c::Error>> {
        // Configure INT1 pin settings
        self.device
            .int_1_config_2()
            .modify_async(|w| {
                w.set_int_1_polarity(Int1Polarity::ActiveHigh);
                w.set_int_1_mode(Int1Mode::Pulse);
                w.set_int_1_drive(Int1Drive::PushPull);
            })
            .await?;

        // Enable/disable FIFO watermark interrupt
        self.device
            .int_1_config_0()
            .modify_async(|w| w.set_int_1_status_en_fifo_ths(enable))
            .await?;

        Ok(())
    }

    /// Flush FIFO
    pub async fn flush_fifo(&mut self) -> Result<(), Error<I2c::Error>> {
        Ok(self
            .device
            .fifo_config_2()
            .modify_async(|w| w.set_fifo_flush(true))
            .await?)
    }

    /// Start pedometer detection
    pub async fn start_pedometer(&mut self) -> Result<(), Error<I2c::Error>> {
        // Configure APEX parameters for pedometer
        self.device
            .edmp_apex_en_0()
            .modify_async(|w| w.set_pedo_en(true))
            .await?;

        // Set accelerometer ODR and FSR for pedometer
        self.start_accel(AccelOdr::Odr50Hz, AccelFsr::Fs4G).await?;

        // Configure interrupt
        self.device
            .int_apex_config_0()
            .modify_async(|w| {
                w.set_int_status_mask_pin_step_det(false);
                w.set_int_status_mask_pin_step_cnt_ovfl(false);
            })
            .await?;

        Ok(())
    }

    /// Start tilt detection
    pub async fn start_tilt_detection(
        &mut self,
    ) -> Result<(), Error<I2c::Error>> {
        // Configure APEX parameters for tilt detection
        self.device
            .edmp_apex_en_0()
            .modify_async(|w| w.set_tilt_en(true))
            .await?;

        // Set accelerometer ODR and FSR for tilt detection
        self.start_accel(AccelOdr::Odr50Hz, AccelFsr::Fs4G).await?;

        // Configure interrupt
        self.device
            .int_apex_config_0()
            .modify_async(|w| w.set_int_status_mask_pin_tilt_det(false))
            .await?;

        Ok(())
    }

    /// Start tap detection
    pub async fn start_tap_detection(
        &mut self,
    ) -> Result<(), Error<I2c::Error>> {
        // Configure APEX parameters for tap detection
        self.device
            .edmp_apex_en_0()
            .modify_async(|w| w.set_tap_en(true))
            .await?;

        // Set accelerometer ODR and FSR for tap detection
        self.start_accel(AccelOdr::Odr400Hz, AccelFsr::Fs4G).await?;

        // Configure interrupt
        self.device
            .int_apex_config_0()
            .modify_async(|w| w.set_int_status_mask_pin_tap_det(false))
            .await?;

        Ok(())
    }

    /// Start raise to wake detection
    pub async fn start_raise_to_wake(
        &mut self,
    ) -> Result<(), Error<I2c::Error>> {
        // Configure APEX parameters for raise to wake
        self.device
            .edmp_apex_en_0()
            .modify_async(|w| w.set_r_2_w_en(true))
            .await?;

        // Set accelerometer ODR and FSR for raise to wake
        self.start_accel(AccelOdr::Odr100Hz, AccelFsr::Fs4G).await?;

        // Configure interrupt
        self.device
            .int_apex_config_0()
            .modify_async(|w| w.set_int_status_mask_pin_r_2_w_wake_det(false))
            .await?;

        Ok(())
    }

    /// Start wake on motion detection
    pub async fn start_wake_on_motion(
        &mut self,
        _threshold_mg: u8,
    ) -> Result<(), Error<I2c::Error>> {
        // Set accelerometer ODR and FSR for WoM
        self.start_accel(AccelOdr::Odr50Hz, AccelFsr::Fs4G).await?;

        // Configure interrupt
        self.device
            .int_1_config_1()
            .modify_async(|w| {
                w.set_int_1_status_en_wom_x(true);
                w.set_int_1_status_en_wom_y(true);
                w.set_int_1_status_en_wom_z(true);
            })
            .await?;

        Ok(())
    }

    /// Get pedometer data
    pub async fn get_pedometer_data(
        &mut self,
    ) -> Result<Option<PedometerData>, Error<I2c::Error>> {
        let status = self.device.int_apex_status_0().read_async().await?;

        if status.int_status_step_det() {
            // Read step count and other data from appropriate registers
            // This is a simplified implementation - you'll need to add the actual register reads
            Ok(Some(PedometerData {
                step_count: 0,     // Read from appropriate register
                step_cadence: 0.0, // Calculate from appropriate register
                activity: PedometerActivity::Unknown, // Determine from appropriate register
            }))
        } else {
            Ok(None)
        }
    }

    /// Get tap detection data
    pub async fn get_tap_data(
        &mut self,
    ) -> Result<Option<TapData>, Error<I2c::Error>> {
        let status = self.device.int_apex_status_0().read_async().await?;

        if status.int_status_tap_det() {
            // Read tap data from appropriate registers
            // This is a simplified implementation - you'll need to add the actual register reads
            Ok(Some(TapData {
                count: 0,     // Read from appropriate register
                axis: 0,      // Read from appropriate register
                direction: 0, // Read from appropriate register
            }))
        } else {
            Ok(None)
        }
    }

    /// Check if tilt was detected
    pub async fn get_tilt_detected(
        &mut self,
    ) -> Result<bool, Error<I2c::Error>> {
        let status = self.device.int_apex_status_0().read_async().await?;
        Ok(status.int_status_tilt_det())
    }

    /// Check raise to wake status
    pub async fn get_raise_to_wake_status(
        &mut self,
    ) -> Result<bool, Error<I2c::Error>> {
        let status = self.device.int_apex_status_0().read_async().await?;
        Ok(status.int_status_r_2_w_wake_det())
    }

    /// Stop a specific APEX feature
    pub async fn stop_apex_feature(
        &mut self,
        feature: ApexFeature,
    ) -> Result<(), Error<I2c::Error>> {
        Ok(match feature {
            ApexFeature::Pedometer => {
                self.device
                    .edmp_apex_en_0()
                    .modify_async(|w| w.set_pedo_en(false))
                    .await
            }
            ApexFeature::Tilt => {
                self.device
                    .edmp_apex_en_0()
                    .modify_async(|w| w.set_tilt_en(false))
                    .await
            }
            ApexFeature::Tap => {
                self.device
                    .edmp_apex_en_0()
                    .modify_async(|w| w.set_tap_en(false))
                    .await
            }
            ApexFeature::RaiseToWake => {
                self.device
                    .edmp_apex_en_0()
                    .modify_async(|w| w.set_r_2_w_en(false))
                    .await
            }
            ApexFeature::WakeOnMotion => {
                self.device
                    .int_1_config_1()
                    .modify_async(|w| {
                        w.set_int_1_status_en_wom_x(false);
                        w.set_int_1_status_en_wom_y(false);
                        w.set_int_1_status_en_wom_z(false);
                    })
                    .await
            }
        }?)
    }

    /// Returns the scalar corresponding to the unit and range configured for accelerometer
    fn acc_scalar(&self) -> f32 {
        self.config.acc_unit.scalar()
            / match self.config.acc_fsr {
                AccelFsr::Fs16G => 2048.0,
                AccelFsr::Fs8G => 4096.0,
                AccelFsr::Fs4G => 8192.0,
                AccelFsr::Fs2G => 16384.0,
            }
    }

    /// Returns the scalar corresponding to the unit and range configured for gyroscope
    fn gyr_scalar(&self) -> f32 {
        self.config.gyr_unit.scalar()
            / match self.config.gyr_fsr {
                GyroFsr::Fs15625Dps => 2096.0,
                GyroFsr::Fs3125Dps => 1048.0,
                GyroFsr::Fs625Dps => 524.0,
                GyroFsr::Fs125Dps => 262.0,
                GyroFsr::Fs250Dps => 131.0,
                GyroFsr::Fs500Dps => 65.5,
                GyroFsr::Fs1000Dps => 32.8,
                GyroFsr::Fs2000Dps => 16.4,
            }
    }

    /// Takes 2 bytes converts them into a temperature as a float
    fn scaled_tmp_from_bytes(&self, bytes: [u8; 2]) -> f32 {
        // According to ICM-45605 datasheet:
        // Temperature in degrees C = (TEMP_DATA / 132.48) + 25
        f32::from(i16::from_be_bytes(bytes)) / 132.48 + 25.0
    }

    /// Returns whether new data is ready
    ///
    /// In FIFO mode, this checks the FIFO watermark interrupt status.
    /// In direct read mode, this checks the data ready interrupt status.
    pub async fn new_data_ready(&mut self) -> Result<bool, Error<I2c::Error>> {
        let status = self.device.int_1_status_0().read_async().await?;

        // Check if FIFO is enabled
        let fifo_config = self.device.fifo_config_0().read_async().await?;
        let fifo_enabled = matches!(fifo_config.fifo_mode(), Ok(mode) if mode != FifoMode::Bypass);

        if fifo_enabled {
            // In FIFO mode, check FIFO watermark status
            Ok(status.int_1_status_fifo_ths())
        } else {
            // In direct read mode, check data ready status
            Ok(status.int_1_status_drdy())
        }
    }

    /// Get scaled measurements for accelerometer and gyroscope, and temperature
    pub async fn read_6dof(
        &mut self,
    ) -> Result<CalibSensorData, Error<I2c::Error>> {
        let raw = self.read_raw_data().await?;

        Ok(CalibSensorData {
            accel_x: f32::from(raw.accel_x) * self.acc_scalar(),
            accel_y: f32::from(raw.accel_y) * self.acc_scalar(),
            accel_z: f32::from(raw.accel_z) * self.acc_scalar(),
            gyro_x: f32::from(raw.gyro_x) * self.gyr_scalar(),
            gyro_y: f32::from(raw.gyro_y) * self.gyr_scalar(),
            gyro_z: f32::from(raw.gyro_z) * self.gyr_scalar(),
            temp: self.scaled_tmp_from_bytes(raw.temp.to_be_bytes()),
        })
    }

    /// Set accelerometer calibration offsets
    pub async fn set_acc_offsets(
        &mut self,
        _offsets: [i16; 3],
    ) -> Result<(), Error<I2c::Error>> {
        // TODO: Implement when we find the appropriate offset registers in the ICM-45605
        // The ICM-20948 implementation used specific offset registers, but we need to find
        // the equivalent in the ICM-45605
        Err(Error::InvalidConfiguration)
    }

    /// Set gyroscope calibration offsets
    pub async fn set_gyr_offsets(
        &mut self,
        _offsets: [i16; 3],
    ) -> Result<(), Error<I2c::Error>> {
        // TODO: Implement when we find the appropriate offset registers in the ICM-45605
        // The ICM-20948 implementation used specific offset registers, but we need to find
        // the equivalent in the ICM-45605
        Err(Error::InvalidConfiguration)
    }

    /// Collects and averages `num` samples for gyro calibration
    pub async fn gyr_calibrate(
        &mut self,
        num: usize,
    ) -> Result<(), Error<I2c::Error>> {
        let mut offset = [0i32; 3];
        for _ in 0..num {
            let data = self.read_raw_data().await?;
            offset[0] += data.gyro_x as i32;
            offset[1] += data.gyro_y as i32;
            offset[2] += data.gyro_z as i32;
            self.device.interface.delay.delay_ms(10).await;
        }

        let offsets = offset.map(|x| (x / num as i32) as i16);
        self.set_gyr_offsets(offsets).await
    }

    /// Set returned unit of accelerometer
    pub fn set_acc_unit(&mut self, unit: AccUnit) {
        self.config.acc_unit = unit;
    }

    /// Set returned unit of gyroscope
    pub fn set_gyr_unit(&mut self, unit: GyrUnit) {
        self.config.gyr_unit = unit;
    }
}
