use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use embassy_boot::{BlockingFirmwareState, FirmwareUpdaterConfig};
use embassy_embedded_hal::flash::partition::Partition;
use embassy_nrf::nvmc::Nvmc;
use embassy_nrf::qspi::Qspi;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use embassy_sync::mutex::Mutex;

/// The DFU partition size (992K, from linkerfile).
pub const DFU_PARTITION_SIZE: u32 = 992 * 1024;

/// Async partition over external QSPI flash for DFU firmware writes.
pub type DfuPartition<'a> = Partition<'a, NoopRawMutex, Qspi<'static>>;

/// Shared DFU resources used by both BLE and USB firmware update paths.
///
/// Holds the external QSPI flash (DFU partition) and internal NVMC (state partition)
/// behind mutexes so they can be accessed from both transports.
pub struct DfuResources {
    /// External QSPI flash for DFU firmware staging.
    pub dfu_flash: Mutex<NoopRawMutex, Qspi<'static>>,
    /// Internal NVMC for bootloader state (mark_updated/mark_booted).
    /// Wrapped in BlockingMutex<RefCell<>> for blocking embassy-boot API.
    state_flash: BlockingMutex<NoopRawMutex, RefCell<Nvmc<'static>>>,
    /// Prevents concurrent DFU from BLE and USB.
    pub dfu_active: AtomicBool,
    /// Bytes written so far (for USB progress reporting).
    dfu_offset: AtomicU32,
    /// Total firmware size (for USB progress reporting).
    dfu_total_size: AtomicU32,
}

impl DfuResources {
    /// Create new DFU resources from the QSPI and NVMC peripherals.
    ///
    /// # Safety
    /// The NVMC instance provided here must only write to the BOOTLOADER_STATE region
    /// (0x6000..0x7000). The ProfileManager's NVMC writes to the STORAGE region
    /// (0xFE000..0x100000). These are non-overlapping regions serialized by hardware.
    pub fn new(qspi: Qspi<'static>, nvmc: Nvmc<'static>) -> Self {
        Self {
            dfu_flash: Mutex::new(qspi),
            state_flash: BlockingMutex::new(RefCell::new(nvmc)),
            dfu_active: AtomicBool::new(false),
            dfu_offset: AtomicU32::new(0),
            dfu_total_size: AtomicU32::new(0),
        }
    }

    /// Create an async DFU partition for writing firmware data.
    /// Uses linkerfile symbols to determine the DFU region in external flash.
    pub fn dfu_partition(&self) -> DfuPartition<'_> {
        extern "C" {
            static __bootloader_dfu_start: u32;
            static __bootloader_dfu_end: u32;
        }
        let (start, size) = unsafe {
            let start = &__bootloader_dfu_start as *const u32 as u32;
            let end = &__bootloader_dfu_end as *const u32 as u32;
            (start, end - start)
        };
        Partition::new(&self.dfu_flash, start, size)
    }

    /// Mark the DFU partition as updated (triggers bootloader swap on next reset).
    /// This is a blocking operation on the NVMC state partition.
    pub fn mark_updated(
        &self,
    ) -> Result<(), embassy_boot::FirmwareUpdaterError> {
        let dfu_stub = self.dfu_flash_blocking_stub();
        let config = FirmwareUpdaterConfig::from_linkerfile_blocking(
            &dfu_stub,
            &self.state_flash,
        );
        let mut aligned = [0u8; 4];
        let mut state =
            BlockingFirmwareState::from_config(config, &mut aligned);
        state.mark_updated()
    }

    /// Try to claim the DFU lock. Returns true if successfully acquired.
    pub fn try_start(&self) -> bool {
        self.dfu_active
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    /// Release the DFU lock and reset progress counters.
    pub fn finish(&self) {
        self.dfu_active.store(false, Ordering::SeqCst);
        self.dfu_offset.store(0, Ordering::SeqCst);
        self.dfu_total_size.store(0, Ordering::SeqCst);
    }

    /// Set the total firmware size for progress tracking.
    pub fn set_total_size(&self, size: u32) {
        self.dfu_total_size.store(size, Ordering::SeqCst);
    }

    /// Add bytes to the progress offset counter.
    pub fn add_offset(&self, bytes: u32) {
        self.dfu_offset.fetch_add(bytes, Ordering::SeqCst);
    }

    /// Get current progress as (offset, total_size).
    pub fn progress(&self) -> (u32, u32) {
        (
            self.dfu_offset.load(Ordering::SeqCst),
            self.dfu_total_size.load(Ordering::SeqCst),
        )
    }

    /// Check if a DFU is currently in progress.
    pub fn is_active(&self) -> bool {
        self.dfu_active.load(Ordering::SeqCst)
    }

    /// Creates a dummy blocking mutex wrapper around the async QSPI flash mutex
    /// for use with `from_linkerfile_blocking`. The DFU flash partition is only used
    /// for size calculation in `BlockingFirmwareState`, not actual writes.
    ///
    /// We only need the state partition for mark_updated, so we pass a stub for DFU.
    fn dfu_flash_blocking_stub(
        &self,
    ) -> BlockingMutex<NoopRawMutex, RefCell<StubFlash>> {
        BlockingMutex::new(RefCell::new(StubFlash))
    }
}

/// Stub flash that satisfies `from_linkerfile_blocking`'s DFU flash type requirement.
/// Only the state partition is actually used for mark_updated/mark_booted.
struct StubFlash;

impl embedded_storage::nor_flash::ErrorType for StubFlash {
    type Error = core::convert::Infallible;
}

impl embedded_storage::nor_flash::ReadNorFlash for StubFlash {
    const READ_SIZE: usize = 4;
    fn read(
        &mut self,
        _offset: u32,
        _buf: &mut [u8],
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    fn capacity(&self) -> usize {
        DFU_PARTITION_SIZE as usize
    }
}

impl embedded_storage::nor_flash::NorFlash for StubFlash {
    const WRITE_SIZE: usize = 4;
    const ERASE_SIZE: usize = 4096;
    fn write(&mut self, _offset: u32, _buf: &[u8]) -> Result<(), Self::Error> {
        Ok(())
    }
    fn erase(&mut self, _from: u32, _to: u32) -> Result<(), Self::Error> {
        Ok(())
    }
}
