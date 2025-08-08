//! nRF Softdevice Controller Configuration for Bluetooth Peripheral
//!
//! Used with `trouble-host` crate.

use embassy_nrf::peripherals;
use embassy_nrf::{bind_interrupts, rng, Peripheral};
use nrf_sdc::{self as sdc, mpsl};
pub use nrf_sdc::{
    mpsl::MultiprotocolServiceLayer, Error as SoftdeviceError,
    SoftdeviceController,
};
use static_cell::StaticCell;

#[cfg(feature = "usb")]
use embassy_nrf::usb;

/// Default memory allocation for softdevice controller in bytes.
/// - Minimum 2168 bytes,
/// - maximum associated with [task-arena-size](https://docs.embassy.dev/embassy-executor/git/cortex-m/index.html)
const SDC_MEMORY_SIZE: usize = 1448; // bytes

/// Softdevice Bluetooth Controller Builder.
pub struct BleControllerBuilder<'d> {
    /// Softdevice Controller peripherals
    sdc_peripherals: sdc::Peripherals<'d>,
    /// Softdevice Controller memory
    sdc_mem: sdc::Mem<SDC_MEMORY_SIZE>,
    // Required peripherals for the Multiprotocol Service Layer (MPSL)
    rtc0: peripherals::RTC0,
    temp: peripherals::TEMP,
    ppi_ch19: peripherals::PPI_CH19,
    ppi_ch30: peripherals::PPI_CH30,
    ppi_ch31: peripherals::PPI_CH31,
}

bind_interrupts!(pub struct BleIrqs {
    RNG => rng::InterruptHandler<peripherals::RNG>;
    EGU2_SWI2 => nrf_sdc::mpsl::LowPrioInterruptHandler;
    RADIO => nrf_sdc::mpsl::HighPrioInterruptHandler;
    TIMER0 => nrf_sdc::mpsl::HighPrioInterruptHandler;
    RTC0 => nrf_sdc::mpsl::HighPrioInterruptHandler;
    #[cfg(not(feature = "usb"))]
    CLOCK_POWER => nrf_sdc::mpsl::ClockInterruptHandler;
    #[cfg(feature = "usb")]
    CLOCK_POWER => nrf_sdc::mpsl::ClockInterruptHandler, usb::vbus_detect::InterruptHandler;
});

impl<'d> BleControllerBuilder<'d>
where
    'd: 'static,
{
    /// Low frequency clock configuration
    const LF_CLOCK_CONFIG: mpsl::raw::mpsl_clock_lfclk_cfg_t =
        mpsl::raw::mpsl_clock_lfclk_cfg_t {
            source: mpsl::raw::MPSL_CLOCK_LF_SRC_RC as u8,
            rc_ctiv: mpsl::raw::MPSL_RECOMMENDED_RC_CTIV as u8,
            rc_temp_ctiv: mpsl::raw::MPSL_RECOMMENDED_RC_TEMP_CTIV as u8,
            accuracy_ppm: mpsl::raw::MPSL_DEFAULT_CLOCK_ACCURACY_PPM as u16,
            skip_wait_lfclk_started:
                mpsl::raw::MPSL_DEFAULT_SKIP_WAIT_LFCLK_STARTED != 0,
        };
    /// Create a new instance of the Softdevice Controller BLE builder
    pub(crate) fn new(
        rtc0: peripherals::RTC0,
        temp: peripherals::TEMP,
        ppi_ch17: peripherals::PPI_CH17,
        ppi_ch18: peripherals::PPI_CH18,
        ppi_ch19: peripherals::PPI_CH19,
        ppi_ch20: peripherals::PPI_CH20,
        ppi_ch21: peripherals::PPI_CH21,
        ppi_ch22: peripherals::PPI_CH22,
        ppi_ch23: peripherals::PPI_CH23,
        ppi_ch24: peripherals::PPI_CH24,
        ppi_ch25: peripherals::PPI_CH25,
        ppi_ch26: peripherals::PPI_CH26,
        ppi_ch27: peripherals::PPI_CH27,
        ppi_ch28: peripherals::PPI_CH28,
        ppi_ch29: peripherals::PPI_CH29,
        ppi_ch30: peripherals::PPI_CH30,
        ppi_ch31: peripherals::PPI_CH31,
    ) -> Self {
        // Softdevice Controller peripherals
        let sdc_peripherals = sdc::Peripherals::new(
            ppi_ch17, ppi_ch18, ppi_ch20, ppi_ch21, ppi_ch22, ppi_ch23,
            ppi_ch24, ppi_ch25, ppi_ch26, ppi_ch27, ppi_ch28, ppi_ch29,
        );

        let sdc_mem = sdc::Mem::<SDC_MEMORY_SIZE>::new();
        Self {
            sdc_peripherals,
            sdc_mem,
            rtc0,
            temp,
            ppi_ch19,
            ppi_ch30,
            ppi_ch31,
        }
    }

    pub fn init(
        self,
        timer0: impl Peripheral<P = peripherals::TIMER0> + 'd,
        rng: impl Peripheral<P = peripherals::RNG> + 'd,
    ) -> Result<
        (SoftdeviceController<'d>, &'static MultiprotocolServiceLayer<'d>),
        SoftdeviceError,
    > {
        let mpsl = {
            let p = mpsl::Peripherals::new(
                self.rtc0,
                timer0,
                self.temp,
                self.ppi_ch19,
                self.ppi_ch30,
                self.ppi_ch31,
            );
            mpsl::MultiprotocolServiceLayer::new(
                p,
                BleIrqs,
                Self::LF_CLOCK_CONFIG,
            )
        }?;
        let sdc_rng = {
            static SDC_RNG: StaticCell<rng::Rng<'static, peripherals::RNG>> =
                StaticCell::new();
            SDC_RNG.init(rng::Rng::new(rng, BleIrqs))
        };
        let mem = {
            static SDC_MEM: StaticCell<sdc::Mem<SDC_MEMORY_SIZE>> =
                StaticCell::new();
            SDC_MEM.init(self.sdc_mem)
        };
        let mpsl = {
            static MPSL: StaticCell<MultiprotocolServiceLayer> =
                StaticCell::new();
            MPSL.init(mpsl)
        };
        let sdc = build_sdc(self.sdc_peripherals, sdc_rng, mpsl, mem)?;
        Ok((sdc, mpsl))
    }
}

/// Build the Softdevice Controller layer to pass to trouble-host
fn build_sdc<'d, const N: usize>(
    p: nrf_sdc::Peripherals<'d>,
    rng: &'d mut rng::Rng<peripherals::RNG>,
    mpsl: &'d MultiprotocolServiceLayer,
    mem: &'d mut sdc::Mem<N>,
) -> Result<nrf_sdc::SoftdeviceController<'d>, SoftdeviceError> {
    sdc::Builder::new()?
        .support_adv()?
        .support_peripheral()?
        .peripheral_count(1)?
        // .buffer_cfg(128 as u8, 128 as u8, L2CAP_TXQ, L2CAP_RXQ)? // this is missing
        .build(p, rng, mpsl, mem)
}
