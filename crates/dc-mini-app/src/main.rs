#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(impl_trait_in_assoc_type)]
extern crate alloc;

use embassy_executor::Spawner;
use embassy_sync::mutex::Mutex;

use static_cell::StaticCell;

#[cfg(feature = "defmt")]
use defmt_rtt as _;
#[cfg(feature = "defmt")]
use panic_probe as _;
#[cfg(not(feature = "defmt"))]
use panic_reset as _;

use dc_mini_app::tasks::dfu::DfuResources;
use dc_mini_app::{init_event_channel, prelude::*, FW_VERSION};
use embassy_nrf::nvmc::Nvmc;

static ADS_RESOURCES: StaticCell<
    Mutex<CriticalSectionRawMutex, AdsResources>,
> = StaticCell::new();
static SD_CARD_RESOURCES: StaticCell<
    Mutex<CriticalSectionRawMutex, SdCardResources>,
> = StaticCell::new();
static SPI3_BUS_RESOURCES: StaticCell<
    Mutex<CriticalSectionRawMutex, Spi3BusResources>,
> = StaticCell::new();
static I2C_BUS_MANAGER: StaticCell<I2cBusManager> = StaticCell::new();
static IMU_RESOURCES: StaticCell<
    Mutex<CriticalSectionRawMutex, ImuResources>,
> = StaticCell::new();
static MIC_RESOURCES: StaticCell<
    Mutex<CriticalSectionRawMutex, MicResources>,
> = StaticCell::new();
static APP_CONTEXT: StaticCell<Mutex<CriticalSectionRawMutex, AppContext>> =
    StaticCell::new();
static DFU_RESOURCES: StaticCell<DfuResources> = StaticCell::new();
static EXT_FLASH_RES: StaticCell<dc_mini_bsp::ExternalFlashResources> =
    StaticCell::new();

// Application main entry point. The spawner can be used to start async tasks.
#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("In main!");
    // First we initialize our board.
    let mut board = DCMini::default();

    // Phase 0: Confirm boot to prevent rollback on next reset.
    // Temporarily init QSPI + NVMC to set boot state, then drop them
    // so the peripherals remain available for later use.
    {
        use core::cell::RefCell;
        use embassy_boot::{BlockingFirmwareUpdater, FirmwareUpdaterConfig};
        use embassy_sync::blocking_mutex::Mutex as BlockingMutex;

        let ext_flash = board.external_flash.configure();
        let ext_flash =
            BlockingMutex::<NoopRawMutex, _>::new(RefCell::new(ext_flash));
        // Safety: NVMC is not used by anything else at this early init stage.
        let nvmc =
            unsafe { Nvmc::new(embassy_nrf::peripherals::NVMC::steal()) };
        let nvmc = BlockingMutex::<NoopRawMutex, _>::new(RefCell::new(nvmc));

        let config =
            FirmwareUpdaterConfig::from_linkerfile_blocking(&ext_flash, &nvmc);
        let mut aligned = [0u8; 4];
        let mut updater = BlockingFirmwareUpdater::new(config, &mut aligned);
        match updater.mark_booted() {
            Ok(()) => info!("Firmware boot confirmed (mark_booted ok)"),
            Err(_e) => warn!("mark_booted failed"),
        }
        // ext_flash and nvmc dropped here, QSPI/NVMC peripherals freed.
    }

    // Initialize persistent DFU resources for firmware updates (BLE + USB).
    // ExternalFlashResources moved to StaticCell so QSPI gets 'static lifetime.
    let ext_flash_res = EXT_FLASH_RES.init(board.external_flash);
    let dfu_qspi = ext_flash_res.configure();
    // Safety: This NVMC instance only writes to BOOTLOADER_STATE (0x6000..0x7000).
    // The ProfileManager's NVMC writes to STORAGE (0xFE000..0x100000).
    // Non-overlapping regions, serialized by hardware.
    let dfu_nvmc = unsafe { embassy_nrf::peripherals::NVMC::steal() };
    let dfu_nvmc = Nvmc::new(dfu_nvmc);
    let dfu_resources =
        DFU_RESOURCES.init(DfuResources::new(dfu_qspi, dfu_nvmc));

    let mut power_manager = PowerManager::new(board.en5v.into());

    #[cfg(feature = "trouble")]
    let sdc = {
        let (sdc, mpsl) = board
            .ble
            .init(board.timer0, board.rng)
            .expect("BLE stack failed to initialize");
        spawner.must_spawn(mpsl_task(mpsl));
        sdc
    };

    // Initialize the allocator BEFORE you use it
    init_heap();
    // spawner.must_spawn(heap_usage());

    // Initialize the global event channel.
    let (sender, receiver) = init_event_channel();

    // Create our Profile Manager.
    let flash = embassy_embedded_hal::adapter::BlockingAsync::new(
        embassy_nrf::nvmc::Nvmc::new(board.nvmc),
    );

    let profile_manager = ProfileManager::new(flash);

    let (medium_prio_spawner, high_prio_spawner) = init_executors();

    let app_context = APP_CONTEXT.init(Mutex::new(AppContext {
        device_info: DeviceInfo {
            hardware_revision: heapless::String::try_from(HW_VERSION).unwrap(),
            software_revision: heapless::String::try_from(FW_VERSION).unwrap(),
            manufacturer_name: heapless::String::try_from(MANUFACTURER)
                .unwrap(),
        },
        high_prio_spawner,
        medium_prio_spawner,
        low_prio_spawner: spawner.make_send(),
        event_sender: sender,
        profile_manager,
        state: State {
            usb_powered: false,
            vsys_voltage: 0.0,
            recording_status: false,
        },
    }));
    let spi3_bus_resources =
        SPI3_BUS_RESOURCES.init(Mutex::new(board.spi3_bus_resources));
    let ads_resources = ADS_RESOURCES.init(Mutex::new(board.ads_resources));
    let sd_card_resources =
        SD_CARD_RESOURCES.init(Mutex::new(board.sd_card_resources));
    let i2c_bus_manager =
        I2C_BUS_MANAGER.init(I2cBusManager::new(board.twim1_bus_resources));
    let imu_resources = IMU_RESOURCES.init(Mutex::new(board.imu_resources));
    let mic_resources = MIC_RESOURCES.init(Mutex::new(board.mic));

    spawner.must_spawn(watchdog_task(board.wdt));

    Timer::after_millis(50).await;

    use npm1300::{
        charger::ChargerTerminationVoltage,
        gpios::{Gpio, GpioConfigBuilder, GpioMode, GpioPolarity},
        ldsw::LdoVoltage,
        sysreg::VbusInCurrentLimit,
        Ldsw1Ldosel, Ldsw1Softstartdisable, Ldsw1Softstartsel,
        NtcThermistorType, VsysThreshold, NPM1300,
    };

    // Acquire bus handle - configures bus if needed
    let handle = i2c_bus_manager.acquire().await.unwrap();
    let mut npm1300 = NPM1300::new(
        embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice::new(
            handle.bus(),
        ),
        embassy_time::Delay,
    );

    info!("Created nPM1300 driver!");
    Timer::after_millis(200).await;

    power_manager.handle_event(PowerEvent::Enable).await;

    npm1300
        .set_ldsw1_gpio_control(Gpio::None, GpioPolarity::NotInverted)
        .await
        .unwrap();
    Timer::after_millis(200).await;
    npm1300
        .set_ldsw2_gpio_control(Gpio::None, GpioPolarity::NotInverted)
        .await
        .unwrap();
    Timer::after_millis(200).await;

    info!("Check Status...");
    let status = npm1300.get_ldsw_status().await.unwrap();
    info!("LDSW status: {:?}", status);

    info!("Waiting 2s...");
    Timer::after_millis(200).await;

    info!("Configuring LDSW1 as Load Switch");
    let _ = npm1300.set_ldsw1_mode(Ldsw1Ldosel::Ldsw).await;
    let _ = npm1300
        .configure_ldsw1_soft_start(
            Ldsw1Softstartdisable::Noeffect,
            Ldsw1Softstartsel::Ma50,
        )
        .await;

    // Enable LDSW1
    info!("Pre-charging analog frontend...");
    let _ = npm1300.enable_ldsw1().await;

    Timer::after_millis(500).await;

    info!("Switching LDSW1 to LDO with 3.3V output...");
    // Set LDO1 output voltage to 3.3V
    let _ = npm1300.set_ldsw1_ldo_voltage(LdoVoltage::V3_3).await;
    info!("After set_ldsw1_ldo_voltage...");
    // Configure LDSW1 as LDO mode
    let _ = npm1300.set_ldsw1_mode(Ldsw1Ldosel::Ldo).await;
    info!("After set_ldsw1_mode...");

    info!("Check Status...");
    let status = npm1300.get_ldsw_status().await.unwrap();
    info!("LDSW status: {:?}", status);

    // Clear Charger Errors
    npm1300.clear_charger_errors().await.unwrap();

    // Set up battery charging
    npm1300
        .set_vbus_in_current_limit(VbusInCurrentLimit::MA100)
        .await
        .unwrap();
    npm1300.set_charger_current(32).await.unwrap(); // mA
    npm1300
        .configure_ntc_resistance(NtcThermistorType::Ntc10K, Some(4250.0))
        .await
        .unwrap();
    npm1300
        .set_normal_temperature_termination_voltage(
            ChargerTerminationVoltage::V4_20,
        )
        .await
        .unwrap();
    npm1300
        .set_warm_temperature_termination_voltage(
            ChargerTerminationVoltage::V4_10,
        )
        .await
        .unwrap();
    npm1300.enable_battery_charging().await.unwrap();

    Timer::after_millis(500).await;

    let chg_status = npm1300.get_charger_status().await.unwrap();
    info!("Charger status: {:?}", chg_status);

    let chg_error =
        npm1300.get_charger_error_reason_and_sensor_value().await.unwrap();
    info!("Charger Error: {:?}", chg_error);

    let mut pofena =
        npm1300.is_power_failure_detection_enabled().await.unwrap();
    info!("Power failure detection enabled: {:?}", pofena);

    let plw_config =
        GpioConfigBuilder::new().mode(GpioMode::GpoPowerLossWarning).build();
    npm1300.configure_gpio(1, plw_config).await.unwrap();
    npm1300.set_vsys_threshold(VsysThreshold::V32).await.unwrap();
    npm1300.enable_power_failure_detection(true).await.unwrap();

    pofena = npm1300.is_power_failure_detection_enabled().await.unwrap();
    info!("Power failure detection enabled?: {:?}", pofena);

    let ads_manager =
        AdsManager::new(spi3_bus_resources, ads_resources, app_context);
    let imu_manager =
        ImuManager::new(i2c_bus_manager, imu_resources, app_context);
    let apds_manager = ApdsManager::new(i2c_bus_manager, app_context);
    let haptic_manager = HapticManager::new(i2c_bus_manager, app_context);
    let mic_manager = MicManager::new(mic_resources, app_context);
    let session_manager = SessionManager::new(app_context, sd_card_resources);

    let _usbsel = {
        use embassy_nrf::gpio::{Level, Output, OutputDrive};
        Output::new(board.usbsel, Level::High, OutputDrive::Standard)
    };
    spawner.must_spawn(orchestrate(
        receiver,
        ads_manager.clone(),
        apds_manager,
        session_manager,
        imu_manager,
        mic_manager,
        haptic_manager,
        power_manager,
    ));

    {
        let mut context = app_context.lock().await;
        context
            .low_prio_spawner
            .must_spawn(button_task(board.pwrbtn.into(), sender));
        context
            .low_prio_spawner
            .must_spawn(neopix_task(board.pwm0, board.neopix.into()));

        // Check for ADS config.
        // create a default config.
        let config = context.profile_manager.get_ads_config().await;
        if config.is_none() {
            // create a default config.
            let num_chs = ads_manager.get_num_channels().await;
            let config = default_ads_settings(num_chs);
            info!("Settings ADS config: {:?}", config);
            context.save_ads_config(config).await;
        } else {
            info!("{:?}", config)
        }

        // Need to power down the ADS at startup.
        ads_manager.power_down(context.low_prio_spawner);
    }

    #[cfg(feature = "usb")]
    spawner.must_spawn(usb_task(
        spawner,
        board.usb,
        app_context,
        dfu_resources,
    ));

    #[cfg(feature = "trouble")]
    spawner.must_spawn(ble_run_task(sdc, app_context, dfu_resources));

    #[cfg(feature = "demo")]
    spawner.must_spawn(demo_task(sender));

    {
        let app_ctx = app_context.lock().await;
        app_ctx.event_sender.send(ImuEvent::StartStream.into()).await;
    }

    loop {
        Timer::after_secs(100).await;
        // match npm1300.measure_ntc().await {
        //     Ok(temp) => {
        //         info!("NPM1300 NTC meaurement = {:?} degrees Celsius", temp);
        //     }
        //     Err(e) => {
        //         info!("Error making NTC measurment: {:?}", e);
        //     }
        // }
    }
}
