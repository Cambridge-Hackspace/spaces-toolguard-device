//! What reaches the panel is what the panel can draw.
//!
//! An HD44780 is sent bytes and draws whatever its character ROM holds at each
//! one, so a string is safe for it only if every character is a single byte
//! the ROM draws as itself.

use toolguard_core::screen::{denied, drawable, Screen, COLS};

#[test]
fn a_character_the_panel_cannot_draw_becomes_one_question_mark() {
    let s = Screen::new("caf\u{e9}", "a \u{2014} b \u{2019}c");
    assert_eq!(s.top(), "caf?");
    assert_eq!(s.bottom(), "a ? b ?c");
}

#[test]
fn the_two_ascii_characters_the_rom_gets_wrong_are_replaced_too() {
    // A00 draws a yen sign for a backslash and an arrow for a tilde.
    assert_eq!(drawable('\\'), '?');
    assert_eq!(drawable('~'), '?');
    // Control characters have no glyph at all.
    assert_eq!(drawable('\n'), '?');
    assert_eq!(drawable('\t'), '?');
}

#[test]
fn printable_ascii_is_left_alone() {
    let plain = "Present card 0:12:34 ACCESS DENIED {x} | 192.168.1.50";
    assert!(plain.chars().all(|c| drawable(c) == c));
}

#[test]
fn a_line_full_of_them_is_still_sixteen_columns_and_sixteen_bytes() {
    // Sixteen columns is sixteen bytes once everything is ASCII, which is what
    // keeps a line from spilling past the panel's edge.
    let s = denied("\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}");
    assert_eq!(s.bottom().chars().count(), COLS);
    assert_eq!(
        s.bottom().len(),
        COLS,
        "a line is longer in bytes than columns"
    );
    assert!(s.bottom().is_ascii());
}
