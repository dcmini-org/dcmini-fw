//! nRF SDC BLE Controller Configuration for Bluetooth Peripheral
//!
//! Used with `trouble-host` crate.

use embassy_nrf::mode::Async;
use embassy_nrf::peripherals;
use embassy_nrf::{bind_interrupts, rng, Peri};
use nrf_sdc::{self as sdc, mpsl};
pub use nrf_sdc::{
    mpsl::MultiprotocolServiceLayer, Error as SoftdeviceError,
    SoftdeviceController,
};
use static_cell::StaticCell;

#[cfg(feature = "usb")]
use embassy_nrf::usb;

/// How many outgoing L2CAP buffers per link
const L2CAP_TXQ: u8 = 3;

/// How many incoming L2CAP buffers per link
const L2CAP_RXQ: u8 = 3;

/// L2CAP packet MTU — must match trouble-host's DefaultPacketPool::MTU (251).
const L2CAP_MTU: u16 = 251;

/// Memory allocation for SDC BLE controller in bytes.
/// Must be large enough to accommodate the configured buffer sizes and connection count.
const SDC_MEMORY_SIZE: usize = 4720;

/// SDC BLE Controller Builder.
pub struct BleControllerBuilder<'d> {
    /// SDC Controller peripherals
    sdc_peripherals: sdc::Peripherals<'d>,
    /// SDC Controller memory
    sdc_mem: sdc::Mem<SDC_MEMORY_SIZE>,
    // Required peripherals for the Multiprotocol Service Layer (MPSL)
    rtc0: Peri<'d, peripherals::RTC0>,
    temp: Peri<'d, peripherals::TEMP>,
    ppi_ch19: Peri<'d, peripherals::PPI_CH19>,
    ppi_ch30: Peri<'d, peripherals::PPI_CH30>,
    ppi_ch31: Peri<'d, peripherals::PPI_CH31>,
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
    /// Create a new instance of the SDC BLE controller builder
    pub(crate) fn new(
        rtc0: Peri<'d, peripherals::RTC0>,
        temp: Peri<'d, peripherals::TEMP>,
        ppi_ch17: Peri<'d, peripherals::PPI_CH17>,
        ppi_ch18: Peri<'d, peripherals::PPI_CH18>,
        ppi_ch19: Peri<'d, peripherals::PPI_CH19>,
        ppi_ch20: Peri<'d, peripherals::PPI_CH20>,
        ppi_ch21: Peri<'d, peripherals::PPI_CH21>,
        ppi_ch22: Peri<'d, peripherals::PPI_CH22>,
        ppi_ch23: Peri<'d, peripherals::PPI_CH23>,
        ppi_ch24: Peri<'d, peripherals::PPI_CH24>,
        ppi_ch25: Peri<'d, peripherals::PPI_CH25>,
        ppi_ch26: Peri<'d, peripherals::PPI_CH26>,
        ppi_ch27: Peri<'d, peripherals::PPI_CH27>,
        ppi_ch28: Peri<'d, peripherals::PPI_CH28>,
        ppi_ch29: Peri<'d, peripherals::PPI_CH29>,
        ppi_ch30: Peri<'d, peripherals::PPI_CH30>,
        ppi_ch31: Peri<'d, peripherals::PPI_CH31>,
    ) -> Self {
        // SDC peripherals
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
        timer0: Peri<'d, peripherals::TIMER0>,
        rng: Peri<'d, peripherals::RNG>,
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
            static SDC_RNG: StaticCell<rng::Rng<'static, Async>> =
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

/// Build the SDC controller layer to pass to trouble-host
fn build_sdc<'d, const N: usize>(
    p: nrf_sdc::Peripherals<'d>,
    rng: &'d mut rng::Rng<'d, Async>,
    mpsl: &'d MultiprotocolServiceLayer,
    mem: &'d mut sdc::Mem<N>,
) -> Result<nrf_sdc::SoftdeviceController<'d>, SoftdeviceError> {
    sdc::Builder::new()?
        .support_adv()
        .support_peripheral()
        .peripheral_count(1)?
        .buffer_cfg(L2CAP_MTU, L2CAP_MTU, L2CAP_TXQ, L2CAP_RXQ)?
        .build(p, rng, mpsl, mem)
}
