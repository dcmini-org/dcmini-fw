use dc_mini_host::ui::DevicePanel;
use rerun::external::{
    eframe, egui, re_crash_handler, re_grpc_server, re_log, re_memory,
    re_viewer,
};

// Use memory allocator for Rerun
#[global_allocator]
static GLOBAL: re_memory::AccountingAllocator<mimalloc::MiMalloc> =
    re_memory::AccountingAllocator::new(mimalloc::MiMalloc);

pub struct DcMiniApp {
    device_panel: DevicePanel,
    rerun_app: re_viewer::App,
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let main_thread_token =
        re_viewer::MainThreadToken::i_promise_i_am_on_the_main_thread();

    // Direct calls using the `log` crate to stderr. Control with `RUST_LOG=debug` etc.
    re_log::setup_logging();

    // Install handlers for panics and crashes
    re_crash_handler::install_crash_handlers(re_viewer::build_info());

    // Listen for gRPC connections from Rerun's logging SDKs
    let rx = re_grpc_server::spawn_with_recv(
        "0.0.0.0:9876".parse()?,
        Default::default(),
        re_grpc_server::shutdown::never(),
    );

    let mut native_options = re_viewer::native::eframe_options(None);
    native_options.viewport = native_options
        .viewport
        .with_app_id("dc_mini_gui")
        .with_inner_size([1200.0, 800.0])
        .with_min_inner_size([800.0, 600.0]);

    let startup_options = re_viewer::StartupOptions::default();
    let app_env = re_viewer::AppEnvironment::Custom("DC Mini GUI".to_owned());

    eframe::run_native(
        "DC Mini",
        native_options,
        Box::new(move |cc| {
            re_viewer::customize_eframe_and_setup_renderer(cc)?;

            // Set up dark mode
            cc.egui_ctx.set_visuals(egui::Visuals::dark());

            let mut rerun_app = re_viewer::App::new(
                main_thread_token,
                re_viewer::build_info(),
                app_env,
                startup_options,
                cc,
                None,
                re_viewer::AsyncRuntimeHandle::from_current_tokio_runtime_or_wasmbindgen()?,
            );
            rerun_app.add_log_receiver(rx);

            // Create recording stream connected to the local gRPC server
            let recording = rerun::RecordingStreamBuilder::new("dc_mini_host")
                .connect_grpc()?;

            let handle = tokio::runtime::Handle::current();

            Ok(Box::new(DcMiniApp {
                rerun_app,
                device_panel: DevicePanel::new(
                    handle,
                    Some(dc_mini_host::log_ads_frame(recording.clone())),
                    Some(dc_mini_host::log_mic_frame(recording)),
                ),
            }))
        }),
    )?;

    Ok(())
}
