MEMORY
{
  /* NOTE 1 K = 1 KiBi = 1024 bytes */
  FLASH                             : ORIGIN = 0x00000000, LENGTH = 24K
  ACTIVE                            : ORIGIN = 0x00007000, LENGTH = 988K
  RAM                         (rwx) : ORIGIN = 0x20000000, LENGTH = 256K
}
