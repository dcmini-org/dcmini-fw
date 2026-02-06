use crate::icd;
use egui::{Color32, RichText};

use super::Message;

pub(super) fn show_channel_config(
    ui: &mut egui::Ui,
    index: usize,
    channel: &mut icd::ChannelConfig,
    sender: &dyn Fn(Message),
) {
    if ui
        .checkbox(&mut channel.power_down, "Disabled")
        .on_hover_ui(|ui| {
            ui.label(
                RichText::new(format!("CH{}SET: PDn", index)).color(Color32::RED),
            );
            ui.label(format!("Power-down channel {}", index));
            ui.label(
                "NB: It's recommended to mux disabled channels to InputShorted",
            );
            ui.label(format!("☑: Disable/Power-down Channel {}", index));
            ui.label("☐: Normal Operation");
        })
        .changed()
    {
        sender(Message::PowerDown((index as u8, channel.power_down)));
    }

    ui.horizontal(|ui| {
        ui.label("Gain:");
        egui::ComboBox::new(format!("gain_{}", index), "")
            .selected_text(format!("{:?}", channel.gain))
            .show_ui(ui, |ui| {
                for g in [
                    icd::Gain::X1,
                    icd::Gain::X2,
                    icd::Gain::X4,
                    icd::Gain::X6,
                    icd::Gain::X8,
                    icd::Gain::X12,
                    icd::Gain::X24,
                ] {
                    if ui
                        .selectable_value(
                            &mut channel.gain,
                            g,
                            format!("{:?}", g),
                        )
                        .clicked()
                    {
                        sender(Message::Gain((index as u8, channel.gain)));
                    }
                }
            })
    });

    ui.horizontal(|ui| {
        ui.label("Mux:");
        egui::ComboBox::new(format!("mux_{}", index), "")
            .selected_text(format!("{:?}", channel.mux))
            .show_ui(ui, |ui| {
                for m in [
                    icd::Mux::NormalElectrodeInput,
                    icd::Mux::InputShorted,
                    icd::Mux::RldMeasure,
                    icd::Mux::MVDD,
                    icd::Mux::TemperatureSensor,
                    icd::Mux::TestSignal,
                    icd::Mux::RldDrp,
                    icd::Mux::RldDrn,
                ] {
                    if ui
                        .selectable_value(
                            &mut channel.mux,
                            m,
                            format!("{:?}", m),
                        )
                        .clicked()
                    {
                        sender(Message::Mux((index as u8, channel.mux)));
                    }
                }
            })
    });

    if ui
        .checkbox(&mut channel.bias_sensp, "Bias Sense on Positive Input")
        .on_hover_ui(|ui| {
            ui.label(
                RichText::new(format!("BIAS_SENSP: BIASP{}", index))
                    .color(Color32::RED),
            );
            ui.label(format!(
                "Include Channel {} Positive lead for Bias Calculation",
                index
            ));
            ui.label(format!("☑: Add IN{}P to Bias Calculation", index));
            ui.label(format!(
                "☐: Don't include IN{}P in Bias Calculation",
                index
            ));
        })
        .changed()
    {
        sender(Message::BiasSensP((index as u8, channel.bias_sensp)));
    }

    if ui
        .checkbox(&mut channel.bias_sensn, "Bias Sense on Negative Input")
        .on_hover_ui(|ui| {
            ui.label(
                RichText::new(format!("BIAS_SENSN: BIASN{}", index))
                    .color(Color32::RED),
            );
            ui.label(format!(
                "Include Channel {} Negative lead for Bias Calculation",
                index
            ));
            ui.label(format!("☑: Add IN{}N to Bias Calculation", index));
            ui.label(format!(
                "☐: Don't include IN{}N in Bias Calculation",
                index
            ));
        })
        .changed()
    {
        sender(Message::BiasSensN((index as u8, channel.bias_sensn)));
    }

    if ui
        .checkbox(
            &mut channel.lead_off_sensp,
            "Lead-off Sense on Positive Input",
        )
        .on_hover_ui(|ui| {
            ui.label(
                RichText::new(format!("LOFF_SENSP: LOFFP{}", index))
                    .color(Color32::RED),
            );
            ui.label(format!("☑: Enable Lead-off sensing on IN{}P", index));
            ui.label(format!("☐: Disable Lead-off sensing on IN{}P", index));
        })
        .changed()
    {
        sender(Message::LeadOffSensP((index as u8, channel.lead_off_sensp)));
    }

    if ui
        .checkbox(
            &mut channel.lead_off_sensn,
            "Lead-off Sense on Negative Input",
        )
        .on_hover_ui(|ui| {
            ui.label(
                RichText::new(format!("LOFF_SENSN: LOFFN{}", index))
                    .color(Color32::RED),
            );
            ui.label(format!("☑: Enable Lead-off sensing on IN{}N", index));
            ui.label(format!("☐: Disable Lead-off sensing on IN{}N", index));
        })
        .changed()
    {
        sender(Message::LeadOffSensN((index as u8, channel.lead_off_sensn)));
    }

    if ui
        .checkbox(&mut channel.lead_off_flip, "Lead-off Flip")
        .on_hover_ui(|ui| {
            ui.label(
                RichText::new(format!("LOFF_FLIP: LOFFF{}", index))
                    .color(Color32::RED),
            );
            ui.label(format!(
                "☑: IN{}P is pulled to AVSS and IN{}N pulled to AVDD",
                index, index
            ));
            ui.label(format!(
                "☐: IN{}P is pulled to AVDD and IN{}N pulled to AVSS",
                index, index
            ));
        })
        .changed()
    {
        sender(Message::LeadOffFlip((index as u8, channel.lead_off_flip)));
    }

    if ui
        .checkbox(&mut channel.srb2, "SRB2")
        .on_hover_ui(|ui| {
            ui.label(
                RichText::new(format!("CH{}SET: SRB2", index))
                    .color(Color32::RED),
            );
            ui.label(format!(
                "Connect SRB2 to positive input (IN{}P); useful for common reference",
                index
            ));
            ui.label(format!("☑: Connect SRB2 to IN{}P", index));
            ui.label(format!("☐: Disconnect SRB2 from IN{}P", index));
        })
        .changed()
    {
        sender(Message::Srb2((index as u8, channel.srb2)));
    }
}
