use dc_mini_host::ui::DevicePanel;
use eframe::egui;
use re_viewer::external::{eframe, re_log, re_memory};
use tokio::runtime::Runtime;

// Use memory allocator for Rerun
#[global_allocator]
static GLOBAL: re_memory::AccountingAllocator<mimalloc::MiMalloc> =
    re_memory::AccountingAllocator::new(mimalloc::MiMalloc);

pub struct DcMiniApp {
    device_panel: DevicePanel,
    rerun_app: re_viewer::App,
    _rt: Runtime,
}

impl DcMiniApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Set up dark mode
        cc.egui_ctx.set_visuals(egui::Visuals::dark());

        // Set up Rerun viewer
        let main_thread_token =
            re_viewer::MainThreadToken::i_promise_i_am_on_the_main_thread();
        re_log::setup_logging();
        re_crash_handler::install_crash_handlers(re_viewer::build_info());

        let startup_options = re_viewer::StartupOptions::default();
        let app_env =
            re_viewer::AppEnvironment::Custom("DC Mini GUI".to_owned());

        let mut rerun_app = re_viewer::App::new(
            main_thread_token,
            re_viewer::build_info(),
            &app_env,
            startup_options,
            cc,
        );

        // Listen for TCP connections from Rerun's logging SDKs
        let rx = re_sdk_comms::serve(
            "0.0.0.0",
            re_sdk_comms::DEFAULT_SERVER_PORT,
            Default::default(),
        )?;
        rerun_app.add_receiver(rx);

        // Create recording stream
        let recording = rerun::RecordingStreamBuilder::new("dc_mini_host")
            .connect_tcp()?;

        let rt = Runtime::new()?;
        let handle = rt.handle().clone();

        Ok(Self {
            rerun_app,
            _rt: rt,
            device_panel: DevicePanel::new(
                handle,
                Some(dc_mini_host::log_ads_frame(recording)),
            ),
        })
    }
}

impl eframe::App for DcMiniApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.rerun_app.save(storage);
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Split the main area into device panel and rerun viewer
        egui::SidePanel::right("device_panel")
            .resizable(true)
            .default_width(400.0)
            .width_range(300.0..=600.0)
            .show(ctx, |ui| {
                ui.heading("DC Mini Host");
                ui.separator();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.device_panel.show(ui);
                });
            });

        // Show the Rerun Viewer in the remaining space
        self.rerun_app.update(ctx, frame);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut native_options = re_viewer::native::eframe_options(None);
    native_options.viewport = native_options
        .viewport
        .with_app_id("dc_mini_gui")
        .with_inner_size([1200.0, 800.0])
        .with_min_inner_size([800.0, 600.0]);

    eframe::run_native(
        "DC Mini",
        native_options,
        Box::new(|cc| {
            re_viewer::customize_eframe_and_setup_renderer(cc)?;
            DcMiniApp::new(cc).map(|app| Box::new(app) as Box<dyn eframe::App>)
        }),
    )?;

    Ok(())
}
