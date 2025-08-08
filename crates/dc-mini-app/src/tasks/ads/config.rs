use crate::prelude::*;
use ads1299;
use core::ops::Range;
use dc_mini_bsp::PoweredAdsFrontend;
use dc_mini_icd::{AdsConfig, ChannelConfig};
use embassy_sync::blocking_mutex::raw::RawMutex;

pub fn default_ads_settings(num_channels: u8) -> AdsConfig {
    let channel_config = ChannelConfig {
        power_down: false,
        gain: dc_mini_icd::Gain::X24,
        srb2: false,
        mux: dc_mini_icd::Mux::NormalElectrodeInput,
        bias_sensp: false,
        bias_sensn: false,
        lead_off_sensp: false,
        lead_off_sensn: false,
        lead_off_flip: false,
    };

    let mut channels = heapless::Vec::new();
    for _ in 0..num_channels {
        unwrap!(channels.push(channel_config.clone()));
    }

    AdsConfig {
        daisy_en: false,
        clk_en: false,
        sample_rate: dc_mini_icd::SampleRate::Sps250,
        internal_calibration: false,
        calibration_amplitude: false,
        calibration_frequency: dc_mini_icd::CalFreq::FclkBy21,
        pd_refbuf: false,
        bias_meas: false,
        biasref_int: false,
        pd_bias: false,
        bias_loff_sens: false,
        bias_stat: false,
        comparator_threshold_pos: dc_mini_icd::CompThreshPos::_95,
        lead_off_current: dc_mini_icd::ILeadOff::_6nA,
        lead_off_frequency: dc_mini_icd::FLeadOff::Dc,
        gpioc: [true; 4],
        srb1: false,
        single_shot: false,
        pd_loff_comp: false,
        channels,
    }
}

pub async fn apply_ads_config<MutexType: RawMutex>(
    frontend: &mut PoweredAdsFrontend<'_, '_, MutexType>,
    config: &AdsConfig,
) {
    let mut ch_start = 0;
    for ads_dev in frontend.ads.iter_mut() {
        unwrap!(
            ads_dev
                .modify_register(ads1299::Register::CONFIG1, |reg_value| {
                    ads1299::Config1::from_bits_retain(reg_value)
                        .with_clk_en(if ch_start == 0 { true } else { false })
                        .with_daisy_en(config.daisy_en)
                        .with_odr(config.sample_rate.into())
                        .bits()
                })
                .await
        );

        unwrap!(
            ads_dev
                .modify_register(ads1299::Register::CONFIG2, |reg_value| {
                    ads1299::Config2::from_bits_retain(reg_value)
                        .with_int_cal(config.internal_calibration)
                        .with_cal_amp(config.calibration_amplitude)
                        .with_cal_freq(config.calibration_frequency.into())
                        .bits()
                })
                .await
        );

        unwrap!(
            ads_dev
                .modify_register(ads1299::Register::CONFIG3, |reg_value| {
                    ads1299::Config3::from_bits_retain(reg_value)
                        .with_pd_refbuf(config.pd_refbuf)
                        .with_bias_meas(config.bias_meas)
                        .with_biasref_int(config.biasref_int)
                        .with_pd_bias(config.pd_bias)
                        .with_bias_loff_sens(config.bias_loff_sens)
                        .with_bias_stat(config.bias_stat)
                        .bits()
                })
                .await
        );

        unwrap!(
            ads_dev
                .modify_register(ads1299::Register::LOFF, |reg_value| {
                    ads1299::Loff::from_bits_retain(reg_value)
                        .with_comp_th(config.comparator_threshold_pos.into())
                        .with_ilead_off(config.lead_off_current.into())
                        .with_flead_off(config.lead_off_frequency.into())
                        .bits()
                })
                .await
        );

        info!("ADS device found to have {:?} channels", ads_dev.num_chs);
        let ads_chs = Range { start: 0, end: ads_dev.num_chs.unwrap() };
        for ch in ads_chs {
            let reg = ads1299::Register::from_channel_number(ch);
            let conf_idx: usize = (ch + ch_start).into();
            let conf = &config.channels[conf_idx];
            unwrap!(
                ads_dev
                    .modify_register(reg, |reg_value| {
                        ads1299::ChSet::from_bits_retain(reg_value)
                            .with_pd(conf.power_down)
                            .with_gain(conf.gain.into())
                            .with_srb2(conf.srb2)
                            .with_mux(conf.mux.into())
                            .bits()
                    })
                    .await
            );

            unwrap!(
                ads_dev
                    .modify_register(
                        ads1299::Register::LOFF_SENSP,
                        |reg_value| {
                            let flag = ads1299::LoffSensP::from_bits_retain(
                                0x01 << ch,
                            );
                            let reg = ads1299::LoffSensP::from_bits_retain(
                                reg_value,
                            )
                            .difference(flag);
                            let reg = match conf.lead_off_sensp {
                                false => reg,
                                true => reg.union(flag),
                            };
                            reg.bits()
                        }
                    )
                    .await
            );

            unwrap!(
                ads_dev
                    .modify_register(
                        ads1299::Register::LOFF_SENSN,
                        |reg_value| {
                            let flag = ads1299::LoffSensN::from_bits_retain(
                                0x01 << ch,
                            );
                            let reg = ads1299::LoffSensN::from_bits_retain(
                                reg_value,
                            )
                            .difference(flag);
                            let reg = match conf.lead_off_sensn {
                                false => reg,
                                true => reg.union(flag),
                            };
                            reg.bits()
                        }
                    )
                    .await
            );

            unwrap!(
                ads_dev
                    .modify_register(
                        ads1299::Register::LOFF_FLIP,
                        |reg_value| {
                            let flag = ads1299::LoffFlip::from_bits_retain(
                                0x01 << ch,
                            );
                            let reg =
                                ads1299::LoffFlip::from_bits_retain(reg_value)
                                    .difference(flag);
                            let reg = match conf.lead_off_flip {
                                false => reg,
                                true => reg.union(flag),
                            };
                            reg.bits()
                        }
                    )
                    .await
            );

            unwrap!(
                ads_dev
                    .modify_register(
                        ads1299::Register::BIAS_SENSP,
                        |reg_value| {
                            let flag = ads1299::BiasSensP::from_bits_retain(
                                0x01 << ch,
                            );
                            let reg = ads1299::BiasSensP::from_bits_retain(
                                reg_value,
                            )
                            .difference(flag);
                            let reg = match conf.bias_sensp {
                                false => reg,
                                true => reg.union(flag),
                            };
                            reg.bits()
                        }
                    )
                    .await
            );

            unwrap!(
                ads_dev
                    .modify_register(
                        ads1299::Register::BIAS_SENSN,
                        |reg_value| {
                            let flag = ads1299::BiasSensN::from_bits_retain(
                                0x01 << ch,
                            );
                            let reg = ads1299::BiasSensN::from_bits_retain(
                                reg_value,
                            )
                            .difference(flag);
                            let reg = match conf.bias_sensn {
                                false => reg,
                                true => reg.union(flag),
                            };
                            reg.bits()
                        }
                    )
                    .await
            );
        }

        unwrap!(
            ads_dev
                .modify_register(ads1299::Register::GPIO, |reg_value| {
                    let mut reg = ads1299::Gpio::from_bits_retain(reg_value);
                    for (idx, state) in config.gpioc.iter().enumerate() {
                        reg = reg.with_gpioc(idx + 1, *state);
                    }
                    reg.bits()
                })
                .await
        );

        unwrap!(
            ads_dev
                .modify_register(ads1299::Register::MISC1, |reg_value| {
                    ads1299::Misc1::from_bits_retain(reg_value)
                        .with_srb1(config.srb1)
                        .bits()
                })
                .await
        );

        unwrap!(
            ads_dev
                .modify_register(ads1299::Register::CONFIG4, |reg_value| {
                    ads1299::Config4::from_bits_retain(reg_value)
                        .with_single_shot(config.single_shot)
                        .with_pd_loff_comp(config.pd_loff_comp)
                        .bits()
                })
                .await
        );

        ch_start += ads_dev.num_chs.unwrap();
    }
}
