//! Low level register and interface definitions
use core::slice;
use device_driver::AsyncRegisterInterface;
use embedded_hal_async::{delay, i2c};
use heapless::Vec;

const ADDR: u8 = 0b1101000; // AP_AD0 = 0
                            // const ADDR: u8 = 0b1101001; // AP_AD0 = 1

#[derive(derive_more::From, Debug, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum DeviceInterfaceError<I2cError> {
    I2c(I2cError),
    OutOfBounds,
    InvalidBank,
    Timeout,
    HeaplessExtendFailed,
}

device_driver::create_device!(
    device_name: Device,
    manifest: "device.yaml"
);

#[derive(Debug)]
pub struct DeviceInterface<I2c: i2c::I2c, D: delay::DelayNs> {
    pub i2c: I2c,
    pub(crate) delay: D,
}

// Constants for indirect register access
const IREG_ADDR_15_8: u8 = 0x7c; // High byte of indirect register address
const IREG_DATA: u8 = 0x7e; // Data register for indirect access
const DELAY_US: u32 = 4; // Delay between operations

impl<I2c: i2c::I2c, D: delay::DelayNs> DeviceInterface<I2c, D> {
    /// Check if an indirect register access would be out of bounds
    fn check_out_of_bounds_mreg(
        reg: u16,
        len: u16,
    ) -> Result<(), DeviceInterfaceError<I2c::Error>> {
        let min_addr = reg;
        let max_addr = reg + len - 1;

        // Check forbidden address ranges as per AN-000364
        if ((min_addr > 0x000023FF) && (min_addr <= 0x00003FFF))
            || ((max_addr > 0x000023FF) && (max_addr <= 0x00003FFF))
            || ((min_addr <= 0x000023FF) && (max_addr > 0x00003FFF))
            || ((min_addr > 0x000083FF) && (min_addr <= 0x00009FFF))
            || ((max_addr > 0x000083FF) && (max_addr <= 0x00009FFF))
            || ((min_addr <= 0x000083FF) && (max_addr > 0x00009FFF))
            || (max_addr > 0x0000AFFF)
        {
            return Err(DeviceInterfaceError::OutOfBounds);
        }

        Ok(())
    }

    /// Read from a direct register
    async fn read_dreg(
        &mut self,
        reg: u8,
        buf: &mut [u8],
    ) -> Result<(), DeviceInterfaceError<I2c::Error>> {
        self.i2c
            .write_read(ADDR, &[reg], buf)
            .await
            .map_err(DeviceInterfaceError::I2c)
    }

    /// Write to a direct register
    async fn write_dreg(
        &mut self,
        reg: u8,
        buf: &[u8],
    ) -> Result<(), DeviceInterfaceError<I2c::Error>> {
        let mut write_buf = Vec::<u8, 32>::new();
        write_buf
            .extend_from_slice(&[reg])
            .map_err(|_| DeviceInterfaceError::HeaplessExtendFailed)?;
        write_buf
            .extend_from_slice(buf)
            .map_err(|_| DeviceInterfaceError::HeaplessExtendFailed)?;

        self.i2c
            .write(ADDR, &write_buf)
            .await
            .map_err(DeviceInterfaceError::I2c)
    }

    /// Read from an indirect register
    async fn read_mreg(
        &mut self,
        reg: u16,
        buf: &mut [u8],
    ) -> Result<(), DeviceInterfaceError<I2c::Error>> {
        Self::check_out_of_bounds_mreg(reg, buf.len() as u16)?;

        // Write address first
        let addr_bytes = [(reg >> 8) as u8, reg as u8];
        self.delay.delay_us(DELAY_US).await;
        self.write_dreg(IREG_ADDR_15_8, &addr_bytes).await?;

        // Read data bytes one by one
        for byte in buf.iter_mut() {
            self.delay.delay_us(DELAY_US).await;
            self.read_dreg(IREG_DATA, slice::from_mut(byte)).await?;
        }

        Ok(())
    }

    /// Write to an indirect register
    async fn write_mreg(
        &mut self,
        reg: u16,
        buf: &[u8],
    ) -> Result<(), DeviceInterfaceError<I2c::Error>> {
        Self::check_out_of_bounds_mreg(reg, buf.len() as u16)?;

        // Write address and first byte
        let mut write_buf = [0u8; 3];
        write_buf[0] = (reg >> 8) as u8;
        write_buf[1] = reg as u8;
        write_buf[2] = buf[0];

        self.delay.delay_us(DELAY_US).await;
        self.write_dreg(IREG_ADDR_15_8, &write_buf).await?;
        self.delay.delay_us(DELAY_US).await;

        // Write remaining bytes
        for byte in buf.iter().skip(1) {
            self.write_dreg(IREG_DATA, slice::from_ref(byte)).await?;
            self.delay.delay_us(DELAY_US).await;
        }

        Ok(())
    }

    /// Read from SRAM (always uses indirect access)
    pub async fn read_sram(
        &mut self,
        addr: u16,
        buf: &mut [u8],
    ) -> Result<(), DeviceInterfaceError<I2c::Error>> {
        self.read_mreg(addr, buf).await
    }

    /// Write to SRAM (always uses indirect access)
    pub async fn write_sram(
        &mut self,
        addr: u16,
        buf: &[u8],
    ) -> Result<(), DeviceInterfaceError<I2c::Error>> {
        self.write_mreg(addr, buf).await
    }

    pub fn new(i2c: I2c, delay: D) -> Self {
        Self { i2c, delay }
    }
}

impl<I2c: i2c::I2c, D: delay::DelayNs> AsyncRegisterInterface
    for DeviceInterface<I2c, D>
{
    type AddressType = u16;
    type Error = DeviceInterfaceError<I2c::Error>;

    async fn read_register(
        &mut self,
        address: Self::AddressType,
        _size_bits: u32,
        data: &mut [u8],
    ) -> Result<(), Self::Error> {
        if address >= 0xb000 {
            self.read_sram(address & 0x0FFF, data).await
        } else if address > 0xFF {
            self.read_mreg(address, data).await
        } else {
            self.read_dreg(address as u8, data).await
        }
    }

    async fn write_register(
        &mut self,
        address: Self::AddressType,
        _size_bits: u32,
        data: &[u8],
    ) -> Result<(), Self::Error> {
        if address >= 0xb000 {
            self.write_sram(address & 0x0FFF, data).await
        } else if address > 0xFF {
            self.write_mreg(address, data).await
        } else {
            self.write_dreg(address as u8, data).await
        }
    }
}
