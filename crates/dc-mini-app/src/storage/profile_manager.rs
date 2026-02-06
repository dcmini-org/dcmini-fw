use super::data::*;
use super::keys::{Setting, StorageKey};
use dc_mini_icd::{AdsConfig, ImuConfig, SessionId};
use embedded_storage_async::nor_flash::NorFlash;
use sequential_storage::cache::NoCache;
use sequential_storage::map::{MapConfig, MapStorage};
use sequential_storage::Error;
pub extern crate paste;

macro_rules! config_accessors {
    ($profile_field:ident, $key_variant:ident, $config_type:ty) => {
        paste::paste! {
            pub async fn [<get_ $profile_field>](&mut self) -> Option<&$config_type> {
                if self.$profile_field.is_none() {
                    let key = StorageKey::UserProfile {
                        profile_id: self.current_profile,
                        setting: Setting::$key_variant,
                    }
                    .into();
                    if let Some(StorageData::$key_variant(config)) = self.load(key).await.ok()? {
                        self.$profile_field = Some(config);
                    }
                }
                self.$profile_field.as_ref()
            }

            pub async fn [<set_ $profile_field>](
                &mut self,
                config: $config_type,
            ) -> Result<(), Error<Flash::Error>> {
                self.$profile_field = {
                    let data = StorageData::$key_variant(config);
                    self.save(data.key(self.current_profile), &data).await?;
                    if let StorageData::$key_variant(config) = data {
                        Some(config)
                    } else {
                        panic!("This should be impossible");
                    }
                };
                Ok(())
            }
        }
    };
}

pub struct ProfileManager<Flash: NorFlash, const N: usize> {
    map: MapStorage<u16, Flash, NoCache>,
    buffer: [u8; N],
    current_profile: u8,
    session_id: Option<SessionId>,
    ads_config: Option<AdsConfig>,
    imu_config: Option<ImuConfig>,
    haptic_config: Option<HapticConfig>,
    neopixel_config: Option<NeopixelConfig>,
    ambient_light_config: Option<AmbientLightConfig>,
}

impl<Flash: NorFlash, const N: usize> ProfileManager<Flash, N> {
    /// Creates a new `ProfileManager` and initializes the current profile.
    pub fn new(flash: Flash) -> Self {
        // Our memory.x file should declare the following
        extern "C" {
            static __storage_start: u32;
            static __storage_end: u32;
        }

        let range = unsafe {
            let start = &__storage_start as *const u32 as u32;
            let end = &__storage_end as *const u32 as u32;
            start..end
        };
        let config = MapConfig::new(range);
        let map = MapStorage::new(flash, config, NoCache::new());
        let mut manager = Self {
            map,
            buffer: [0; N],
            current_profile: 0,
            session_id: None,
            ads_config: None,
            imu_config: None,
            haptic_config: None,
            neopixel_config: None,
            ambient_light_config: None,
        };

        manager.current_profile = match embassy_futures::block_on(
            manager.load(StorageKey::CurrentProfile.into()),
        ) {
            Ok(Some(StorageData::CurrentProfile(profile))) => profile,
            _ => 0, // Default to profile 0 if loading fails
        };

        manager
    }

    /// Loads data from persistent storage.
    async fn load(
        &mut self,
        key: u16,
    ) -> Result<Option<StorageData>, Error<Flash::Error>> {
        self.map.fetch_item(&mut self.buffer, &key).await
    }

    /// Saves data to persistent storage.
    async fn save(
        &mut self,
        key: u16,
        value: &StorageData,
    ) -> Result<(), Error<Flash::Error>> {
        self.map.store_item(&mut self.buffer, &key, value).await
    }

    pub async fn get_current_profile(&self) -> u8 {
        self.current_profile
    }

    pub async fn set_current_profile(
        &mut self,
        profile: u8,
    ) -> Result<(), Error<Flash::Error>> {
        if profile == self.current_profile {
            return Ok(());
        }

        match self.switch_profile(profile).await {
            Ok(_) => {
                let key = StorageKey::CurrentProfile.into();
                self.current_profile = {
                    let data = StorageData::CurrentProfile(profile);
                    self.save(key, &data).await?;
                    if let StorageData::CurrentProfile(profile) = data {
                        profile
                    } else {
                        core::panic!("This should be impossible");
                    }
                };
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Switch the active profile and reload any previously loaded settings.
    pub async fn switch_profile(
        &mut self,
        new_profile: u8,
    ) -> Result<(), Error<Flash::Error>> {
        if new_profile == self.current_profile {
            return Ok(());
        }

        self.current_profile = new_profile;

        // Reload only the settings that have been lazy-loaded previously.
        if self.session_id.is_some() {
            self.session_id = None;
            self.get_session_id().await;
        }
        if self.ads_config.is_some() {
            self.ads_config = None;
            self.get_ads_config().await;
        }
        if self.imu_config.is_some() {
            self.imu_config = None;
            self.get_imu_config().await;
        }
        if self.haptic_config.is_some() {
            self.haptic_config = None;
            self.get_haptic_config().await;
        }
        if self.neopixel_config.is_some() {
            self.neopixel_config = None;
            self.get_neopixel_config().await;
        }
        if self.ambient_light_config.is_some() {
            self.ambient_light_config = None;
            self.get_ambient_light_config().await;
        }
        Ok(())
    }

    config_accessors!(session_id, SessionId, SessionId);
    config_accessors!(ads_config, AdsConfig, AdsConfig);
    config_accessors!(imu_config, ImuConfig, ImuConfig);
    config_accessors!(haptic_config, HapticConfig, HapticConfig);
    config_accessors!(neopixel_config, NeopixelConfig, NeopixelConfig);
    config_accessors!(
        ambient_light_config,
        AmbientLightConfig,
        AmbientLightConfig
    );
}
