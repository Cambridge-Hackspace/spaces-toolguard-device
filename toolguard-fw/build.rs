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
const REQUIRED: [&str; 13] = [
    "light", "setup", "sclk", "mosi", "miso", "cs", "rst", "sda", "scl", "sense", "relay", "stop",
    "strike",
];

/// Roles a board may have. Each one present sets `cfg(board_has_<role>)` and
/// gets its own `board_<role>!` macro.
const OPTIONAL: [&str; 1] = ["powered"];

/// Every board there is, for `check-cfg`. A board file not listed here is
/// refused, so a new board is added in one place on purpose.
const BOARDS: [&str; 4] = ["c6-supermini", "c5-devkitc1", "c3-supermini", "s3-devkitc1"];

/// The board a chip gets when `BOARD` is not set.
fn default_board(mcu: &str) -> Option<&'static str> {
    match mcu {
        "esp32c6" => Some("c6-supermini"),
        "esp32c5" => Some("c5-devkitc1"),
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
    for role in OPTIONAL {
        println!("cargo::rustc-check-cfg=cfg(board_has_{role})");
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

    for role in pins.keys() {
        assert!(
            REQUIRED.contains(&role.as_str()) || OPTIONAL.contains(&role.as_str()),
            "{board}: `{role}` is not a role this firmware has; the roles are \
             {REQUIRED:?} and optionally {OPTIONAL:?}"
        );
    }
    let mut by_gpio: BTreeMap<u32, &str> = BTreeMap::new();
    for (role, gpio) in &pins {
        if let Some(other) = by_gpio.insert(*gpio, role) {
            panic!("{board}: GPIO{gpio} is both `{other}` and `{role}`");
        }
    }

    let tuple: Vec<String> = REQUIRED
        .iter()
        .map(|role| {
            let gpio = pins
                .get(*role)
                .unwrap_or_else(|| panic!("{board}: no pin for the required role `{role}`"));
            format!("$p.gpio{gpio}")
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
    for role in OPTIONAL {
        if let Some(gpio) = pins.get(role) {
            println!("cargo:rustc-cfg=board_has_{role}");
            out.push_str(&format!(
                "\n/// This board's `{role}` pin.\nmacro_rules! board_{role} {{\n    \
                 ($p:ident) => {{\n        $p.gpio{gpio}\n    }};\n}}\n"
            ));
        }
    }

    println!("cargo:rustc-cfg=board=\"{board}\"");
    println!("cargo:rustc-env=TOOLGUARD_BOARD={board}");
    let out_path = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("board_pins.rs");
    std::fs::write(&out_path, out)
        .unwrap_or_else(|e| panic!("cannot write {}: {e}", out_path.display()));
}
