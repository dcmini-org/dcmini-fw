use std::collections::{HashMap, VecDeque};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use dc_mini_icd::{
    self as icd, AdsConfig, AdsDataFrame, AdsGetConfigEndpoint,
    AdsResetConfigEndpoint, AdsSetConfigEndpoint, AdsStartEndpoint,
    AdsStopEndpoint, BatteryGetLevelEndpoint, CvepConfig, CvepDecision,
    CvepGetConfigEndpoint, CvepGetStatusEndpoint, CvepStartEndpoint,
    CvepStopEndpoint, DeviceInfo, DeviceInfoGetEndpoint, DfuAbortEndpoint,
    DfuBegin, DfuBeginEndpoint, DfuFinishEndpoint, DfuProgress,
    DfuProgressState, DfuStatusEndpoint, DfuWriteChunk, DfuWriteEndpoint,
    MicConfig, MicGetConfigEndpoint, MicSetConfigEndpoint, MicStartEndpoint,
    MicStopEndpoint, ProfileCommand, ProfileCommandEndpoint,
    ProfileGetEndpoint, ProfileSetEndpoint, SessionGetIdEndpoint,
    SessionGetStatusEndpoint, SessionId, SessionSetIdEndpoint,
    SessionStartEndpoint, SessionStopEndpoint,
};
use heapless::{String as HeaplessString, Vec as HeaplessVec};
use postcard_rpc::{
    host_client::{HostClient, MultiSubRxError},
    standard_icd::WireError,
};
use tokio::task::JoinHandle;

#[cfg(any(target_os = "android", target_os = "linux"))]
use nusb::{
    transfer::{Direction, EndpointType, Queue, RequestBuffer, TransferError},
    Device, Interface,
};
#[cfg(any(target_os = "android", target_os = "linux"))]
use postcard_rpc::host_client::{WireRx, WireSpawn, WireTx};
#[cfg(any(target_os = "android", target_os = "linux"))]
use postcard_rpc::{header::VarSeqKind, standard_icd::ERROR_PATH};
#[cfg(any(target_os = "android", target_os = "linux"))]
use std::future::Future;
#[cfg(any(target_os = "android", target_os = "linux"))]
use thiserror::Error;

pub const DCMINI_ABI_VERSION: u32 = 1;
pub const DCMINI_DEVICE_INFO_UTF8_CAPACITY: u32 = 32;
pub const DCMINI_SESSION_ID_UTF8_CAPACITY: u32 = 4;
pub const DCMINI_MAX_ADS_CHANNELS: u32 = 16;
pub const DCMINI_DFU_MAX_WRITE_SIZE: u32 = 512;
pub const DCMINI_WAIT_CLOSED_INFINITE_MS: u32 = u32::MAX;
pub const DCMINI_ADS_AUX_ACCEL_X_PRESENT: u32 = 1 << 0;
pub const DCMINI_ADS_AUX_ACCEL_Y_PRESENT: u32 = 1 << 1;
pub const DCMINI_ADS_AUX_ACCEL_Z_PRESENT: u32 = 1 << 2;
pub const DCMINI_ADS_AUX_GYRO_X_PRESENT: u32 = 1 << 3;
pub const DCMINI_ADS_AUX_GYRO_Y_PRESENT: u32 = 1 << 4;
pub const DCMINI_ADS_AUX_GYRO_Z_PRESENT: u32 = 1 << 5;

const ADS_QUEUE_CAPACITY: usize = 32;
const MIC_QUEUE_CAPACITY: usize = 64;
const CVEP_QUEUE_CAPACITY: usize = 64;
const SUBSCRIPTION_DEPTH: usize = 32;
#[cfg(any(target_os = "android", target_os = "linux"))]
const OUTGOING_DEPTH: usize = 8;
#[cfg(any(target_os = "android", target_os = "linux"))]
const MAX_TRANSFER_SIZE: usize = 1024;
#[cfg(any(target_os = "android", target_os = "linux"))]
const IN_FLIGHT_REQS: usize = 4;
#[cfg(any(target_os = "android", target_os = "linux"))]
const MAX_STALL_RETRIES: usize = 10;

#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DcMiniStatus {
    Ok = 0,
    InvalidHandle = 1,
    InvalidArgument = 2,
    BufferTooSmall = 3,
    NotConnected = 4,
    Unimplemented = 5,
    InternalError = 6,
}

#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DcMiniTransportKind {
    None = 0,
    AndroidUsb = 1,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DcMiniAdsConfig {
    pub sample_rate_hz: u32,
    pub channel_count: u32,
    pub daisy_enabled: u8,
    pub clk_enabled: u8,
    pub internal_calibration_enabled: u8,
    pub calibration_amplitude_enabled: u8,
    pub calibration_frequency: u32,
    pub pd_refbuf: u8,
    pub bias_meas_enabled: u8,
    pub biasref_int_enabled: u8,
    pub pd_bias: u8,
    pub bias_loff_sens_enabled: u8,
    pub bias_stat_enabled: u8,
    pub comparator_threshold_pos: u32,
    pub lead_off_current: u32,
    pub lead_off_frequency: u32,
    pub gpioc0: u8,
    pub gpioc1: u8,
    pub gpioc2: u8,
    pub gpioc3: u8,
    pub srb1_enabled: u8,
    pub single_shot_enabled: u8,
    pub pd_loff_comp: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DcMiniAdsChannelConfig {
    pub gain: u32,
    pub mux: u32,
    pub power_down: u8,
    pub srb2_enabled: u8,
    pub bias_sensp_enabled: u8,
    pub bias_sensn_enabled: u8,
    pub lead_off_sensp_enabled: u8,
    pub lead_off_sensn_enabled: u8,
    pub lead_off_flip_enabled: u8,
    pub reserved0: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DcMiniMicConfig {
    pub gain_db: i32,
    pub sample_rate_hz: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DcMiniAdsFrameHeader {
    pub timestamp_us: u64,
    pub sample_count: u32,
    pub channel_count: u32,
    pub samples_offset: u32,
    pub aux_offset: u32,
    pub flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DcMiniAdsSampleAux {
    pub lead_off_positive: u32,
    pub lead_off_negative: u32,
    pub gpio: u32,
    pub accel_x: f32,
    pub accel_y: f32,
    pub accel_z: f32,
    pub gyro_x: f32,
    pub gyro_y: f32,
    pub gyro_z: f32,
    pub flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DcMiniMicPacketHeader {
    pub timestamp_us: u64,
    pub packet_counter: u64,
    pub sample_rate_hz: u32,
    pub predictor: i32,
    pub step_index: u32,
    pub data_offset: u32,
    pub data_len: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DcMiniCvepConfig {
    pub model_enabled: u8,
    pub channels: u32,
    pub classes: u32,
    pub window_samples: u32,
    pub inference_stride_samples: u32,
    pub has_score_threshold: u8,
    pub score_threshold: f32,
    pub has_margin_threshold: u8,
    pub margin_threshold: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DcMiniCvepDecision {
    pub timestamp_us: u64,
    pub class_index: u32,
    pub raw_score: i64,
    pub normalized_score: f32,
    pub margin: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DcMiniStreamStats {
    pub ads_queue_len: u32,
    pub mic_queue_len: u32,
    pub cvep_queue_len: u32,
    pub ads_frames_dropped: u64,
    pub mic_packets_dropped: u64,
    pub cvep_decisions_dropped: u64,
}

#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DcMiniProfileCommand {
    Reset = 0,
    Next = 1,
    Previous = 2,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DcMiniDfuProgress {
    pub state: i32,
    pub offset: u32,
    pub total_size: u32,
}

struct DcMiniContext {
    runtime: tokio::runtime::Runtime,
    last_error: Arc<Mutex<String>>,
    transport: TransportState,
    connection: Option<LiveConnection>,
    shared: Arc<SharedState>,
    ads_config_cache: AdsConfig,
    mic_config_cache: MicConfig,
    device_info_cache: Option<DeviceInfo>,
    cvep_config_cache: CvepConfig,
    cvep_active_cache: bool,
    profile_cache: u8,
    session_id_cache: String,
    session_active_cache: bool,
    battery_percent_cache: u8,
    dfu_progress_cache: DfuProgress,
}

struct LiveConnection {
    client: Arc<HostClient<WireError>>,
    ads_task: Option<JoinHandle<()>>,
    mic_task: Option<JoinHandle<()>>,
    cvep_task: Option<JoinHandle<()>>,
}

#[derive(Clone, Copy, Debug)]
enum TransportState {
    None,
    AndroidUsb { _interface_index: u8 },
}

struct SharedState {
    connected: AtomicBool,
    ads_queue: Mutex<VecDeque<OwnedAdsFrame>>,
    mic_queue: Mutex<VecDeque<OwnedMicPacket>>,
    cvep_queue: Mutex<VecDeque<OwnedCvepDecision>>,
    stats: Mutex<DcMiniStreamStats>,
    last_error: Arc<Mutex<String>>,
}

#[derive(Debug)]
struct OwnedAdsFrame {
    header: DcMiniAdsFrameHeader,
    samples: Vec<i32>,
    sample_aux: Vec<DcMiniAdsSampleAux>,
}

#[derive(Debug)]
struct OwnedMicPacket {
    header: DcMiniMicPacketHeader,
    bytes: Vec<u8>,
}

#[derive(Debug)]
struct OwnedCvepDecision {
    decision: DcMiniCvepDecision,
}

static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);
static GLOBAL_ERROR: OnceLock<Mutex<String>> = OnceLock::new();
static REGISTRY: OnceLock<Mutex<HashMap<u64, DcMiniContext>>> =
    OnceLock::new();

fn registry() -> &'static Mutex<HashMap<u64, DcMiniContext>> {
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn global_error() -> &'static Mutex<String> {
    GLOBAL_ERROR.get_or_init(|| Mutex::new(String::new()))
}

fn lock_registry() -> MutexGuard<'static, HashMap<u64, DcMiniContext>> {
    registry().lock().expect("dc-mini-host-unity registry poisoned")
}

fn set_global_error(message: impl Into<String>) {
    *global_error()
        .lock()
        .expect("dc-mini-host-unity global error poisoned") = message.into();
}

fn set_last_error(
    last_error: &Arc<Mutex<String>>,
    message: impl Into<String>,
) {
    *last_error.lock().expect("dc-mini-host-unity last error poisoned") =
        message.into();
}

fn with_context_mut<T>(
    handle: u64,
    f: impl FnOnce(&mut DcMiniContext) -> T,
) -> Result<T, DcMiniStatus> {
    let mut registry = lock_registry();
    let Some(ctx) = registry.get_mut(&handle) else {
        set_global_error(format!("invalid dc-mini handle: {handle}"));
        return Err(DcMiniStatus::InvalidHandle);
    };
    Ok(f(ctx))
}

fn copy_utf8_bytes(
    source: &str,
    buffer: *mut u8,
    buffer_len: u32,
    out_len: *mut u32,
) -> DcMiniStatus {
    if out_len.is_null() {
        return DcMiniStatus::InvalidArgument;
    }

    let bytes = source.as_bytes();
    unsafe {
        *out_len = bytes.len() as u32;
    }

    if buffer.is_null() {
        return DcMiniStatus::InvalidArgument;
    }
    if buffer_len < bytes.len() as u32 {
        return DcMiniStatus::BufferTooSmall;
    }

    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, bytes.len());
    }

    DcMiniStatus::Ok
}

fn default_channel_config() -> icd::ChannelConfig {
    icd::ChannelConfig {
        power_down: false,
        gain: icd::Gain::X24,
        srb2: false,
        mux: icd::Mux::NormalElectrodeInput,
        bias_sensp: false,
        bias_sensn: false,
        lead_off_sensp: false,
        lead_off_sensn: false,
        lead_off_flip: false,
    }
}

fn new_context() -> Result<DcMiniContext, String> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("dcmini-unity")
        .build()
        .map_err(|err| format!("failed to build tokio runtime: {err}"))?;

    let last_error = Arc::new(Mutex::new(String::new()));
    let shared = Arc::new(SharedState {
        connected: AtomicBool::new(false),
        ads_queue: Mutex::new(VecDeque::new()),
        mic_queue: Mutex::new(VecDeque::new()),
        cvep_queue: Mutex::new(VecDeque::new()),
        stats: Mutex::new(DcMiniStreamStats::default()),
        last_error: last_error.clone(),
    });

    Ok(DcMiniContext {
        runtime,
        last_error,
        transport: TransportState::None,
        connection: None,
        shared,
        ads_config_cache: AdsConfig::default(),
        mic_config_cache: MicConfig::default(),
        device_info_cache: None,
        cvep_config_cache: CvepConfig {
            model_enabled: false,
            channels: 0,
            classes: 0,
            window_samples: 0,
            inference_stride_samples: 0,
            score_threshold: None,
            margin_threshold: None,
        },
        cvep_active_cache: false,
        profile_cache: 0,
        session_id_cache: String::from("0000"),
        session_active_cache: false,
        battery_percent_cache: 0,
        dfu_progress_cache: DfuProgress {
            state: DfuProgressState::Idle,
            offset: 0,
            total_size: 0,
        },
    })
}

fn map_wire_error(
    err: postcard_rpc::host_client::HostErr<WireError>,
) -> String {
    err.to_string()
}

fn map_lag_error(kind: &str, dropped: u64, last_error: &Arc<Mutex<String>>) {
    set_last_error(
        last_error,
        format!("{kind} stream lagged and dropped {dropped} frame(s)"),
    );
}

fn sample_rate_to_hz(sample_rate: icd::SampleRate) -> u32 {
    match sample_rate {
        icd::SampleRate::Sps250 => 250,
        icd::SampleRate::Sps500 => 500,
        icd::SampleRate::KSps1 => 1_000,
        icd::SampleRate::KSps2 => 2_000,
        icd::SampleRate::KSps4 => 4_000,
        icd::SampleRate::KSps8 => 8_000,
        icd::SampleRate::KSps16 => 16_000,
    }
}

fn sample_rate_from_hz(sample_rate_hz: u32) -> Option<icd::SampleRate> {
    match sample_rate_hz {
        250 => Some(icd::SampleRate::Sps250),
        500 => Some(icd::SampleRate::Sps500),
        1_000 => Some(icd::SampleRate::KSps1),
        2_000 => Some(icd::SampleRate::KSps2),
        4_000 => Some(icd::SampleRate::KSps4),
        8_000 => Some(icd::SampleRate::KSps8),
        16_000 => Some(icd::SampleRate::KSps16),
        _ => None,
    }
}

fn mic_sample_rate_to_hz(sample_rate: icd::MicSampleRate) -> u32 {
    sample_rate.as_hz()
}

fn mic_sample_rate_from_hz(sample_rate_hz: u32) -> Option<icd::MicSampleRate> {
    match sample_rate_hz {
        16_000 => Some(icd::MicSampleRate::Rate16000),
        12_800 => Some(icd::MicSampleRate::Rate12800),
        20_000 => Some(icd::MicSampleRate::Rate20000),
        _ => None,
    }
}

fn gain_from_raw(raw: u32) -> Option<icd::Gain> {
    match raw {
        0 => Some(icd::Gain::X1),
        1 => Some(icd::Gain::X2),
        2 => Some(icd::Gain::X4),
        3 => Some(icd::Gain::X6),
        4 => Some(icd::Gain::X8),
        5 => Some(icd::Gain::X12),
        6 => Some(icd::Gain::X24),
        _ => None,
    }
}

fn gain_to_raw(gain: icd::Gain) -> u32 {
    let raw: u8 = gain.into();
    raw as u32
}

fn mux_from_raw(raw: u32) -> Option<icd::Mux> {
    match raw {
        0 => Some(icd::Mux::NormalElectrodeInput),
        1 => Some(icd::Mux::InputShorted),
        2 => Some(icd::Mux::RldMeasure),
        3 => Some(icd::Mux::MVDD),
        4 => Some(icd::Mux::TemperatureSensor),
        5 => Some(icd::Mux::TestSignal),
        6 => Some(icd::Mux::RldDrp),
        7 => Some(icd::Mux::RldDrn),
        _ => None,
    }
}

fn mux_to_raw(mux: icd::Mux) -> u32 {
    let raw: u8 = mux.into();
    raw as u32
}

fn cal_freq_to_raw(value: icd::CalFreq) -> u32 {
    let raw: u8 = value.into();
    raw as u32
}

fn cal_freq_from_raw(raw: u32) -> Option<icd::CalFreq> {
    match raw {
        0 => Some(icd::CalFreq::FclkBy21),
        1 => Some(icd::CalFreq::FclkBy20),
        2 => Some(icd::CalFreq::DoNotUse),
        3 => Some(icd::CalFreq::DC),
        _ => None,
    }
}

fn comp_thresh_to_raw(value: icd::CompThreshPos) -> u32 {
    let raw: u8 = value.into();
    raw as u32
}

fn comp_thresh_from_raw(raw: u32) -> Option<icd::CompThreshPos> {
    match raw {
        0 => Some(icd::CompThreshPos::_95),
        1 => Some(icd::CompThreshPos::_92_5),
        2 => Some(icd::CompThreshPos::_90),
        3 => Some(icd::CompThreshPos::_87_5),
        4 => Some(icd::CompThreshPos::_85),
        5 => Some(icd::CompThreshPos::_80),
        6 => Some(icd::CompThreshPos::_75),
        7 => Some(icd::CompThreshPos::_70),
        _ => None,
    }
}

fn lead_off_current_to_raw(value: icd::ILeadOff) -> u32 {
    let raw: u8 = value.into();
    raw as u32
}

fn lead_off_current_from_raw(raw: u32) -> Option<icd::ILeadOff> {
    match raw {
        0 => Some(icd::ILeadOff::_6nA),
        1 => Some(icd::ILeadOff::_24nA),
        2 => Some(icd::ILeadOff::_6uA),
        3 => Some(icd::ILeadOff::_24uA),
        _ => None,
    }
}

fn lead_off_freq_to_raw(value: icd::FLeadOff) -> u32 {
    let raw: u8 = value.into();
    raw as u32
}

fn lead_off_freq_from_raw(raw: u32) -> Option<icd::FLeadOff> {
    match raw {
        0 => Some(icd::FLeadOff::Dc),
        1 => Some(icd::FLeadOff::Ac7_8),
        2 => Some(icd::FLeadOff::Ac31_2),
        3 => Some(icd::FLeadOff::AcFdrBy4),
        _ => None,
    }
}

fn profile_command_from_ffi(command: DcMiniProfileCommand) -> ProfileCommand {
    match command {
        DcMiniProfileCommand::Reset => ProfileCommand::Reset,
        DcMiniProfileCommand::Next => ProfileCommand::Next,
        DcMiniProfileCommand::Previous => ProfileCommand::Previous,
    }
}

fn dfu_progress_to_ffi(progress: &DfuProgress) -> DcMiniDfuProgress {
    let state = match progress.state {
        DfuProgressState::Idle => 0,
        DfuProgressState::Receiving => 1,
        DfuProgressState::Complete => 2,
        DfuProgressState::Error => 3,
    };
    DcMiniDfuProgress {
        state,
        offset: progress.offset,
        total_size: progress.total_size,
    }
}

fn ads_config_to_ffi(config: &AdsConfig) -> DcMiniAdsConfig {
    DcMiniAdsConfig {
        sample_rate_hz: sample_rate_to_hz(config.sample_rate),
        channel_count: config.channels.len() as u32,
        daisy_enabled: u8::from(config.daisy_en),
        clk_enabled: u8::from(config.clk_en),
        internal_calibration_enabled: u8::from(config.internal_calibration),
        calibration_amplitude_enabled: u8::from(config.calibration_amplitude),
        calibration_frequency: cal_freq_to_raw(config.calibration_frequency),
        pd_refbuf: u8::from(config.pd_refbuf),
        bias_meas_enabled: u8::from(config.bias_meas),
        biasref_int_enabled: u8::from(config.biasref_int),
        pd_bias: u8::from(config.pd_bias),
        bias_loff_sens_enabled: u8::from(config.bias_loff_sens),
        bias_stat_enabled: u8::from(config.bias_stat),
        comparator_threshold_pos: comp_thresh_to_raw(
            config.comparator_threshold_pos,
        ),
        lead_off_current: lead_off_current_to_raw(config.lead_off_current),
        lead_off_frequency: lead_off_freq_to_raw(config.lead_off_frequency),
        gpioc0: u8::from(config.gpioc[0]),
        gpioc1: u8::from(config.gpioc[1]),
        gpioc2: u8::from(config.gpioc[2]),
        gpioc3: u8::from(config.gpioc[3]),
        srb1_enabled: u8::from(config.srb1),
        single_shot_enabled: u8::from(config.single_shot),
        pd_loff_comp: u8::from(config.pd_loff_comp),
    }
}

fn apply_ffi_to_ads_config(
    config: &mut AdsConfig,
    ffi: DcMiniAdsConfig,
) -> Result<(), String> {
    let Some(sample_rate) = sample_rate_from_hz(ffi.sample_rate_hz) else {
        return Err(format!(
            "unsupported ADS sample rate: {}",
            ffi.sample_rate_hz
        ));
    };

    if ffi.channel_count > DCMINI_MAX_ADS_CHANNELS {
        return Err(format!(
            "channel_count {} exceeded max {}",
            ffi.channel_count, DCMINI_MAX_ADS_CHANNELS
        ));
    }

    config.sample_rate = sample_rate;
    config.daisy_en = ffi.daisy_enabled != 0;
    config.clk_en = ffi.clk_enabled != 0;
    config.internal_calibration = ffi.internal_calibration_enabled != 0;
    config.calibration_amplitude = ffi.calibration_amplitude_enabled != 0;
    config.calibration_frequency =
        cal_freq_from_raw(ffi.calibration_frequency).ok_or_else(|| {
            format!(
                "invalid calibration_frequency: {}",
                ffi.calibration_frequency
            )
        })?;
    config.pd_refbuf = ffi.pd_refbuf != 0;
    config.bias_meas = ffi.bias_meas_enabled != 0;
    config.biasref_int = ffi.biasref_int_enabled != 0;
    config.pd_bias = ffi.pd_bias != 0;
    config.bias_loff_sens = ffi.bias_loff_sens_enabled != 0;
    config.bias_stat = ffi.bias_stat_enabled != 0;
    config.comparator_threshold_pos = comp_thresh_from_raw(
        ffi.comparator_threshold_pos,
    )
    .ok_or_else(|| {
        format!(
            "invalid comparator_threshold_pos: {}",
            ffi.comparator_threshold_pos
        )
    })?;
    config.lead_off_current = lead_off_current_from_raw(ffi.lead_off_current)
        .ok_or_else(|| {
            format!("invalid lead_off_current: {}", ffi.lead_off_current)
        })?;
    config.lead_off_frequency = lead_off_freq_from_raw(ffi.lead_off_frequency)
        .ok_or_else(|| {
            format!("invalid lead_off_frequency: {}", ffi.lead_off_frequency)
        })?;
    config.gpioc =
        [ffi.gpioc0 != 0, ffi.gpioc1 != 0, ffi.gpioc2 != 0, ffi.gpioc3 != 0];
    config.srb1 = ffi.srb1_enabled != 0;
    config.single_shot = ffi.single_shot_enabled != 0;
    config.pd_loff_comp = ffi.pd_loff_comp != 0;

    let desired = ffi.channel_count as usize;
    if config.channels.len() > desired {
        config.channels.truncate(desired);
    } else {
        while config.channels.len() < desired {
            let _ = config.channels.push(default_channel_config());
        }
    }

    Ok(())
}

fn channel_config_to_ffi(
    config: &icd::ChannelConfig,
) -> DcMiniAdsChannelConfig {
    DcMiniAdsChannelConfig {
        gain: gain_to_raw(config.gain),
        mux: mux_to_raw(config.mux),
        power_down: u8::from(config.power_down),
        srb2_enabled: u8::from(config.srb2),
        bias_sensp_enabled: u8::from(config.bias_sensp),
        bias_sensn_enabled: u8::from(config.bias_sensn),
        lead_off_sensp_enabled: u8::from(config.lead_off_sensp),
        lead_off_sensn_enabled: u8::from(config.lead_off_sensn),
        lead_off_flip_enabled: u8::from(config.lead_off_flip),
        reserved0: 0,
    }
}

fn channel_config_from_ffi(
    config: DcMiniAdsChannelConfig,
) -> Option<icd::ChannelConfig> {
    Some(icd::ChannelConfig {
        power_down: config.power_down != 0,
        gain: gain_from_raw(config.gain)?,
        srb2: config.srb2_enabled != 0,
        mux: mux_from_raw(config.mux)?,
        bias_sensp: config.bias_sensp_enabled != 0,
        bias_sensn: config.bias_sensn_enabled != 0,
        lead_off_sensp: config.lead_off_sensp_enabled != 0,
        lead_off_sensn: config.lead_off_sensn_enabled != 0,
        lead_off_flip: config.lead_off_flip_enabled != 0,
    })
}

fn mic_config_to_ffi(config: &MicConfig) -> DcMiniMicConfig {
    DcMiniMicConfig {
        gain_db: config.gain_db as i32,
        sample_rate_hz: mic_sample_rate_to_hz(config.sample_rate),
    }
}

fn apply_ffi_to_mic_config(
    config: &mut MicConfig,
    ffi: DcMiniMicConfig,
) -> Result<(), String> {
    let Some(sample_rate) = mic_sample_rate_from_hz(ffi.sample_rate_hz) else {
        return Err(format!(
            "unsupported MIC sample rate: {}",
            ffi.sample_rate_hz
        ));
    };

    config.gain_db = ffi.gain_db as i8;
    config.sample_rate = sample_rate;
    Ok(())
}

fn cvep_config_to_ffi(config: &CvepConfig) -> DcMiniCvepConfig {
    DcMiniCvepConfig {
        model_enabled: u8::from(config.model_enabled),
        channels: config.channels as u32,
        classes: config.classes as u32,
        window_samples: config.window_samples as u32,
        inference_stride_samples: config.inference_stride_samples as u32,
        has_score_threshold: u8::from(config.score_threshold.is_some()),
        score_threshold: config.score_threshold.unwrap_or_default(),
        has_margin_threshold: u8::from(config.margin_threshold.is_some()),
        margin_threshold: config.margin_threshold.unwrap_or_default(),
    }
}

fn cvep_decision_to_ffi(decision: &CvepDecision) -> DcMiniCvepDecision {
    DcMiniCvepDecision {
        timestamp_us: decision.ts,
        class_index: decision.class_index as u32,
        raw_score: decision.raw_score,
        normalized_score: decision.normalized_score,
        margin: decision.margin,
    }
}

fn clear_shared_queues(shared: &SharedState) {
    shared
        .ads_queue
        .lock()
        .expect("dc-mini-host-unity ads queue poisoned")
        .clear();
    shared
        .mic_queue
        .lock()
        .expect("dc-mini-host-unity mic queue poisoned")
        .clear();
    shared
        .cvep_queue
        .lock()
        .expect("dc-mini-host-unity cvep queue poisoned")
        .clear();
    let mut stats =
        shared.stats.lock().expect("dc-mini-host-unity stream stats poisoned");
    stats.ads_queue_len = 0;
    stats.mic_queue_len = 0;
    stats.cvep_queue_len = 0;
}

fn push_ads_frame(shared: &SharedState, frame: AdsDataFrame) {
    let sample_count = frame.samples.len();
    let channel_count = frame
        .samples
        .iter()
        .map(|sample| sample.data.len())
        .max()
        .unwrap_or(0);

    let mut flattened =
        Vec::with_capacity(sample_count.saturating_mul(channel_count));
    let mut sample_aux = Vec::with_capacity(sample_count);
    let mut flags = 0u32;
    for sample in &frame.samples {
        let mut sample_flags = 0u32;
        if sample.accel_x.is_some()
            || sample.accel_y.is_some()
            || sample.accel_z.is_some()
        {
            flags |= 1;
        }
        if sample.gyro_x.is_some()
            || sample.gyro_y.is_some()
            || sample.gyro_z.is_some()
        {
            flags |= 2;
        }
        if sample.accel_x.is_some() {
            sample_flags |= DCMINI_ADS_AUX_ACCEL_X_PRESENT;
        }
        if sample.accel_y.is_some() {
            sample_flags |= DCMINI_ADS_AUX_ACCEL_Y_PRESENT;
        }
        if sample.accel_z.is_some() {
            sample_flags |= DCMINI_ADS_AUX_ACCEL_Z_PRESENT;
        }
        if sample.gyro_x.is_some() {
            sample_flags |= DCMINI_ADS_AUX_GYRO_X_PRESENT;
        }
        if sample.gyro_y.is_some() {
            sample_flags |= DCMINI_ADS_AUX_GYRO_Y_PRESENT;
        }
        if sample.gyro_z.is_some() {
            sample_flags |= DCMINI_ADS_AUX_GYRO_Z_PRESENT;
        }

        for idx in 0..channel_count {
            flattened.push(sample.data.get(idx).copied().unwrap_or_default());
        }

        sample_aux.push(DcMiniAdsSampleAux {
            lead_off_positive: sample.lead_off_positive,
            lead_off_negative: sample.lead_off_negative,
            gpio: sample.gpio,
            accel_x: sample.accel_x.unwrap_or_default(),
            accel_y: sample.accel_y.unwrap_or_default(),
            accel_z: sample.accel_z.unwrap_or_default(),
            gyro_x: sample.gyro_x.unwrap_or_default(),
            gyro_y: sample.gyro_y.unwrap_or_default(),
            gyro_z: sample.gyro_z.unwrap_or_default(),
            flags: sample_flags,
        });
    }

    let mut queue = shared
        .ads_queue
        .lock()
        .expect("dc-mini-host-unity ads queue poisoned");
    let mut stats =
        shared.stats.lock().expect("dc-mini-host-unity stream stats poisoned");

    if queue.len() >= ADS_QUEUE_CAPACITY {
        queue.pop_front();
        stats.ads_frames_dropped += 1;
    }

    queue.push_back(OwnedAdsFrame {
        header: DcMiniAdsFrameHeader {
            timestamp_us: frame.ts,
            sample_count: sample_count as u32,
            channel_count: channel_count as u32,
            samples_offset: 0,
            aux_offset: 0,
            flags,
        },
        samples: flattened,
        sample_aux,
    });
    stats.ads_queue_len = queue.len() as u32;
}

fn push_mic_packet(shared: &SharedState, frame: icd::MicDataFrame) {
    let mut queue = shared
        .mic_queue
        .lock()
        .expect("dc-mini-host-unity mic queue poisoned");
    let mut stats =
        shared.stats.lock().expect("dc-mini-host-unity stream stats poisoned");

    if queue.len() >= MIC_QUEUE_CAPACITY {
        queue.pop_front();
        stats.mic_packets_dropped += 1;
    }

    let bytes = frame.adpcm_data;
    queue.push_back(OwnedMicPacket {
        header: DcMiniMicPacketHeader {
            timestamp_us: frame.ts,
            packet_counter: frame.packet_counter,
            sample_rate_hz: frame.sample_rate,
            predictor: frame.predictor,
            step_index: frame.step_index,
            data_offset: 0,
            data_len: bytes.len() as u32,
        },
        bytes,
    });
    stats.mic_queue_len = queue.len() as u32;
}

fn push_cvep_decision(shared: &SharedState, decision: CvepDecision) {
    let mut queue = shared
        .cvep_queue
        .lock()
        .expect("dc-mini-host-unity cvep queue poisoned");
    let mut stats =
        shared.stats.lock().expect("dc-mini-host-unity stream stats poisoned");

    if queue.len() >= CVEP_QUEUE_CAPACITY {
        queue.pop_front();
        stats.cvep_decisions_dropped += 1;
    }

    queue.push_back(OwnedCvepDecision {
        decision: cvep_decision_to_ffi(&decision),
    });
    stats.cvep_queue_len = queue.len() as u32;
}

async fn ads_stream_worker(
    client: Arc<HostClient<WireError>>,
    shared: Arc<SharedState>,
) {
    let mut sub = match client
        .subscribe_multi::<icd::AdsTopic>(SUBSCRIPTION_DEPTH)
        .await
    {
        Ok(sub) => sub,
        Err(_) => {
            set_last_error(
                &shared.last_error,
                "failed to subscribe to ADS topic; transport was already closed",
            );
            shared.connected.store(false, Ordering::Relaxed);
            client.close();
            return;
        }
    };

    loop {
        match sub.recv().await {
            Ok(frame) => push_ads_frame(&shared, frame),
            Err(MultiSubRxError::Lagged(dropped)) => {
                let mut stats = shared
                    .stats
                    .lock()
                    .expect("dc-mini-host-unity stream stats poisoned");
                stats.ads_frames_dropped += dropped;
                map_lag_error("ADS", dropped, &shared.last_error);
            }
            Err(MultiSubRxError::IoClosed) => {
                shared.connected.store(false, Ordering::Relaxed);
                set_last_error(&shared.last_error, "ADS subscription closed");
                break;
            }
        }
    }
}

async fn mic_stream_worker(
    client: Arc<HostClient<WireError>>,
    shared: Arc<SharedState>,
) {
    let mut sub = match client
        .subscribe_multi::<icd::MicTopic>(SUBSCRIPTION_DEPTH)
        .await
    {
        Ok(sub) => sub,
        Err(_) => {
            set_last_error(
                &shared.last_error,
                "failed to subscribe to MIC topic; transport was already closed",
            );
            shared.connected.store(false, Ordering::Relaxed);
            client.close();
            return;
        }
    };

    loop {
        match sub.recv().await {
            Ok(frame) => push_mic_packet(&shared, frame),
            Err(MultiSubRxError::Lagged(dropped)) => {
                let mut stats = shared
                    .stats
                    .lock()
                    .expect("dc-mini-host-unity stream stats poisoned");
                stats.mic_packets_dropped += dropped;
                map_lag_error("MIC", dropped, &shared.last_error);
            }
            Err(MultiSubRxError::IoClosed) => {
                shared.connected.store(false, Ordering::Relaxed);
                set_last_error(&shared.last_error, "MIC subscription closed");
                break;
            }
        }
    }
}

async fn cvep_stream_worker(
    client: Arc<HostClient<WireError>>,
    shared: Arc<SharedState>,
) {
    let mut sub = match client
        .subscribe_multi::<icd::CvepTopic>(SUBSCRIPTION_DEPTH)
        .await
    {
        Ok(sub) => sub,
        Err(_) => {
            set_last_error(
                &shared.last_error,
                "failed to subscribe to CVEP topic; transport was already closed",
            );
            shared.connected.store(false, Ordering::Relaxed);
            client.close();
            return;
        }
    };

    loop {
        match sub.recv().await {
            Ok(decision) => push_cvep_decision(&shared, decision),
            Err(MultiSubRxError::Lagged(dropped)) => {
                let mut stats = shared
                    .stats
                    .lock()
                    .expect("dc-mini-host-unity stream stats poisoned");
                stats.cvep_decisions_dropped += dropped;
                map_lag_error("CVEP", dropped, &shared.last_error);
            }
            Err(MultiSubRxError::IoClosed) => {
                shared.connected.store(false, Ordering::Relaxed);
                set_last_error(&shared.last_error, "CVEP subscription closed");
                break;
            }
        }
    }
}

fn client_from_context(
    ctx: &mut DcMiniContext,
) -> Result<Arc<HostClient<WireError>>, DcMiniStatus> {
    let Some(connection) = ctx.connection.as_ref() else {
        set_last_error(&ctx.last_error, "no active DC Mini connection");
        return Err(DcMiniStatus::NotConnected);
    };

    if connection.client.is_closed()
        || !ctx.shared.connected.load(Ordering::Relaxed)
    {
        set_last_error(&ctx.last_error, "DC Mini connection is closed");
        return Err(DcMiniStatus::NotConnected);
    }

    Ok(connection.client.clone())
}

fn refresh_ads_config(ctx: &mut DcMiniContext) -> Result<(), DcMiniStatus> {
    let client = client_from_context(ctx)?;
    match ctx.runtime.block_on(async move {
        client.send_resp::<AdsGetConfigEndpoint>(&()).await
    }) {
        Ok(config) => {
            ctx.ads_config_cache = config;
            Ok(())
        }
        Err(err) => {
            set_last_error(&ctx.last_error, map_wire_error(err));
            Err(DcMiniStatus::InternalError)
        }
    }
}

fn refresh_mic_config(ctx: &mut DcMiniContext) -> Result<(), DcMiniStatus> {
    let client = client_from_context(ctx)?;
    match ctx.runtime.block_on(async move {
        client.send_resp::<MicGetConfigEndpoint>(&()).await
    }) {
        Ok(config) => {
            ctx.mic_config_cache = config;
            Ok(())
        }
        Err(err) => {
            set_last_error(&ctx.last_error, map_wire_error(err));
            Err(DcMiniStatus::InternalError)
        }
    }
}

fn refresh_device_info(ctx: &mut DcMiniContext) -> Result<(), DcMiniStatus> {
    let client = client_from_context(ctx)?;
    match ctx.runtime.block_on(async move {
        client.send_resp::<DeviceInfoGetEndpoint>(&()).await
    }) {
        Ok(device_info) => {
            ctx.device_info_cache = Some(device_info);
            Ok(())
        }
        Err(err) => {
            set_last_error(&ctx.last_error, map_wire_error(err));
            Err(DcMiniStatus::InternalError)
        }
    }
}

fn refresh_session_id(ctx: &mut DcMiniContext) -> Result<(), DcMiniStatus> {
    let client = client_from_context(ctx)?;
    match ctx.runtime.block_on(async move {
        client.send_resp::<SessionGetIdEndpoint>(&()).await
    }) {
        Ok(session_id) => {
            ctx.session_id_cache = session_id.0.as_str().to_owned();
            Ok(())
        }
        Err(err) => {
            set_last_error(&ctx.last_error, map_wire_error(err));
            Err(DcMiniStatus::InternalError)
        }
    }
}

fn refresh_profile(ctx: &mut DcMiniContext) -> Result<(), DcMiniStatus> {
    let client = client_from_context(ctx)?;
    match ctx.runtime.block_on(async move {
        client.send_resp::<ProfileGetEndpoint>(&()).await
    }) {
        Ok(profile) => {
            ctx.profile_cache = profile;
            Ok(())
        }
        Err(err) => {
            set_last_error(&ctx.last_error, map_wire_error(err));
            Err(DcMiniStatus::InternalError)
        }
    }
}

fn refresh_session_status(
    ctx: &mut DcMiniContext,
) -> Result<(), DcMiniStatus> {
    let client = client_from_context(ctx)?;
    match ctx.runtime.block_on(async move {
        client.send_resp::<SessionGetStatusEndpoint>(&()).await
    }) {
        Ok(active) => {
            ctx.session_active_cache = active;
            Ok(())
        }
        Err(err) => {
            set_last_error(&ctx.last_error, map_wire_error(err));
            Err(DcMiniStatus::InternalError)
        }
    }
}

fn refresh_battery_percent(
    ctx: &mut DcMiniContext,
) -> Result<(), DcMiniStatus> {
    let client = client_from_context(ctx)?;
    match ctx.runtime.block_on(async move {
        client.send_resp::<BatteryGetLevelEndpoint>(&()).await
    }) {
        Ok(level) => {
            ctx.battery_percent_cache = level.0;
            Ok(())
        }
        Err(err) => {
            set_last_error(&ctx.last_error, map_wire_error(err));
            Err(DcMiniStatus::InternalError)
        }
    }
}

fn refresh_dfu_progress(ctx: &mut DcMiniContext) -> Result<(), DcMiniStatus> {
    let client = client_from_context(ctx)?;
    match ctx.runtime.block_on(async move {
        client.send_resp::<DfuStatusEndpoint>(&()).await
    }) {
        Ok(progress) => {
            ctx.dfu_progress_cache = progress;
            Ok(())
        }
        Err(err) => {
            set_last_error(&ctx.last_error, map_wire_error(err));
            Err(DcMiniStatus::InternalError)
        }
    }
}

fn refresh_cvep_config(ctx: &mut DcMiniContext) -> Result<(), DcMiniStatus> {
    let client = client_from_context(ctx)?;
    match ctx.runtime.block_on(async move {
        client.send_resp::<CvepGetConfigEndpoint>(&()).await
    }) {
        Ok(config) => {
            ctx.cvep_config_cache = config;
            Ok(())
        }
        Err(err) => {
            set_last_error(&ctx.last_error, map_wire_error(err));
            Err(DcMiniStatus::InternalError)
        }
    }
}

fn refresh_cvep_status(ctx: &mut DcMiniContext) -> Result<(), DcMiniStatus> {
    let client = client_from_context(ctx)?;
    match ctx.runtime.block_on(async move {
        client.send_resp::<CvepGetStatusEndpoint>(&()).await
    }) {
        Ok(active) => {
            ctx.cvep_active_cache = active;
            Ok(())
        }
        Err(err) => {
            set_last_error(&ctx.last_error, map_wire_error(err));
            Err(DcMiniStatus::InternalError)
        }
    }
}

fn close_connection(ctx: &mut DcMiniContext) {
    if let Some(mut connection) = ctx.connection.take() {
        if let Some(task) = connection.ads_task.take() {
            task.abort();
        }
        if let Some(task) = connection.mic_task.take() {
            task.abort();
        }
        if let Some(task) = connection.cvep_task.take() {
            task.abort();
        }
        connection.client.close();
    }

    ctx.transport = TransportState::None;
    ctx.shared.connected.store(false, Ordering::Relaxed);
    ctx.cvep_active_cache = false;
    clear_shared_queues(&ctx.shared);
}

fn set_out_bool(
    out_value: *mut u8,
    value: bool,
    last_error: &Arc<Mutex<String>>,
) -> Result<(), DcMiniStatus> {
    if out_value.is_null() {
        set_last_error(last_error, "out_value was null");
        return Err(DcMiniStatus::InvalidArgument);
    }

    unsafe {
        *out_value = u8::from(value);
    }
    Ok(())
}

#[cfg(any(target_os = "android", target_os = "linux"))]
fn duplicate_fd(fd: i32) -> Result<std::os::fd::OwnedFd, String> {
    use std::os::fd::{FromRawFd, OwnedFd};

    let dup_fd = unsafe { libc::dup(fd) };
    if dup_fd < 0 {
        return Err(format!(
            "failed to duplicate USB fd {fd}: {}",
            std::io::Error::last_os_error()
        ));
    }

    Ok(unsafe { OwnedFd::from_raw_fd(dup_fd) })
}

#[cfg(any(target_os = "android", target_os = "linux"))]
fn host_client_from_usb_fd(
    fd: i32,
    interface_index: u8,
) -> Result<HostClient<WireError>, String> {
    let owned_fd = duplicate_fd(fd)?;
    let device = Device::from_fd(owned_fd)
        .map_err(|err| format!("failed to wrap USB fd: {err}"))?;
    let interface =
        device.claim_interface(interface_index).map_err(|err| {
            format!("failed to claim USB interface {interface_index}: {err}")
        })?;
    host_client_from_interface(interface)
}

#[cfg(not(any(target_os = "android", target_os = "linux")))]
fn host_client_from_usb_fd(
    fd: i32,
    interface_index: u8,
) -> Result<HostClient<WireError>, String> {
    let _ = (fd, interface_index);
    Err(String::from(
        "opening a DC Mini from a raw USB fd is only supported on android/linux targets",
    ))
}

#[cfg(any(target_os = "android", target_os = "linux"))]
fn host_client_from_interface(
    interface: Interface,
) -> Result<HostClient<WireError>, String> {
    let mut max_packet_size: Option<usize> = None;
    let mut endpoint_in = None;
    let mut endpoint_out = None;

    for alt in interface.descriptors() {
        for endpoint in alt
            .endpoints()
            .filter(|endpoint| endpoint.transfer_type() == EndpointType::Bulk)
        {
            match endpoint.direction() {
                Direction::Out => {
                    max_packet_size = Some(match max_packet_size {
                        Some(existing) => {
                            existing.min(endpoint.max_packet_size())
                        }
                        None => endpoint.max_packet_size(),
                    });
                    endpoint_out = Some(endpoint.address());
                }
                Direction::In => {
                    endpoint_in = Some(endpoint.address());
                }
            }
        }
    }

    let endpoint_out =
        endpoint_out.ok_or("failed to find bulk OUT endpoint")?;
    let endpoint_in = endpoint_in.ok_or("failed to find bulk IN endpoint")?;

    let out_queue = interface.bulk_out_queue(endpoint_out);
    let in_queue = interface.bulk_in_queue(endpoint_in);

    Ok(HostClient::new_with_wire(
        NusbWireTx { queue: out_queue, max_packet_size },
        NusbWireRx { queue: in_queue, consecutive_errors: 0 },
        NusbSpawn,
        VarSeqKind::Seq2,
        ERROR_PATH,
        OUTGOING_DEPTH,
    ))
}

#[no_mangle]
pub extern "C" fn dcmini_get_abi_version() -> u32 {
    DCMINI_ABI_VERSION
}

#[no_mangle]
pub extern "C" fn dcmini_create() -> u64 {
    match new_context() {
        Ok(ctx) => {
            let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
            lock_registry().insert(handle, ctx);
            handle
        }
        Err(err) => {
            set_global_error(err);
            0
        }
    }
}

#[no_mangle]
pub extern "C" fn dcmini_destroy(handle: u64) -> DcMiniStatus {
    let Some(mut ctx) = lock_registry().remove(&handle) else {
        set_global_error(format!("invalid dc-mini handle: {handle}"));
        return DcMiniStatus::InvalidHandle;
    };

    close_connection(&mut ctx);
    DcMiniStatus::Ok
}

#[no_mangle]
pub extern "C" fn dcmini_copy_last_global_error_utf8(
    buffer: *mut u8,
    buffer_len: u32,
    out_len: *mut u32,
) -> DcMiniStatus {
    let message = global_error()
        .lock()
        .expect("dc-mini-host-unity global error poisoned")
        .clone();
    copy_utf8_bytes(&message, buffer, buffer_len, out_len)
}

#[no_mangle]
pub extern "C" fn dcmini_copy_last_error_utf8(
    handle: u64,
    buffer: *mut u8,
    buffer_len: u32,
    out_len: *mut u32,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        let message = ctx
            .last_error
            .lock()
            .expect("dc-mini-host-unity last error poisoned")
            .clone();
        copy_utf8_bytes(&message, buffer, buffer_len, out_len)
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_get_transport_kind(
    handle: u64,
) -> DcMiniTransportKind {
    match with_context_mut(handle, |ctx| match ctx.transport {
        TransportState::None => DcMiniTransportKind::None,
        TransportState::AndroidUsb { .. } => DcMiniTransportKind::AndroidUsb,
    }) {
        Ok(kind) => kind,
        Err(_) => DcMiniTransportKind::None,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_is_connected(handle: u64) -> u8 {
    match with_context_mut(handle, |ctx| {
        let is_connected = ctx
            .connection
            .as_ref()
            .map(|connection| {
                !connection.client.is_closed()
                    && ctx.shared.connected.load(Ordering::Relaxed)
            })
            .unwrap_or(false);
        u8::from(is_connected)
    }) {
        Ok(value) => value,
        Err(_) => 0,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_wait_closed(
    handle: u64,
    timeout_ms: u32,
    out_closed: *mut u8,
) -> DcMiniStatus {
    let (client, last_error) = {
        let registry = lock_registry();
        let Some(ctx) = registry.get(&handle) else {
            set_global_error(format!("invalid dc-mini handle: {handle}"));
            return DcMiniStatus::InvalidHandle;
        };
        (
            ctx.connection
                .as_ref()
                .map(|connection| connection.client.clone()),
            ctx.last_error.clone(),
        )
    };

    let Some(client) = client else {
        return match set_out_bool(out_closed, true, &last_error) {
            Ok(()) => DcMiniStatus::Ok,
            Err(status) => status,
        };
    };

    let started_at = std::time::Instant::now();
    let poll_interval = std::time::Duration::from_millis(10);
    let timeout = (timeout_ms != DCMINI_WAIT_CLOSED_INFINITE_MS)
        .then(|| std::time::Duration::from_millis(timeout_ms as u64));

    let closed = loop {
        if client.is_closed() {
            break true;
        }

        if let Some(timeout) = timeout {
            if started_at.elapsed() >= timeout {
                break false;
            }
        }

        std::thread::sleep(poll_interval);
    };

    if closed {
        let _ = with_context_mut(handle, |ctx| {
            ctx.shared.connected.store(false, Ordering::Relaxed);
        });
    }

    match set_out_bool(out_closed, closed, &last_error) {
        Ok(()) => DcMiniStatus::Ok,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_android_open_usb_fd(
    handle: u64,
    fd: i32,
    interface_index: u8,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        if fd < 0 {
            set_last_error(
                &ctx.last_error,
                format!("invalid Android USB file descriptor: {fd}"),
            );
            return DcMiniStatus::InvalidArgument;
        }

        close_connection(ctx);

        let client = match host_client_from_usb_fd(fd, interface_index) {
            Ok(client) => Arc::new(client),
            Err(err) => {
                set_last_error(&ctx.last_error, err);
                return DcMiniStatus::InternalError;
            }
        };

        ctx.transport =
            TransportState::AndroidUsb { _interface_index: interface_index };
        ctx.connection = Some(LiveConnection {
            client: client.clone(),
            ads_task: None,
            mic_task: None,
            cvep_task: None,
        });
        ctx.shared.connected.store(true, Ordering::Relaxed);
        clear_shared_queues(&ctx.shared);

        let _ = refresh_device_info(ctx);
        let _ = refresh_ads_config(ctx);
        let _ = refresh_mic_config(ctx);
        let _ = refresh_profile(ctx);
        let _ = refresh_session_id(ctx);
        let _ = refresh_session_status(ctx);
        let _ = refresh_cvep_config(ctx);
        let _ = refresh_cvep_status(ctx);
        let _ = refresh_battery_percent(ctx);
        let _ = refresh_dfu_progress(ctx);
        set_last_error(&ctx.last_error, "");

        DcMiniStatus::Ok
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_close(handle: u64) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        close_connection(ctx);
        DcMiniStatus::Ok
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_copy_hardware_revision_utf8(
    handle: u64,
    buffer: *mut u8,
    buffer_len: u32,
    out_len: *mut u32,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        if let Err(status) = refresh_device_info(ctx) {
            return status;
        }
        let value = ctx
            .device_info_cache
            .as_ref()
            .map(|info| info.hardware_revision.as_str())
            .unwrap_or("");
        copy_utf8_bytes(value, buffer, buffer_len, out_len)
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_copy_software_revision_utf8(
    handle: u64,
    buffer: *mut u8,
    buffer_len: u32,
    out_len: *mut u32,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        if let Err(status) = refresh_device_info(ctx) {
            return status;
        }
        let value = ctx
            .device_info_cache
            .as_ref()
            .map(|info| info.software_revision.as_str())
            .unwrap_or("");
        copy_utf8_bytes(value, buffer, buffer_len, out_len)
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_copy_manufacturer_name_utf8(
    handle: u64,
    buffer: *mut u8,
    buffer_len: u32,
    out_len: *mut u32,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        if let Err(status) = refresh_device_info(ctx) {
            return status;
        }
        let value = ctx
            .device_info_cache
            .as_ref()
            .map(|info| info.manufacturer_name.as_str())
            .unwrap_or("");
        copy_utf8_bytes(value, buffer, buffer_len, out_len)
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_copy_session_id_utf8(
    handle: u64,
    buffer: *mut u8,
    buffer_len: u32,
    out_len: *mut u32,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        if let Err(status) = refresh_session_id(ctx) {
            return status;
        }
        copy_utf8_bytes(&ctx.session_id_cache, buffer, buffer_len, out_len)
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_set_session_id_utf8(
    handle: u64,
    session_id_ptr: *const u8,
    session_id_len: u32,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        if session_id_ptr.is_null() {
            set_last_error(&ctx.last_error, "session_id_ptr was null");
            return DcMiniStatus::InvalidArgument;
        }
        if session_id_len as usize > icd::MAX_ID_LEN {
            set_last_error(
                &ctx.last_error,
                format!(
                    "session id length {} exceeded max {}",
                    session_id_len,
                    icd::MAX_ID_LEN
                ),
            );
            return DcMiniStatus::InvalidArgument;
        }

        let session_bytes = unsafe {
            std::slice::from_raw_parts(session_id_ptr, session_id_len as usize)
        };
        let session_str = match std::str::from_utf8(session_bytes) {
            Ok(value) => value,
            Err(err) => {
                set_last_error(
                    &ctx.last_error,
                    format!("session id was not valid UTF-8: {err}"),
                );
                return DcMiniStatus::InvalidArgument;
            }
        };

        let heapless_id =
            match HeaplessString::<{ icd::MAX_ID_LEN }>::from_str(session_str)
            {
                Ok(value) => value,
                Err(_) => {
                    set_last_error(
                        &ctx.last_error,
                        "failed to encode session id",
                    );
                    return DcMiniStatus::InvalidArgument;
                }
            };

        let client = match client_from_context(ctx) {
            Ok(client) => client,
            Err(status) => return status,
        };
        match ctx.runtime.block_on(async move {
            client
                .send_resp::<SessionSetIdEndpoint>(&SessionId(heapless_id))
                .await
        }) {
            Ok(true) => {
                ctx.session_id_cache = session_str.to_owned();
                DcMiniStatus::Ok
            }
            Ok(false) => {
                set_last_error(
                    &ctx.last_error,
                    "device rejected session id update",
                );
                DcMiniStatus::InternalError
            }
            Err(err) => {
                set_last_error(&ctx.last_error, map_wire_error(err));
                DcMiniStatus::InternalError
            }
        }
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_get_session_active(
    handle: u64,
    out_active: *mut u8,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        if let Err(status) = refresh_session_status(ctx) {
            return status;
        }
        match set_out_bool(
            out_active,
            ctx.session_active_cache,
            &ctx.last_error,
        ) {
            Ok(()) => DcMiniStatus::Ok,
            Err(status) => status,
        }
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_start_session(
    handle: u64,
    out_success: *mut u8,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        let client = match client_from_context(ctx) {
            Ok(client) => client,
            Err(status) => return status,
        };
        match ctx.runtime.block_on(async move {
            client.send_resp::<SessionStartEndpoint>(&()).await
        }) {
            Ok(success) => {
                ctx.session_active_cache = success;
                match set_out_bool(out_success, success, &ctx.last_error) {
                    Ok(()) => DcMiniStatus::Ok,
                    Err(status) => status,
                }
            }
            Err(err) => {
                set_last_error(&ctx.last_error, map_wire_error(err));
                DcMiniStatus::InternalError
            }
        }
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_stop_session(
    handle: u64,
    out_success: *mut u8,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        let client = match client_from_context(ctx) {
            Ok(client) => client,
            Err(status) => return status,
        };
        match ctx.runtime.block_on(async move {
            client.send_resp::<SessionStopEndpoint>(&()).await
        }) {
            Ok(success) => {
                ctx.session_active_cache =
                    !success && ctx.session_active_cache;
                if success {
                    ctx.session_active_cache = false;
                }
                match set_out_bool(out_success, success, &ctx.last_error) {
                    Ok(()) => DcMiniStatus::Ok,
                    Err(status) => status,
                }
            }
            Err(err) => {
                set_last_error(&ctx.last_error, map_wire_error(err));
                DcMiniStatus::InternalError
            }
        }
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_get_profile(
    handle: u64,
    out_profile: *mut u8,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        if out_profile.is_null() {
            set_last_error(&ctx.last_error, "out_profile was null");
            return DcMiniStatus::InvalidArgument;
        }
        if let Err(status) = refresh_profile(ctx) {
            return status;
        }
        unsafe {
            *out_profile = ctx.profile_cache;
        }
        DcMiniStatus::Ok
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_set_profile(
    handle: u64,
    profile: u8,
    out_success: *mut u8,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        let client = match client_from_context(ctx) {
            Ok(client) => client,
            Err(status) => return status,
        };
        match ctx.runtime.block_on(async move {
            client.send_resp::<ProfileSetEndpoint>(&profile).await
        }) {
            Ok(success) => {
                if success {
                    ctx.profile_cache = profile;
                }
                match set_out_bool(out_success, success, &ctx.last_error) {
                    Ok(()) => DcMiniStatus::Ok,
                    Err(status) => status,
                }
            }
            Err(err) => {
                set_last_error(&ctx.last_error, map_wire_error(err));
                DcMiniStatus::InternalError
            }
        }
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_send_profile_command(
    handle: u64,
    command: DcMiniProfileCommand,
    out_success: *mut u8,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        let client = match client_from_context(ctx) {
            Ok(client) => client,
            Err(status) => return status,
        };
        let request = profile_command_from_ffi(command);
        match ctx.runtime.block_on(async move {
            client.send_resp::<ProfileCommandEndpoint>(&request).await
        }) {
            Ok(success) => {
                if success {
                    let _ = refresh_profile(ctx);
                }
                match set_out_bool(out_success, success, &ctx.last_error) {
                    Ok(()) => DcMiniStatus::Ok,
                    Err(status) => status,
                }
            }
            Err(err) => {
                set_last_error(&ctx.last_error, map_wire_error(err));
                DcMiniStatus::InternalError
            }
        }
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_get_battery_percent(
    handle: u64,
    out_percent: *mut u8,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        if out_percent.is_null() {
            set_last_error(&ctx.last_error, "out_percent was null");
            return DcMiniStatus::InvalidArgument;
        }
        if let Err(status) = refresh_battery_percent(ctx) {
            return status;
        }

        unsafe {
            *out_percent = ctx.battery_percent_cache;
        }
        DcMiniStatus::Ok
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_get_ads_config(
    handle: u64,
    out_config: *mut DcMiniAdsConfig,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        if out_config.is_null() {
            set_last_error(&ctx.last_error, "out_config was null");
            return DcMiniStatus::InvalidArgument;
        }
        if let Err(status) = refresh_ads_config(ctx) {
            return status;
        }

        unsafe {
            *out_config = ads_config_to_ffi(&ctx.ads_config_cache);
        }
        DcMiniStatus::Ok
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_set_ads_config(
    handle: u64,
    config: DcMiniAdsConfig,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        if let Err(err) =
            apply_ffi_to_ads_config(&mut ctx.ads_config_cache, config)
        {
            set_last_error(&ctx.last_error, err);
            return DcMiniStatus::InvalidArgument;
        }

        let client = match client_from_context(ctx) {
            Ok(client) => client,
            Err(status) => return status,
        };
        let request = ctx.ads_config_cache.clone();
        match ctx.runtime.block_on(async move {
            client.send_resp::<AdsSetConfigEndpoint>(&request).await
        }) {
            Ok(true) => DcMiniStatus::Ok,
            Ok(false) => {
                set_last_error(
                    &ctx.last_error,
                    "device rejected ADS config update",
                );
                DcMiniStatus::InternalError
            }
            Err(err) => {
                set_last_error(&ctx.last_error, map_wire_error(err));
                DcMiniStatus::InternalError
            }
        }
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_reset_ads_config(
    handle: u64,
    out_success: *mut u8,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        let client = match client_from_context(ctx) {
            Ok(client) => client,
            Err(status) => return status,
        };

        match ctx.runtime.block_on(async move {
            client.send_resp::<AdsResetConfigEndpoint>(&()).await
        }) {
            Ok(success) => {
                if success {
                    let _ = refresh_ads_config(ctx);
                }
                match set_out_bool(out_success, success, &ctx.last_error) {
                    Ok(()) => DcMiniStatus::Ok,
                    Err(status) => status,
                }
            }
            Err(err) => {
                set_last_error(&ctx.last_error, map_wire_error(err));
                DcMiniStatus::InternalError
            }
        }
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_get_ads_channel_config(
    handle: u64,
    channel_index: u32,
    out_config: *mut DcMiniAdsChannelConfig,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        if out_config.is_null() {
            set_last_error(&ctx.last_error, "out_config was null");
            return DcMiniStatus::InvalidArgument;
        }
        if let Err(status) = refresh_ads_config(ctx) {
            return status;
        }

        let Some(channel) =
            ctx.ads_config_cache.channels.get(channel_index as usize)
        else {
            set_last_error(
                &ctx.last_error,
                format!("invalid ADS channel index: {channel_index}"),
            );
            return DcMiniStatus::InvalidArgument;
        };

        unsafe {
            *out_config = channel_config_to_ffi(channel);
        }
        DcMiniStatus::Ok
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_set_ads_channel_config(
    handle: u64,
    channel_index: u32,
    config: DcMiniAdsChannelConfig,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        if let Err(status) = refresh_ads_config(ctx) {
            return status;
        }

        let Some(channel) =
            ctx.ads_config_cache.channels.get_mut(channel_index as usize)
        else {
            set_last_error(
                &ctx.last_error,
                format!("invalid ADS channel index: {channel_index}"),
            );
            return DcMiniStatus::InvalidArgument;
        };
        let Some(updated) = channel_config_from_ffi(config) else {
            set_last_error(
                &ctx.last_error,
                "invalid ADS channel configuration values",
            );
            return DcMiniStatus::InvalidArgument;
        };
        *channel = updated;

        let client = match client_from_context(ctx) {
            Ok(client) => client,
            Err(status) => return status,
        };
        let request = ctx.ads_config_cache.clone();
        match ctx.runtime.block_on(async move {
            client.send_resp::<AdsSetConfigEndpoint>(&request).await
        }) {
            Ok(true) => DcMiniStatus::Ok,
            Ok(false) => {
                set_last_error(
                    &ctx.last_error,
                    "device rejected ADS channel config update",
                );
                DcMiniStatus::InternalError
            }
            Err(err) => {
                set_last_error(&ctx.last_error, map_wire_error(err));
                DcMiniStatus::InternalError
            }
        }
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_get_mic_config(
    handle: u64,
    out_config: *mut DcMiniMicConfig,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        if out_config.is_null() {
            set_last_error(&ctx.last_error, "out_config was null");
            return DcMiniStatus::InvalidArgument;
        }
        if let Err(status) = refresh_mic_config(ctx) {
            return status;
        }

        unsafe {
            *out_config = mic_config_to_ffi(&ctx.mic_config_cache);
        }
        DcMiniStatus::Ok
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_set_mic_config(
    handle: u64,
    config: DcMiniMicConfig,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        if let Err(err) =
            apply_ffi_to_mic_config(&mut ctx.mic_config_cache, config)
        {
            set_last_error(&ctx.last_error, err);
            return DcMiniStatus::InvalidArgument;
        }

        let client = match client_from_context(ctx) {
            Ok(client) => client,
            Err(status) => return status,
        };
        let request = ctx.mic_config_cache.clone();
        match ctx.runtime.block_on(async move {
            client.send_resp::<MicSetConfigEndpoint>(&request).await
        }) {
            Ok(true) => DcMiniStatus::Ok,
            Ok(false) => {
                set_last_error(
                    &ctx.last_error,
                    "device rejected MIC config update",
                );
                DcMiniStatus::InternalError
            }
            Err(err) => {
                set_last_error(&ctx.last_error, map_wire_error(err));
                DcMiniStatus::InternalError
            }
        }
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_start_ads_stream(handle: u64) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        let client = match client_from_context(ctx) {
            Ok(client) => client,
            Err(status) => return status,
        };

        if let Some(connection) = ctx.connection.as_mut() {
            if let Some(task) = connection.ads_task.take() {
                task.abort();
            }
        }

        let shared = ctx.shared.clone();
        let worker_client = client.clone();
        let task = ctx.runtime.spawn(ads_stream_worker(worker_client, shared));

        match ctx.runtime.block_on(async move {
            client.send_resp::<AdsStartEndpoint>(&()).await
        }) {
            Ok(config) => {
                ctx.ads_config_cache = config;
                if let Some(connection) = ctx.connection.as_mut() {
                    connection.ads_task = Some(task);
                }
                DcMiniStatus::Ok
            }
            Err(err) => {
                task.abort();
                set_last_error(&ctx.last_error, map_wire_error(err));
                DcMiniStatus::InternalError
            }
        }
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_stop_ads_stream(handle: u64) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        let client = match client_from_context(ctx) {
            Ok(client) => client,
            Err(status) => return status,
        };

        match ctx.runtime.block_on(async move {
            client.send_resp::<AdsStopEndpoint>(&()).await
        }) {
            Ok(()) => {
                if let Some(connection) = ctx.connection.as_mut() {
                    if let Some(task) = connection.ads_task.take() {
                        task.abort();
                    }
                }
                DcMiniStatus::Ok
            }
            Err(err) => {
                set_last_error(&ctx.last_error, map_wire_error(err));
                DcMiniStatus::InternalError
            }
        }
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_start_mic_stream(handle: u64) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        let client = match client_from_context(ctx) {
            Ok(client) => client,
            Err(status) => return status,
        };

        if let Some(connection) = ctx.connection.as_mut() {
            if let Some(task) = connection.mic_task.take() {
                task.abort();
            }
        }

        let shared = ctx.shared.clone();
        let worker_client = client.clone();
        let task = ctx.runtime.spawn(mic_stream_worker(worker_client, shared));

        match ctx.runtime.block_on(async move {
            client.send_resp::<MicStartEndpoint>(&()).await
        }) {
            Ok(config) => {
                ctx.mic_config_cache = config;
                if let Some(connection) = ctx.connection.as_mut() {
                    connection.mic_task = Some(task);
                }
                DcMiniStatus::Ok
            }
            Err(err) => {
                task.abort();
                set_last_error(&ctx.last_error, map_wire_error(err));
                DcMiniStatus::InternalError
            }
        }
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_stop_mic_stream(handle: u64) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        let client = match client_from_context(ctx) {
            Ok(client) => client,
            Err(status) => return status,
        };

        match ctx.runtime.block_on(async move {
            client.send_resp::<MicStopEndpoint>(&()).await
        }) {
            Ok(()) => {
                if let Some(connection) = ctx.connection.as_mut() {
                    if let Some(task) = connection.mic_task.take() {
                        task.abort();
                    }
                }
                DcMiniStatus::Ok
            }
            Err(err) => {
                set_last_error(&ctx.last_error, map_wire_error(err));
                DcMiniStatus::InternalError
            }
        }
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_get_cvep_config(
    handle: u64,
    out_config: *mut DcMiniCvepConfig,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        if out_config.is_null() {
            set_last_error(&ctx.last_error, "out_config was null");
            return DcMiniStatus::InvalidArgument;
        }
        if let Err(status) = refresh_cvep_config(ctx) {
            return status;
        }

        unsafe {
            *out_config = cvep_config_to_ffi(&ctx.cvep_config_cache);
        }
        DcMiniStatus::Ok
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_get_cvep_active(
    handle: u64,
    out_active: *mut u8,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        if let Err(status) = refresh_cvep_status(ctx) {
            return status;
        }
        match set_out_bool(out_active, ctx.cvep_active_cache, &ctx.last_error)
        {
            Ok(()) => DcMiniStatus::Ok,
            Err(status) => status,
        }
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_start_cvep_stream(handle: u64) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        let client = match client_from_context(ctx) {
            Ok(client) => client,
            Err(status) => return status,
        };

        if let Some(connection) = ctx.connection.as_mut() {
            if let Some(task) = connection.cvep_task.take() {
                task.abort();
            }
        }

        let shared = ctx.shared.clone();
        let worker_client = client.clone();
        let task =
            ctx.runtime.spawn(cvep_stream_worker(worker_client, shared));

        match ctx.runtime.block_on(async move {
            client.send_resp::<CvepStartEndpoint>(&()).await
        }) {
            Ok(config) => {
                ctx.cvep_config_cache = config.clone();
                ctx.cvep_active_cache = config.model_enabled;
                if let Some(connection) = ctx.connection.as_mut() {
                    connection.cvep_task = Some(task);
                }
                DcMiniStatus::Ok
            }
            Err(err) => {
                task.abort();
                set_last_error(&ctx.last_error, map_wire_error(err));
                DcMiniStatus::InternalError
            }
        }
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_stop_cvep_stream(
    handle: u64,
    out_success: *mut u8,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        let client = match client_from_context(ctx) {
            Ok(client) => client,
            Err(status) => return status,
        };

        match ctx.runtime.block_on(async move {
            client.send_resp::<CvepStopEndpoint>(&()).await
        }) {
            Ok(success) => {
                if let Some(connection) = ctx.connection.as_mut() {
                    if let Some(task) = connection.cvep_task.take() {
                        task.abort();
                    }
                }
                if success {
                    ctx.cvep_active_cache = false;
                }
                match set_out_bool(out_success, success, &ctx.last_error) {
                    Ok(()) => DcMiniStatus::Ok,
                    Err(status) => status,
                }
            }
            Err(err) => {
                set_last_error(&ctx.last_error, map_wire_error(err));
                DcMiniStatus::InternalError
            }
        }
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_get_stream_stats(
    handle: u64,
    out_stats: *mut DcMiniStreamStats,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        if out_stats.is_null() {
            set_last_error(&ctx.last_error, "out_stats was null");
            return DcMiniStatus::InvalidArgument;
        }

        let mut stats = *ctx
            .shared
            .stats
            .lock()
            .expect("dc-mini-host-unity stream stats poisoned");
        stats.ads_queue_len = ctx
            .shared
            .ads_queue
            .lock()
            .expect("dc-mini-host-unity ads queue poisoned")
            .len() as u32;
        stats.mic_queue_len = ctx
            .shared
            .mic_queue
            .lock()
            .expect("dc-mini-host-unity mic queue poisoned")
            .len() as u32;
        stats.cvep_queue_len = ctx
            .shared
            .cvep_queue
            .lock()
            .expect("dc-mini-host-unity cvep queue poisoned")
            .len() as u32;

        unsafe {
            *out_stats = stats;
        }
        DcMiniStatus::Ok
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_poll_cvep_decisions(
    handle: u64,
    out_decisions: *mut DcMiniCvepDecision,
    decision_capacity: u32,
    out_decision_count: *mut u32,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        if out_decision_count.is_null() {
            set_last_error(&ctx.last_error, "out_decision_count was null");
            return DcMiniStatus::InvalidArgument;
        }

        unsafe {
            *out_decision_count = 0;
        }

        if decision_capacity > 0 && out_decisions.is_null() {
            set_last_error(&ctx.last_error, "out_decisions was null");
            return DcMiniStatus::InvalidArgument;
        }

        let mut queue = ctx
            .shared
            .cvep_queue
            .lock()
            .expect("dc-mini-host-unity cvep queue poisoned");
        let mut stats = ctx
            .shared
            .stats
            .lock()
            .expect("dc-mini-host-unity stream stats poisoned");

        let mut written = 0usize;
        while written < decision_capacity as usize {
            let Some(decision) = queue.pop_front() else {
                break;
            };
            unsafe {
                *out_decisions.add(written) = decision.decision;
            }
            written += 1;
        }

        stats.cvep_queue_len = queue.len() as u32;
        unsafe {
            *out_decision_count = written as u32;
        }
        DcMiniStatus::Ok
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_poll_ads_frames(
    handle: u64,
    out_headers: *mut DcMiniAdsFrameHeader,
    header_capacity: u32,
    out_samples: *mut i32,
    sample_capacity: u32,
    out_frame_count: *mut u32,
    out_sample_count: *mut u32,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        if out_frame_count.is_null() || out_sample_count.is_null() {
            set_last_error(&ctx.last_error, "out count pointers were null");
            return DcMiniStatus::InvalidArgument;
        }

        unsafe {
            *out_frame_count = 0;
            *out_sample_count = 0;
        }

        if header_capacity > 0 && out_headers.is_null() {
            set_last_error(&ctx.last_error, "out_headers was null");
            return DcMiniStatus::InvalidArgument;
        }
        if sample_capacity > 0 && out_samples.is_null() {
            set_last_error(&ctx.last_error, "out_samples was null");
            return DcMiniStatus::InvalidArgument;
        }

        let mut queue = ctx
            .shared
            .ads_queue
            .lock()
            .expect("dc-mini-host-unity ads queue poisoned");
        let mut stats = ctx
            .shared
            .stats
            .lock()
            .expect("dc-mini-host-unity stream stats poisoned");

        let mut written_frames = 0usize;
        let mut written_samples = 0usize;
        while written_frames < header_capacity as usize {
            let Some(frame) = queue.front() else {
                break;
            };
            let next_sample_total = written_samples + frame.samples.len();
            if next_sample_total > sample_capacity as usize {
                break;
            }

            let mut frame = queue.pop_front().expect("front checked above");
            frame.header.samples_offset = written_samples as u32;
            unsafe {
                *out_headers.add(written_frames) = frame.header;
                std::ptr::copy_nonoverlapping(
                    frame.samples.as_ptr(),
                    out_samples.add(written_samples),
                    frame.samples.len(),
                );
            }
            written_frames += 1;
            written_samples = next_sample_total;
        }

        stats.ads_queue_len = queue.len() as u32;
        unsafe {
            *out_frame_count = written_frames as u32;
            *out_sample_count = written_samples as u32;
        }
        DcMiniStatus::Ok
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_poll_ads_frames_rich(
    handle: u64,
    out_headers: *mut DcMiniAdsFrameHeader,
    header_capacity: u32,
    out_samples: *mut i32,
    sample_capacity: u32,
    out_sample_aux: *mut DcMiniAdsSampleAux,
    aux_capacity: u32,
    out_frame_count: *mut u32,
    out_sample_count: *mut u32,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        if out_frame_count.is_null() || out_sample_count.is_null() {
            set_last_error(&ctx.last_error, "out count pointers were null");
            return DcMiniStatus::InvalidArgument;
        }

        unsafe {
            *out_frame_count = 0;
            *out_sample_count = 0;
        }

        if header_capacity > 0 && out_headers.is_null() {
            set_last_error(&ctx.last_error, "out_headers was null");
            return DcMiniStatus::InvalidArgument;
        }
        if sample_capacity > 0 && out_samples.is_null() {
            set_last_error(&ctx.last_error, "out_samples was null");
            return DcMiniStatus::InvalidArgument;
        }
        if aux_capacity > 0 && out_sample_aux.is_null() {
            set_last_error(&ctx.last_error, "out_sample_aux was null");
            return DcMiniStatus::InvalidArgument;
        }

        let mut queue = ctx
            .shared
            .ads_queue
            .lock()
            .expect("dc-mini-host-unity ads queue poisoned");
        let mut stats = ctx
            .shared
            .stats
            .lock()
            .expect("dc-mini-host-unity stream stats poisoned");

        let mut written_frames = 0usize;
        let mut written_samples = 0usize;
        while written_frames < header_capacity as usize {
            let Some(frame) = queue.front() else {
                break;
            };
            let next_sample_total = written_samples + frame.samples.len();
            let next_aux_total = written_samples + frame.sample_aux.len();
            if next_sample_total > sample_capacity as usize
                || next_aux_total > aux_capacity as usize
            {
                break;
            }

            let mut frame = queue.pop_front().expect("front checked above");
            frame.header.samples_offset = written_samples as u32;
            frame.header.aux_offset = written_samples as u32;
            unsafe {
                *out_headers.add(written_frames) = frame.header;
                std::ptr::copy_nonoverlapping(
                    frame.samples.as_ptr(),
                    out_samples.add(written_samples),
                    frame.samples.len(),
                );
                std::ptr::copy_nonoverlapping(
                    frame.sample_aux.as_ptr(),
                    out_sample_aux.add(written_samples),
                    frame.sample_aux.len(),
                );
            }
            written_frames += 1;
            written_samples = next_sample_total;
        }

        stats.ads_queue_len = queue.len() as u32;
        unsafe {
            *out_frame_count = written_frames as u32;
            *out_sample_count = written_samples as u32;
        }
        DcMiniStatus::Ok
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_poll_mic_packets(
    handle: u64,
    out_headers: *mut DcMiniMicPacketHeader,
    header_capacity: u32,
    out_bytes: *mut u8,
    byte_capacity: u32,
    out_packet_count: *mut u32,
    out_byte_count: *mut u32,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        if out_packet_count.is_null() || out_byte_count.is_null() {
            set_last_error(&ctx.last_error, "out count pointers were null");
            return DcMiniStatus::InvalidArgument;
        }

        unsafe {
            *out_packet_count = 0;
            *out_byte_count = 0;
        }

        if header_capacity > 0 && out_headers.is_null() {
            set_last_error(&ctx.last_error, "out_headers was null");
            return DcMiniStatus::InvalidArgument;
        }
        if byte_capacity > 0 && out_bytes.is_null() {
            set_last_error(&ctx.last_error, "out_bytes was null");
            return DcMiniStatus::InvalidArgument;
        }

        let mut queue = ctx
            .shared
            .mic_queue
            .lock()
            .expect("dc-mini-host-unity mic queue poisoned");
        let mut stats = ctx
            .shared
            .stats
            .lock()
            .expect("dc-mini-host-unity stream stats poisoned");

        let mut written_packets = 0usize;
        let mut written_bytes = 0usize;
        while written_packets < header_capacity as usize {
            let Some(packet) = queue.front() else {
                break;
            };
            let next_byte_total = written_bytes + packet.bytes.len();
            if next_byte_total > byte_capacity as usize {
                break;
            }

            let mut packet = queue.pop_front().expect("front checked above");
            packet.header.data_offset = written_bytes as u32;
            unsafe {
                *out_headers.add(written_packets) = packet.header;
                std::ptr::copy_nonoverlapping(
                    packet.bytes.as_ptr(),
                    out_bytes.add(written_bytes),
                    packet.bytes.len(),
                );
            }
            written_packets += 1;
            written_bytes = next_byte_total;
        }

        stats.mic_queue_len = queue.len() as u32;
        unsafe {
            *out_packet_count = written_packets as u32;
            *out_byte_count = written_bytes as u32;
        }
        DcMiniStatus::Ok
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_debug_enqueue_mock_ads_frame(
    handle: u64,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        let channel_count = ctx.ads_config_cache.channels.len().max(1);
        let sample_count = 4usize;
        let mut samples = Vec::with_capacity(channel_count * sample_count);
        for sample_idx in 0..sample_count {
            for channel_idx in 0..channel_count {
                samples.push((sample_idx * 100 + channel_idx) as i32);
            }
        }

        push_ads_frame(
            &ctx.shared,
            AdsDataFrame {
                ts: 0,
                samples: (0..sample_count)
                    .map(|sample_idx| {
                        let start = sample_idx * channel_count;
                        let end = start + channel_count;
                        dc_mini_icd::AdsSample {
                            lead_off_positive: 0,
                            lead_off_negative: 0,
                            gpio: 0,
                            data: samples[start..end].to_vec(),
                            accel_x: None,
                            accel_y: None,
                            accel_z: None,
                            gyro_x: None,
                            gyro_y: None,
                            gyro_z: None,
                        }
                    })
                    .collect(),
            },
        );
        DcMiniStatus::Ok
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_get_dfu_progress(
    handle: u64,
    out_progress: *mut DcMiniDfuProgress,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        if out_progress.is_null() {
            set_last_error(&ctx.last_error, "out_progress was null");
            return DcMiniStatus::InvalidArgument;
        }
        if let Err(status) = refresh_dfu_progress(ctx) {
            return status;
        }

        unsafe {
            *out_progress = dfu_progress_to_ffi(&ctx.dfu_progress_cache);
        }
        DcMiniStatus::Ok
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_dfu_begin(
    handle: u64,
    firmware_size: u32,
    out_success: *mut u8,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        let client = match client_from_context(ctx) {
            Ok(client) => client,
            Err(status) => return status,
        };

        match ctx.runtime.block_on(async move {
            client
                .send_resp::<DfuBeginEndpoint>(&DfuBegin { firmware_size })
                .await
        }) {
            Ok(result) => {
                if !result.success {
                    set_last_error(&ctx.last_error, result.message.as_str());
                }
                let _ = refresh_dfu_progress(ctx);
                match set_out_bool(
                    out_success,
                    result.success,
                    &ctx.last_error,
                ) {
                    Ok(()) => DcMiniStatus::Ok,
                    Err(status) => status,
                }
            }
            Err(err) => {
                set_last_error(&ctx.last_error, map_wire_error(err));
                DcMiniStatus::InternalError
            }
        }
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_dfu_write(
    handle: u64,
    offset: u32,
    data_ptr: *const u8,
    data_len: u32,
    out_success: *mut u8,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        if data_ptr.is_null() {
            set_last_error(&ctx.last_error, "data_ptr was null");
            return DcMiniStatus::InvalidArgument;
        }
        if data_len > DCMINI_DFU_MAX_WRITE_SIZE {
            set_last_error(
                &ctx.last_error,
                format!(
                    "dfu chunk length {} exceeded max {}",
                    data_len, DCMINI_DFU_MAX_WRITE_SIZE
                ),
            );
            return DcMiniStatus::InvalidArgument;
        }

        let data =
            unsafe { std::slice::from_raw_parts(data_ptr, data_len as usize) };
        let chunk = match HeaplessVec::<u8, 512>::from_slice(data) {
            Ok(data) => DfuWriteChunk { offset, data },
            Err(_) => {
                set_last_error(&ctx.last_error, "failed to encode dfu chunk");
                return DcMiniStatus::InvalidArgument;
            }
        };

        let client = match client_from_context(ctx) {
            Ok(client) => client,
            Err(status) => return status,
        };
        match ctx.runtime.block_on(async move {
            client.send_resp::<DfuWriteEndpoint>(&chunk).await
        }) {
            Ok(result) => {
                if !result.success {
                    set_last_error(&ctx.last_error, result.message.as_str());
                }
                let _ = refresh_dfu_progress(ctx);
                match set_out_bool(
                    out_success,
                    result.success,
                    &ctx.last_error,
                ) {
                    Ok(()) => DcMiniStatus::Ok,
                    Err(status) => status,
                }
            }
            Err(err) => {
                set_last_error(&ctx.last_error, map_wire_error(err));
                DcMiniStatus::InternalError
            }
        }
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_dfu_finish(
    handle: u64,
    out_success: *mut u8,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        let client = match client_from_context(ctx) {
            Ok(client) => client,
            Err(status) => return status,
        };
        match ctx.runtime.block_on(async move {
            client.send_resp::<DfuFinishEndpoint>(&()).await
        }) {
            Ok(result) => {
                if !result.success {
                    set_last_error(&ctx.last_error, result.message.as_str());
                }
                let _ = refresh_dfu_progress(ctx);
                match set_out_bool(
                    out_success,
                    result.success,
                    &ctx.last_error,
                ) {
                    Ok(()) => DcMiniStatus::Ok,
                    Err(status) => status,
                }
            }
            Err(err) => {
                set_last_error(&ctx.last_error, map_wire_error(err));
                DcMiniStatus::InternalError
            }
        }
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_dfu_abort(
    handle: u64,
    out_success: *mut u8,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        let client = match client_from_context(ctx) {
            Ok(client) => client,
            Err(status) => return status,
        };
        match ctx.runtime.block_on(async move {
            client.send_resp::<DfuAbortEndpoint>(&()).await
        }) {
            Ok(result) => {
                if !result.success {
                    set_last_error(&ctx.last_error, result.message.as_str());
                }
                let _ = refresh_dfu_progress(ctx);
                match set_out_bool(
                    out_success,
                    result.success,
                    &ctx.last_error,
                ) {
                    Ok(()) => DcMiniStatus::Ok,
                    Err(status) => status,
                }
            }
            Err(err) => {
                set_last_error(&ctx.last_error, map_wire_error(err));
                DcMiniStatus::InternalError
            }
        }
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[no_mangle]
pub extern "C" fn dcmini_debug_enqueue_mock_mic_packet(
    handle: u64,
) -> DcMiniStatus {
    match with_context_mut(handle, |ctx| {
        push_mic_packet(
            &ctx.shared,
            icd::MicDataFrame {
                ts: 0,
                packet_counter: 0,
                sample_rate: mic_sample_rate_to_hz(
                    ctx.mic_config_cache.sample_rate,
                ),
                predictor: 0,
                step_index: 0,
                adpcm_data: vec![1, 2, 3, 4, 5, 6],
            },
        );
        DcMiniStatus::Ok
    }) {
        Ok(status) => status,
        Err(status) => status,
    }
}

#[cfg(any(target_os = "android", target_os = "linux"))]
struct NusbSpawn;

#[cfg(any(target_os = "android", target_os = "linux"))]
impl WireSpawn for NusbSpawn {
    fn spawn(&mut self, fut: impl Future<Output = ()> + Send + 'static) {
        core::mem::drop(tokio::task::spawn(fut));
    }
}

#[cfg(any(target_os = "android", target_os = "linux"))]
struct NusbWireTx {
    queue: Queue<Vec<u8>>,
    max_packet_size: Option<usize>,
}

#[derive(Debug, Error)]
#[cfg(any(target_os = "android", target_os = "linux"))]
enum NusbWireTxError {
    #[error("USB transfer failed while sending")]
    Transfer(#[from] TransferError),
}

#[cfg(any(target_os = "android", target_os = "linux"))]
impl WireTx for NusbWireTx {
    type Error = NusbWireTxError;

    fn send(
        &mut self,
        data: Vec<u8>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.send_inner(data)
    }
}

#[cfg(any(target_os = "android", target_os = "linux"))]
impl NusbWireTx {
    async fn send_inner(
        &mut self,
        data: Vec<u8>,
    ) -> Result<(), NusbWireTxError> {
        let needs_zero_length_packet =
            if let Some(max_packet_size) = self.max_packet_size {
                data.len() % max_packet_size == 0
            } else {
                true
            };

        self.queue.submit(data);
        if needs_zero_length_packet {
            self.queue.submit(vec![]);
        }

        let result = self.queue.next_complete().await;
        if let Err(err) = result.status {
            return Err(err.into());
        }

        if needs_zero_length_packet {
            let result = self.queue.next_complete().await;
            if let Err(err) = result.status {
                return Err(err.into());
            }
        }

        Ok(())
    }
}

#[cfg(any(target_os = "android", target_os = "linux"))]
struct NusbWireRx {
    queue: Queue<RequestBuffer>,
    consecutive_errors: usize,
}

#[derive(Debug, Error)]
#[cfg(any(target_os = "android", target_os = "linux"))]
enum NusbWireRxError {
    #[error("USB transfer failed while receiving")]
    Transfer(#[from] TransferError),
    #[error("USB endpoint recovery failed")]
    Nusb(#[from] nusb::Error),
}

#[cfg(any(target_os = "android", target_os = "linux"))]
impl WireRx for NusbWireRx {
    type Error = NusbWireRxError;

    fn receive(
        &mut self,
    ) -> impl Future<Output = Result<Vec<u8>, Self::Error>> + Send {
        self.receive_inner()
    }
}

#[cfg(any(target_os = "android", target_os = "linux"))]
impl NusbWireRx {
    async fn receive_inner(&mut self) -> Result<Vec<u8>, NusbWireRxError> {
        loop {
            let pending = self.queue.pending();
            for _ in 0..IN_FLIGHT_REQS.saturating_sub(pending) {
                self.queue.submit(RequestBuffer::new(MAX_TRANSFER_SIZE));
            }

            let result = self.queue.next_complete().await;
            if let Err(err) = result.status {
                self.consecutive_errors += 1;

                let recoverable = match err {
                    TransferError::Stall | TransferError::Unknown => {
                        self.consecutive_errors <= MAX_STALL_RETRIES
                    }
                    TransferError::Cancelled
                    | TransferError::Disconnected
                    | TransferError::Fault => false,
                };

                if !recoverable {
                    return Err(err.into());
                }

                self.queue.cancel_all();
                for _ in 0..IN_FLIGHT_REQS.saturating_sub(1) {
                    let _ = self.queue.next_complete().await;
                }

                if let Err(clear_err) = self.queue.clear_halt() {
                    return Err(clear_err.into());
                }

                continue;
            }

            self.consecutive_errors = 0;
            return Ok(result.data);
        }
    }
}
