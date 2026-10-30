//! What the setup page's words mean.
//!
//! Every setting is typed by a person into a text field, so what a typo, a
//! blank or an odd spelling reads as is a decision about safety: a relay that a
//! typo turns on, or a sensor a blank field claims, is a box that does the
//! wrong thing quietly.

use std::collections::HashMap;

use toolguard_core::settings::key;
use toolguard_core::Settings;

/// Settings read from exactly these values, and nothing else set.
fn with(values: &[(&str, &str)]) -> Settings {
    let map: HashMap<String, String> = values
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    Settings::read(|k| map.get(k).cloned())
}

#[test]
fn nothing_set_is_a_plain_badge_reader() {
    let s = with(&[]);
    assert!(s.config.reads_cards);
    assert!(!s.config.gates_power);
    assert!(!s.config.senses_running);
    assert!(!s.config.drives_latch);
    assert!(s.config.reports_usage);
    assert!(
        s.config.stop_on_second_tap,
        "a reader with no relay has no other stop"
    );
    assert!(s.sense_active_low);
    assert!(!s.stop_button);
    assert_eq!(s.config.use_hold_ms, 10_000);
    assert_eq!(s.config.use_assert_ms, 100);
    assert_eq!(s.config.idle_end_ms, 0);
    assert_eq!(s.config.max_session_ms, 0);
    assert_eq!(s.config.usage_interval_ms, 30_000);
    assert_eq!(s.config.first_lease_timeout_ms, 5_000);
    assert_eq!(s.roles(), vec!["reader"]);
    assert!(s.warnings.is_empty());
}

#[test]
fn every_spelling_of_yes_and_no() {
    for yes in ["yes", "y", "true", "t", "1", "on", "YES", " Yes ", "On"] {
        assert!(
            with(&[(key::GATES_POWER, yes)]).config.gates_power,
            "{yes:?} was not read as yes"
        );
    }
    for no in ["no", "n", "false", "f", "0", "off", "NO", " No "] {
        assert!(
            !with(&[(key::READS_CARDS, no)]).config.reads_cards,
            "{no:?} was not read as no"
        );
    }
}

#[test]
fn anything_else_is_the_default_and_never_false() {
    // A typo in "Has a card reader" must not turn the reader off, and a typo
    // in "Switches the tool" must not claim a relay.
    for junk in ["", "  ", "yse", "maybe", "2"] {
        let s = with(&[(key::READS_CARDS, junk), (key::GATES_POWER, junk)]);
        assert!(s.config.reads_cards, "{junk:?} turned the reader off");
        assert!(!s.config.gates_power, "{junk:?} claimed a relay");
    }
}

#[test]
fn a_typo_is_reported_and_a_blank_is_not() {
    let s = with(&[(key::GATES_POWER, "yse"), (key::SENSES_RUN, "")]);
    assert_eq!(s.warnings.len(), 1, "{:?}", s.warnings);
    assert!(s.warnings[0].contains(key::GATES_POWER), "{:?}", s.warnings);
}

#[test]
fn numbers_fall_back_to_their_default_when_they_are_not_numbers() {
    let s = with(&[
        (key::MAX_SESS_S, ""),
        (key::IDLE_END_S, "ten"),
        (key::USE_ASSERT, "-5"),
        (key::USAGE_INT_S, "99999999999999999999999"),
        (key::LEASE_WAIT_S, " 12 "),
    ]);
    assert_eq!(s.config.max_session_ms, 0);
    assert_eq!(s.config.idle_end_ms, 0);
    assert_eq!(s.config.use_assert_ms, 100);
    assert_eq!(
        s.config.usage_interval_ms, 30_000,
        "an overflow is not a number"
    );
    assert_eq!(s.config.first_lease_timeout_ms, 12_000);
    assert_eq!(s.warnings.len(), 3, "{:?}", s.warnings);
}

#[test]
fn a_huge_number_saturates_and_does_not_wrap() {
    // Seconds to milliseconds can overflow; a hold that wrapped round to a
    // small number would break one engrave into hundreds of fragments.
    let s = with(&[
        (key::MAX_SESS_S, &u64::MAX.to_string()),
        (key::USE_HOLD_S, "5000000"),
        (key::USE_ASSERT, "5000000000"),
    ]);
    assert_eq!(s.config.max_session_ms, u64::MAX);
    assert_eq!(
        s.config.use_hold_ms,
        u32::MAX,
        "5,000,000 s of hold wrapped"
    );
    assert_eq!(s.config.use_assert_ms, u32::MAX);
}

/// Settings as typed, and the roles they register.
type Row<'a> = (&'a [(&'a str, &'a str)], &'a [&'a str]);

#[test]
fn the_roles_for_every_row_of_the_installing_table() {
    // docs/INSTALLING.md §6, "registers as".
    let rows: &[Row] = &[
        (&[], &["reader"]),
        (&[(key::GATES_POWER, "yes")], &["reader", "power"]),
        (&[(key::SENSES_RUN, "yes")], &["reader", "sensor"]),
        (
            &[(key::GATES_POWER, "yes"), (key::SENSES_RUN, "yes")],
            &["reader", "power", "sensor"],
        ),
        (
            &[(key::READS_CARDS, "no"), (key::GATES_POWER, "yes")],
            &["power"],
        ),
        (&[(key::DRIVES_LATCH, "yes")], &["reader", "strike"]),
        (
            &[(key::GATES_POWER, "yes"), (key::DRIVES_LATCH, "yes")],
            &["reader", "power", "strike"],
        ),
    ];
    for (values, roles) in rows {
        assert_eq!(&with(values).roles(), roles, "for {values:?}");
    }
}

#[test]
fn a_second_tap_ends_a_session_by_default_only_where_nothing_else_can() {
    assert!(with(&[]).config.stop_on_second_tap);
    assert!(!with(&[(key::GATES_POWER, "yes")]).config.stop_on_second_tap);
    // Set explicitly, it wins either way.
    assert!(
        with(&[(key::GATES_POWER, "yes"), (key::STOP_2TAP, "yes")])
            .config
            .stop_on_second_tap
    );
    assert!(!with(&[(key::STOP_2TAP, "no")]).config.stop_on_second_tap);
}

#[test]
fn every_key_fits_the_store() {
    // NVS keys are at most fifteen characters; a longer one fails to save.
    for k in [
        key::READS_CARDS,
        key::GATES_POWER,
        key::SENSES_RUN,
        key::SENSE_LOW,
        key::USE_HOLD_S,
        key::USE_ASSERT,
        key::STOP_2TAP,
        key::IDLE_END_S,
        key::MAX_SESS_S,
        key::USAGE_INT_S,
        key::STOP_BTN,
        key::REPORTS_USAGE,
        key::LEASE_WAIT_S,
        key::DRIVES_LATCH,
    ] {
        assert!(k.len() <= 15, "{k} is {} characters", k.len());
    }
}

// ------------------------------- what the platform has, against the settings

use toolguard_core::settings::{check_roles, roles_record, RolesCheck};

#[test]
fn the_same_roles_in_any_order_are_the_same() {
    assert_eq!(roles_record(&["reader", "power"]), "power,reader");
    assert_eq!(roles_record(&["power", "reader"]), "power,reader");
    assert_eq!(
        check_roles(Some("power,reader"), &["reader", "power"]),
        RolesCheck::Same
    );
    // A record in another order, or with stray spaces, still matches.
    assert_eq!(
        check_roles(Some(" reader , power"), &["power", "reader"]),
        RolesCheck::Same
    );
}

#[test]
fn turning_a_role_on_after_registration_is_a_change() {
    // The example in #6: a reader that is now told it switches the tool. The
    // platform still has it as a reader, so it can never be leased.
    let registered = roles_record(&with(&[]).roles());
    let now = with(&[(key::GATES_POWER, "yes")]).roles();
    assert_eq!(
        check_roles(Some(&registered), &now),
        RolesCheck::Changed {
            registered: "reader".into(),
            now: "power,reader".into(),
        }
    );
}

#[test]
fn turning_a_role_off_is_a_change_too() {
    assert!(matches!(
        check_roles(Some("power,reader,sensor"), &["reader", "power"]),
        RolesCheck::Changed { .. }
    ));
}

#[test]
fn a_box_registered_before_the_record_existed_is_unknown_not_a_change() {
    // An upgraded box: nothing stored. Calling that a change would put every
    // deployed box's panel into a warning at its first boot on this firmware.
    assert_eq!(check_roles(None, &["reader"]), RolesCheck::Unknown);
}

#[test]
fn the_record_key_fits_the_store() {
    assert!(key::REG_ROLES.len() <= 15);
}
