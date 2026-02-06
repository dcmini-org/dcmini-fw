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

use dc_mini_app::{init_event_channel, prelude::*, FW_VERSION};

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
static APP_CONTEXT: StaticCell<Mutex<CriticalSectionRawMutex, AppContext>> =
    StaticCell::new();

// Application main entry point. The spawner can be used to start async tasks.
#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("In main!");
    // First we initialize our board.
    let board = DCMini::default();

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

    spawner.must_spawn(watchdog_task(board.wdt));

    Timer::after_millis(50).await;

    {
        use npm1300::{
            gpios::{Gpio, GpioPolarity},
            ldsw::LdoVoltage,
            Ldsw1Ldosel, Ldsw1Softstartdisable, Ldsw1Softstartsel,
            Ldsw2Ldosel, Ldsw2Softstartdisable, Ldsw2Softstartsel, NPM1300,
        };
        // Acquire bus handle - configures bus if needed
        let handle = i2c_bus_manager.acquire().await.unwrap();
        let mut npm1300 = NPM1300::new(handle.device(), embassy_time::Delay);

        info!("Created nPM1300 driver!");
        Timer::after_millis(200).await;

        power_manager.handle_event(PowerEvent::Enable).await;

        // let buck_status = npm1300.get_buck_status().await;
        // info!("Buck status: {:?}", buck_status);
        //
        // info!("Waiting 2s...");
        // Timer::after_millis(200).await;
        //
        // info!("Setting buck 1 voltage to 1.8V...");
        // let _ = npm1300.set_buck1_normal_voltage(BuckVoltage::V1_8).await;
        // let buck1_current_voltage = npm1300.get_buck1_vout_status().await;
        // info!("Set buck 1 voltage to {}", buck1_current_voltage);
        //
        // info!("Waiting 2s...");
        // Timer::after_millis(200).await;

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

        info!("Configuring LDSW1 as LDO with 3.3V output...");
        // Configure LDSW1 as LDO mode
        let _ = npm1300.set_ldsw1_mode(Ldsw1Ldosel::Ldo).await;
        info!("After set_ldsw1_mode...");
        Timer::after_millis(200).await;
        // Set LDO1 output voltage to 3.3V
        let _ = npm1300.set_ldsw1_ldo_voltage(LdoVoltage::V3_3).await;
        info!("After set_ldsw1_ldo_voltage...");
        Timer::after_millis(200).await;
        // Configure soft start
        let _ = npm1300
            .configure_ldsw1_soft_start(
                Ldsw1Softstartdisable::Noeffect,
                Ldsw1Softstartsel::Ma10,
            )
            .await;
        info!("After configure_ldsw1_soft_start...");
        Timer::after_millis(200).await;

        info!("Check Status...");
        let status = npm1300.get_ldsw_status().await.unwrap();
        info!("LDSW status: {:?}", status);

        // Enable LDSW1
        let _ = npm1300.enable_ldsw1().await;

        info!("After enable_ldsw1...");
        Timer::after_millis(200).await;

        info!("Configuring LDSW2...");
        // Configure LDSW2 as LDSW mode
        let _ = npm1300.set_ldsw2_mode(Ldsw2Ldosel::Ldsw).await;
        info!("After set_ldsw2_mode...");
        Timer::after_millis(200).await;

        // Configure soft start
        let _ = npm1300
            .configure_ldsw2_soft_start(
                Ldsw2Softstartdisable::Noeffect,
                Ldsw2Softstartsel::Ma50,
            )
            .await;
        info!("After configure_ldsw2_soft_start...");
        Timer::after_millis(200).await;

        info!("Checking LDSW status...");
        let status = npm1300.get_ldsw_status().await.unwrap();
        info!("LDSW status: {:?}", status);
    }

    {
        use apds9253::{Apds9253, LsGainRange, LsResolution};
        use embassy_time::Timer;
        // Acquire bus handle - configures bus if needed
        let handle = i2c_bus_manager.acquire().await.unwrap();
        let mut apds_sensor = Apds9253::new(handle.device());

        info!("Created APDS-9253 sensor driver!");
        // Initialize the sensor
        match apds_sensor.init_async().await {
            Ok(_) => {
                info!("APDS-9253 sensor initialized successfully!");

                // Get and verify device ID
                let result = apds_sensor.get_device_id_async().await;

                if let Ok((part_id, revision_id)) = result {
                    info!(
                        "APDS-9253 Device ID: 0x{:X}, Revision: 0x{:X}",
                        part_id, revision_id
                    );
                }

                // Configure sensor settings
                let _ = apds_sensor.set_gain_async(LsGainRange::Gain3X).await;
                info!("Set gain to 3X");
                Timer::after_millis(100).await;

                let _ = apds_sensor
                    .set_resolution_async(LsResolution::Bits18100Ms)
                    .await;
                info!("Set resolution to 18-bit 100ms integration");
                Timer::after_millis(100).await;

                // Enable RGB mode (all channels: R, G, B, IR)
                let _ = apds_sensor.enable_rgb_mode_async(true).await;
                info!("Enabled RGB mode");
                Timer::after_millis(100).await;

                // Enable the sensor
                let _ = apds_sensor.enable_async(true).await;
                info!("Enabled APDS-9253 sensor");
                Timer::after_millis(200).await;

                // Take a few sample readings
                for i in 0..3 {
                    Timer::after_millis(150).await; // Wait for measurement

                    if let Ok(true) = apds_sensor.is_data_ready_async().await {
                        if let Ok(rgb_data) =
                            apds_sensor.read_rgb_data_async().await
                        {
                            // Calculate lux
                            if let Ok(lux) = apds_sensor
                                .calculate_lux_async(&rgb_data)
                                .await
                            {
                                info!("Calculated lux: {:?}", lux);
                            }
                        }
                    } else {
                        info!("APDS data not ready on attempt {}", i + 1);
                    }
                }

                // Disable sensor when done
                let _ = apds_sensor.enable_async(false).await;
            }
            Err(e) => {
                info!("Failed to initialize APDS-9253 sensor: {:?}", e);
            }
        }
    }

    let ads_manager =
        AdsManager::new(spi3_bus_resources, ads_resources, app_context);
    let imu_manager =
        ImuManager::new(i2c_bus_manager, imu_resources, app_context);
    let session_manager = SessionManager::new(app_context, sd_card_resources);

    let _usbsel = {
        use embassy_nrf::gpio::{Level, Output, OutputDrive};
        Output::new(board.usbsel, Level::High, OutputDrive::Standard)
    };
    spawner.must_spawn(orchestrate(
        receiver,
        ads_manager.clone(),
        session_manager,
        imu_manager,
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
    ));

    #[cfg(feature = "trouble")]
    spawner.must_spawn(ble_run_task(sdc, app_context));
}
