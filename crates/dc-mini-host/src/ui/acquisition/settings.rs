use crate::icd::{self, AdsConfig};
use egui::{Color32, RichText};

use super::Message;

pub(super) fn show_global_settings(
    ui: &mut egui::Ui,
    config: &mut AdsConfig,
    sender: &dyn Fn(Message),
) {
    ui.collapsing("Global Settings", |ui| {
        if ui
            .checkbox(&mut config.daisy_en, "Multiple Readback Mode")
            .on_hover_ui(|ui| {
                ui.label(
                    RichText::new("CONFIG1: ~DAISY_EN").color(Color32::RED),
                );
                ui.label(
                    "Controls which multi-ADS1299 mode is enabled.",
                );
                ui.label(
                    " DCMini schematic is set up for multiple readback mode.",
                );
                ui.hyperlink_to(
                    "(See ADS1299 Datasheet 10.1.4.2.)",
                    "https://www.ti.com/document-viewer/ADS1299/datasheet#applications-and-implementation/SBAS459200",
                );
                ui.label("☑: Multiple readback mode **Recommended");
                ui.label("☐: Daisy-chain mode");
            })
            .changed()
        {
            sender(Message::DiasyEn(config.daisy_en));
        }

        if ui
            .checkbox(&mut config.clk_en, "Clock Output")
            .on_hover_ui(|ui| {
                ui.label(
                    RichText::new("CONFIG1: CLK_EN").color(Color32::RED),
                );
                ui.label("Enables clock output driver on base ADS1299 for multiple ADS1299 configurations");
                ui.label("NB: On DCMini, CLKSEL pin is pulled high on base AND daisy, but clock output is disabled on daisy by firmware.");
                ui.hyperlink_to(
                    "(See ADS1299 Datasheet 9.3.2.2)",
                    "https://www.ti.com/document-viewer/ADS1299/datasheet#detailed-description/SBAS4597213",
                );
                ui.label("☑: Oscillator clock output enabled; CLK pin is output **Recommended");
                ui.label("☐: Oscilattor clock output disabled; CLK pin is tri-state input.");
            })
            .changed()
        {
            sender(Message::ClkEn(config.clk_en));
        }

        ui.horizontal(|ui| {
            ui.label("Sampling Rate:");
            egui::ComboBox::new("sample_rate", "")
                .selected_text(format!("{:?}", config.sample_rate))
                .show_ui(ui, |ui| {
                    for rate in [
                        icd::SampleRate::Sps250,
                        icd::SampleRate::Sps500,
                        icd::SampleRate::KSps1,
                        icd::SampleRate::KSps2,
                        icd::SampleRate::KSps4,
                        icd::SampleRate::KSps8,
                        icd::SampleRate::KSps16,
                    ] {
                        if ui
                            .selectable_value(
                                &mut config.sample_rate,
                                rate,
                                format!("{:?}", rate),
                            )
                            .clicked()
                        {
                            sender(Message::SamplingRate(config.sample_rate));
                        }
                    }
                })
        });

        if ui
            .checkbox(
                &mut config.internal_calibration,
                "Internal Test Signal Generation",
            )
            .on_hover_ui(|ui| {
                ui.label(
                    RichText::new("CONFIG2: INT_CAL").color(Color32::RED),
                );
                ui.label("Source for the test signal (when channels are mux'd to TestSignal");
                ui.label(
                    "☑: Test signals are generated internally **Recommended",
                );
                ui.label("☐: Test signals are driven externally");
            })
            .changed()
        {
            sender(Message::InternalCalibration(
                config.internal_calibration,
            ));
        }

        if ui
            .checkbox(
                &mut config.calibration_amplitude,
                "2X Calibration Amplitude",
            )
            .on_hover_ui(|ui| {
                ui.label(
                    RichText::new("CONFIG2: CAL_AMP").color(Color32::RED),
                );
                ui.label("Test signal amplitude");
                ui.label("(On DCMini, VREFP = 2.5V, VREFN = -2.5V)");
                ui.label("☑: 4.1666 mV **Recommended");
                ui.label("☐: 2.0833 mV");
            })
            .changed()
        {
            sender(Message::CalibrationAmplitude(
                config.calibration_amplitude,
            ));
        }

        ui.horizontal(|ui| {
            ui.label("Calibration Frequency:");
            egui::ComboBox::new("calibration_frequency", "")
                .selected_text(format!(
                    "{:?}",
                    config.calibration_frequency
                ))
                .show_ui(ui, |ui| {
                    for freq in [
                        icd::CalFreq::FclkBy21,
                        icd::CalFreq::FclkBy20,
                        icd::CalFreq::DoNotUse,
                        icd::CalFreq::DC,
                    ] {
                        if ui
                            .selectable_value(
                                &mut config.calibration_frequency,
                                freq,
                                format!("{:?}", freq),
                            )
                            .clicked()
                        {
                            sender(Message::CalibrationFrequency(
                                config.calibration_frequency,
                            ));
                        }
                    }
                })
        });

        if ui
            .checkbox(
                &mut config.pd_refbuf,
                "Enable Internal Reference Buffer",
            )
            .on_hover_ui(|ui| {
                ui.label(
                    RichText::new("CONFIG3: ~PD_REFBUF").color(Color32::RED),
                );
                ui.label("Power-down Reference Buffer");
                ui.label(
                    "☑: Enable internal reference buffer **Recommended",
                );
                ui.label("☐: Power-down internal reference buffer");
            })
            .changed()
        {
            sender(Message::PdRefBuf(config.pd_refbuf));
        }

        if ui
            .checkbox(&mut config.bias_meas, "Enable Bias Measurement")
            .on_hover_ui(|ui| {
                ui.label(
                    RichText::new("CONFIG3: BIAS_MEAS").color(Color32::RED),
                );
                ui.label("Enable routing of RldMeasure to channels");
                ui.label("☑: BIAS_IN signal is routed to (the?) channel(s?) mux'd to RldMeasure");
                ui.label("☐: Don't route BIAS_IN to any channels");
            })
            .changed()
        {
            sender(Message::BiasMeas(config.bias_meas));
        }

        if ui
            .checkbox(
                &mut config.biasref_int,
                "Internal Bias Reference Generation",
            )
            .on_hover_ui(|ui| {
                ui.label(
                    RichText::new("CONFIG3: BIASREF_INT").color(Color32::RED),
                );
                ui.label("BIASREF signal source selection");
                ui.label("☑: Enable internal Bias reference signal @ ~0V **Recommended");
                ui.label("☐: Bias reference signal is fed externally");
            })
            .changed()
        {
            sender(Message::BiasRefInt(config.biasref_int));
        }

        if ui
            .checkbox(&mut config.pd_bias, "Enable Bias")
            .on_hover_ui(|ui| {
                ui.label(
                    RichText::new("CONFIG3: ~PD_BIAS").color(Color32::RED),
                );
                ui.label("Power-down Bias Buffer");
                ui.label("☑: Enable Bias");
                ui.label("☐: Power-down Bias Buffer");
            })
            .changed()
        {
            sender(Message::PdBias(config.pd_bias));
        }

        if ui
            .checkbox(
                &mut config.bias_loff_sens,
                "Enable Bias Lead-Off Sense",
            )
            .on_hover_ui(|ui| {
                ui.label(
                    RichText::new("CONFIG3: BIAS_LOFF_SENS")
                        .color(Color32::RED),
                );
                ui.label("Enable Bias Sense function");
                ui.label("☑: Bias Lead-Off Sensing is Enabled");
                ui.label("☐: Bias Lead-Off Sensing is Disabled");
            })
            .changed()
        {
            sender(Message::BiasLoffSens(config.bias_loff_sens));
        }

        if ui
            .checkbox(&mut config.srb1, "Connect SRB1")
            .on_hover_ui(|ui| {
                ui.label(
                    RichText::new("MISC1: SRB1").color(Color32::RED),
                );
                ui.label(
                    "Connect SRB1 to ALL inverting (IN1N, IN2N, ...) inputs.",
                );
                ui.label("☑: SRB1 Connected to ALL inverting inputs");
                ui.label("☐: Disconnect SRB1 **Recommended");
            })
            .changed()
        {
            sender(Message::Srb1(config.srb1));
        }

        if ui
            .checkbox(&mut config.single_shot, "Single Shot Mode")
            .on_hover_ui(|ui| {
                ui.label(
                    RichText::new("CONFIG4: SINGLE_SHOT").color(Color32::RED),
                );
                ui.label("Set conversion mode");
                ui.label("☑: Perform a single conversion then stop");
                ui.label("☐: Enable continuous conversion (streaming) mode **Recommended");
            })
            .changed()
        {
            sender(Message::SingleShot(config.single_shot));
        }

        if ui
            .checkbox(
                &mut config.pd_loff_comp,
                "Enable Lead-Off Comparators",
            )
            .on_hover_ui(|ui| {
                ui.label(
                    RichText::new("CONFIG4: ~PD_LOFF_COMP")
                        .color(Color32::RED),
                );
                ui.label(
                    "Enable Lead-off (channel disconnected) comparators",
                );
                ui.label("☑: Lead-off comparators enabled");
                ui.label("☐: Lead-off comparators disabled");
            })
            .changed()
        {
            sender(Message::PdLoffComp(config.pd_loff_comp));
        }
    });
}

pub(super) fn show_leadoff_settings(
    ui: &mut egui::Ui,
    config: &mut AdsConfig,
    sender: &dyn Fn(Message),
) {
    ui.collapsing("Lead-Off Settings", |ui| {
        ui.horizontal(|ui| {
            ui.label("Lead-Off Current:");
            egui::ComboBox::new("lead_off_current", "")
                .selected_text(format!("{:?}", config.lead_off_current))
                .show_ui(ui, |ui| {
                    for current in [
                        icd::ILeadOff::_6nA,
                        icd::ILeadOff::_24nA,
                        icd::ILeadOff::_6uA,
                        icd::ILeadOff::_24uA,
                    ] {
                        if ui
                            .selectable_value(
                                &mut config.lead_off_current,
                                current,
                                format!("{:?}", current),
                            )
                            .clicked()
                        {
                            sender(Message::LeadOffCurrent(
                                config.lead_off_current,
                            ));
                        }
                    }
                })
        });

        ui.horizontal(|ui| {
            ui.label("Lead-Off Frequency:");
            egui::ComboBox::new("lead_off_freq", "")
                .selected_text(format!("{:?}", config.lead_off_frequency))
                .show_ui(ui, |ui| {
                    for freq in [
                        icd::FLeadOff::Dc,
                        icd::FLeadOff::Ac7_8,
                        icd::FLeadOff::Ac31_2,
                        icd::FLeadOff::AcFdrBy4,
                    ] {
                        if ui
                            .selectable_value(
                                &mut config.lead_off_frequency,
                                freq,
                                format!("{:?}", freq),
                            )
                            .clicked()
                        {
                            sender(Message::LeadOffFrequency(
                                config.lead_off_frequency,
                            ));
                        }
                    }
                })
        });

        ui.horizontal(|ui| {
            ui.label("Comparator Threshold:");
            egui::ComboBox::new("comp_thresh", "")
                .selected_text(format!(
                    "{:?}",
                    config.comparator_threshold_pos
                ))
                .show_ui(ui, |ui| {
                    for thresh in [
                        icd::CompThreshPos::_95,
                        icd::CompThreshPos::_92_5,
                        icd::CompThreshPos::_90,
                        icd::CompThreshPos::_87_5,
                        icd::CompThreshPos::_85,
                        icd::CompThreshPos::_80,
                        icd::CompThreshPos::_75,
                        icd::CompThreshPos::_70,
                    ] {
                        if ui
                            .selectable_value(
                                &mut config.comparator_threshold_pos,
                                thresh,
                                format!("{:?}", thresh),
                            )
                            .clicked()
                        {
                            sender(Message::ComparatorThresholdPos(
                                config.comparator_threshold_pos,
                            ));
                        }
                    }
                })
        });
    });
}

pub(super) fn show_gpio_config(
    ui: &mut egui::Ui,
    config: &mut AdsConfig,
    sender: &dyn Fn(Message),
) {
    ui.collapsing("GPIO Configuration", |ui| {
        let mut gpioc = config.gpioc;
        let mut changed = false;
        for (i, gpio) in gpioc.iter_mut().enumerate() {
            if ui
                .checkbox(gpio, format!("GPIO {} is Input", i))
                .on_hover_ui(|ui| {
                    ui.label(
                        RichText::new(format!("GPIO: GPIOC{}", i))
                            .color(Color32::RED),
                    );
                    ui.label(format!(
                        "Set if corresponding GPIOD{} is input or output",
                        i
                    ));
                    ui.label(format!("☑: GPIO{} is input", i));
                    ui.label(format!("☐: GPIO{} is output", i));
                })
                .changed()
            {
                changed = true;
            }
        }
        if changed {
            config.gpioc = gpioc;
            sender(Message::Gpioc(gpioc));
        }
    });
}
