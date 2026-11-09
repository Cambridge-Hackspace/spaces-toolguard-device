//! The board files are checked against the docs, against themselves, and
//! against what each chip does at boot.
//!
//! `toolguard-fw/boards/<board>.toml` is each board's pin map, and the
//! firmware's pin assignment is generated from it (`toolguard-fw/build.rs`),
//! so these files ARE what the firmware does.
//!
//! The table in `docs/INSTALLING.md` is what somebody follows with a soldering
//! iron, and a pin table is exactly the kind of thing that drifts: the source
//! moves a signal, the table does not, and the next person wires a relay to
//! the I²C clock. The failure is expensive and silent until the smoke. So the
//! mapping is written twice on purpose -- once in the board files, once in the
//! document -- and this compares them. Deriving the table from the files would
//! make the check agree with itself; the duplication **is** the check.
//!
//! **Why it lives in `toolguard-core`.** The firmware crate only compiles for an
//! ESP target, and nothing that runs on a host can link against it. The board
//! files are plain data, so a host test reads them directly.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// Every board, as `(board, chip)`. Kept in step with `BOARDS` in build.rs by
/// [`every_board_file_is_listed_and_every_listed_board_has_a_file`].
const BOARDS: [(&str, &str); 4] = [
    ("c6-supermini", "esp32c6"),
    ("c5-devkitc1", "esp32c5"),
    ("c3-supermini", "esp32c3"),
    ("s3-devkitc1", "esp32s3"),
];

/// The roles every board has, as build.rs requires them.
const REQUIRED: [&str; 13] = [
    "light", "setup", "sclk", "mosi", "miso", "cs", "rst", "sda", "scl", "sense", "relay", "stop",
    "strike",
];
const OPTIONAL: [&str; 1] = ["powered"];

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is this package; the repository is its parent.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("toolguard-core must have a parent directory")
        .to_path_buf()
}

fn read(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// A board file, as build.rs reads it.
struct Board {
    chip: String,
    pins: BTreeMap<String, u32>,
}

fn parse_board(name: &str, text: &str) -> Board {
    let file: toml::Table = text
        .parse()
        .unwrap_or_else(|e| panic!("{name}: not valid TOML: {e}"));
    let named = file.get("board").and_then(|v| v.as_str());
    assert_eq!(named, Some(name), "{name}.toml names itself {named:?}");
    let chip = file
        .get("chip")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("{name}: no chip"))
        .to_string();
    let pins = file
        .get("pins")
        .and_then(|v| v.as_table())
        .unwrap_or_else(|| panic!("{name}: no [pins] table"))
        .iter()
        .map(|(role, v)| {
            let gpio = v
                .as_integer()
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or_else(|| panic!("{name}: `{role}` is not a GPIO number"));
            (role.clone(), gpio)
        })
        .collect();
    Board { chip, pins }
}

fn board(name: &str) -> Board {
    parse_board(name, &read(&format!("toolguard-fw/boards/{name}.toml")))
}

/// `signal -> pin number`, from the markdown table for one board.
fn from_doc(doc: &str, board: &str) -> BTreeMap<String, u32> {
    let marker = "<!-- PINMAP:";
    let start = doc
        .find(marker)
        .expect("the pin table's PINMAP marker is gone; this check cannot find its table");
    let table = &doc[start..];

    let header = table
        .lines()
        .find(|l| l.starts_with('|') && l.contains("signal"))
        .expect("no header row after the PINMAP marker");
    let column = header
        .split('|')
        .map(str::trim)
        .position(|c| c == board)
        .unwrap_or_else(|| panic!("the table has no `{board}` column"));

    let mut out = BTreeMap::new();
    for line in table.lines().skip_while(|l| !std::ptr::eq(*l, header)) {
        if !line.starts_with('|') {
            // Past the end of the table.
            if !out.is_empty() {
                break;
            }
            continue;
        }
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        let Some(signal) = cells.get(1) else { continue };
        // Skip the header and the `|---|` separator.
        if *signal == "signal" || signal.starts_with("---") || signal.is_empty() {
            continue;
        }
        let Some(cell) = cells.get(column) else {
            continue;
        };
        if let Ok(pin) = cell.parse::<u32>() {
            out.insert(signal.to_string(), pin);
        }
    }
    out
}

#[test]
fn the_documented_pin_table_matches_the_board_files() {
    let doc = read("docs/INSTALLING.md");
    for (name, _) in BOARDS {
        let pins = board(name).pins;
        let documented = from_doc(&doc, name);
        assert!(
            !documented.is_empty(),
            "{name}: parsed no pins out of the document, so the comparison \
             below would pass over an empty set"
        );
        assert_eq!(
            documented, pins,
            "{name}: the pin table in docs/INSTALLING.md and \
             toolguard-fw/boards/{name}.toml disagree.\n  documented: \
             {documented:?}\n  board file: {pins:?}\n\nSomebody follows that \
             table with a soldering iron."
        );
    }
}

/// What build.rs refuses, checked on the host too, so a bad board file fails
/// `cargo test` and not only a firmware build nobody ran for that board.
#[test]
fn every_board_file_is_complete_and_names_each_gpio_once() {
    for (name, chip) in BOARDS {
        let b = board(name);
        assert_eq!(b.chip, chip, "{name}: chip");
        for role in REQUIRED {
            assert!(b.pins.contains_key(role), "{name}: no pin for `{role}`");
        }
        for role in b.pins.keys() {
            assert!(
                REQUIRED.contains(&role.as_str()) || OPTIONAL.contains(&role.as_str()),
                "{name}: `{role}` is not a role the firmware has"
            );
        }
        let mut seen: BTreeMap<u32, &str> = BTreeMap::new();
        for (role, gpio) in &b.pins {
            if let Some(other) = seen.insert(*gpio, role) {
                panic!("{name}: GPIO{gpio} is both `{other}` and `{role}`");
            }
        }
    }
}

#[test]
fn every_board_file_is_listed_and_every_listed_board_has_a_file() {
    let dir = repo_root().join("toolguard-fw/boards");
    let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot list {}: {e}", dir.display()))
        .filter_map(|e| {
            let p = e.ok()?.path();
            if p.extension()? != "toml" {
                return None;
            }
            Some(p.file_stem()?.to_string_lossy().into_owned())
        })
        .collect();
    on_disk.sort();
    let mut listed: Vec<String> = BOARDS.iter().map(|(b, _)| b.to_string()).collect();
    listed.sort();
    assert_eq!(
        on_disk, listed,
        "the board files and this test's BOARDS differ"
    );

    // And build.rs's own list, which decides what BOARD may be.
    let build = read("toolguard-fw/build.rs");
    for b in &listed {
        assert!(
            build.contains(&format!("\"{b}\"")),
            "build.rs does not list `{b}`, so BOARD={b} would be refused"
        );
    }
}

/// The parser must be capable of disagreeing, or the comparison above is two
/// empty maps matching.
#[test]
fn the_doc_parser_reports_only_what_it_is_given() {
    let doc = "\
<!-- PINMAP: x -->

| signal | c6-supermini | c5-devkitc1 | c3-supermini | s3-devkitc1 |
|---|---|---|---|---|
| light | 8 | 27 | 7 | 48 |
| relay | 19 | 9 | 3 | 16 |
";
    let c6 = from_doc(doc, "c6-supermini");
    assert_eq!(c6.get("light"), Some(&8));
    assert_eq!(c6.get("relay"), Some(&19));
    // And it reads the right COLUMN, which a parser that always took the first
    // number would get wrong on exactly one board.
    assert_eq!(
        from_doc(doc, "c3-supermini").get("light"),
        Some(&7),
        "the column lookup is not per board"
    );
    assert_eq!(from_doc(doc, "s3-devkitc1").get("light"), Some(&48));
    assert_eq!(from_doc(doc, "c5-devkitc1").get("relay"), Some(&9));
}

/// And a transposed pin must fail, which is the defect this exists for.
#[test]
fn a_transposed_pin_is_caught() {
    let pins = board("c6-supermini").pins;
    let doc = read("docs/INSTALLING.md");
    let mut documented = from_doc(&doc, "c6-supermini");

    let sda = *documented.get("sda").expect("sda should be documented");
    let scl = *documented.get("scl").expect("scl should be documented");
    documented.insert("sda".into(), scl);
    documented.insert("scl".into(), sda);
    assert_ne!(
        documented, pins,
        "swapping SDA and SCL in the documented table still compared equal, so \
         this check would not notice the table drifting"
    );
}

/// A board file naming one GPIO twice is refused.
#[test]
fn a_gpio_named_twice_is_caught() {
    let text = "board = \"x\"\nchip = \"esp32c6\"\n[pins]\nsda = 22\nscl = 22\n";
    let b = parse_board("x", text);
    let mut seen = std::collections::BTreeSet::new();
    let twice = b.pins.values().any(|g| !seen.insert(*g));
    assert!(twice, "the duplicate in the fixture was not seen");
}

/// The outputs that switch something must not sit on a pin the chip drives,
/// or pulls high, before the firmware runs (#20).
///
/// A relay or strike driver is held off at boot by its gate pull-down, and
/// only while the pin itself floats or is pulled low. A pin the boot ROM
/// drives defeats that: the C3's map put the strike on GPIO21, U0TXD, and by
/// default the ROM prints its boot messages on UART0 whatever the
/// application's console is -- so the latch would pulse on every power-up,
/// crash and update.
///
/// UART0 TX is listed for every chip, from each chip's datasheet. The C3 also
/// lists MTCK (GPIO6), which has a pull-up after reset unless JTAG is disabled
/// in eFuse (ESP32-C3 datasheet v2.4, table 2-1, note 6). Other chips' reset
/// pulls are not yet checked pin by pin; add them here as they are.
#[test]
fn switching_outputs_avoid_pins_driven_or_pulled_high_at_boot() {
    const SWITCHING: [&str; 2] = ["relay", "strike"];
    let unsafe_at_boot: [(&str, &[(u32, &str)]); 4] = [
        ("esp32c6", &[(16, "U0TXD")]),
        ("esp32c5", &[(11, "U0TXD")]),
        (
            "esp32c3",
            &[(21, "U0TXD"), (6, "MTCK, pulled up after reset")],
        ),
        ("esp32s3", &[(43, "U0TXD")]),
    ];
    for (name, _) in BOARDS {
        let b = board(name);
        let pins = unsafe_at_boot
            .iter()
            .find(|(chip, _)| *chip == b.chip)
            .map(|(_, p)| *p)
            .unwrap_or_else(|| panic!("{name}: no boot-safety list for {}", b.chip));
        for signal in SWITCHING {
            let pin = b
                .pins
                .get(signal)
                .unwrap_or_else(|| panic!("{name}: no `{signal}` in the pin map"));
            if let Some((_, why)) = pins.iter().find(|(p, _)| p == pin) {
                panic!(
                    "{name}: `{signal}` is on GPIO{pin} ({why}), which is driven or \
                     pulled high before the firmware runs -- it would switch on at boot"
                );
            }
        }
    }
}
