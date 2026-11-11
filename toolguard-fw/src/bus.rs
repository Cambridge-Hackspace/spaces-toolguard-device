//! The one I²C bus, shared by everything on it (#23, #25).
//!
//! The panel used to own the bus outright. On the carrier the bus also carries
//! the card reader (a converted RC522), the joystick's expander and the Qwiic
//! port, so the driver is made once here and each part gets a [`Device`]: a
//! handle that borrows the bus for one transaction at a time.
//!
//! Single-threaded on purpose. Everything on the bus is driven from the
//! device loop's thread (`Hardware::poll` and `apply`), so a `RefCell` is
//! enough; a second thread reaching for the bus would be a design change, and
//! the `RefCell` would say so at the first overlap rather than corrupt a
//! transaction.

use std::cell::RefCell;

use anyhow::Context;
use embedded_hal_bus::i2c::RefCellDevice;
use esp_idf_svc::hal::gpio::{InputPin, OutputPin};
use esp_idf_svc::hal::i2c::{I2c, I2cConfig, I2cDriver};
use esp_idf_svc::hal::units::FromValueType as _;

/// The bus, for the life of the program.
pub type Bus = RefCell<I2cDriver<'static>>;

/// One part's handle on the bus.
pub type Device = RefCellDevice<'static, I2cDriver<'static>>;

/// Bring the bus up. It lives as long as the program, so it is leaked once
/// here rather than threaded through as a lifetime.
pub fn start(
    i2c: impl I2c + 'static,
    sda: impl InputPin + OutputPin + 'static,
    scl: impl InputPin + OutputPin + 'static,
) -> anyhow::Result<&'static Bus> {
    // 100 kHz: the PCF8574 is a standard-mode part, and the HD44780 behind the
    // LCD's is slower still. Internal pull-ups on, because a cheap backpack may
    // not populate them and a floating bus reads as "nothing is there"; the
    // carrier has its own 4.7 k to 3.3 V as well.
    let config = I2cConfig::new()
        .baudrate(100_u32.kHz().into())
        .sda_enable_pullup(true)
        .scl_enable_pullup(true);
    let driver = I2cDriver::new(i2c, sda, scl, &config).context("the I2C bus")?;
    Ok(Box::leak(Box::new(RefCell::new(driver))))
}

/// A handle on `bus` for one part.
pub fn device(bus: &'static Bus) -> Device {
    RefCellDevice::new(bus)
}
