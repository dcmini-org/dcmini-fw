//! nRF USB Driver Peripheral
use embassy_nrf::peripherals;
use embassy_nrf::usb::Driver;
use embassy_nrf::Peri;
use embassy_nrf::{bind_interrupts, usb};

/// USB Driver Builder.
pub struct UsbDriverBuilder {
    /// USB peripheral
    usbd: Peri<'static, peripherals::USBD>,
}

bind_interrupts!(pub struct UsbIrqs {
    USBD => usb::InterruptHandler<peripherals::USBD>;
    #[cfg(not(feature="trouble"))]
    CLOCK_POWER => usb::vbus_detect::InterruptHandler;
});

impl UsbDriverBuilder {
    /// Create a new instance of the USB driver builder
    pub(crate) fn new(usbd: Peri<'static, peripherals::USBD>) -> Self {
        Self { usbd }
    }

    cfg_if::cfg_if! {
        if #[cfg(feature = "trouble")] {
            pub fn init<'a>(
                self,
            ) -> Driver<'a, embassy_nrf::usb::vbus_detect::HardwareVbusDetect> {
                Driver::new(
                    self.usbd,
                    UsbIrqs,
                    embassy_nrf::usb::vbus_detect::HardwareVbusDetect::new(crate::ble::BleIrqs),
                )
            }
        }
        else {
            pub fn init<'a>(
                self,
            ) -> Driver<'a, embassy_nrf::usb::vbus_detect::HardwareVbusDetect> {
                Driver::new(self.usbd, UsbIrqs, embassy_nrf::usb::vbus_detect::HardwareVbusDetect::new(UsbIrqs))
            }
        }
    }
}
