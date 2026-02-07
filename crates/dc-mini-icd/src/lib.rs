#![cfg_attr(not(feature = "use-std"), no_std)]
extern crate alloc;

use heapless::String;
use postcard_rpc::{endpoints, topics, TopicDirection};
use postcard_schema::Schema;
use serde::{Deserialize, Serialize};

macro_rules! define_config_enum {
    ($wrapper:ident, $external:path, { $($variant:ident),* $(,)? }) => {
        #[derive(Debug, PartialEq, Serialize, Deserialize, Schema, Clone, Copy)]
        #[cfg_attr(feature = "defmt", derive(defmt::Format))]
        pub enum $wrapper {
            $($variant),*
        }

        impl From<u8> for $wrapper {
            fn from(value: u8) -> Self {
                match value {
                    $(x if x == Self::$variant as u8 => Self::$variant),*,
                    _ => panic!("Invalid value for enum conversion"),
                }
            }
        }

        impl Into<u8> for $wrapper {
            fn into(self) -> u8 {
                self as u8
            }
        }

        impl From<$external> for $wrapper {
            fn from(value: $external) -> Self {
                match value {
                    $(<$external>::$variant => Self::$variant),*
                }
            }
        }

        impl From<$wrapper> for $external {
            fn from(value: $wrapper) -> Self {
                match value {
                    $(<$wrapper>::$variant => <$external>::$variant),*
                }
            }
        }
    };
}

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/ads.rs"));
}

pub mod mic_proto {
    include!(concat!(env!("OUT_DIR"), "/mic.rs"));
}

mod ads;
pub use ads::*;

mod imu;
pub use imu::*;

mod mic;
pub use mic::*;

mod apds;
pub use apds::*;

// Constants
pub const MAX_PROFILES: u8 = 16;
pub const MAX_ID_LEN: usize = 4;

// Battery Service types
#[derive(Debug, PartialEq, Serialize, Deserialize, Schema, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct BatteryLevel(pub u8);

// Device Information types
#[derive(Debug, PartialEq, Serialize, Deserialize, Schema, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct DeviceInfo {
    pub hardware_revision: heapless::String<32>,
    pub software_revision: heapless::String<32>,
    pub manufacturer_name: heapless::String<32>,
}

// Profile Service types
#[derive(Debug, PartialEq, Serialize, Deserialize, Schema, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ProfileCommand {
    Reset,
    Next,
    Previous,
}

impl TryFrom<u8> for ProfileCommand {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(ProfileCommand::Reset),
            1 => Ok(ProfileCommand::Next),
            2 => Ok(ProfileCommand::Previous),
            _ => Err("Invalid profile command"),
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct SessionId(pub String<MAX_ID_LEN>);

// DFU types
/// Begin a DFU transfer with the total firmware size.
#[derive(Debug, PartialEq, Serialize, Deserialize, Schema, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct DfuBegin {
    pub firmware_size: u32,
}

/// Write a chunk of firmware data at the given offset.
#[derive(Debug, PartialEq, Serialize, Deserialize, Schema, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct DfuWriteChunk {
    pub offset: u32,
    pub data: heapless::Vec<u8, 512>,
}

/// Result of a DFU operation.
#[derive(Debug, PartialEq, Serialize, Deserialize, Schema, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct DfuResult {
    pub success: bool,
    pub message: String<64>,
}

/// Current DFU progress state.
#[derive(Debug, PartialEq, Serialize, Deserialize, Schema, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum DfuProgressState {
    Idle,
    Receiving,
    Complete,
    Error,
}

/// DFU progress report.
#[derive(Debug, PartialEq, Serialize, Deserialize, Schema, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct DfuProgress {
    pub state: DfuProgressState,
    pub offset: u32,
    pub total_size: u32,
}

endpoints! {
    list = ENDPOINT_LIST;
    omit_std = true;
    | EndpointTy                | RequestTy         | ResponseTy            | Path              |
    | ----------                | ---------         | ----------            | ----              |
    // ADS endpoints
    | AdsStartEndpoint          | ()                | AdsConfig             | "ads/start"       |
    | AdsStopEndpoint           | ()                | ()                    | "ads/stop"        |
    | AdsResetConfigEndpoint    | ()                | bool                  | "ads/reset"       |
    | AdsGetConfigEndpoint      | ()                | AdsConfig             | "ads/get_config"  |
    | AdsSetConfigEndpoint      | AdsConfig         | bool                  | "ads/set_config"  |
    // Battery endpoint (read-only)
    | BatteryGetLevelEndpoint   | ()                | BatteryLevel          | "battery/level"   |
    // Device Info endpoint (read-only)
    | DeviceInfoGetEndpoint     | ()                | DeviceInfo            | "device/info"     |
    // Profile endpoints
    | ProfileGetEndpoint        | ()                | u8                    | "profile/get"     |
    | ProfileSetEndpoint        | u8                | bool                  | "profile/set"     |
    | ProfileCommandEndpoint    | ProfileCommand    | bool                  | "profile/command" |
    // Mic endpoints
    | MicStartEndpoint          | ()                | MicConfig             | "mic/start"       |
    | MicStopEndpoint           | ()                | ()                    | "mic/stop"        |
    | MicGetConfigEndpoint      | ()                | MicConfig             | "mic/get_config"  |
    | MicSetConfigEndpoint      | MicConfig         | bool                  | "mic/set_config"  |
    // Session endpoints
    | SessionGetStatusEndpoint  | ()                | bool                  | "session/status"  |
    | SessionGetIdEndpoint      | ()                | SessionId             | "session/id"      |
    | SessionSetIdEndpoint      | SessionId         | bool                  | "session/set_id"  |
    | SessionStartEndpoint      | ()                | bool                  | "session/start"   |
    | SessionStopEndpoint       | ()                | bool                  | "session/stop"    |
    // DFU endpoints
    | DfuBeginEndpoint          | DfuBegin          | DfuResult             | "dfu/begin"       |
    | DfuWriteEndpoint          | DfuWriteChunk     | DfuResult             | "dfu/write"       |
    | DfuFinishEndpoint         | ()                | DfuResult             | "dfu/finish"      |
    | DfuAbortEndpoint          | ()                | DfuResult             | "dfu/abort"       |
    | DfuStatusEndpoint         | ()                | DfuProgress           | "dfu/status"      |
}

topics! {
    list = TOPICS_IN_LIST;
    direction = TopicDirection::ToServer;
    | TopicTy                   | MessageTy     | Path              |
    | -------                   | ---------     | ----              |
}

topics! {
    list = TOPICS_OUT_LIST;
    direction = TopicDirection::ToClient;
    | TopicTy                   | MessageTy     | Path              | Cfg                           |
    | -------                   | ---------     | ----              | ---                           |
    | AdsTopic                  | AdsDataFrame  | "ads/data"        |                               |
    | MicTopic                  | MicDataFrame  | "mic/data"        |                               |
}
