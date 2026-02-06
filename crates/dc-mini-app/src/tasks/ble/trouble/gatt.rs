use super::{ads::*, session::*};
use crate::prelude::*;
use heapless::Vec;
use trouble_host::prelude::*;

// Helper macro to handle single-field updates
macro_rules! handle_single_field_read {
    // For fields that need type conversion
    ($server:expr, $field:ident, $config:expr, $type:ty) => {
        unwrap!($server.set(&$server.ads.$field, &($config.$field as u8)));
    };
    // For fields that don't need conversion
    ($server:expr, $field:ident, $config:expr) => {
        unwrap!($server.set(&$server.ads.$field, &$config.$field));
    };
}

// Helper macro to handle vector field updates
macro_rules! handle_vector_field_read {
    // For fields that need type conversion
    ($server:expr, $field:ident, $config:expr, $type:ty) => {
        let mut values = Vec::new();
        for channel in $config.channels.iter() {
            values.push(channel.$field as u8).unwrap();
        }
        while !values.is_full() {
            values.push(0).unwrap();
        }
        unwrap!($server.set(&$server.ads.$field, &values));
    };
    // For boolean fields
    ($server:expr, $field:ident, $config:expr) => {
        let mut values = Vec::new();
        for channel in $config.channels.iter() {
            values.push(if channel.$field { 1 } else { 0 }).unwrap();
        }
        while !values.is_full() {
            values.push(0).unwrap();
        }
        unwrap!($server.set(&$server.ads.$field, &values));
    };
}

// Helper macro to handle single-field updates
macro_rules! handle_single_field_write {
    // For fields that need type conversion
    ($server:expr, $field:ident, $config:expr, $type:ty) => {
        if let Ok(value) = $server.get(&$server.ads.$field) {
            $config.$field = <$type>::from(value);
        }
    };
    // For fields that don't need conversion
    ($server:expr, $field:ident, $config:expr) => {
        if let Ok(value) = $server.get(&$server.ads.$field) {
            $config.$field = value;
        }
    };
}

// Helper macro to handle vector field updates
macro_rules! handle_vector_field_write {
    // For fields that need type conversion
    ($server:expr, $field:ident, $config:expr, $type:ty) => {
        if let Ok(values) = $server.get(&$server.ads.$field) {
            for (i, &value) in values.iter().enumerate() {
                if let Some(channel) = $config.channels.get_mut(i) {
                    channel.$field = <$type>::from(value);
                }
            }
        }
    };
    // For boolean fields
    ($server:expr, $field:ident, $config:expr) => {
        if let Ok(values) = $server.get(&$server.ads.$field) {
            for (i, &value) in values.iter().enumerate() {
                if let Some(channel) = $config.channels.get_mut(i) {
                    channel.$field = value != 0;
                }
            }
        }
    };
}

#[gatt_server]
pub struct Server {
    pub battery: BatteryService,
    pub device_info: DeviceInfoService,
    pub profile: ProfileService,
    pub ads: AdsService,
    pub session: SessionService,
}

impl<'d> Server<'d> {
    pub async fn handle_read_event(
        &self,
        handle: u16,
        app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    ) {
        let mut app_ctx = app_context.lock().await;
        let profile_manager = &mut app_ctx.profile_manager;
        let ads_config =
            unwrap!(profile_manager.get_ads_config().await).clone();

        // Match on characteristic handle
        if handle == self.ads.daisy_en.handle {
            handle_single_field_read!(self, daisy_en, ads_config);
        } else if handle == self.ads.clk_en.handle {
            handle_single_field_read!(self, clk_en, ads_config);
        } else if handle == self.ads.sample_rate.handle {
            handle_single_field_read!(
                self,
                sample_rate,
                ads_config,
                dc_mini_icd::SampleRate
            );
        } else if handle == self.ads.internal_calibration.handle {
            handle_single_field_read!(self, internal_calibration, ads_config);
        } else if handle == self.ads.calibration_amplitude.handle {
            handle_single_field_read!(self, calibration_amplitude, ads_config);
        } else if handle == self.ads.calibration_frequency.handle {
            handle_single_field_read!(
                self,
                calibration_frequency,
                ads_config,
                dc_mini_icd::CalFreq
            );
        } else if handle == self.ads.pd_refbuf.handle {
            handle_single_field_read!(self, pd_refbuf, ads_config);
        } else if handle == self.ads.bias_meas.handle {
            handle_single_field_read!(self, bias_meas, ads_config);
        } else if handle == self.ads.biasref_int.handle {
            handle_single_field_read!(self, biasref_int, ads_config);
        } else if handle == self.ads.pd_bias.handle {
            handle_single_field_read!(self, pd_bias, ads_config);
        } else if handle == self.ads.bias_loff_sens.handle {
            handle_single_field_read!(self, bias_loff_sens, ads_config);
        } else if handle == self.ads.bias_stat.handle {
            handle_single_field_read!(self, bias_stat, ads_config);
        } else if handle == self.ads.comparator_threshold_pos.handle {
            handle_single_field_read!(
                self,
                comparator_threshold_pos,
                ads_config,
                dc_mini_icd::CompThreshPos
            );
        } else if handle == self.ads.lead_off_current.handle {
            handle_single_field_read!(
                self,
                lead_off_current,
                ads_config,
                dc_mini_icd::ILeadOff
            );
        } else if handle == self.ads.lead_off_frequency.handle {
            handle_single_field_read!(
                self,
                lead_off_frequency,
                ads_config,
                dc_mini_icd::FLeadOff
            );
        } else if handle == self.ads.srb1.handle {
            handle_single_field_read!(self, srb1, ads_config);
        } else if handle == self.ads.single_shot.handle {
            handle_single_field_read!(self, single_shot, ads_config);
        } else if handle == self.ads.pd_loff_comp.handle {
            handle_single_field_read!(self, pd_loff_comp, ads_config);
        } else if handle >= self.device_info.hardware_revision.handle
            && handle <= self.device_info.manufacturer_name.handle
        {
            self.handle_device_info_read_event(handle, app_context).await;
        } else if handle >= self.profile.current_profile.handle
            && handle <= self.profile.command.handle
        {
            self.handle_profile_read_event(handle, app_context).await;
        }
        // Vector fields
        else if handle == self.ads.power_down.handle {
            handle_vector_field_read!(self, power_down, ads_config);
        } else if handle == self.ads.gain.handle {
            handle_vector_field_read!(
                self,
                gain,
                ads_config,
                dc_mini_icd::Gain
            );
        } else if handle == self.ads.srb2.handle {
            handle_vector_field_read!(self, srb2, ads_config);
        } else if handle == self.ads.mux.handle {
            handle_vector_field_read!(self, mux, ads_config, dc_mini_icd::Mux);
        } else if handle == self.ads.bias_sensp.handle {
            handle_vector_field_read!(self, bias_sensp, ads_config);
        } else if handle == self.ads.bias_sensn.handle {
            handle_vector_field_read!(self, bias_sensn, ads_config);
        } else if handle == self.ads.lead_off_sensp.handle {
            handle_vector_field_read!(self, lead_off_sensp, ads_config);
        } else if handle == self.ads.lead_off_sensn.handle {
            handle_vector_field_read!(self, lead_off_sensn, ads_config);
        } else if handle == self.ads.lead_off_flip.handle {
            handle_vector_field_read!(self, lead_off_flip, ads_config);
        }
    }

    pub async fn handle_write_event(
        &self,
        handle: u16,
        app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    ) {
        let mut app_ctx = app_context.lock().await;
        let mut ads_config =
            unwrap!(app_ctx.profile_manager.get_ads_config().await).clone();

        // Match on characteristic handle
        if handle == self.ads.daisy_en.handle {
            handle_single_field_write!(self, daisy_en, ads_config);
        } else if handle == self.ads.clk_en.handle {
            handle_single_field_write!(self, clk_en, ads_config);
        } else if handle == self.ads.sample_rate.handle {
            handle_single_field_write!(
                self,
                sample_rate,
                ads_config,
                dc_mini_icd::SampleRate
            );
        } else if handle == self.ads.internal_calibration.handle {
            handle_single_field_write!(self, internal_calibration, ads_config);
        } else if handle == self.ads.calibration_amplitude.handle {
            handle_single_field_write!(
                self,
                calibration_amplitude,
                ads_config
            );
        } else if handle == self.ads.calibration_frequency.handle {
            handle_single_field_write!(
                self,
                calibration_frequency,
                ads_config,
                dc_mini_icd::CalFreq
            );
        } else if handle == self.ads.pd_refbuf.handle {
            handle_single_field_write!(self, pd_refbuf, ads_config);
        } else if handle == self.ads.bias_meas.handle {
            handle_single_field_write!(self, bias_meas, ads_config);
        } else if handle == self.ads.biasref_int.handle {
            handle_single_field_write!(self, biasref_int, ads_config);
        } else if handle == self.ads.pd_bias.handle {
            handle_single_field_write!(self, pd_bias, ads_config);
        } else if handle == self.ads.bias_loff_sens.handle {
            handle_single_field_write!(self, bias_loff_sens, ads_config);
        } else if handle == self.ads.bias_stat.handle {
            handle_single_field_write!(self, bias_stat, ads_config);
        } else if handle == self.ads.comparator_threshold_pos.handle {
            handle_single_field_write!(
                self,
                comparator_threshold_pos,
                ads_config,
                dc_mini_icd::CompThreshPos
            );
        } else if handle == self.ads.lead_off_current.handle {
            handle_single_field_write!(
                self,
                lead_off_current,
                ads_config,
                dc_mini_icd::ILeadOff
            );
        } else if handle == self.ads.lead_off_frequency.handle {
            handle_single_field_write!(
                self,
                lead_off_frequency,
                ads_config,
                dc_mini_icd::FLeadOff
            );
        } else if handle == self.ads.srb1.handle {
            handle_single_field_write!(self, srb1, ads_config);
        } else if handle == self.ads.single_shot.handle {
            handle_single_field_write!(self, single_shot, ads_config);
        } else if handle == self.ads.pd_loff_comp.handle {
            handle_single_field_write!(self, pd_loff_comp, ads_config);
        } else if handle >= self.profile.current_profile.handle
            && handle <= self.profile.command.handle
        {
            self.handle_profile_write_event(handle, app_context).await;
        }
        // Vector fields
        else if handle == self.ads.power_down.handle {
            handle_vector_field_write!(self, power_down, ads_config);
        } else if handle == self.ads.gain.handle {
            handle_vector_field_write!(
                self,
                gain,
                ads_config,
                dc_mini_icd::Gain
            );
        } else if handle == self.ads.srb2.handle {
            handle_vector_field_write!(self, srb2, ads_config);
        } else if handle == self.ads.mux.handle {
            handle_vector_field_write!(
                self,
                mux,
                ads_config,
                dc_mini_icd::Mux
            );
        } else if handle == self.ads.bias_sensp.handle {
            handle_vector_field_write!(self, bias_sensp, ads_config);
        } else if handle == self.ads.bias_sensn.handle {
            handle_vector_field_write!(self, bias_sensn, ads_config);
        } else if handle == self.ads.lead_off_sensp.handle {
            handle_vector_field_write!(self, lead_off_sensp, ads_config);
        } else if handle == self.ads.lead_off_sensn.handle {
            handle_vector_field_write!(self, lead_off_sensn, ads_config);
        } else if handle == self.ads.lead_off_flip.handle {
            handle_vector_field_write!(self, lead_off_flip, ads_config);
        } else if handle == self.ads.command.handle {
            if let Ok(value) = self.get(&self.ads.command) {
                let evt = AdsEvent::try_from(value);
                match evt {
                    Ok(e) => app_ctx.event_sender.send(e.into()).await,
                    Err(e) => warn!("{:?}", e),
                };
            }
        }

        // Update the profile manager with the modified config
        app_ctx.save_ads_config(ads_config).await;
    }
}

/// A BLE GATT server event loop.
pub async fn gatt_server_task<P: PacketPool>(
    server: &Server<'_>,
    conn: &GattConnection<'_, '_, P>,
    app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
) {
    loop {
        match conn.next().await {
            GattConnectionEvent::Disconnected { reason } => {
                info!("[gatt] Disconnected: {:?}", reason);
                break;
            }
            GattConnectionEvent::Gatt { event } => {
                match &event {
                    GattEvent::Read(event) => {
                        let handle = event.handle();
                        if handle >= server.ads.daisy_en.handle
                            && handle <= server.ads.command.handle
                        {
                            server
                                .handle_read_event(handle, app_context)
                                .await;
                        } else if handle >= server.session.recording_id.handle
                            && handle <= server.session.command.handle
                        {
                            server
                                .handle_session_read_event(
                                    handle,
                                    app_context,
                                )
                                .await;
                        } else if handle == server.battery.battery_level.handle
                        {
                            server
                                .handle_battery_read_event(
                                    handle,
                                    app_context,
                                )
                                .await;
                        } else if handle
                            >= server.device_info.hardware_revision.handle
                            && handle
                                <= server.device_info.manufacturer_name.handle
                        {
                            server
                                .handle_device_info_read_event(
                                    handle,
                                    app_context,
                                )
                                .await;
                        } else if handle
                            >= server.profile.current_profile.handle
                            && handle <= server.profile.command.handle
                        {
                            server
                                .handle_profile_read_event(
                                    handle,
                                    app_context,
                                )
                                .await;
                        }
                    }
                    GattEvent::Write(event) => {
                        let handle = event.handle();
                        if handle >= server.ads.daisy_en.handle
                            && handle <= server.ads.command.handle
                        {
                            server
                                .handle_write_event(
                                    handle,
                                    app_context,
                                )
                                .await;
                        } else if handle >= server.session.recording_id.handle
                            && handle <= server.session.command.handle
                        {
                            server
                                .handle_session_write_event(
                                    handle,
                                    app_context,
                                )
                                .await;
                        } else if handle
                            >= server.profile.current_profile.handle
                            && handle <= server.profile.command.handle
                        {
                            server
                                .handle_profile_write_event(
                                    handle,
                                    app_context,
                                )
                                .await;
                        }
                    }
                    _ => {}
                }
                match event.accept() {
                    Ok(reply) => reply.send().await,
                    Err(e) => warn!("[gatt] error sending response: {:?}", e),
                }
            }
            _ => {}
        }
    }
    info!("Gatt server task finished");
}
