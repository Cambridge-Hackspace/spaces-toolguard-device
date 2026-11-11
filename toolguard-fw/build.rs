//! ESP-IDF's environment, and this board's pin map.
//!
//! **The board is chosen by `BOARD`**, the way the chip is chosen by `MCU`, and
//! each chip has a default so a plain build for it needs nothing more. Its pin
//! map is `boards/<BOARD>.toml`, the single source of which GPIO does what:
//! this script turns it into a `board_pins!` macro that `main.rs` expands, so
//! the pins stay concrete `pins.gpioN` values and using one twice is still a
//! use-after-move error at compile time -- the check that caught the S3 using
//! GPIO8 for the LED and for I2C (`board.rs`). A GPIO named twice in the file
//! is also refused here first, with the board and both roles in the message.
//!
//! `toolguard-core/tests/pinmap.rs` checks the same files against the table in
//! `docs/INSTALLING.md` and against the pins each chip drives at boot.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// Every board has all of these, in this order: the order of the tuple
/// `main.rs` destructures.
const REQUIRED: [&str; 8] = [
    "light", "setup", "sda", "scl", "sense", "relay", "stop", "strike",
];

/// Groups of pins a board may have, all of a group or none of it. Each group
/// present sets `cfg(board_has_<group>)` and gets a `board_<group>!` macro:
/// the pin, or a tuple of them in this order.
///
/// - `spi`: an RC522 on SPI. A board without it has its reader on I2C (#23).
/// - `rst`: the RC522's reset line. A board may tie it to 3.3 V instead.
/// - `powered`: the machine-powered input (#8).
const GROUPS: [(&str, &[&str]); 3] = [
    ("spi", &["sclk", "mosi", "miso", "cs"]),
    ("rst", &["rst"]),
    ("powered", &["powered"]),
];

/// The tables a board file may have besides `[pins]`.
const TABLES: [&str; 3] = ["light", "stop", "i2c"];

/// Every board there is, for `check-cfg`. A board file not listed here is
/// refused, so a new board is added in one place on purpose.
const BOARDS: [&str; 5] = [
    "c6-supermini",
    "c5-xiao-carrier",
    "c5-devkitc1",
    "c3-supermini",
    "s3-devkitc1",
];

/// The board a chip gets when `BOARD` is not set.
fn default_board(mcu: &str) -> Option<&'static str> {
    match mcu {
        "esp32c6" => Some("c6-supermini"),
        // The C5 is the main board, and the carrier is what it goes on (#25).
        "esp32c5" => Some("c5-xiao-carrier"),
        "esp32c3" => Some("c3-supermini"),
        "esp32s3" => Some("s3-devkitc1"),
        _ => None,
    }
}

/// The chip, as esp-idf-sys will resolve it: `MCU` if set (`.cargo/config.toml`
/// sets it for the default target, CI sets it per job), else the first chip
/// the triple supports, which is what esp-idf-sys picks too.
fn mcu() -> String {
    if let Ok(mcu) = std::env::var("MCU") {
        return mcu;
    }
    let target = std::env::var("TARGET").unwrap_or_default();
    match target.as_str() {
        "riscv32imac-esp-espidf" => "esp32c6",
        "riscv32imc-esp-espidf" => "esp32c3",
        "xtensa-esp32s3-espidf" => "esp32s3",
        other => panic!("no MCU set, and no default chip for target `{other}`"),
    }
    .to_string()
}

fn main() {
    embuild::espidf::sysenv::output();

    println!("cargo:rerun-if-env-changed=BOARD");
    println!("cargo:rerun-if-env-changed=MCU");
    for board in BOARDS {
        println!("cargo::rustc-check-cfg=cfg(board, values(\"{board}\"))");
    }
    for (group, _) in GROUPS {
        println!("cargo::rustc-check-cfg=cfg(board_has_{group})");
    }

    let mcu = mcu();
    let board = match std::env::var("BOARD") {
        Ok(b) => b,
        Err(_) => default_board(&mcu)
            .unwrap_or_else(|| panic!("no BOARD set, and no default board for MCU `{mcu}`"))
            .to_string(),
    };
    assert!(
        BOARDS.contains(&board.as_str()),
        "BOARD `{board}` is not one of {BOARDS:?}; a new board is added to BOARDS in build.rs"
    );

    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("boards");
    let path = dir.join(format!("{board}.toml"));
    println!("cargo:rerun-if-changed={}", path.display());
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let file: toml::Table = text
        .parse()
        .unwrap_or_else(|e| panic!("{} is not valid TOML: {e}", path.display()));

    let named = file.get("board").and_then(|v| v.as_str());
    assert_eq!(
        named,
        Some(board.as_str()),
        "{} names itself {named:?}, not `{board}`",
        path.display()
    );
    let chip = file
        .get("chip")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("{} has no `chip`", path.display()));
    assert_eq!(
        chip, mcu,
        "BOARD `{board}` is for {chip}, but this build is for MCU `{mcu}`: \
         set both, or neither and take the chip's default board"
    );

    let pins: BTreeMap<String, u32> = file
        .get("pins")
        .and_then(|v| v.as_table())
        .unwrap_or_else(|| panic!("{} has no [pins] table", path.display()))
        .iter()
        .map(|(role, v)| {
            let gpio = v
                .as_integer()
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or_else(|| panic!("{board}: pin `{role}` is not a GPIO number"));
            (role.clone(), gpio)
        })
        .collect();

    let optional = || GROUPS.iter().flat_map(|(_, roles)| roles.iter());
    for role in pins.keys() {
        assert!(
            REQUIRED.contains(&role.as_str()) || optional().any(|r| r == role),
            "{board}: `{role}` is not a role this firmware has; the roles are \
             {REQUIRED:?} and the groups {GROUPS:?}"
        );
    }
    for key in file.keys() {
        assert!(
            ["board", "chip", "pins"].contains(&key.as_str()) || TABLES.contains(&key.as_str()),
            "{board}: `{key}` is not something a board file says"
        );
    }
    let mut by_gpio: BTreeMap<u32, &str> = BTreeMap::new();
    for (role, gpio) in &pins {
        if let Some(other) = by_gpio.insert(*gpio, role) {
            panic!("{board}: GPIO{gpio} is both `{other}` and `{role}`");
        }
    }

    let gpio = |role: &str| format!("$p.gpio{}", pins[role]);
    let tuple: Vec<String> = REQUIRED
        .iter()
        .map(|role| {
            assert!(
                pins.contains_key(*role),
                "{board}: no pin for the required role `{role}`"
            );
            gpio(role)
        })
        .collect();

    let mut out = format!(
        "// Generated by build.rs from boards/{board}.toml. Edit that file, not this.\n\
         \n\
         /// This board's pins, in the order `main.rs` destructures them:\n\
         /// {}.\n\
         macro_rules! board_pins {{\n    ($p:ident) => {{\n        ({},)\n    }};\n}}\n",
        REQUIRED.join(", "),
        tuple.join(", ")
    );
    for (group, roles) in GROUPS {
        let present = roles.iter().filter(|r| pins.contains_key(**r)).count();
        if present == 0 {
            continue;
        }
        assert_eq!(
            present,
            roles.len(),
            "{board}: the `{group}` pins are all or none of {roles:?}"
        );
        println!("cargo:rustc-cfg=board_has_{group}");
        let value = if roles.len() == 1 {
            gpio(roles[0])
        } else {
            format!(
                "({},)",
                roles.iter().map(|r| gpio(r)).collect::<Vec<_>>().join(", ")
            )
        };
        out.push_str(&format!(
            "\n/// This board's `{group}` pins: {}.\nmacro_rules! board_{group} {{\n    \
             ($p:ident) => {{\n        {value}\n    }};\n}}\n",
            roles.join(", ")
        ));
    }

    // What the board says besides its pins, as constants for board.rs.
    let table = |name: &str| file.get(name).and_then(|v| v.as_table());
    let light_kind = table("light")
        .and_then(|t| t.get("kind"))
        .map(|v| v.as_str().expect("light.kind is a string"))
        .unwrap_or("ws2812");
    let light_active_low = table("light")
        .and_then(|t| t.get("active_low"))
        .map(|v| v.as_bool().expect("light.active_low is true or false"))
        .unwrap_or(false);
    let led = match light_kind {
        "ws2812" => "Led::Ws2812".to_string(),
        "plain" => format!("Led::Plain {{ active_low: {light_active_low} }}"),
        other => panic!("{board}: light.kind `{other}` is not `ws2812` or `plain`"),
    };
    let stop_nc = table("stop")
        .and_then(|t| t.get("normally_closed"))
        .map(|v| v.as_bool().expect("stop.normally_closed is true or false"))
        .unwrap_or(false);
    let addresses = |key: &str| match table("i2c").and_then(|t| t.get(key)) {
        None => "None".to_string(),
        Some(v) => {
            let addresses: Vec<String> = v
                .as_array()
                .unwrap_or_else(|| panic!("{board}: i2c.{key} is a list of addresses"))
                .iter()
                .map(|a| {
                    let a = a.as_integer().expect("an I2C address is a number");
                    assert!((0x08..=0x77).contains(&a), "{board}: I2C address {a:#x}");
                    format!("{a:#04x}")
                })
                .collect();
            format!("Some(&[{}])", addresses.join(", "))
        }
    };
    if let Some(i2c) = table("i2c") {
        for key in i2c.keys() {
            assert!(
                ["reader", "lcd"].contains(&key.as_str()),
                "{board}: i2c.{key} is not a part a board file places"
            );
        }
    }
    let reader_i2c = addresses("reader");
    let lcd_i2c = addresses("lcd");
    let consts = format!(
        "// Generated by build.rs from boards/{board}.toml. Edit that file, not this.\n\
         \n\
         /// The kind of status LED this board has.\n\
         pub const LED: spaces_device_espidf::status_light::Led =\n    \
         spaces_device_espidf::status_light::{led};\n\
         \n\
         /// Whether the stop button is wired normally closed: resting closed to\n\
         /// ground, so pressed -- or a cut wire -- reads high on the pull-up.\n\
         pub const STOP_NORMALLY_CLOSED: bool = {stop_nc};\n\
         \n\
         /// Where this board puts an RC522 on I2C, or `None` to look wherever\n\
         /// the part can be.\n\
         pub const READER_I2C: Option<&[u8]> = {reader_i2c};\n\
         \n\
         /// Where this board puts the LCD's backpack, or `None` for every\n\
         /// address a backpack ships at ([`LCD_ADDRESSES`]).\n\
         pub const LCD_I2C: Option<&[u8]> = {lcd_i2c};\n"
    );

    println!("cargo:rustc-cfg=board=\"{board}\"");
    println!("cargo:rustc-env=TOOLGUARD_BOARD={board}");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    for (name, text) in [("board_pins.rs", out), ("board_consts.rs", consts)] {
        let out_path = out_dir.join(name);
        std::fs::write(&out_path, text)
            .unwrap_or_else(|e| panic!("cannot write {}: {e}", out_path.display()));
    }
}
