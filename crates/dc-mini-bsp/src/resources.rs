use crate::board::{
    AdsResources, ExternalFlashResources, HapticResources, ImuResources,
    MicResources, SdCardResources, Spi3BusResources, Twim1BusResources,
};
use ads1299::{Ads1299, AdsFrontend};
use bus_manager::BusFactory;
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice;
use embassy_nrf::{
    bind_interrupts,
    gpio::{Input, Level, Output, OutputDrive, Pull},
    interrupt::{self, InterruptExt},
    pdm, peripherals, qspi, spim, twim,
};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::{blocking_mutex::raw::RawMutex, mutex::Mutex};
use embassy_time::Timer;
use embedded_hal_bus::spi::ExclusiveDevice;
use embedded_sdmmc::SdCard;
use grounded::uninit::GroundedArrayCell;
use heapless::Vec;
use icm_45605::Icm45605;

/// Destructor token for recovering TWIM1 peripheral resources.
pub struct Twim1Destructor;

/// Factory for creating the shared I2C bus from TWIM1 peripheral resources.
pub struct Twim1Factory;

/// DMA buffer for TWIM operations, stored in a sound `GroundedArrayCell`.
static TWIM1_DMA_BUF: GroundedArrayCell<u8, 32> =
    GroundedArrayCell::const_init();

impl BusFactory for Twim1Factory {
    type Bus = Mutex<CriticalSectionRawMutex, twim::Twim<'static>>;
    type Resources = Twim1BusResources;
    type Destructor = Twim1Destructor;
    type Error = core::convert::Infallible;

    fn create(
        resources: Self::Resources,
    ) -> Result<(Self::Bus, Self::Destructor), (Self::Error, Self::Resources)>
    {
        let config = twim::Config::default();
        interrupt::TWISPI1.set_priority(interrupt::Priority::P3);

        // SAFETY: We have exclusive access because the bus manager mutex is held
        // during create, and this is only called when transitioning Idle→Active
        // (no live references to this buffer exist). The static is `const_init`
        // so it is already zero-initialized.
        let buf: &'static mut [u8; 32] =
            unsafe { &mut *(TWIM1_DMA_BUF.as_mut_ptr() as *mut [u8; 32]) };

        let bus = Mutex::new(twim::Twim::new(
            resources.twim,
            TwimIrqs,
            resources.sda,
            resources.scl,
            config,
            buf,
        ));

        Ok((bus, Twim1Destructor))
    }

    fn recover(_destructor: Self::Destructor) -> Self::Resources {
        // SAFETY: The bus has been dropped (BusManager guarantees users == 0 and
        // drops the bus before calling recover). We reconstruct Peri wrappers
        // via steal(), which is safe because no other code holds these peripherals.
        unsafe {
            Twim1BusResources {
                twim: embassy_nrf::peripherals::TWISPI1::steal(),
                sda: embassy_nrf::peripherals::P0_04::steal(),
                scl: embassy_nrf::peripherals::P0_06::steal(),
            }
        }
    }
}

pub type PoweredAdsFrontend<'a, 'b, MutexType> = AdsFrontend<
    SpiDevice<'a, MutexType, spim::Spim<'b>, Output<'a>>,
    Output<'a>,
    Output<'a>,
    Output<'a>,
    Input<'a>,
    2,
>;

pub type Imu<'a, 'b, MutexType> =
    Icm45605<I2cDevice<'a, MutexType, twim::Twim<'b>>, embassy_time::Delay>;

pub type Haptic<'a, 'b, MutexType> =
    drv260x::Drv260x<I2cDevice<'a, MutexType, twim::Twim<'b>>>;

/// Represents a structure for an external flash configuration using the QSPI protocol.
pub type ExternalFlash<'d> = qspi::Qspi<'d>;

bind_interrupts!(struct SpiIrq {
    SPIM3 => spim::InterruptHandler<peripherals::SPI3>;
    SPI2 => spim::InterruptHandler<peripherals::SPI2>;
});

bind_interrupts!(struct TwimIrqs {
    TWISPI1 => twim::InterruptHandler<peripherals::TWISPI1>;
});

bind_interrupts!(struct PdmIrqs {
    PDM => pdm::InterruptHandler<peripherals::PDM>;
});

impl AdsResources {
    pub async fn configure<'a, 'b, MutexType: RawMutex>(
        &'a mut self,
        bus: &'a Mutex<MutexType, spim::Spim<'b>>,
    ) -> PoweredAdsFrontend<'a, 'b, MutexType> {
        let start = Output::new(
            self.start.reborrow(),
            Level::High,
            OutputDrive::Standard,
        );
        let mut reset = Output::new(
            self.reset.reborrow(),
            Level::High,
            OutputDrive::Standard,
        );
        let pwdn = Output::new(
            self.pwdn.reborrow(),
            Level::High,
            OutputDrive::Standard,
        );
        let drdy = Input::new(self.drdy.reborrow(), Pull::None);

        // Properly reset the ADS
        Timer::after_nanos(ads1299::MIN_T_POR as u64).await;
        reset.set_low();
        Timer::after_nanos(ads1299::MIN_T_RST as u64).await;
        reset.set_high();
        Timer::after_nanos(ads1299::MIN_RST_WAIT as u64).await;

        let mut ads_vec: Vec<_, 2> = Vec::new();
        // Create and check primary ADS device.
        let mut primary_ads = Ads1299::new(SpiDevice::new(
            bus,
            Output::new(
                self.cs1.reborrow(),
                Level::High,
                OutputDrive::Standard,
            ),
        ));

        match primary_ads.smell().await {
            Ok(_) => {
                let _ = ads_vec.push(primary_ads);
            }
            Err(_e) => {
                #[cfg(feature = "defmt")]
                defmt::warn!("On-board ADS not detected! {:?}", _e);
            }
        }

        // Create and check daisy ADS device.
        let mut daisy_ads = Ads1299::new(SpiDevice::new(
            bus,
            Output::new(
                self.cs2.reborrow(),
                Level::High,
                OutputDrive::Standard,
            ),
        ));
        match daisy_ads.smell().await {
            Ok(_) => {
                let _ = ads_vec.push(daisy_ads);
            }
            Err(_e) => {
                #[cfg(feature = "defmt")]
                defmt::warn!("Daisy ADS not detected! {:?}", _e);
            }
        }

        AdsFrontend::new(ads_vec, start, reset, pwdn, drdy)
    }
}

impl ImuResources {
    pub async fn configure<'a, 'b, MutexType: RawMutex>(
        &'a mut self,
        bus: &'a Mutex<MutexType, twim::Twim<'b>>,
    ) -> Imu<'a, 'b, MutexType> {
        Icm45605::new(I2cDevice::new(bus), embassy_time::Delay)
    }

    /// Configure IMU with an existing I2cDevice (for use with bus manager)
    pub async fn configure_with_device<'a, 'b, MutexType: RawMutex>(
        &'a mut self,
        device: I2cDevice<'a, MutexType, twim::Twim<'b>>,
    ) -> Icm45605<I2cDevice<'a, MutexType, twim::Twim<'b>>, embassy_time::Delay>
    {
        Icm45605::new(device, embassy_time::Delay)
    }
}

impl HapticResources {
    pub fn configure_with_device<'a, 'b, MutexType: RawMutex>(
        &'a mut self,
        device: I2cDevice<'a, MutexType, twim::Twim<'b>>,
    ) -> Haptic<'a, 'b, MutexType> {
        drv260x::Drv260x::new(device)
    }
}

impl MicResources {
    pub fn configure<'a>(
        &'a mut self,
        config: spk0838_pdm::Config,
    ) -> spk0838_pdm::Spk0838<'a> {
        spk0838_pdm::Spk0838::new(
            self.pdm.reborrow(),
            PdmIrqs,
            self.clk.reborrow(),
            self.din.reborrow(),
            config,
        )
    }
}

impl Twim1BusResources {
    pub fn get_bus<'a, MutexType: RawMutex>(
        &'a mut self,
    ) -> Mutex<MutexType, twim::Twim<'a>> {
        let config = twim::Config::default();
        interrupt::TWISPI1.set_priority(interrupt::Priority::P3);
        static RAM_BUFFER: static_cell::ConstStaticCell<[u8; 32]> =
            static_cell::ConstStaticCell::new([0; 32]);

        Mutex::new(twim::Twim::new(
            self.twim.reborrow(),
            TwimIrqs,
            self.sda.reborrow(),
            self.scl.reborrow(),
            config,
            RAM_BUFFER.take(),
        ))
    }
}

impl Spi3BusResources {
    pub fn get_bus<'a, MutexType: RawMutex>(
        &'a mut self,
    ) -> Mutex<MutexType, spim::Spim<'a>> {
        let mut config = spim::Config::default();
        config.mode = spim::MODE_1;
        config.frequency = spim::Frequency::M4;
        config.mosi_drive = OutputDrive::HighDrive;
        config.sck_drive = OutputDrive::HighDrive;
        interrupt::SPIM3.set_priority(interrupt::Priority::P3);
        Mutex::new(spim::Spim::new(
            self.spim.reborrow(),
            SpiIrq,
            self.sclk.reborrow(),
            self.miso.reborrow(),
            self.mosi.reborrow(),
            config,
        ))
    }
}

impl SdCardResources {
    pub fn get_card<'a>(
        &'a mut self,
    ) -> SdCard<
        ExclusiveDevice<spim::Spim<'a>, Output<'a>, embassy_time::Delay>,
        embassy_time::Delay,
    > {
        let mut config = spim::Config::default();
        config.mode = spim::MODE_0;
        config.frequency = spim::Frequency::K250;
        interrupt::SPI2.set_priority(interrupt::Priority::P3);

        // We first need to create the spi driver with a low frequency clock to correctly
        // initialize the SD card.
        let mut cs_pin = Output::new(
            self.cs.reborrow(),
            Level::High,
            OutputDrive::Standard,
        );

        // Create SPI with final configuration directly
        config.frequency = spim::Frequency::M16;
        let spi = spim::Spim::new(
            self.spim.reborrow(),
            SpiIrq,
            self.sclk.reborrow(),
            self.miso.reborrow(),
            self.mosi.reborrow(),
            config.clone(),
        );
        // Initialize SD card with dummy bytes
        cs_pin.set_high();
        // Note: SD card initialization is now handled by the SdCard driver itself

        let spi = ExclusiveDevice::new(spi, cs_pin, embassy_time::Delay)
            .expect("Failed to create SD card spi device.");
        SdCard::new(spi, embassy_time::Delay)
    }
}

impl ExternalFlashResources {
    /// Configures an external flash instance based on the defined pins.
    ///
    /// # Returns
    /// An initialized `ExternalFlash` instance.
    pub fn configure<'a>(&'a mut self) -> ExternalFlash<'a> {
        bind_interrupts!(struct Irqs {
            QSPI => qspi::InterruptHandler<peripherals::QSPI>;
        });

        let mut config = qspi::Config::default();
        config.capacity = 2048 * 1024; // 2048K-Byte
        config.frequency = qspi::Frequency::M16;
        config.read_opcode = qspi::ReadOpcode::READ4IO;
        config.write_opcode = qspi::WriteOpcode::PP4O;
        config.write_page_size = qspi::WritePageSize::_256BYTES;

        interrupt::QSPI.set_priority(interrupt::Priority::P3);

        let mut q = qspi::Qspi::new(
            self.qspi.reborrow(),
            Irqs,
            self.sck.reborrow(),
            self.csn.reborrow(),
            self.io0.reborrow(),
            self.io1.reborrow(),
            self.io2.reborrow(),
            self.io3.reborrow(),
            config,
        );

        // Setup QSPI
        let mut status = [4; 2];
        q.blocking_custom_instruction(0x05, &[], &mut status[..1]).unwrap();

        q.blocking_custom_instruction(0x35, &[], &mut status[1..2]).unwrap();

        if status[1] & 0x02 == 0 {
            status[1] |= 0x02;
            q.blocking_custom_instruction(0x01, &status, &mut []).unwrap();
        }
        q
    }
}
