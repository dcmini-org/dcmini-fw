use std::sync::Arc;

mod ble;
mod usb;

pub use ble::BleClient;
pub use usb::{UsbClient, UsbError};

#[derive(Clone)]
pub enum DeviceConnection {
    Usb(Arc<UsbClient>),
    Ble(Arc<BleClient>),
}
