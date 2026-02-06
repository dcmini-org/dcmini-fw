use super::{Setting, StorageKey};
use dc_mini_icd::{AdsConfig, ImuConfig, SessionId};
use postcard_schema::Schema;
use sequential_storage::map::SerializationError;
use serde::{Deserialize, Serialize};

/// The data types stored in the system, corresponding to `StorageKey`.
#[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum StorageData {
    CurrentProfile(u8),
    SessionId(SessionId),
    AdsConfig(AdsConfig),
    ImuConfig(ImuConfig),
    HapticConfig(HapticConfig),
    NeopixelConfig(NeopixelConfig),
    AmbientLightConfig(AmbientLightConfig),
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct HapticConfig {
    pub pattern: u32,
    pub intensity: u8,
    pub duration: u16,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct NeopixelConfig {
    pub r: u32,
    pub g: u32,
    pub b: u32,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct AmbientLightConfig {
    pub sensitivity: u8,
}

/// Abstraction for storage keys based on profiles or global keys.
pub trait KeyedEnum {
    type Key;
    fn key(&self, active_profile: u8) -> Self::Key;
}

/// Trait implementation to generate keys for `StorageData`.
impl KeyedEnum for StorageData {
    type Key = u16;

    fn key(&self, active_profile: u8) -> Self::Key {
        match self {
            StorageData::CurrentProfile(_) => {
                StorageKey::CurrentProfile.into()
            }
            StorageData::AdsConfig(_) => StorageKey::UserProfile {
                profile_id: active_profile,
                setting: Setting::AdsConfig,
            }
            .into(),
            StorageData::ImuConfig(_) => StorageKey::UserProfile {
                profile_id: active_profile,
                setting: Setting::ImuConfig,
            }
            .into(),
            StorageData::HapticConfig(_) => StorageKey::UserProfile {
                profile_id: active_profile,
                setting: Setting::HapticConfig,
            }
            .into(),
            StorageData::NeopixelConfig(_) => StorageKey::UserProfile {
                profile_id: active_profile,
                setting: Setting::NeopixelConfig,
            }
            .into(),
            StorageData::AmbientLightConfig(_) => StorageKey::UserProfile {
                profile_id: active_profile,
                setting: Setting::AmbientLightConfig,
            }
            .into(),
            StorageData::SessionId(_) => StorageKey::UserProfile {
                profile_id: active_profile,
                setting: Setting::SessionId,
            }
            .into(),
        }
    }
}

/// Trait implementation to support serialization for `sequential_storage`.
impl<'a> sequential_storage::map::Value<'a> for StorageData {
    fn serialize_into(
        &self,
        buffer: &mut [u8],
    ) -> Result<usize, SerializationError> {
        postcard::to_slice(self, buffer)
            .map_err(|_| SerializationError::BufferTooSmall)
            .map(|slice| slice.len())
    }

    fn deserialize_from(buffer: &'a [u8]) -> Result<(Self, usize), SerializationError> {
        postcard::from_bytes(buffer)
            .map(|v| (v, buffer.len()))
            .map_err(|_| SerializationError::BufferTooSmall)
    }
}
