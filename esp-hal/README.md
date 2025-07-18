# esp-hal

[![Crates.io](https://img.shields.io/crates/v/esp-hal?labelColor=1C2C2E&color=C96329&logo=Rust&style=flat-square)](https://crates.io/crates/esp-hal)
[![docs.rs](https://img.shields.io/docsrs/esp-hal?labelColor=1C2C2E&color=C96329&logo=rust&style=flat-square)](https://docs.espressif.com/projects/rust/esp-hal/latest/)
![MSRV](https://img.shields.io/badge/MSRV-1.88.0-blue?labelColor=1C2C2E&style=flat-square)
![Crates.io](https://img.shields.io/crates/l/esp-hal?labelColor=1C2C2E&style=flat-square)
[![Matrix](https://img.shields.io/matrix/esp-rs:matrix.org?label=join%20matrix&labelColor=1C2C2E&color=BEC5C9&logo=matrix&style=flat-square)](https://matrix.to/#/#esp-rs:matrix.org)

Bare-metal (`no_std`) hardware abstraction layer for Espressif devices.

Implements a number of blocking and, where applicable, async traits from the various packages in the [embedded-hal] repository.

For help getting started with this HAL, please refer to [The Rust on ESP Book] and the [documentation].

[embedded-hal]: https://github.com/rust-embedded/embedded-hal
[the rust on esp book]: https://docs.espressif.com/projects/rust/book/

## [Documentation]

[documentation]: https://docs.espressif.com/projects/rust/

## Supported Devices

|   Chip   |        Datasheet         | Technical Reference Manual |             Target             |
| :------: | :----------------------: | :------------------------: | :----------------------------: |
|  ESP32   |  [ESP32][32-datasheet]   |      [ESP32][32-trm]       |    `xtensa-esp32-none-elf`     |
| ESP32-C2 | [ESP32-C2][c2-datasheet] |     [ESP32-C2][c2-trm]     | `riscv32imc-unknown-none-elf`  |
| ESP32-C3 | [ESP32-C3][c3-datasheet] |     [ESP32-C3][c3-trm]     | `riscv32imc-unknown-none-elf`  |
| ESP32-C6 | [ESP32-C6][c6-datasheet] |     [ESP32-C6][c6-trm]     | `riscv32imac-unknown-none-elf` |
| ESP32-H2 | [ESP32-H2][h2-datasheet] |     [ESP32-H2][h2-trm]     | `riscv32imac-unknown-none-elf` |
| ESP32-S2 | [ESP32-S2][s2-datasheet] |     [ESP32-S2][s2-trm]     |   `xtensa-esp32s2-none-elf`    |
| ESP32-S3 | [ESP32-S3][s3-datasheet] |     [ESP32-S3][s3-trm]     |   `xtensa-esp32s3-none-elf`    |

[32-datasheet]: https://www.espressif.com/sites/default/files/documentation/esp32_datasheet_en.pdf
[c2-datasheet]: https://www.espressif.com/sites/default/files/documentation/esp8684_datasheet_en.pdf
[c3-datasheet]: https://www.espressif.com/sites/default/files/documentation/esp32-c3_datasheet_en.pdf
[c6-datasheet]: https://www.espressif.com/sites/default/files/documentation/esp32-c6_datasheet_en.pdf
[h2-datasheet]: https://www.espressif.com/sites/default/files/documentation/esp32-h2_datasheet_en.pdf
[s2-datasheet]: https://www.espressif.com/sites/default/files/documentation/esp32-s2_datasheet_en.pdf
[s3-datasheet]: https://www.espressif.com/sites/default/files/documentation/esp32-s3_datasheet_en.pdf
[32-trm]: https://www.espressif.com/sites/default/files/documentation/esp32_technical_reference_manual_en.pdf
[c2-trm]: https://www.espressif.com/sites/default/files/documentation/esp8684_technical_reference_manual_en.pdf
[c3-trm]: https://www.espressif.com/sites/default/files/documentation/esp32-c3_technical_reference_manual_en.pdf
[c6-trm]: https://www.espressif.com/sites/default/files/documentation/esp32-c6_technical_reference_manual_en.pdf
[h2-trm]: https://www.espressif.com/sites/default/files/documentation/esp32-h2_technical_reference_manual_en.pdf
[s2-trm]: https://www.espressif.com/sites/default/files/documentation/esp32-s2_technical_reference_manual_en.pdf
[s3-trm]: https://www.espressif.com/sites/default/files/documentation/esp32-s3_technical_reference_manual_en.pdf

## Peripheral support

<!-- The following table is machine generated. Do not edit the comments and the table by hand! -->
<!-- start chip support table -->
| Driver             | ESP32 | ESP32-C2 | ESP32-C3 | ESP32-C6 | ESP32-H2 | ESP32-S2 | ESP32-S3 |
| ------------------ |:-----:|:--------:|:--------:|:--------:|:--------:|:--------:|:--------:|
| ADC                | ⚒️   | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      |
| AES                | ⚒️   |          | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      |
| ASSIST_DEBUG       |       | ⚒️      | ⚒️      | ⚒️      | ⚒️      |          | ⚒️      |
| DAC                | ⚒️   |          |          |          |          | ⚒️      |          |
| DMA                | ⚒️   | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      |
| DS                 |       |          | ❌       | ❌       | ❌       | ❌       | ❌       |
| ECC                |       | ⚒️      |          | ⚒️      | ⚒️      |          |          |
| Ethernet           | ❌    |          |          |          |          |          |          |
| ETM                |       |          |          | ⚒️      | ⚒️      |          |          |
| GPIO               | ✔️   | ✔️      | ✔️      | ✔️      | ✔️      | ✔️      | ✔️      |
| HMAC               |       |          | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      |
| I2C master         | ✔️   | ✔️      | ✔️      | ✔️      | ✔️      | ✔️      | ✔️      |
| LP I2C master      |       |          |          | ⚒️      |          |          |          |
| I2C slave          | ❌    |          | ❌       | ❌       | ❌       | ❌       | ❌       |
| I2S                | ⚒️   |          | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      |
| Interrupts         | ⚒️   | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      |
| IOMUX              | ⚒️   | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      |
| Camera interface   | ❌    |          |          |          |          | ❌       | ⚒️      |
| RGB display        | ⚒️   |          |          |          |          | ❌       | ⚒️      |
| LEDC               | ⚒️   | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      |
| MCPWM              | ⚒️   |          |          | ⚒️      | ⚒️      |          | ⚒️      |
| PARL_IO            |       |          |          | ⚒️      | ⚒️      |          |          |
| PCNT               | ⚒️   |          |          | ⚒️      | ⚒️      | ⚒️      | ⚒️      |
| PSRAM              | ⚒️   |          |          |          |          | ⚒️      | ⚒️      |
| RMT                | ⚒️   |          | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      |
| RNG                | ⚒️   | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      |
| RSA                | ⚒️   |          | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      |
| SDIO host          | ⚒️   |          |          |          |          |          | ⚒️      |
| SDIO slave         | ⚒️   |          |          | ⚒️      |          |          |          |
| Light/deep sleep   | ⚒️   | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      |
| SHA                | ⚒️   | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      |
| SPI master         | ✔️   | ✔️      | ✔️      | ✔️      | ✔️      | ✔️      | ✔️      |
| SPI slave          | ⚒️   | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      |
| SYSTIMER           |       | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      |
| Temperature sensor | ⚒️   | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      |
| Timers             | ⚒️   | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      |
| Touch              | ⚒️   |          |          |          |          | ❌       | ❌       |
| TWAI               | ⚒️   |          | ⚒️      | ⚒️      | ⚒️      | ⚒️      | ⚒️      |
| UART               | ✔️   | ✔️      | ✔️      | ✔️      | ✔️      | ✔️      | ✔️      |
| LP UART            |       |          |          | ⚒️      |          |          |          |
| ULP (FSM)          | ⚒️   |          |          |          |          | ⚒️      | ⚒️      |
| ULP (RISC-V)       |       |          |          | ⚒️      |          | ⚒️      | ⚒️      |
| USB OTG FS         |       |          |          |          |          | ⚒️      | ⚒️      |
| USB Serial/JTAG    |       |          | ⚒️      | ⚒️      | ⚒️      |          | ⚒️      |
| WIFI               | ⚒️   | ⚒️      | ⚒️      | ⚒️      |          | ⚒️      | ⚒️      |
| Bluetooth          | ⚒️   | ⚒️      | ⚒️      | ⚒️      | ⚒️      |          | ⚒️      |
| IEEE 802.15.4      |       |          |          | ⚒️      | ⚒️      |          |          |

 * Empty cell: not available
 * ❌: Not supported
 * ⚒️: Partial support
 * ✔️: Supported
<!-- end chip support table -->

## `unstable` feature

The stable feature set is designed to remain consistent and reliable. Other parts guarded by the `unstable` feature, however, are still under active development and may undergo breaking changes and are disabled by default.

## Minimum Supported Rust Version (MSRV)

This crate is guaranteed to compile when using the latest stable Rust version at the time of the crate's release. It _might_ compile with older versions, but that may change in any new release, including patches.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](../LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](../LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without
any additional terms or conditions.
