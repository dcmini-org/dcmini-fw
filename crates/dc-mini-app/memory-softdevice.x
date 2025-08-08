MEMORY
{
  /* NOTE 1 K = 1 KiBi = 1024 bytes */
  MBR                               : ORIGIN = 0x00000000, LENGTH = 4K
  SOFTDEVICE                        : ORIGIN = 0x00001000, LENGTH = 152K
  FLASH                             : ORIGIN = 0x00027000, LENGTH = 832K
  BOOTLOADER                        : ORIGIN = 0x000f7000, LENGTH = 24K
  BOOTLOADER_STATE                  : ORIGIN = 0x000fd000, LENGTH = 4K
  STORAGE                           : ORIGIN = 0x000fe000, LENGTH = 8K
  RAM                         (rwx) : ORIGIN = 0x20000000 + 40592, LENGTH = 256K - 40592

  /* DFU is stored in external flash */
  DFU                               : ORIGIN = 0x00000000, LENGTH = 836K
  EXTERNAL_STORAGE                  : ORIGIN = 0x000f8000, LENGTH = 1056K
}

__storage_start = ORIGIN(STORAGE);
__storage_end = ORIGIN(STORAGE) + LENGTH(STORAGE);
__external_storage = ORIGIN(EXTERNAL_STORAGE);
