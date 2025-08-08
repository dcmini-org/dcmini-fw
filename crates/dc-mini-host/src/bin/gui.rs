use dc_mini_host::ui::DevicePanel;
use eframe::egui;
use tokio::runtime::Runtime;

pub struct DcMiniApp {
    device_panel: DevicePanel,
    dark_mode: bool,
    _rt: Runtime,
}

impl DcMiniApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Set up dark mode
        cc.egui_ctx.set_visuals(egui::Visuals::dark());

        let rt = Runtime::new()?;
        let handle = rt.handle().clone();

        Ok(Self {
            device_panel: DevicePanel::new(handle, None),
            dark_mode: true,
            _rt: rt,
        })
    }
}

impl eframe::App for DcMiniApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Quit").clicked() {
                        std::process::exit(0);
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui.checkbox(&mut self.dark_mode, "Dark Mode").clicked()
                    {
                        if self.dark_mode {
                            ctx.set_visuals(egui::Visuals::dark());
                        } else {
                            ctx.set_visuals(egui::Visuals::light());
                        }
                    }
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("DC Mini Host");
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                self.device_panel.show(ui);
            });
        });

        // Request a repaint
        ctx.request_repaint();
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_min_inner_size([300.0, 220.0])
            .with_title("DC Mini"),
        ..Default::default()
    };

    eframe::run_native(
        "DC Mini",
        options,
        Box::new(|cc| {
            DcMiniApp::new(cc).map(|app| Box::new(app) as Box<dyn eframe::App>)
        }),
    )?;

    Ok(())
}
