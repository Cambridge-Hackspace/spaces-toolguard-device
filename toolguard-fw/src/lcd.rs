//! The 1602 panel, behind a PCF8574 I2C backpack.
//!
//! The content is decided in `toolguard_core::screen`; this is only the wire.
//! What it adds is the two things that make a character LCD work on a board
//! nobody has seen yet: finding the backpack, and not writing to it more than
//! it can take.
//!
//! ## Finding it
//!
//! The address is **scanned**, not configured. A PCF8574T answers in
//! `0x20..=0x27` and a PCF8574**A**T in `0x38..=0x3F`; the parts look the same
//! and vendors ship whichever. A hardcoded `0x27` is a reader that works on one
//! board and is blank on the next, with nothing in the log to say why — so the
//! scan happens at boot, through `spaces_device::i2c`, at the addresses the
//! board file allows (`board::LCD_I2C`) or every one a backpack ships at, and
//! the address it found is logged. The bus is shared (`crate::bus`).
//!
//! ## Not over-writing it
//!
//! A PCF8574 in 4-bit mode costs **three I2C transactions per nibble**: set the
//! data with E high, drop E, and the backlight bit rides along. A full 16x2
//! repaint is therefore on the order of a hundred transactions at 100 kHz, and
//! repainting every poll would be the entire I2C budget spent re-displaying the
//! same characters. The core only emits `Action::Show` when the content
//! actually changes, and this module writes only what it is given.

use anyhow::anyhow;
use esp_idf_svc::hal::delay::Delay;
use i2c_character_display::{CharacterDisplayPCF8574T, LcdDisplayType};
use spaces_device::i2c::{self, Found};
use toolguard_core::Screen;

use crate::bus::{self, Bus, Device};

/// The panel, once something has answered on the bus.
pub struct Panel {
    lcd: CharacterDisplayPCF8574T<Device, Delay>,
}

impl Panel {
    /// Put a screen up. Both rows, every time.
    ///
    /// Rewriting both rows rather than diffing them: a 1602 has no way to be
    /// asked what it is currently showing, so a diff would be against this
    /// firmware's *belief*, and the one time that belief is wrong — a glitched
    /// transaction, a brown-out on the backpack — is exactly when the panel is
    /// stuck showing something stale and a diff would never correct it. The
    /// change detection that matters already happened in the core.
    pub fn show(&mut self, screen: &Screen) -> anyhow::Result<()> {
        self.lcd
            .clear()
            .map_err(|e| anyhow!("clearing the panel: {e:?}"))?;
        for (row, line) in screen.rows().enumerate() {
            self.lcd
                .set_cursor(0, row as u8)
                .map_err(|e| anyhow!("moving to row {row}: {e:?}"))?;
            self.lcd
                .print(line)
                .map_err(|e| anyhow!("writing row {row}: {e:?}"))?;
        }
        Ok(())
    }
}

/// Find the backpack at one of `addresses` on the shared bus, and initialise
/// the panel. Returns where it was found too, so nothing else on the bus is
/// looked for there.
pub fn start(bus: &'static Bus, addresses: &[u8]) -> anyhow::Result<(Panel, Found)> {
    let mut device = bus::device(bus);
    // A PCF8574 has no identity register, so this is "something answered
    // where a backpack would be", and a probe that writes nothing to it.
    let found = addresses
        .iter()
        .find_map(|&a| {
            let part = if (0x38..=0x3F).contains(&a) {
                &i2c::PCF8574A
            } else {
                &i2c::PCF8574
            };
            i2c::find(&mut device, part, Some(&[a]), &[])
        })
        .ok_or_else(|| {
            anyhow!(
                "no I2C device answered at any of {addresses:02X?}. A PCF8574T is 0x20-0x27 and \
                 a PCF8574AT is 0x38-0x3F; check SDA/SCL are not swapped, and that the backpack \
                 has power -- its LCD rail is usually 5V even where the bus is 3.3V"
            )
        })?;
    log::info!("LCD backpack: {found}");

    // At the address it was FOUND at. `new` would use the crate's default
    // address whatever the scan found, and a backpack at 0x3F would be
    // written to at 0x27.
    let mut lcd = CharacterDisplayPCF8574T::new_with_address(
        device,
        found.address,
        LcdDisplayType::Lcd16x2,
        Delay::new_default(),
    );
    lcd.init()
        .map_err(|e| anyhow!("initialising the 1602 at {:#04x}: {e:?}", found.address))?;
    lcd.backlight(true)
        .map_err(|e| anyhow!("backlight on: {e:?}"))?;

    Ok((Panel { lcd }, found))
}
