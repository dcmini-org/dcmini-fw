use embassy_nrf::interrupt::Priority;
use embassy_nrf::peripherals::{
    self, I2S, NVMC, P0_00, P0_02, P0_03, P0_11, P0_12, P0_27, P0_30, P0_31,
    P1_01, P1_02, P1_03, P1_04, P1_05, P1_06, P1_07, P1_09, P1_11, P1_12,
    PDM, PWM0, PWM1, PWM2, PWM3, QDEC, RNG, RTC2, SAADC, TIMER0, TIMER1,
    TIMER2, TIMER3, TIMER4, TWISPI0, UARTE0, UARTE1, WDT,
};
use embassy_nrf::Peri;

#[cfg(feature = "trouble")]
use crate::ble;
#[cfg(feature = "usb")]
use crate::usb;

// Need 3.3V rail for following:
// - inidicator LED neopixel
// - haptic driver
// - SD card
// - AFE of ADS1299

pub struct ImuResources {
    pub irq: Peri<'static, peripherals::P0_01>,
    pub sync: Peri<'static, peripherals::P0_08>,
}

pub struct Twim1BusResources {
    pub twim: Peri<'static, peripherals::TWISPI1>,
    pub sda: Peri<'static, peripherals::P0_04>,
    pub scl: Peri<'static, peripherals::P0_06>,
}

pub struct AdsResources {
    pub pwdn: Peri<'static, peripherals::P0_24>,
    pub reset: Peri<'static, peripherals::P0_17>,
    pub start: Peri<'static, peripherals::P0_15>,
    pub cs1: Peri<'static, peripherals::P0_16>,
    pub cs2: Peri<'static, peripherals::P0_18>,
    pub drdy: Peri<'static, peripherals::P0_28>,
}

pub struct Spi3BusResources {
    pub sclk: Peri<'static, peripherals::P0_13>,
    pub mosi: Peri<'static, peripherals::P0_25>,
    pub miso: Peri<'static, peripherals::P0_14>,
    pub spim: Peri<'static, peripherals::SPI3>,
}

pub struct SdCardResources {
    pub sclk: Peri<'static, peripherals::P0_05>,
    pub mosi: Peri<'static, peripherals::P0_07>,
    pub miso: Peri<'static, peripherals::P0_26>,
    pub cs: Peri<'static, peripherals::P1_08>,
    pub sdio: Peri<'static, peripherals::P0_29>,
    pub spim: Peri<'static, peripherals::SPI2>,
}

pub struct MicResources {
    pub pdm: Peri<'static, PDM>,
    pub clk: Peri<'static, P0_27>,
    pub din: Peri<'static, P0_00>,
}

/// Pins for External QSPI flash
pub struct ExternalFlashResources {
    /// The QSPI instance.
    pub qspi: Peri<'static, peripherals::QSPI>,
    /// The Serial Clock Line (SCLK) pin.
    pub sck: Peri<'static, peripherals::P0_19>,
    /// The Chip Select (CSN) pin.
    pub csn: Peri<'static, peripherals::P0_20>,
    /// Input/Output pin 0.
    pub io0: Peri<'static, peripherals::P1_00>,
    /// Input/Output pin 1.
    pub io1: Peri<'static, peripherals::P0_21>,
    /// Input/Output pin 2.
    pub io2: Peri<'static, peripherals::P0_22>,
    /// Input/Output pin 3.
    pub io3: Peri<'static, peripherals::P0_23>,
}

/// Represents all the peripherals and pins available for the DCMini device.
pub struct DCMini {
    /// Pulled low means ext vbus
    /// Pulled high means through usb isolator
    /// Needs internal pull up
    /// If vbus connected through EXT, don't allow EEG
    pub vbus_src: Peri<'static, P1_11>,
    /// Pin for the user/power button.
    pub pwrbtn: Peri<'static, P0_31>,
    /// Pin to control Neopixels.
    pub neopix: Peri<'static, P0_11>,
    /// PDM microphone resources (SPK0838HT4H).
    pub mic: MicResources,
    /// Interrupt pin for the ambient light sensor.
    pub apds_irq: Peri<'static, P1_09>,
    /// Power enable for 5V rail
    /// pull low to turn on 5V rail.
    pub en5v: Peri<'static, P0_30>,
    /// Haptics engine trigger
    pub haptrig: Peri<'static, P1_02>,

    // USB Select, set default pull-up,
    // down when we want to use usb that
    // is connected on board to board connector
    pub usbsel: Peri<'static, P1_01>,

    // General purpose nRF gpio that connects to b2b connector.
    pub nrf_gpio1: Peri<'static, P1_03>,
    pub nrf_gpio2: Peri<'static, P1_06>,
    pub nrf_gpio3: Peri<'static, P0_03>,
    pub nrf_gpio4: Peri<'static, P0_12>,
    pub nrf_gpio5: Peri<'static, P1_05>,
    pub nrf_gpio6: Peri<'static, P1_07>,
    pub nrf_gpio7: Peri<'static, P1_04>,
    pub nrf_gpio8: Peri<'static, P0_02>,

    // Power Chip Interrupt (useful for power low interrupt)
    pub npm_gpio: Peri<'static, P1_12>,

    /// Configuration pins for external flash memory.
    pub external_flash: ExternalFlashResources,
    /// Peripherals for ADS1299.
    pub ads_resources: AdsResources,
    /// Peripherals for SPI 3 bus.
    pub spi3_bus_resources: Spi3BusResources,
    /// Peripherals for SD Card.
    pub sd_card_resources: SdCardResources,
    /// Peripherals for I2C bus.
    pub twim1_bus_resources: Twim1BusResources,
    /// Peripherals for the Imu.
    pub imu_resources: ImuResources,
    /// Real-Time Clock 2.
    pub rtc2: Peri<'static, RTC2>,
    /// Watchdog Timer.
    pub wdt: Peri<'static, WDT>,
    /// Non-Volatile Memory Controller.
    pub nvmc: Peri<'static, NVMC>,
    /// Random Number Generator.
    pub rng: Peri<'static, RNG>,
    /// Quadrature Decoder.
    pub qdec: Peri<'static, QDEC>,
    /// UART (Universal Asynchronous Receiver-Transmitter) 0.
    pub uarte0: Peri<'static, UARTE0>,
    /// UART (Universal Asynchronous Receiver-Transmitter) 1.
    pub uarte1: Peri<'static, UARTE1>,
    /// Two-Wire Interface/SPI 0.
    pub twispi0: Peri<'static, TWISPI0>,
    /// Successive Approximation Analog-to-Digital Converter.
    pub saadc: Peri<'static, SAADC>,
    /// Pulse-Width Modulation 0.
    pub pwm0: Peri<'static, PWM0>,
    /// Pulse-Width Modulation 1.
    pub pwm1: Peri<'static, PWM1>,
    /// Pulse-Width Modulation 2.
    pub pwm2: Peri<'static, PWM2>,
    /// Pulse-Width Modulation 3.
    pub pwm3: Peri<'static, PWM3>,
    /// Timer 0.
    pub timer0: Peri<'static, TIMER0>,
    /// Timer 1.
    pub timer1: Peri<'static, TIMER1>,
    /// Timer 2.
    pub timer2: Peri<'static, TIMER2>,
    /// Timer 3.
    pub timer3: Peri<'static, TIMER3>,
    /// Timer 4.
    pub timer4: Peri<'static, TIMER4>,
    /// Inter-IC Sound.
    pub i2s: Peri<'static, I2S>,
    #[cfg(feature = "trouble")]
    /// Bluetooth Low Energy peripheral
    pub ble: ble::BleControllerBuilder<'static>,
    #[cfg(feature = "usb")]
    /// USB driver builder
    pub usb: usb::UsbDriverBuilder,
}

impl Default for DCMini {
    fn default() -> Self {
        let mut config = embassy_nrf::config::Config::default();
        config.gpiote_interrupt_priority = Priority::P2;
        config.time_interrupt_priority = Priority::P2;
        Self::new(config)
    }
}

impl DCMini {
    /// Create a new instance based on HAL configuration
    pub fn new(config: embassy_nrf::config::Config) -> Self {
        let p = embassy_nrf::init(config);

        Self {
            vbus_src: p.P1_11,
            pwrbtn: p.P0_31,
            neopix: p.P0_11,
            mic: MicResources {
                pdm: p.PDM,
                clk: p.P0_27,
                din: p.P0_00,
            },
            apds_irq: p.P1_09,
            en5v: p.P0_30,
            haptrig: p.P1_02,
            usbsel: p.P1_01,
            nrf_gpio1: p.P1_03,
            nrf_gpio2: p.P1_06,
            nrf_gpio3: p.P0_03,
            nrf_gpio4: p.P0_12,
            nrf_gpio5: p.P1_05,
            nrf_gpio6: p.P1_07,
            nrf_gpio7: p.P1_04,
            nrf_gpio8: p.P0_02,
            npm_gpio: p.P1_12,
            rtc2: p.RTC2,
            wdt: p.WDT,
            nvmc: p.NVMC,
            rng: p.RNG,
            qdec: p.QDEC,
            uarte0: p.UARTE0,
            uarte1: p.UARTE1,
            twispi0: p.TWISPI0,
            saadc: p.SAADC,
            pwm0: p.PWM0,
            pwm1: p.PWM1,
            pwm2: p.PWM2,
            pwm3: p.PWM3,
            timer0: p.TIMER0,
            timer1: p.TIMER1,
            timer2: p.TIMER2,
            timer3: p.TIMER3,
            timer4: p.TIMER4,
            i2s: p.I2S,
            external_flash: ExternalFlashResources {
                qspi: p.QSPI,
                sck: p.P0_19,
                csn: p.P0_20,
                io0: p.P1_00,
                io1: p.P0_21,
                io2: p.P0_22,
                io3: p.P0_23,
            },
            ads_resources: AdsResources {
                pwdn: p.P0_24,
                reset: p.P0_17,
                start: p.P0_15,
                cs1: p.P0_16,
                cs2: p.P0_18,
                drdy: p.P0_28,
            },
            spi3_bus_resources: Spi3BusResources {
                sclk: p.P0_13,
                mosi: p.P0_25,
                miso: p.P0_14,
                spim: p.SPI3,
            },
            sd_card_resources: SdCardResources {
                sclk: p.P0_05,
                mosi: p.P0_07,
                miso: p.P0_26,
                cs: p.P1_08,
                sdio: p.P0_29,
                spim: p.SPI2,
            },
            twim1_bus_resources: Twim1BusResources {
                twim: p.TWISPI1,
                sda: p.P0_04,
                scl: p.P0_06,
            },
            imu_resources: ImuResources { irq: p.P0_01, sync: p.P0_08 },
            #[cfg(feature = "trouble")]
            ble: ble::BleControllerBuilder::new(
                p.RTC0, p.TEMP, p.PPI_CH17, p.PPI_CH18, p.PPI_CH19,
                p.PPI_CH20, p.PPI_CH21, p.PPI_CH22, p.PPI_CH23, p.PPI_CH24,
                p.PPI_CH25, p.PPI_CH26, p.PPI_CH27, p.PPI_CH28, p.PPI_CH29,
                p.PPI_CH30, p.PPI_CH31,
            ),
            #[cfg(feature = "usb")]
            usb: usb::UsbDriverBuilder::new(p.USBD),
        }
    }
}
