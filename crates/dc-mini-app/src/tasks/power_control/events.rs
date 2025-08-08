use embassy_nrf::gpio::{AnyPin, Level, Output, OutputDrive};
use embassy_nrf::Peri;

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum PowerEvent {
    Enable,
    Disable,
}

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum PowerEventError {
    InvalidConversion(u8),
}

impl TryFrom<u8> for PowerEvent {
    type Error = PowerEventError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(PowerEvent::Enable),
            1 => Ok(PowerEvent::Disable),
            _ => Err(PowerEventError::InvalidConversion(value)),
        }
    }
}

pub struct PowerManager {
    count: u8,
    pwctl: Output<'static>,
}

impl PowerManager {
    pub fn new(pwctl_pin: Peri<'static, AnyPin>) -> Self {
        #[cfg(feature = "sr2")]
        let pwctl = Output::new(pwctl_pin, Level::Low, OutputDrive::Standard);
        #[cfg(not(feature = "sr2"))]
        let pwctl = Output::new(pwctl_pin, Level::High, OutputDrive::Standard);
        Self { count: 0, pwctl }
    }
    pub async fn handle_event(&mut self, event: PowerEvent) {
        match event {
            PowerEvent::Enable => {
                if self.count == 0 {
                    #[cfg(feature = "sr2")]
                    self.pwctl.set_high();
                    #[cfg(feature = "sr3")]
                    self.pwctl.set_low();
                }
                self.count = self.count.wrapping_add(1);
            }
            PowerEvent::Disable => {
                if self.count > 0 {
                    self.count = self.count.wrapping_sub(1);
                    if self.count <= 0 {
                        #[cfg(feature = "sr2")]
                        self.pwctl.set_low();
                        #[cfg(feature = "sr3")]
                        self.pwctl.set_high();
                    }
                }
            }
        }
    }
}
