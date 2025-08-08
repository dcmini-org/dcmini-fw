#![no_std]

use byteorder::{BigEndian, ByteOrder};
use embedded_hal::{digital::OutputPin, spi::Operation};
use embedded_hal_async::digital::Wait;
use embedded_hal_async::spi::SpiDevice;
use heapless::Vec;

pub use crate::errors::Error;
pub use crate::registers::*;
use core::result::Result;

pub mod errors;
pub mod registers;

// Maximum ADS clock period.
const MAX_ADS_CLK_PER_NS: u32 = 700;
// Clock calculations
pub const MIN_T_POR: u32 = MAX_ADS_CLK_PER_NS << 18;
pub const MIN_T_RST: u32 = MAX_ADS_CLK_PER_NS << 1;
pub const MIN_RST_WAIT: u32 = 18 * MAX_ADS_CLK_PER_NS;

pub struct Ads1299<SPI> {
    spi: SPI,
    pub num_chs: Option<u8>,
}

impl<E, SPI> Ads1299<SPI>
where
    SPI: SpiDevice<Error = E>,
{
    pub fn new(spi: SPI) -> Self {
        Self { spi, num_chs: None }
    }

    pub async fn init(&mut self) -> Result<(), Error<E>> {
        let _ = self.cmd(Command::SDATAC).await;
        let _ = self.get_num_ch().await;
        Ok(())
    }

    pub async fn smell(&mut self) -> Result<(), Error<E>> {
        let _ = self.cmd(Command::SDATAC).await;
        let reg_value = self.read_register(registers::Register::ID).await?;
        let primary_ads_id = registers::Id::from_bits_retain(reg_value);
        primary_ads_id.smell().map_err(|e| e.into())
    }

    pub async fn cmd(&mut self, command: Command) -> Result<(), Error<E>> {
        let (buf, len) = command.into();
        self.spi.write(&buf[0..len]).await.map_err(Error::SpiError)
    }

    pub async fn register_op(
        &mut self,
        command: Command,
        buffer: &mut [u8],
    ) -> Result<(), Error<E>> {
        let (bytes, len) = command.into();

        self.spi
            .transaction(&mut [
                Operation::Write(&bytes[0..len]),
                Operation::TransferInPlace(buffer),
            ])
            .await
            .map_err(Error::SpiError)
    }
    pub async fn read_register_sequential(
        &mut self,
        reg: Register,
        buffer: &mut [u8],
    ) -> Result<(), Error<E>> {
        let command = Command::RREG(reg as u8, buffer.len() as u8);
        self.register_op(command, buffer).await
    }

    pub async fn write_register_sequential(
        &mut self,
        reg: Register,
        buffer: &mut [u8],
    ) -> Result<(), Error<E>> {
        let command = Command::WREG(reg as u8, buffer.len() as u8);
        self.register_op(command, buffer).await
    }

    pub async fn read_register(
        &mut self,
        reg: Register,
    ) -> Result<u8, Error<E>> {
        let mut buffer = [0];
        self.read_register_sequential(reg, &mut buffer).await?;
        Ok(buffer[0])
    }

    pub async fn write_register(
        &mut self,
        reg: Register,
        val: u8,
    ) -> Result<(), Error<E>> {
        self.write_register_sequential(reg, &mut [val]).await
    }

    pub async fn modify_register<F>(
        &mut self,
        register: Register,
        f: F,
    ) -> Result<(), Error<E>>
    where
        F: FnOnce(u8) -> u8,
    {
        let value = self.read_register(register).await?;

        self.write_register(register, f(value)).await
    }

    pub async fn rdata(&mut self) -> Result<AdsData, Error<E>> {
        let mut sample = [0u8; 27];
        let (bytes, len) = Command::RDATA.into();

        let bytes_to_read = match self.num_chs {
            None | Some(8) => 29,
            Some(6) => 23,
            Some(4) => 17,
            Some(e) => panic!("Invalid channels count in rdata. This should be unreachable! {:?}", e),
        };

        self.spi
            .transaction(&mut [
                Operation::Write(&bytes[0..len]),
                Operation::TransferInPlace(&mut sample[0..bytes_to_read]),
            ])
            .await
            .map_err(Error::SpiError)?;

        Ok(AdsData::new(sample, *self.num_chs.get_or_insert(8)))
    }

    pub async fn rdatac(&mut self) -> Result<AdsData, Error<E>> {
        let mut sample = [0u8; 27];

        let bytes_to_read = match self.num_chs {
            None | Some(8) => 27,
            Some(6) => 21,
            Some(4) => 15,
            Some(e) => panic!("Invalid channels count in rdatac. This should be unreachable! {:?}", e),
        };

        self.spi
            .read(&mut sample[0..bytes_to_read])
            .await
            .map_err(Error::SpiError)?;
        if (sample[0] & 0xF0) != 0xC0 {
            panic!("MAGIC DOESN'T EXIST");
        }
        Ok(AdsData::new(sample, *self.num_chs.get_or_insert(8)))
    }

    pub async fn get_num_ch(&mut self) -> Result<u8, Error<E>> {
        let reg_value: u8 = self.read_register(Register::ID).await?;
        let id = Id::from_bits_retain(reg_value);

        let chs = id.num_chs()?;
        self.num_chs = Some(chs);
        Ok(chs)
    }

    pub async fn get_sampling_rate(&mut self) -> Result<SampleRate, Error<E>> {
        let reg_value: u8 = self.read_register(Register::CONFIG1).await?;
        let config1 = Config1::from_bits_retain(reg_value);

        config1.odr().map_err(Error::from)
    }

    pub async fn set_sampling_rate(
        &mut self,
        sample_rate: SampleRate,
    ) -> Result<(), Error<E>> {
        self.modify_register(Register::CONFIG1, |reg_value| {
            Config1::from_bits_retain(reg_value).with_odr(sample_rate).bits()
        })
        .await
    }

    pub async fn get_channel_pd(&mut self, ch: u8) -> Result<bool, Error<E>> {
        let reg_value: u8 =
            self.read_register(Register::from_channel_number(ch)).await?;
        let chset = ChSet::from_bits_retain(reg_value);

        Ok(chset.pd())
    }

    pub async fn set_channel_pd(
        &mut self,
        ch: u8,
        pd: bool,
    ) -> Result<(), Error<E>> {
        self.modify_register(Register::from_channel_number(ch), |reg_value| {
            ChSet::from_bits_retain(reg_value).with_pd(pd).bits()
        })
        .await
    }

    pub async fn get_channel_mux(&mut self, ch: u8) -> Result<Mux, Error<E>> {
        let reg_value: u8 =
            self.read_register(Register::from_channel_number(ch)).await?;
        let chset = ChSet::from_bits_retain(reg_value);

        chset.mux().map_err(Error::from)
    }

    pub async fn set_channel_mux(
        &mut self,
        ch: u8,
        mux: Mux,
    ) -> Result<(), Error<E>> {
        self.modify_register(Register::from_channel_number(ch), |reg_value| {
            ChSet::from_bits_retain(reg_value).with_mux(mux).bits()
        })
        .await
    }

    pub async fn get_channel_gain(
        &mut self,
        ch: u8,
    ) -> Result<Gain, Error<E>> {
        let reg_value: u8 =
            self.read_register(Register::from_channel_number(ch)).await?;
        let chset = ChSet::from_bits_retain(reg_value);

        chset.gain().map_err(Error::from)
    }

    pub async fn set_channel_gain(
        &mut self,
        ch: u8,
        gain: Gain,
    ) -> Result<(), Error<E>> {
        self.modify_register(Register::from_channel_number(ch), |reg_value| {
            ChSet::from_bits_retain(reg_value).with_gain(gain).bits()
        })
        .await
    }

    pub async fn set_calibration_frequency(
        &mut self,
        cal_freq: CalFreq,
    ) -> Result<(), Error<E>> {
        self.modify_register(Register::CONFIG2, |reg_value| {
            Config2::from_bits_retain(reg_value).with_cal_freq(cal_freq).bits()
        })
        .await
    }
}

#[derive(Clone)]
pub struct AdsData {
    pub lead_off_status_pos: LoffStatP,
    pub lead_off_status_neg: LoffStatN,
    pub gpio: Gpio,
    pub data: Vec<i32, 8>,
}

impl AdsData {
    pub fn new(buffer: [u8; 27], num_chs: u8) -> Self {
        Self {
            lead_off_status_pos: Self::read_statusp(
                buffer[0..3].try_into().unwrap(),
            ),
            lead_off_status_neg: Self::read_statusn(
                buffer[0..3].try_into().unwrap(),
            ),
            gpio: Self::read_gpio(buffer[0..3].try_into().unwrap()),
            data: Self::read_channels(
                buffer[3..27].try_into().unwrap(),
                num_chs,
            ),
        }
    }

    fn read_statusp(buffer: [u8; 3]) -> LoffStatP {
        LoffStatP::from_bits_retain(buffer[0] << 4 | buffer[1] >> 4)
    }

    fn read_statusn(buffer: [u8; 3]) -> LoffStatN {
        LoffStatN::from_bits_retain(buffer[1] << 4 | buffer[2] >> 4)
    }

    fn read_gpio(buffer: [u8; 3]) -> Gpio {
        Gpio::from_bits_retain(buffer[2] << 4)
    }

    fn read_channels(buffer: [u8; 24], num_chs: u8) -> Vec<i32, 8> {
        buffer
            .chunks(3)
            .take(num_chs.into())
            .map(BigEndian::read_i24)
            .collect()
    }
}

pub struct AdsFrontend<SPI, START, RESET, PWDN, DRDY, const N: usize = 2> {
    pub ads: Vec<Ads1299<SPI>, N>,
    start: START,
    reset: RESET,
    pwdn: PWDN,
    drdy: DRDY,
}

impl<E, SPI, START, RESET, PWDN, DRDY, const N: usize>
    AdsFrontend<SPI, START, RESET, PWDN, DRDY, N>
where
    SPI: SpiDevice<Error = E>,
    START: OutputPin,
    RESET: OutputPin,
    PWDN: OutputPin,
    DRDY: Wait,
{
    pub fn new(
        ads: Vec<Ads1299<SPI>, N>,
        start: START,
        reset: RESET,
        pwdn: PWDN,
        drdy: DRDY,
    ) -> Self {
        Self { ads, start, reset, pwdn, drdy }
    }

    pub async fn init(&mut self) -> Result<(), Error<E>> {
        for dev in self.ads.iter_mut() {
            dev.init().await?;
        }
        Ok(())
    }

    pub async fn reset(
        &mut self,
        delay: &mut impl embedded_hal_async::delay::DelayNs,
    ) -> Result<(), Error<E>> {
        self.start.set_low().unwrap();
        self.pwdn.set_high().unwrap();
        self.reset.set_high().unwrap();
        delay.delay_ns(MIN_T_POR).await;
        self.reset.set_low().unwrap();
        delay.delay_ns(MIN_T_RST).await;
        self.reset.set_high().unwrap();
        delay.delay_ns(MIN_RST_WAIT).await;

        self.init().await
    }

    pub async fn start_stream(&mut self) -> Result<(), Error<E>> {
        for dev in self.ads.iter_mut() {
            dev.cmd(Command::RDATAC).await?;
        }

        self.start.set_high().unwrap();

        Ok(())
    }

    pub async fn stop_stream(&mut self) -> Result<(), Error<E>> {
        self.start.set_low().unwrap();

        for dev in self.ads.iter_mut() {
            dev.cmd(Command::SDATAC).await?;
        }
        Ok(())
    }

    pub async fn poll(&mut self) -> Result<Vec<AdsData, N>, Error<E>> {
        self.drdy.wait_for_falling_edge().await.unwrap();

        let mut data: Vec<AdsData, N> = Vec::new();
        for dev in self.ads.iter_mut() {
            let _ = data.push(dev.rdatac().await?);
        }
        Ok(data)
    }
}
