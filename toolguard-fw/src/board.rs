//! Which pin is which, per chip, and the two settings that are not the same
//! on all of them.
//!
//! Everything chip-specific lives here so the rest of the firmware never names
//! a GPIO. `spaces-firmware` hardcodes the C6 in five independent places — the
//! triple, `MCU`, two espflash flags, the partition table's arithmetic — and
//! retargeting it means finding all five; this crate keeps the pins in one
//! module and the chip in `.cargo/config.toml`.
//!
//! ## Why these pins
//!
//! On an ESP32-C6 Super Mini, four pins are effectively unavailable and one of
//! them is a trap:
//!
//! - **GPIO8** carries the board's RGB LED *and* is a strapping pin that
//!   download mode needs pulled HIGH. Espressif: "The strapping combination of
//!   GPIO8 = 0 and GPIO9 = 0 is invalid and will trigger unexpected behavior."
//!   An external peripheral here fights both.
//! - **GPIO9** is BOOT, also strapping, and is the hold-for-setup gesture.
//! - **GPIO12/13** are native USB. The Super Mini has no USB-serial chip, so
//!   these are the only console there is; losing them loses flashing and logs.
//!
//! **GPIO4–7 are usable with care, not unavailable.** GPIO4 and GPIO5 are
//! strapping pins (MTMS, MTDI), sampled at reset and ordinary GPIOs after it;
//! GPIO6 and GPIO7 are JTAG (MTCK, MTDO), unused while the USB-JTAG is the
//! debugger. None of them is flash, which on the C6 is GPIO24–30 and not
//! broken out. The care: nothing may drive 4 or 5 at reset, and GPIO6 has a
//! pull-up after reset, so it is no place for an output that must stay off.
//! GPIO7 already carries the powered input.
//!
//! IO0–IO3, IO14 and IO18–IO23 are the freely usable set: eleven pins, and
//! this firmware uses all eleven when every role is wired (seven always; sense,
//! relay, stop and strike by the roles that need them). GPIO21–23 are pads in
//! the middle of the Super Mini rather than edge pins, so they take a fly lead
//! or a breakout. GPIO4–6 are also the C6's only ADC1 pins that SPI (0–3) does
//! not hold, which matters if the sensed input ever goes analog (#10). The C6's GPIO matrix routes SPI2 and I2C to almost any
//! pin, and ESP-IDF is explicit that below 40 MHz routing through the matrix is
//! no worse than the IOMUX pins — so at 1 MHz this is a pin-budget question and
//! not a capability one.
//!
//! ## On the ESP32-C5 (#15, compile-only)
//!
//! Same family, same triple as the C6, and the reason to want it is the radio:
//! the C5 does 5 GHz Wi-Fi, which takes a tool box out of the 2.4 GHz band the
//! Zigbee mesh shares. Its exclusions, from the IDF v5.5.3 GPIO reference, the
//! DevKitC-1 guide and esptool's boot-mode page:
//!
//! - **GPIO27** is the DevKitC-1's RGB LED and must be high at boot; **GPIO28**
//!   is BOOT (low = download). "GPIO27 = 0 and GPIO28 = 0 is invalid" — the
//!   C6's GPIO8/GPIO9 rule. They take the light and setup roles for that reason.
//! - **GPIO13/14** are USB-Serial-JTAG, the console.
//! - **GPIO15–22** are the module's flash and PSRAM (15 is the PSRAM chip
//!   select on modules that have it).
//! - **GPIO11/12** are UART0, and the DevKitC-1 has a USB-UART bridge on them.
//! - **GPIO2, 7, 25** are strapping.
//!
//! Left: 0, 1, 3, 4, 5, 6, 8, 9, 10, 23, 24 — eleven, which is what the eleven
//! other roles need, so the budget is exactly full on the DevKitC-1 (a board
//! with no UART bridge gets 11 and 12 back). The gain over the C6 is the band
//! and that **sense sits on GPIO1 = ADC1_CH0**, so an analog sense input is
//! possible on this chip where it was not on the C6's map.

//! ## Where the pins actually are
//!
//! Not here, and that is forced rather than chosen: `esp-idf-hal` 0.47 takes
//! pins as CONCRETE types behind plain bounds — `sclk: impl OutputPin + 'd`,
//! `sda: impl InputPin + OutputPin + 'd` — which is also how
//! `spaces-device-espidf` takes them (`impl OutputPin + Send + 'static`). A
//! runtime struct of GPIO numbers would have to be erased to `AnyIOPin` to fit
//! one, losing exactly the compile-time check that stops a pin being used
//! twice — and that check is what caught the S3 using GPIO8 for the LED and
//! for I2C SDA.
//!
//! So the assignment lives in `main.rs` as a `cfg`-gated tuple, one arm per
//! chip, and this module holds the reasoning plus the settings that are not
//! pins. The pin table is in this file's docs above; the compiler enforces it
//! in `main.rs`.

/// SPI clock for the RC522, in Hz.
///
/// 1 MHz, well under what the MFRC522 tolerates, chosen for wiring slack: this
/// is a breadboard-and-jumpers peripheral and the data rate is irrelevant when
/// the payload is a card UID.
pub const SPI_HZ: u32 = 1_000_000;

/// The I2C addresses a PCF8574 LCD backpack might answer on.
///
/// The bus is scanned for these at startup rather than one being hardcoded: the
/// PCF8574T answers in `0x20..=0x27` and the PCF8574**A**T in `0x38..=0x3F`,
/// the two parts are visually near-identical, and vendors ship both. A reader
/// that works on one board and shows nothing on the next, for that reason, is a
/// bad afternoon.
pub const LCD_ADDRESSES: &[u8] = &[0x27, 0x3F, 0x20, 0x38];

/// SPI mode for the RC522.
///
/// **Mode 3, and this is not a preference.** ESP-IDF's SPI master on the
/// ESP32-S3 samples MISO on the wrong edge in mode 0, so every register read
/// comes back corrupted while writes still land: the chip resets, polling
/// appears to run, `VersionReg` reads `0x6e` instead of `0x92`, and no tag is
/// ever detected. It presents as "the reader does not work" with nothing in the
/// log (esphome#19065; espressif/esp-idf#7825 and #8815).
///
/// Mode 3 also works on the RISC-V parts, so one value serves all three rather
/// than a per-chip constant nobody can test. The `version()` check at startup
/// is the oracle for this: it reads the exact register the bug corrupts.
///
/// A constant the driver actually passes, not a note: a `bool` saying "use mode
/// 3" would be a second place to disagree with the code.
pub const SPI_MODE: embedded_hal::spi::Mode = embedded_hal::spi::MODE_3;

/// The chip this build is for, for the boot banner and the device record.
pub const CHIP: &str = if cfg!(esp32c6) {
    "esp32c6"
} else if cfg!(esp32c5) {
    "esp32c5"
} else if cfg!(esp32c3) {
    "esp32c3"
} else if cfg!(esp32s3) {
    "esp32s3"
} else {
    "unknown"
};
