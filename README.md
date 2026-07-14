# Expel

An ESP32 (Xtensa) ELF Loader.

It allows you to load and execute ELF binaries from your Rust-based ESP-HAL
(`no_std`) firmware.

To compile compatible programs, build them as relocatable objects (`-Wl,-r` flag).
This can be done in the `.cargo/config.toml` file if using Rust or directly as a
flag in your C compiler.

---

## Spatial Tanks

<sub>(Special Thanks)</sub>

- [ELF Loader IDF Component](https://github.com/espressif/esp-iot-solution/tree/master/components/elf_loader):
This crate is based on the official ELF Loader library by Espressif.
- [niicoooo's ELF Loader](https://github.com/niicoooo/esp32-elfloader):
For reference on the relocations not in Espressif's loader (`R_XTENSA_32` and `R_XTENSA_SLOT0_OP`)
- [Linux Kernel](https://github.com/torvalds/linux/blob/master/arch/xtensa/kernel/module.c):
For reference on relocations not in Espressif's loader
