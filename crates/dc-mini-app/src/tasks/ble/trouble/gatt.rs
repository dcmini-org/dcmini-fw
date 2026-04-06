use super::{ads::*, dfu::*, mic::*, session::*, status::*};
use crate::events::DfuEvent;
use crate::prelude::*;
use crate::tasks::ads::default_ads_settings;
use crate::tasks::dfu::{DfuPartition, DfuResources};
use heapless::Vec;
use nrf_dfu_target::prelude::DfuStatus;
use trouble_host::prelude::*;

macro_rules! handle_single_field_read {
    ($server:expr, $field:ident, $config:expr, $type:ty) => {
        unwrap!($server.set(&$server.ads.$field, &($config.$field as u8)));
    };
    ($server:expr, $field:ident, $config:expr) => {
        unwrap!($server.set(&$server.ads.$field, &$config.$field));
    };
}

macro_rules! handle_vector_field_read {
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

macro_rules! handle_single_field_write {
    ($server:expr, $field:ident, $config:expr, $type:ty) => {
        if let Ok(value) = $server.get(&$server.ads.$field) {
            $config.$field = <$type>::from(value);
        }
    };
    ($server:expr, $field:ident, $config:expr) => {
        if let Ok(value) = $server.get(&$server.ads.$field) {
            $config.$field = value;
        }
    };
}

macro_rules! handle_vector_field_write {
    ($server:expr, $field:ident, $config:expr, $type:ty) => {
        if let Ok(values) = $server.get(&$server.ads.$field) {
            for (i, &value) in values.iter().enumerate() {
                if let Some(channel) = $config.channels.get_mut(i) {
                    channel.$field = <$type>::from(value);
                }
            }
        }
    };
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

mod with_dfu_server {
    use super::*;

    #[gatt_server]
    pub struct ServerWithDfu {
        pub battery: BatteryService,
        pub device_info: DeviceInfoService,
        pub status: StatusService,
        pub profile: ProfileService,
        pub ads: AdsService,
        pub mic: MicService,
        pub session: SessionService,
        pub dfu: NrfDfuService,
    }
}

mod without_dfu_server {
    use super::*;

    #[gatt_server]
    pub struct ServerWithoutDfu {
        pub battery: BatteryService,
        pub device_info: DeviceInfoService,
        pub status: StatusService,
        pub profile: ProfileService,
        pub ads: AdsService,
        pub mic: MicService,
        pub session: SessionService,
    }
}

pub use with_dfu_server::ServerWithDfu;
pub use without_dfu_server::ServerWithoutDfu;

macro_rules! impl_server_common {
    ($server_ty:ident) => {
        impl<'d> $server_ty<'d> {
            async fn load_ads_config(
                &self,
                app_context: &'static Mutex<
                    CriticalSectionRawMutex,
                    AppContext,
                >,
            ) -> AdsConfig {
                let mut app_ctx = app_context.lock().await;
                match app_ctx.profile_manager.get_ads_config().await.cloned() {
                    Some(config) => config,
                    None => {
                        let config = default_ads_settings(0);
                        let _ = app_ctx
                            .profile_manager
                            .set_ads_config(config.clone())
                            .await;
                        report_status(
                            icd::SubsystemId::Ads,
                            icd::SubsystemState::Degraded,
                            icd::FaultCode::ConfigReseeded,
                        )
                        .await;
                        config
                    }
                }
            }

            pub async fn handle_read_event(
                &self,
                handle: u16,
                app_context: &'static Mutex<
                    CriticalSectionRawMutex,
                    AppContext,
                >,
            ) {
                let ads_config = self.load_ads_config(app_context).await;

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
                    handle_single_field_read!(
                        self,
                        internal_calibration,
                        ads_config
                    );
                } else if handle == self.ads.calibration_amplitude.handle {
                    handle_single_field_read!(
                        self,
                        calibration_amplitude,
                        ads_config
                    );
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
                    handle_single_field_read!(
                        self,
                        bias_loff_sens,
                        ads_config
                    );
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
                    handle_single_field_read!(
                        self,
                        pd_loff_comp,
                        ads_config
                    );
                } else if handle == self.ads.power_down.handle {
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
                    handle_vector_field_read!(
                        self,
                        mux,
                        ads_config,
                        dc_mini_icd::Mux
                    );
                } else if handle == self.ads.bias_sensp.handle {
                    handle_vector_field_read!(self, bias_sensp, ads_config);
                } else if handle == self.ads.bias_sensn.handle {
                    handle_vector_field_read!(self, bias_sensn, ads_config);
                } else if handle == self.ads.lead_off_sensp.handle {
                    handle_vector_field_read!(
                        self,
                        lead_off_sensp,
                        ads_config
                    );
                } else if handle == self.ads.lead_off_sensn.handle {
                    handle_vector_field_read!(
                        self,
                        lead_off_sensn,
                        ads_config
                    );
                } else if handle == self.ads.lead_off_flip.handle {
                    handle_vector_field_read!(
                        self,
                        lead_off_flip,
                        ads_config
                    );
                }
            }

            pub async fn handle_write_event(
                &self,
                handle: u16,
                app_context: &'static Mutex<
                    CriticalSectionRawMutex,
                    AppContext,
                >,
            ) {
                if handle >= self.profile.current_profile.handle
                    && handle <= self.profile.command.handle
                {
                    self.handle_profile_write_event(handle, app_context).await;
                    return;
                }

                let mut app_ctx = app_context.lock().await;
                let mut ads_config =
                    match app_ctx.profile_manager.get_ads_config().await.cloned() {
                        Some(config) => config,
                        None => {
                            let config = default_ads_settings(0);
                            let _ = app_ctx
                                .profile_manager
                                .set_ads_config(config.clone())
                                .await;
                            report_status(
                                icd::SubsystemId::Ads,
                                icd::SubsystemState::Degraded,
                                icd::FaultCode::ConfigReseeded,
                            )
                            .await;
                            config
                        }
                    };

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
                    handle_single_field_write!(
                        self,
                        internal_calibration,
                        ads_config
                    );
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
                    handle_single_field_write!(
                        self,
                        bias_loff_sens,
                        ads_config
                    );
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
                    handle_single_field_write!(
                        self,
                        pd_loff_comp,
                        ads_config
                    );
                } else if handle == self.ads.power_down.handle {
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
                    handle_vector_field_write!(
                        self,
                        lead_off_sensp,
                        ads_config
                    );
                } else if handle == self.ads.lead_off_sensn.handle {
                    handle_vector_field_write!(
                        self,
                        lead_off_sensn,
                        ads_config
                    );
                } else if handle == self.ads.lead_off_flip.handle {
                    handle_vector_field_write!(
                        self,
                        lead_off_flip,
                        ads_config
                    );
                } else if handle == self.ads.command.handle {
                    if let Ok(value) = self.get(&self.ads.command) {
                        let evt = AdsEvent::try_from(value);
                        match evt {
                            Ok(e) => app_ctx.event_sender.send(e.into()).await,
                            Err(e) => warn!("{:?}", e),
                        };
                    }
                }

                app_ctx.save_ads_config(ads_config).await;
            }

            pub async fn handle_mic_read_event(
                &self,
                handle: u16,
                app_context: &'static Mutex<
                    CriticalSectionRawMutex,
                    AppContext,
                >,
            ) {
                let mut app_ctx = app_context.lock().await;
                let mic_config = app_ctx
                    .profile_manager
                    .get_mic_config()
                    .await
                    .cloned()
                    .unwrap_or_default();

                if handle == self.mic.gain_db.handle {
                    unwrap!(self.set(&self.mic.gain_db, &mic_config.gain_db));
                } else if handle == self.mic.sample_rate.handle {
                    unwrap!(self.set(
                        &self.mic.sample_rate,
                        &(mic_config.sample_rate as u8)
                    ));
                }
            }

            pub async fn handle_mic_write_event(
                &self,
                handle: u16,
                app_context: &'static Mutex<
                    CriticalSectionRawMutex,
                    AppContext,
                >,
            ) {
                let mut app_ctx = app_context.lock().await;
                let mut mic_config = app_ctx
                    .profile_manager
                    .get_mic_config()
                    .await
                    .cloned()
                    .unwrap_or_default();

                if handle == self.mic.gain_db.handle {
                    if let Ok(value) = self.get(&self.mic.gain_db) {
                        mic_config.gain_db = value;
                    }
                } else if handle == self.mic.sample_rate.handle {
                    if let Ok(value) = self.get(&self.mic.sample_rate) {
                        mic_config.sample_rate =
                            dc_mini_icd::MicSampleRate::from(value);
                    }
                } else if handle == self.mic.command.handle {
                    if let Ok(value) = self.get(&self.mic.command) {
                        match value {
                            0 => {
                                app_ctx
                                    .event_sender
                                    .send(MicEvent::StartStream.into())
                                    .await
                            }
                            1 => {
                                app_ctx
                                    .event_sender
                                    .send(MicEvent::StopStream.into())
                                    .await
                            }
                            _ => warn!("Unknown mic command: {}", value),
                        };
                    }
                }

                app_ctx.save_mic_config(mic_config).await;
            }
        }
    };
}

impl_server_common!(ServerWithDfu);
impl_server_common!(ServerWithoutDfu);

impl<'d> ServerWithDfu<'d> {
    pub async fn handle_dfu_write<P: PacketPool>(
        &self,
        handle: u16,
        target: &mut Target,
        partition: &mut DfuPartition<'_>,
        conn: &GattConnection<'_, '_, P>,
        app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
        dfu_resources: &'static DfuResources,
        dfu_started: &mut bool,
    ) -> Option<DfuStatus> {
        if handle != self.dfu.control.handle
            && handle != self.dfu.packet.handle
        {
            return None;
        }

        if !*dfu_started {
            if crate::tasks::session::is_active() {
                warn!("[ble-dfu] Rejected: recording active");
                return None;
            }
            if !dfu_resources.try_start() {
                warn!("[ble-dfu] Rejected: DFU already active");
                return None;
            }
            *dfu_started = true;
            let app_ctx = app_context.lock().await;
            app_ctx.event_sender.send(DfuEvent::Started.into()).await;
        }

        if handle == self.dfu.control.handle {
            handle_dfu_control(self, target, partition, conn).await
        } else {
            handle_dfu_packet(self, target, partition, conn).await
        }
    }
}

pub async fn gatt_server_task_with_dfu<P: PacketPool>(
    server: &ServerWithDfu<'_>,
    conn: &GattConnection<'_, '_, P>,
    app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    dfu_resources: &'static DfuResources,
) {
    let dfu_size = crate::tasks::dfu::DFU_PARTITION_SIZE;
    let mut dfu_target: Target = Target::new(dfu_size, fw_info(), hw_info());
    let mut dfu_partition = dfu_resources.dfu_partition();
    let mut dfu_started = false;

    loop {
        match conn.next().await {
            GattConnectionEvent::Disconnected { reason } => {
                info!("[gatt] Disconnected: {:?}", reason);
                break;
            }
            GattConnectionEvent::Gatt { event } => {
                let mut dfu_status = None;

                let write_handle = match &event {
                    GattEvent::Read(event) => {
                        let handle = event.handle();
                        if handle >= server.ads.daisy_en.handle
                            && handle <= server.ads.command.handle
                        {
                            server.handle_read_event(handle, app_context).await;
                        } else if handle >= server.session.recording_id.handle
                            && handle <= server.session.command.handle
                        {
                            server
                                .handle_session_read_event(handle, app_context)
                                .await;
                        } else if handle == server.battery.battery_level.handle
                        {
                            server
                                .handle_battery_read_event(handle, app_context)
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
                                .handle_profile_read_event(handle, app_context)
                                .await;
                        } else if handle >= server.status.snapshot.handle
                            && handle <= server.status.event.handle
                        {
                            server
                                .handle_status_read_event(handle, app_context)
                                .await;
                        } else if handle >= server.mic.gain_db.handle
                            && handle <= server.mic.command.handle
                        {
                            server
                                .handle_mic_read_event(handle, app_context)
                                .await;
                        }
                        None
                    }
                    GattEvent::Write(event) => Some(event.handle()),
                    _ => None,
                };

                match event.accept() {
                    Ok(reply) => reply.send().await,
                    Err(e) => warn!("[gatt] error sending response: {:?}", e),
                }

                if let Some(handle) = write_handle {
                    dfu_status = server
                        .handle_dfu_write(
                            handle,
                            &mut dfu_target,
                            &mut dfu_partition,
                            conn,
                            app_context,
                            dfu_resources,
                            &mut dfu_started,
                        )
                        .await;

                    if handle >= server.ads.daisy_en.handle
                        && handle <= server.ads.command.handle
                    {
                        server.handle_write_event(handle, app_context).await;
                    } else if handle >= server.session.recording_id.handle
                        && handle <= server.session.command.handle
                    {
                        server
                            .handle_session_write_event(handle, app_context)
                            .await;
                    } else if handle
                        >= server.profile.current_profile.handle
                        && handle <= server.profile.command.handle
                    {
                        server
                            .handle_profile_write_event(handle, app_context)
                            .await;
                    } else if handle >= server.mic.gain_db.handle
                        && handle <= server.mic.command.handle
                    {
                        server.handle_mic_write_event(handle, app_context).await;
                    }
                }

                if let Some(DfuStatus::DoneReset) = dfu_status {
                    info!("[dfu] Transfer complete, marking updated");
                    {
                        let app_ctx = app_context.lock().await;
                        app_ctx
                            .event_sender
                            .send(DfuEvent::Complete.into())
                            .await;
                    }
                    match dfu_resources.mark_updated() {
                        Ok(()) => {
                            info!("[dfu] Marked updated, resetting in 4s");
                            embassy_time::Timer::after_secs(4).await;
                            cortex_m::peripheral::SCB::sys_reset();
                        }
                        Err(_e) => {
                            warn!("[dfu] Failed to mark updated");
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if dfu_started {
        let app_ctx = app_context.lock().await;
        app_ctx.event_sender.send(DfuEvent::Aborted.into()).await;
    }
    info!("Gatt server task finished");
}

pub async fn gatt_server_task_without_dfu<P: PacketPool>(
    server: &ServerWithoutDfu<'_>,
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
                let write_handle = match &event {
                    GattEvent::Read(event) => {
                        let handle = event.handle();
                        if handle >= server.ads.daisy_en.handle
                            && handle <= server.ads.command.handle
                        {
                            server.handle_read_event(handle, app_context).await;
                        } else if handle >= server.session.recording_id.handle
                            && handle <= server.session.command.handle
                        {
                            server
                                .handle_session_read_event(handle, app_context)
                                .await;
                        } else if handle == server.battery.battery_level.handle
                        {
                            server
                                .handle_battery_read_event(handle, app_context)
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
                                .handle_profile_read_event(handle, app_context)
                                .await;
                        } else if handle >= server.status.snapshot.handle
                            && handle <= server.status.event.handle
                        {
                            server
                                .handle_status_read_event(handle, app_context)
                                .await;
                        } else if handle >= server.mic.gain_db.handle
                            && handle <= server.mic.command.handle
                        {
                            server
                                .handle_mic_read_event(handle, app_context)
                                .await;
                        }
                        None
                    }
                    GattEvent::Write(event) => Some(event.handle()),
                    _ => None,
                };

                match event.accept() {
                    Ok(reply) => reply.send().await,
                    Err(e) => warn!("[gatt] error sending response: {:?}", e),
                }

                if let Some(handle) = write_handle {
                    if handle >= server.ads.daisy_en.handle
                        && handle <= server.ads.command.handle
                    {
                        server.handle_write_event(handle, app_context).await;
                    } else if handle >= server.session.recording_id.handle
                        && handle <= server.session.command.handle
                    {
                        server
                            .handle_session_write_event(handle, app_context)
                            .await;
                    } else if handle
                        >= server.profile.current_profile.handle
                        && handle <= server.profile.command.handle
                    {
                        server
                            .handle_profile_write_event(handle, app_context)
                            .await;
                    } else if handle >= server.mic.gain_db.handle
                        && handle <= server.mic.command.handle
                    {
                        server.handle_mic_write_event(handle, app_context).await;
                    }
                }
            }
            _ => {}
        }
    }

    info!("Gatt server task finished");
}
