use dc_mini_host::clients::UsbClient;
use dc_mini_host::icd::{
    AdsConfig, AdsDataFrame, AdsSample, BatteryLevel, CalFreq, CompThreshPos,
    DeviceInfo, FLeadOff, Gain, ILeadOff, Mux, ProfileCommand, SampleRate,
};
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use std::sync::{Arc, Mutex};
use std::thread;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

// Create custom exception types
create_exception!(dc_mini_host_py, UsbConnectionError, PyException);
create_exception!(dc_mini_host_py, UsbCommunicationError, PyException);

// Helper function to convert UsbError to PyErr
fn convert_error<E: std::fmt::Debug>(
    err: dc_mini_host::clients::UsbError<E>,
) -> PyErr {
    match err {
        dc_mini_host::clients::UsbError::Comms(e) => {
            UsbCommunicationError::new_err(format!(
                "Communication error: {:?}",
                e
            ))
        }
        dc_mini_host::clients::UsbError::Endpoint(e) => {
            UsbCommunicationError::new_err(format!("Endpoint error: {:?}", e))
        }
    }
}

// Python wrapper for AdsSample
#[pyclass]
#[derive(Clone, Debug)]
struct PyAdsSample {
    #[pyo3(get)]
    pub lead_off_positive: u32,
    #[pyo3(get)]
    pub lead_off_negative: u32,
    #[pyo3(get)]
    pub gpio: u32,
    #[pyo3(get)]
    pub data: Vec<i32>,
    #[pyo3(get)]
    pub accel_x: Option<f32>,
    #[pyo3(get)]
    pub accel_y: Option<f32>,
    #[pyo3(get)]
    pub accel_z: Option<f32>,
    #[pyo3(get)]
    pub gyro_x: Option<f32>,
    #[pyo3(get)]
    pub gyro_y: Option<f32>,
    #[pyo3(get)]
    pub gyro_z: Option<f32>,
}

impl From<AdsSample> for PyAdsSample {
    fn from(sample: AdsSample) -> Self {
        Self {
            lead_off_positive: sample.lead_off_positive,
            lead_off_negative: sample.lead_off_negative,
            gpio: sample.gpio,
            data: sample.data,
            accel_x: sample.accel_x,
            accel_y: sample.accel_y,
            accel_z: sample.accel_z,
            gyro_x: sample.gyro_x,
            gyro_y: sample.gyro_y,
            gyro_z: sample.gyro_z,
        }
    }
}

// Python wrapper for AdsDataFrame
#[pyclass]
#[derive(Clone, Debug)]
struct PyAdsDataFrame {
    #[pyo3(get)]
    pub timestamp: u64,
    #[pyo3(get)]
    pub samples: Vec<PyAdsSample>,
    #[pyo3(get)]
    pub channel_data: Vec<Vec<i32>>, // Reorganized data for easier Python use
}

#[pymethods]
impl PyAdsDataFrame {
    #[pyo3(name = "__repr__")]
    fn repr(&self) -> String {
        // You can rely on the Debug trait to format all fields, or do it manually.
        format!("{:?}", self)
    }
}

impl From<AdsDataFrame> for PyAdsDataFrame {
    fn from(frame: AdsDataFrame) -> Self {
        let py_samples = frame
            .samples
            .iter()
            .map(|sample| {
                // Create a new PyAdsSample by manually copying the fields
                PyAdsSample {
                    lead_off_positive: sample.lead_off_positive,
                    lead_off_negative: sample.lead_off_negative,
                    gpio: sample.gpio,
                    data: sample.data.clone(),
                    accel_x: sample.accel_x,
                    accel_y: sample.accel_y,
                    accel_z: sample.accel_z,
                    gyro_x: sample.gyro_x,
                    gyro_y: sample.gyro_y,
                    gyro_z: sample.gyro_z,
                }
            })
            .collect();

        // Reorganize data by channel for easier use in Python
        // First, determine how many channels we have
        let num_channels = if !frame.samples.is_empty()
            && !frame.samples[0].data.is_empty()
        {
            frame.samples[0].data.len()
        } else {
            0
        };

        // Create vectors for each channel
        let mut channel_data = vec![Vec::new(); num_channels];

        // Fill the channel data
        for sample in &frame.samples {
            for (i, value) in sample.data.iter().enumerate() {
                if i < channel_data.len() {
                    channel_data[i].push(*value);
                }
            }
        }

        Self { timestamp: frame.ts, samples: py_samples, channel_data }
    }
}

// Python wrapper for ChannelConfig
#[pyclass]
#[derive(Clone, Debug)]
struct PyChannelConfig {
    #[pyo3(get, set)]
    pub power_down: bool,
    #[pyo3(get, set)]
    pub gain: String,
    #[pyo3(get, set)]
    pub srb2: bool,
    #[pyo3(get, set)]
    pub mux: String,
    #[pyo3(get, set)]
    pub bias_sensp: bool,
    #[pyo3(get, set)]
    pub bias_sensn: bool,
    #[pyo3(get, set)]
    pub lead_off_sensp: bool,
    #[pyo3(get, set)]
    pub lead_off_sensn: bool,
    #[pyo3(get, set)]
    pub lead_off_flip: bool,
}

// Python wrapper for UsbClient
#[pyclass]
struct PyUsbClient {
    client: Arc<UsbClient>,
    runtime: Runtime,
    streaming_callback: Arc<Mutex<Option<PyObject>>>,
    streaming_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    py_callback_thread: Arc<Mutex<Option<thread::JoinHandle<()>>>>,
}

#[pymethods]
impl PyUsbClient {
    #[new]
    fn new() -> PyResult<Self> {
        let runtime = Runtime::new().map_err(|e| {
            PyException::new_err(format!(
                "Failed to create Tokio runtime: {}",
                e
            ))
        })?;

        let client = runtime.block_on(async {
            UsbClient::try_new().map_err(|e| {
                UsbConnectionError::new_err(format!(
                    "Failed to create USB client: {}",
                    e
                ))
            })
        })?;

        Ok(Self {
            client: Arc::new(client),
            runtime,
            streaming_callback: Arc::new(Mutex::new(None)),
            streaming_task: Arc::new(Mutex::new(None)),
            py_callback_thread: Arc::new(Mutex::new(None)),
        })
    }

    // ADS Service Methods
    #[pyo3(signature = (callback=None))]
    fn start_streaming(
        &self,
        py: Python<'_>,
        callback: Option<PyObject>,
    ) -> PyResult<PyAdsConfig> {
        let client = self.client.clone();

        // First, stop any existing streaming
        self.stop_streaming_internal();

        // Set the new callback if provided
        if let Some(cb) = callback {
            if !cb.bind(py).is_callable() {
                return Err(PyException::new_err("Callback must be callable"));
            }
            *self.streaming_callback.lock().unwrap() = Some(cb);
        }

        // Start the streaming
        let config = self.runtime.block_on(async move {
            client.start_streaming().await.map_err(convert_error)
        })?;

        // If we have a callback, start the streaming task
        if self.streaming_callback.lock().unwrap().is_some() {
            self.start_streaming_task();
        }

        Ok(PyAdsConfig::from(config))
    }

    fn stop_streaming(&self) -> PyResult<()> {
        self.stop_streaming_internal();

        let client = self.client.clone();
        self.runtime.block_on(async move {
            client.stop_streaming().await.map_err(convert_error)
        })
    }

    fn reset_ads_config(&self) -> PyResult<bool> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            client.reset_ads_config().await.map_err(convert_error)
        })
    }

    fn get_ads_config(&self) -> PyResult<PyAdsConfig> {
        let client = self.client.clone();
        let config = self.runtime.block_on(async move {
            client.get_ads_config().await.map_err(convert_error)
        })?;
        Ok(PyAdsConfig::from(config))
    }

    fn set_ads_config(&self, config: PyAdsConfig) -> PyResult<bool> {
        let client = self.client.clone();
        let ads_config = config.to_ads_config();
        self.runtime.block_on(async move {
            client.set_ads_config(ads_config).await.map_err(convert_error)
        })
    }

    // Battery Service Methods
    fn get_battery_level(&self) -> PyResult<PyBatteryLevel> {
        let client = self.client.clone();
        let level = self.runtime.block_on(async move {
            client.get_battery_level().await.map_err(convert_error)
        })?;
        Ok(PyBatteryLevel::from(level))
    }

    // Device Info Service Methods
    fn get_device_info(&self) -> PyResult<PyDeviceInfo> {
        let client = self.client.clone();
        let info = self.runtime.block_on(async move {
            client.get_device_info().await.map_err(convert_error)
        })?;
        Ok(PyDeviceInfo::from(info))
    }

    // Profile Service Methods
    fn get_profile(&self) -> PyResult<u8> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            client.get_profile().await.map_err(convert_error)
        })
    }

    fn set_profile(&self, profile: u8) -> PyResult<bool> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            client.set_profile(profile).await.map_err(convert_error)
        })
    }

    fn send_profile_command(&self, cmd: &str) -> PyResult<bool> {
        let client = self.client.clone();
        let command = match cmd {
            // Adjust these to match your actual ProfileCommand enum variants
            "next" => ProfileCommand::Next,
            "previous" => ProfileCommand::Previous,
            "reset" => ProfileCommand::Reset,
            _ => {
                return Err(PyException::new_err(format!(
                    "Invalid command: {}",
                    cmd
                )))
            }
        };
        self.runtime.block_on(async move {
            client.send_profile_command(command).await.map_err(convert_error)
        })
    }

    // Session Service Methods
    fn get_session_status(&self) -> PyResult<bool> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            client.get_session_status().await.map_err(convert_error)
        })
    }

    fn get_session_id(&self) -> PyResult<String> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            client.get_session_id().await.map_err(convert_error)
        })
    }

    fn set_session_id(&self, id: String) -> PyResult<bool> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            client.set_session_id(id).await.map_err(convert_error)
        })
    }

    fn start_session(&self) -> PyResult<bool> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            client.start_session().await.map_err(convert_error)
        })
    }

    fn stop_session(&self) -> PyResult<bool> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            client.stop_session().await.map_err(convert_error)
        })
    }

    fn is_connected(&self) -> bool {
        self.client.is_connected()
    }
}

impl PyUsbClient {
    fn start_streaming_task(&self) {
        let client = self.client.clone();
        let callback = self.streaming_callback.clone();
        let runtime = self.runtime.handle().clone();

        // Create a channel for sending data from the async task to the Python callback thread
        let (tx, mut rx) = mpsc::unbounded_channel();

        // Start the async task to receive data from the device
        let streaming_task = runtime.spawn(async move {
            // Subscribe to the ADS data topic
            let sub = client
                .client
                .subscribe_multi::<dc_mini_host::icd::AdsTopic>(8)
                .await;

            if let Ok(mut sub) = sub {
                println!("Subscribed to ADS data topic");
                while let Ok(frame) = sub.recv().await {
                    // Send the frame to the Python callback thread
                    if tx.send(frame).is_err() {
                        // Channel closed, exit the task
                        break;
                    }
                }
            } else {
                println!("Failed to subscribe to ADS data topic");
            }
        });

        // Store the task handle so we can cancel it later
        *self.streaming_task.lock().unwrap() = Some(streaming_task);

        // Start a thread to call the Python callback
        let py_thread = thread::spawn(move || {
            while let Some(frame) = rx.blocking_recv() {
                // Convert the frame to a Python object
                let py_frame = PyAdsDataFrame::from(frame);

                // Call the Python callback with a fresh GIL acquisition for each frame
                Python::with_gil(|py| {
                    if let Some(callback) = &*callback.lock().unwrap() {
                        let args = (py_frame.clone(),);
                        if let Err(e) = callback.call1(py, args) {
                            println!("Error calling Python callback: {:?}", e);
                            // Continue processing even if the callback fails
                        }
                    }
                });
            }
        });

        // Store the thread handle
        *self.py_callback_thread.lock().unwrap() = Some(py_thread);
    }

    fn stop_streaming_internal(&self) {
        // Cancel the streaming task if it exists
        if let Some(task) = self.streaming_task.lock().unwrap().take() {
            task.abort();
        }

        // Clear the callback
        *self.streaming_callback.lock().unwrap() = None;

        // The Python callback thread will exit when the channel is closed
        if let Some(thread) = self.py_callback_thread.lock().unwrap().take() {
            // We can't join the thread here because it might be waiting for data
            // Just let it exit naturally when the channel is closed
            let _ = thread;
        }
    }
}

impl Drop for PyUsbClient {
    fn drop(&mut self) {
        self.stop_streaming_internal();
    }
}

// Python wrapper for AdsConfig
#[pyclass]
#[derive(Clone, Debug)]
struct PyAdsConfig {
    #[pyo3(get, set)]
    pub daisy_en: bool,
    #[pyo3(get, set)]
    pub clk_en: bool,
    #[pyo3(get, set)]
    pub sample_rate: String,
    #[pyo3(get, set)]
    pub internal_calibration: bool,
    #[pyo3(get, set)]
    pub calibration_amplitude: bool,
    #[pyo3(get, set)]
    pub calibration_frequency: String,
    #[pyo3(get, set)]
    pub pd_refbuf: bool,
    #[pyo3(get, set)]
    pub bias_meas: bool,
    #[pyo3(get, set)]
    pub biasref_int: bool,
    #[pyo3(get, set)]
    pub pd_bias: bool,
    #[pyo3(get, set)]
    pub bias_loff_sens: bool,
    #[pyo3(get, set)]
    pub bias_stat: bool,
    #[pyo3(get, set)]
    pub comparator_threshold_pos: String,
    #[pyo3(get, set)]
    pub lead_off_current: String,
    #[pyo3(get, set)]
    pub lead_off_frequency: String,
    #[pyo3(get, set)]
    pub gpioc: Vec<bool>,
    #[pyo3(get, set)]
    pub srb1: bool,
    #[pyo3(get, set)]
    pub single_shot: bool,
    #[pyo3(get, set)]
    pub pd_loff_comp: bool,
    #[pyo3(get, set)]
    pub channels: Vec<PyChannelConfig>,
}

impl From<AdsConfig> for PyAdsConfig {
    fn from(config: AdsConfig) -> Self {
        let sample_rate = match config.sample_rate {
            SampleRate::Sps250 => "250 SPS",
            SampleRate::Sps500 => "500 SPS",
            SampleRate::KSps1 => "1 KSPS",
            SampleRate::KSps2 => "2 KSPS",
            SampleRate::KSps4 => "4 KSPS",
            SampleRate::KSps8 => "8 KSPS",
            SampleRate::KSps16 => "16 KSPS",
        }
        .to_string();

        let cal_freq = match config.calibration_frequency {
            CalFreq::FclkBy21 => "FCLK/2^21",
            CalFreq::FclkBy20 => "FCLK/2^20",
            CalFreq::DoNotUse => "DO_NOT_USE",
            CalFreq::DC => "DC",
        }
        .to_string();

        let comp_thresh = match config.comparator_threshold_pos {
            CompThreshPos::_95 => "95%",
            CompThreshPos::_92_5 => "92.5%",
            CompThreshPos::_90 => "90%",
            CompThreshPos::_87_5 => "87.5%",
            CompThreshPos::_85 => "85%",
            CompThreshPos::_80 => "80%",
            CompThreshPos::_75 => "75%",
            CompThreshPos::_70 => "70%",
        }
        .to_string();

        let lead_off_current = match config.lead_off_current {
            ILeadOff::_6nA => "6nA",
            ILeadOff::_24nA => "24nA",
            ILeadOff::_6uA => "6uA",
            ILeadOff::_24uA => "24uA",
        }
        .to_string();

        let lead_off_freq = match config.lead_off_frequency {
            FLeadOff::Dc => "DC",
            FLeadOff::Ac7_8 => "7.8Hz",
            FLeadOff::Ac31_2 => "31.2Hz",
            FLeadOff::AcFdrBy4 => "FDR/4",
        }
        .to_string();

        // Convert channel configs
        let channels = config
            .channels
            .iter()
            .map(|ch| {
                let gain = match ch.gain {
                    Gain::X1 => "x1",
                    Gain::X2 => "x2",
                    Gain::X4 => "x4",
                    Gain::X6 => "x6",
                    Gain::X8 => "x8",
                    Gain::X12 => "x12",
                    Gain::X24 => "x24",
                }
                .to_string();

                let mux = match ch.mux {
                    Mux::NormalElectrodeInput => "Normal",
                    Mux::InputShorted => "Shorted",
                    Mux::RldMeasure => "RLD_Measure",
                    Mux::MVDD => "MVDD",
                    Mux::TemperatureSensor => "Temperature",
                    Mux::TestSignal => "TestSignal",
                    Mux::RldDrp => "RLD_DRP",
                    Mux::RldDrn => "RLD_DRN",
                }
                .to_string();

                PyChannelConfig {
                    power_down: ch.power_down,
                    gain,
                    srb2: ch.srb2,
                    mux,
                    bias_sensp: ch.bias_sensp,
                    bias_sensn: ch.bias_sensn,
                    lead_off_sensp: ch.lead_off_sensp,
                    lead_off_sensn: ch.lead_off_sensn,
                    lead_off_flip: ch.lead_off_flip,
                }
            })
            .collect();

        Self {
            daisy_en: config.daisy_en,
            clk_en: config.clk_en,
            sample_rate,
            internal_calibration: config.internal_calibration,
            calibration_amplitude: config.calibration_amplitude,
            calibration_frequency: cal_freq,
            pd_refbuf: config.pd_refbuf,
            bias_meas: config.bias_meas,
            biasref_int: config.biasref_int,
            pd_bias: config.pd_bias,
            bias_loff_sens: config.bias_loff_sens,
            bias_stat: config.bias_stat,
            comparator_threshold_pos: comp_thresh,
            lead_off_current,
            lead_off_frequency: lead_off_freq,
            gpioc: config.gpioc.to_vec(),
            srb1: config.srb1,
            single_shot: config.single_shot,
            pd_loff_comp: config.pd_loff_comp,
            channels,
        }
    }
}

impl PyAdsConfig {
    fn to_ads_config(&self) -> AdsConfig {
        let sample_rate = match self.sample_rate.as_str() {
            "250 SPS" => SampleRate::Sps250,
            "500 SPS" => SampleRate::Sps500,
            "1 KSPS" => SampleRate::KSps1,
            "2 KSPS" => SampleRate::KSps2,
            "4 KSPS" => SampleRate::KSps4,
            "8 KSPS" => SampleRate::KSps8,
            "16 KSPS" => SampleRate::KSps16,
            _ => SampleRate::Sps250, // Default
        };

        let cal_freq = match self.calibration_frequency.as_str() {
            "FCLK/2^21" => CalFreq::FclkBy21,
            "FCLK/2^20" => CalFreq::FclkBy20,
            "DO_NOT_USE" => CalFreq::DoNotUse,
            "DC" => CalFreq::DC,
            _ => CalFreq::FclkBy21, // Default
        };

        let comp_thresh = match self.comparator_threshold_pos.as_str() {
            "95%" => CompThreshPos::_95,
            "92.5%" => CompThreshPos::_92_5,
            "90%" => CompThreshPos::_90,
            "87.5%" => CompThreshPos::_87_5,
            "85%" => CompThreshPos::_85,
            "80%" => CompThreshPos::_80,
            "75%" => CompThreshPos::_75,
            "70%" => CompThreshPos::_70,
            _ => CompThreshPos::_95, // Default
        };

        let lead_off_current = match self.lead_off_current.as_str() {
            "6nA" => ILeadOff::_6nA,
            "24nA" => ILeadOff::_24nA,
            "6uA" => ILeadOff::_6uA,
            "24uA" => ILeadOff::_24uA,
            _ => ILeadOff::_6nA, // Default
        };

        let lead_off_freq = match self.lead_off_frequency.as_str() {
            "DC" => FLeadOff::Dc,
            "7.8Hz" => FLeadOff::Ac7_8,
            "31.2Hz" => FLeadOff::Ac31_2,
            "FDR/4" => FLeadOff::AcFdrBy4,
            _ => FLeadOff::Dc, // Default
        };

        // Convert channel configs
        let mut channels = heapless::Vec::new();
        for ch in &self.channels {
            let gain = match ch.gain.as_str() {
                "x1" => Gain::X1,
                "x2" => Gain::X2,
                "x4" => Gain::X4,
                "x6" => Gain::X6,
                "x8" => Gain::X8,
                "x12" => Gain::X12,
                "x24" => Gain::X24,
                _ => Gain::X1, // Default
            };

            let mux = match ch.mux.as_str() {
                "Normal" => Mux::NormalElectrodeInput,
                "Shorted" => Mux::InputShorted,
                "RLD_Measure" => Mux::RldMeasure,
                "MVDD" => Mux::MVDD,
                "Temperature" => Mux::TemperatureSensor,
                "TestSignal" => Mux::TestSignal,
                "RLD_DRP" => Mux::RldDrp,
                "RLD_DRN" => Mux::RldDrn,
                _ => Mux::NormalElectrodeInput, // Default
            };

            let channel_config = dc_mini_host::icd::ChannelConfig {
                power_down: ch.power_down,
                gain,
                srb2: ch.srb2,
                mux,
                bias_sensp: ch.bias_sensp,
                bias_sensn: ch.bias_sensn,
                lead_off_sensp: ch.lead_off_sensp,
                lead_off_sensn: ch.lead_off_sensn,
                lead_off_flip: ch.lead_off_flip,
            };

            // Use try_push to handle the case where we exceed the capacity
            if channels.push(channel_config).is_err() {
                // We've reached the maximum number of channels
                break;
            }
        }

        // Create a default config and update the fields
        let mut config = AdsConfig::default();
        config.daisy_en = self.daisy_en;
        config.clk_en = self.clk_en;
        config.sample_rate = sample_rate;
        config.internal_calibration = self.internal_calibration;
        config.calibration_amplitude = self.calibration_amplitude;
        config.calibration_frequency = cal_freq;
        config.pd_refbuf = self.pd_refbuf;
        config.bias_meas = self.bias_meas;
        config.biasref_int = self.biasref_int;
        config.pd_bias = self.pd_bias;
        config.bias_loff_sens = self.bias_loff_sens;
        config.bias_stat = self.bias_stat;
        config.comparator_threshold_pos = comp_thresh;
        config.lead_off_current = lead_off_current;
        config.lead_off_frequency = lead_off_freq;

        // Copy GPIOC settings (up to 4)
        for (i, &enabled) in self.gpioc.iter().enumerate().take(4) {
            if i < config.gpioc.len() {
                config.gpioc[i] = enabled;
            }
        }

        config.srb1 = self.srb1;
        config.single_shot = self.single_shot;
        config.pd_loff_comp = self.pd_loff_comp;
        config.channels = channels;

        config
    }
}

#[pymethods]
impl PyAdsConfig {
    #[pyo3(name = "__repr__")]
    fn repr(&self) -> String {
        // You can rely on the Debug trait to format all fields, or do it manually.
        format!("{:?}", self)
    }
}

// Python wrapper for BatteryLevel
#[pyclass]
#[derive(Clone)]
struct PyBatteryLevel {
    #[pyo3(get)]
    pub percentage: u8,
    #[pyo3(get)]
    pub voltage_mv: u16,
    #[pyo3(get)]
    pub charging: bool,
}

impl From<BatteryLevel> for PyBatteryLevel {
    fn from(_level: BatteryLevel) -> Self {
        // Adjust based on your actual BatteryLevel structure
        Self {
            percentage: 100,  // Default value
            voltage_mv: 4200, // Default value
            charging: false,  // Default value
        }
    }
}

// Python wrapper for DeviceInfo
#[pyclass]
#[derive(Clone)]
struct PyDeviceInfo {
    #[pyo3(get)]
    pub hw_version: String,
    #[pyo3(get)]
    pub fw_version: String,
    #[pyo3(get)]
    pub serial_number: String,
}

impl From<DeviceInfo> for PyDeviceInfo {
    fn from(info: DeviceInfo) -> Self {
        Self {
            hw_version: info.hardware_revision.to_string(),
            fw_version: info.software_revision.to_string(),
            serial_number: info.manufacturer_name.to_string(), // Adjust if there's a better field
        }
    }
}

/// A Python module for controlling DC Mini devices via USB.
#[pymodule]
fn dc_mini_host_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyUsbClient>()?;
    m.add_class::<PyAdsConfig>()?;
    m.add_class::<PyChannelConfig>()?;
    m.add_class::<PyBatteryLevel>()?;
    m.add_class::<PyDeviceInfo>()?;
    m.add_class::<PyAdsDataFrame>()?;
    m.add_class::<PyAdsSample>()?;

    // Add custom exceptions
    m.add("UsbConnectionError", m.py().get_type::<UsbConnectionError>())?;
    m.add(
        "UsbCommunicationError",
        m.py().get_type::<UsbCommunicationError>(),
    )?;

    Ok(())
}
