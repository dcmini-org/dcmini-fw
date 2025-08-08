use chrono::NaiveDate;
use eframe::egui;
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use dc_mini_host::fileio::edf::EdfConfig;
use dc_mini_host::fileio::{self, ConversionConfig, Error, Result};

#[derive(Default, Serialize, Deserialize)]
struct SavedMetadata {
    hospital_code: String,
    patient_sex: String,
    patient_birthdate: NaiveDate,
    patient_name: String,
    recording_technician: String,
    recording_equipment: String,
    recording_start_date: NaiveDate,
    electrode_config: Vec<String>,
}

impl SavedMetadata {
    fn load() -> Self {
        let mut md =
            if let Ok(file) = fs::read_to_string("dc_mini_metadata.json") {
                serde_json::from_str(&file).unwrap_or_default()
            } else {
                Self::default()
            };
        md.recording_start_date = chrono::Local::now().date_naive();
        md
    }

    fn save(&self) -> Result<()> {
        fs::write("dc_mini_metadata.json", serde_json::to_string_pretty(self)?)
            .map_err(|e| Error::InvalidData(e.to_string()))
    }
}

#[derive(Default)]
struct ConverterApp {
    input_path: Option<PathBuf>,
    output_path: Option<PathBuf>,
    selected_format: String,
    metadata: SavedMetadata,
    error_message: String,
    success_message: String,
    num_channels: Option<usize>,
}

impl ConverterApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            selected_format: "edf".to_string(),
            metadata: SavedMetadata::load(),
            ..Default::default()
        }
    }

    fn handle_input_file_selected(&mut self, path: PathBuf) -> Result<()> {
        self.input_path = Some(path.clone());
        self.output_path = Some(path.with_extension("edf"));
        let mut reader = fileio::create_reader(&path)?;
        let metadata = reader.read_header()?;
        self.num_channels = Some(metadata.num_channels);
        if self.metadata.electrode_config.len() != metadata.num_channels {
            self.metadata.electrode_config =
                vec!["".to_string(); metadata.num_channels];
        }
        Ok(())
    }

    fn validate_sex(sex: &str) -> Result<char> {
        match sex.to_uppercase().as_str() {
            "M" | "F" => Ok(sex.to_uppercase().chars().next().unwrap()),
            _ => Err(Error::InvalidInput(
                "Sex must be either 'M' or 'F'".to_string(),
            )),
        }
    }

    fn process_file(&self) -> Result<()> {
        match self.selected_format.as_str() {
            "edf" => {
                if self.metadata.hospital_code.is_empty() {
                    return Err(Error::InvalidInput(
                        "Hospital code is required".to_string(),
                    ));
                }
                if self.metadata.patient_sex.is_empty() {
                    return Err(Error::InvalidInput(
                        "Patient sex is required".to_string(),
                    ));
                }
                if self.metadata.patient_name.is_empty() {
                    return Err(Error::InvalidInput(
                        "Patient name is required".to_string(),
                    ));
                }
                if self.metadata.recording_technician.is_empty() {
                    return Err(Error::InvalidInput(
                        "Recording technician is required".to_string(),
                    ));
                }
                if self.metadata.recording_equipment.is_empty() {
                    return Err(Error::InvalidInput(
                        "Recording equipment is required".to_string(),
                    ));
                }

                let num_channels = self.num_channels.ok_or_else(|| {
                    Error::InvalidInput("No input file selected".to_string())
                })?;

                if self.metadata.electrode_config.len() != num_channels {
                    return Err(Error::InvalidInput(format!(
                        "Electrode configuration required for all {} channels",
                        num_channels
                    )));
                }

                for (i, electrode) in
                    self.metadata.electrode_config.iter().enumerate()
                {
                    if electrode.is_empty() {
                        return Err(Error::InvalidInput(format!(
                            "Electrode position required for channel {}",
                            i + 1
                        )));
                    }
                }

                let electrode_labels = self
                    .metadata
                    .electrode_config
                    .iter()
                    .map(|e| format!("EEG {}", e))
                    .collect();

                let edf_config = EdfConfig::new(
                    self.metadata.hospital_code.clone(),
                    Self::validate_sex(&self.metadata.patient_sex)?,
                    self.metadata.patient_birthdate.clone(),
                    self.metadata.patient_name.clone(),
                    self.metadata.recording_technician.clone(),
                    self.metadata.recording_equipment.clone(),
                    self.metadata.recording_start_date.clone(),
                    electrode_labels,
                )?;

                let config = ConversionConfig::Edf {
                    input_path: self.input_path.clone().unwrap(),
                    output_path: self.output_path.clone().unwrap(),
                    config: edf_config,
                };

                let mut reader =
                    fileio::create_reader(&self.input_path.clone().unwrap())?;
                let metadata = reader.read_header()?;

                let mut writer = fileio::create_writer(&config)?;
                writer.set_metadata(metadata);
                writer.write_header()?;

                let records = reader.read_data()?;
                writer.write_data(records)?;

                writer.finalize()?;

                Ok(())
            }
            _ => Err(Error::InvalidInput(format!(
                "Unsupported output format: {}",
                self.selected_format
            ))),
        }
    }
}

impl eframe::App for ConverterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("DC Mini File Converter");
            ui.add_space(20.0);

            ui.horizontal(|ui| {
                ui.label("Output Format:");
                egui::ComboBox::from_id_salt("format_selector")
                    .selected_text(&self.selected_format)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.selected_format,
                            "edf".to_string(),
                            "EDF+",
                        );
                    });
            });

            ui.add_space(10.0);

            ui.horizontal(|ui| {
                if ui.button("Select Input File").clicked() {
                    if let Some(path) = FileDialog::new()
                        .add_filter("DC Mini Recording", &["dat", "DAT", ""])
                        .pick_file()
                    {
                        if let Err(e) = self.handle_input_file_selected(path) {
                            self.error_message =
                                format!("Error reading input file: {}", e);
                        }
                    }
                }
                if let Some(path) = &self.input_path {
                    ui.label(path.to_string_lossy().to_string());
                }
            });

            ui.horizontal(|ui| {
                if ui.button("Select Output Location").clicked() {
                    if let Some(path) = FileDialog::new()
                        .add_filter("EDF", &["edf"])
                        .save_file()
                    {
                        self.output_path = Some(path);
                    }
                }
                if let Some(path) = &self.output_path {
                    ui.label(path.to_string_lossy().to_string());
                }
            });

            ui.add_space(20.0);

            if self.selected_format == "edf" {
                ui.group(|ui| {
                    ui.heading("Hospital Information");
                    ui.horizontal(|ui| {
                        ui.label("Hospital Code:");
                        ui.text_edit_singleline(
                            &mut self.metadata.hospital_code,
                        );
                    });
                });

                ui.add_space(10.0);

                ui.group(|ui| {
                    ui.heading("Patient Information");
                    ui.horizontal(|ui| {
                        ui.label("Sex (M/F):");
                        ui.text_edit_singleline(
                            &mut self.metadata.patient_sex,
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Birth Date:");
                        ui.add(
                            egui_extras::DatePickerButton::new(
                                &mut self.metadata.patient_birthdate,
                            )
                            .id_salt("birth_date"),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(
                            &mut self.metadata.patient_name,
                        );
                    });
                });

                ui.add_space(10.0);

                ui.group(|ui| {
                    ui.heading("Recording Information");
                    ui.horizontal(|ui| {
                        ui.label("Technician:");
                        ui.text_edit_singleline(
                            &mut self.metadata.recording_technician,
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Equipment:");
                        ui.text_edit_singleline(
                            &mut self.metadata.recording_equipment,
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Start Date:");
                        ui.add(
                            egui_extras::DatePickerButton::new(
                                &mut self.metadata.recording_start_date,
                            )
                            .id_salt("recording_start_date"),
                        );
                    });
                });

                if let Some(num_channels) = self.num_channels {
                    ui.add_space(10.0);
                    ui.group(|ui| {
                        ui.heading("Electrode Configuration");
                        ui.label("Enter electrode positions (e.g., 'Fp1', 'C3-A2', 'O1-M2')");
                        ui.small("Standard 10/20 or 10/10% positions required. Reference electrode is optional.");
                        for i in 0..num_channels {
                            ui.horizontal(|ui| {
                                ui.label(format!("Channel {}:", i + 1));
                                ui.text_edit_singleline(
                                    &mut self.metadata.electrode_config[i],
                                );
                                if ui.small("ⓘ").on_hover_text(
                                    "Enter electrode position (e.g., 'Fp1', 'C3-A2')\nValid positions include: Fp1, Fp2, F3, F4, C3, C4, P3, P4, O1, O2, F7, F8, T3, T4, T5, T6, Fz, Cz, Pz, etc."
                                ).clicked() {
                                    ui.memory_mut(|mem| mem.toggle_popup(ui.next_auto_id()));
                                }
                            });
                        }
                    });
                }
            }

            ui.add_space(20.0);

            if ui.button("Save Metadata as Default").clicked() {
                if let Err(e) = self.metadata.save() {
                    self.error_message =
                        format!("Failed to save metadata: {}", e);
                } else {
                    self.success_message =
                        "Metadata saved successfully".to_string();
                }
            }

            if ui
                .add_enabled(
                    self.input_path.is_some() && self.output_path.is_some(),
                    egui::Button::new("Convert"),
                )
                .clicked()
            {
                match self.process_file() {
                    Ok(_) => {
                        self.error_message.clear();
                        self.success_message =
                            "File converted successfully!".to_string();
                    }
                    Err(e) => {
                        self.success_message.clear();
                        self.error_message = format!("Error: {}", e);
                    }
                }
            }

            if !self.error_message.is_empty() {
                ui.colored_label(egui::Color32::RED, &self.error_message);
            }
            if !self.success_message.is_empty() {
                ui.colored_label(egui::Color32::GREEN, &self.success_message);
            }
        });
    }
}

fn main() -> Result<()> {
    let mut native_options = re_viewer::native::eframe_options(None);
    native_options.viewport = native_options
        .viewport
        .with_app_id("dc_convert_gui")
        .with_inner_size([1200.0, 800.0])
        .with_min_inner_size([800.0, 600.0]);

    eframe::run_native(
        "DC Mini File Converter",
        native_options,
        Box::new(|cc| Ok(Box::new(ConverterApp::new(cc)))),
    )
    .map_err(|e| Error::InvalidData(e.to_string()))
}
