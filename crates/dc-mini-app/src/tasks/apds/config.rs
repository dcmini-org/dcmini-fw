use crate::prelude::*;
use apds9253::Apds9253;
use embassy_nrf::twim;

pub type I2cDev<'a> = embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice<
    'a,
    CriticalSectionRawMutex,
    twim::Twim<'static>,
>;

pub async fn apply_apds_config(
    sensor: &mut Apds9253<I2cDev<'_>>,
    config: &ApdsConfig,
) {
    unwrap!(sensor.set_gain_async(config.gain.into()).await);
    unwrap!(sensor.set_resolution_async(config.resolution.into()).await);
    unwrap!(
        sensor
            .set_measurement_rate_async(config.measurement_rate.into())
            .await
    );
    unwrap!(sensor.enable_rgb_mode_async(config.rgb_mode).await);
    unwrap!(sensor.enable_async(true).await);
}
