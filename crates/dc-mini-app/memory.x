MEMORY
{
  /* NOTE 1 K = 1 KiBi = 1024 bytes */
  BOOTLOADER                        : ORIGIN = 0x00000000, LENGTH = 24K
  BOOTLOADER_STATE                  : ORIGIN = 0x00006000, LENGTH = 4K
  FLASH                             : ORIGIN = 0x00007000, LENGTH = 988K
  STORAGE                           : ORIGIN = 0x000fe000, LENGTH = 8K
  RAM                         (rwx) : ORIGIN = 0x20000000, LENGTH = 256K

  /* DFU is stored in external flash */
  DFU                               : ORIGIN = 0x00000000, LENGTH = 992K
  EXTERNAL_STORAGE                  : ORIGIN = 0x000f8000, LENGTH = 1056K
}
 
__storage_start = ORIGIN(STORAGE);
__storage_end = ORIGIN(STORAGE) + LENGTH(STORAGE);
__external_storage = ORIGIN(EXTERNAL_STORAGE);

__bootloader_state_start = ORIGIN(BOOTLOADER_STATE);
__bootloader_state_end = ORIGIN(BOOTLOADER_STATE) + LENGTH(BOOTLOADER_STATE);

__bootloader_dfu_start = ORIGIN(DFU);
__bootloader_dfu_end = ORIGIN(DFU) + LENGTH(DFU);

