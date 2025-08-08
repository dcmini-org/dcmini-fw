MEMORY
{
  /* NOTE 1 K = 1 KiBi = 1024 bytes */
  MBR                               : ORIGIN = 0x00000000, LENGTH = 4K
  SOFTDEVICE                        : ORIGIN = 0x00001000, LENGTH = 152K
  ACTIVE                            : ORIGIN = 0x00027000, LENGTH = 832K
  FLASH                             : ORIGIN = 0x000f7000, LENGTH = 24K
  BOOTLOADER_STATE                  : ORIGIN = 0x000fd000, LENGTH = 4K
  STORAGE                           : ORIGIN = 0x000fe000, LENGTH = 8K
  RAM                         (rwx) : ORIGIN = 0x20000008, LENGTH = 0xfff8
  uicr_bootloader_start_address (r) : ORIGIN = 0x10001014, LENGTH = 0x4

  /* DFU is stored in external flash */
  DFU                               : ORIGIN = 0x00000000, LENGTH = 836K
  EXTERNAL_STORAGE                  : ORIGIN = 0x000f8000, LENGTH = 1056K
}

__bootloader_state_start = ORIGIN(BOOTLOADER_STATE);
__bootloader_state_end = ORIGIN(BOOTLOADER_STATE) + LENGTH(BOOTLOADER_STATE);

__bootloader_active_start = ORIGIN(ACTIVE);
__bootloader_active_end = ORIGIN(ACTIVE) + LENGTH(ACTIVE);

__bootloader_dfu_start = ORIGIN(DFU);
__bootloader_dfu_end = ORIGIN(DFU) + LENGTH(DFU);

__bootloader_start = ORIGIN(FLASH);

SECTIONS
{
  .uicr_bootloader_start_address :
  {
    LONG(__bootloader_start)
  } > uicr_bootloader_start_address
}
