//// Defines storage keys for top-level and user-specific settings.
#[derive(Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum StorageKey {
    CurrentProfile,
    UserProfile { profile_id: u8, setting: Setting },
}

/// Defines settings that are part of user profiles.
#[derive(Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Setting {
    AdsConfig,
    ImuConfig,
    HapticConfig,
    NeopixelConfig,
    ApdsConfig,
    SessionId,
    MicConfig,
}

impl Setting {
    fn offset(&self) -> u16 {
        match self {
            Setting::AdsConfig => 0x00,
            Setting::ImuConfig => 0x01,
            Setting::HapticConfig => 0x02,
            Setting::NeopixelConfig => 0x03,
            Setting::ApdsConfig => 0x04,
            Setting::SessionId => 0x05,
            Setting::MicConfig => 0x06,
        }
    }
}

impl Into<u16> for StorageKey {
    fn into(self) -> u16 {
        match self {
            StorageKey::CurrentProfile => 0x00,
            StorageKey::UserProfile { profile_id, setting } => {
                const BASE: u16 = 0x0100;
                let profile_offset = profile_id as u16 * 0x10;
                BASE + profile_offset + setting.offset()
            }
        }
    }
}
