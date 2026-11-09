//! Whether the machine is working, from what its pins said each pass.
//!
//! The firmware reads its pins once a pass, every 75 ms, with an interrupt
//! latching any pulse in between. These tests play that: each pass is one
//! `Reading`, built from what a signal did during the 75 ms before it.

use spaces_device::wire::topic;
use toolguard_core::sense::{Powered, Reading, Sense, Source};
use toolguard_core::{Config, Health, Reader};

const ME: &str = "dev-uuid-me";
const TOOL: &str = "laser-01";
const ALICE: &str = "04A1B2C3";
const PASS: u64 = 75;

/// A reader with no relay that senses the machine by `source`, signed in by
/// Alice at 100 and 200, with the default blip and gap filters.
fn signed_in(source: Source, powered: Powered) -> Reader {
    let mut config = Config::switches_nothing(ME, TOOL);
    config.senses_running = source.senses();
    config.use_assert_ms = 100;
    config.use_hold_ms = 10_000;
    let mut r = Reader::new(config).sensing(source, powered);
    r.start(0);
    r.health(
        Health {
            wifi: true,
            broker: true,
            address: Some("192.168.1.50".into()),
            clock: None,
        },
        0,
    );
    r.card(ALICE, 100);
    r.message(
        topic::SCAN_RESPONSE,
        &serde_json::json!({"resource": TOOL, "authorized": true, "reason": "ok"})
            .to_string()
            .into_bytes(),
        200,
    );
    r
}

/// Whether the panel says the machine is working.
fn in_use(r: &Reader) -> bool {
    r.screen().expect("a screen").bottom().starts_with("In use")
}

/// Feed a signal pass by pass from `from` to `to`. `asserted(t)` says whether
/// the line is asserted at millisecond `t`; a pass sees the level at its own
/// instant, and a pulse if the line was asserted at any millisecond since the
/// last pass.
fn play(r: &mut Reader, from: u64, to: u64, asserted: impl Fn(u64) -> bool, powered: Option<bool>) {
    let mut t = from;
    while t < to {
        let next = t + PASS;
        let reading = Reading {
            active: asserted(next),
            pulsed: (t + 1..=next).any(&asserted),
            powered,
        };
        r.sense(reading, next);
        t = next;
    }
}

/// An engrave: 2 ms pulses every 25 ms, none of them landing on the 75 ms
/// instants at which a pass reads the level (every pass here starts on a
/// multiple of 25).
fn engraving(t: u64) -> bool {
    t % 25 == 10 || t % 25 == 11
}

// --------------------------------------------------------------- #9: pulses

#[test]
fn an_engrave_of_short_pulses_counts_as_working_from_its_first_pulses() {
    let mut r = signed_in(Source::Pulses, Powered::Always);
    // Past the outcome screen first, so the panel shows the meter.
    play(&mut r, 200, 5_000, |_| false, None);
    assert!(!in_use(&r));
    play(&mut r, 5_000, 5_300, engraving, None);
    assert!(
        in_use(&r),
        "300 ms of 2 ms pulses did not count: {:?}",
        r.screen()
    );
}

#[test]
fn read_as_a_level_the_same_engrave_is_never_seen() {
    // The defect in #9: once a pass, at the wrong instants, every time.
    let mut r = signed_in(Source::Level, Powered::Always);
    play(&mut r, 200, 5_000, |_| false, None);
    play(&mut r, 5_000, 65_000, engraving, None);
    assert!(!in_use(&r), "the level read a 2 ms pulse train after all");
}

#[test]
fn one_short_glitch_every_few_seconds_is_not_work() {
    // Noise next to a laser's HV supply: a 1 ms spike every 3 s. One pass sees
    // it, which is 75 ms of "working", under the 100 ms blip filter.
    let mut r = signed_in(Source::Pulses, Powered::Always);
    play(&mut r, 200, 5_000, |_| false, None);
    play(&mut r, 5_000, 65_000, |t| t % 3_000 == 7, None);
    assert!(!in_use(&r), "glitches were billed as work");
}

#[test]
fn a_held_level_counts_with_either_source() {
    for source in [Source::Level, Source::Pulses] {
        let mut r = signed_in(source, Powered::Always);
        play(&mut r, 200, 5_000, |_| false, None);
        play(&mut r, 5_000, 6_000, |_| true, None);
        assert!(in_use(&r), "{source:?} did not count a held level");
    }
}

// -------------------------------------------- #8: off reads the same as firing

#[test]
fn a_machine_that_is_off_when_the_session_opens_is_not_billed_under_seen_idle() {
    // L-ON1 is ~0 V with the machine off, which is asserted. Nothing about the
    // line alone says the machine is off.
    let mut r = signed_in(Source::Pulses, Powered::SeenIdle);
    play(&mut r, 200, 60_000, |_| true, None);
    assert!(!in_use(&r), "a switched-off laser was billed");

    // Switched on: the line goes to 4.4 V, idle. From then on firing counts.
    play(&mut r, 60_000, 61_000, |_| false, None);
    assert!(!in_use(&r));
    play(&mut r, 61_000, 61_300, engraving, None);
    assert!(in_use(&r), "firing after the machine came on did not count");
}

#[test]
fn without_a_qualifier_the_same_switched_off_laser_is_billed() {
    // What #8 is about: the default, and the reason the qualifier exists.
    let mut r = signed_in(Source::Pulses, Powered::Always);
    play(&mut r, 200, 60_000, |_| true, None);
    assert!(in_use(&r));
}

#[test]
fn seen_idle_starts_again_with_every_session() {
    let mut s = Sense::new(Source::Level, Powered::SeenIdle);
    let idle = Reading::default();
    let on = Reading {
        active: true,
        ..Reading::default()
    };
    // A session in which the machine was on and fired.
    assert!(!s.working(idle, true));
    assert!(s.working(on, true));
    // Closed with the line asserted (switched off), and a new one opened: the
    // first session's idle reading does not carry over.
    s.working(on, false);
    assert!(
        !s.working(on, true),
        "the last session's idle armed this one"
    );
}

#[test]
fn a_powered_input_decides_whatever_the_line_does() {
    let mut r = signed_in(Source::Pulses, Powered::Input);
    play(&mut r, 200, 60_000, |_| true, Some(false));
    assert!(!in_use(&r), "billed while the powered input said off");
    play(&mut r, 60_000, 61_000, |_| true, Some(true));
    assert!(in_use(&r), "not billed while powered and asserted");
}

#[test]
fn a_powered_input_that_says_nothing_is_off() {
    // No reading at all must fail towards billing nobody.
    let mut s = Sense::new(Source::Level, Powered::Input);
    let reading = Reading {
        active: true,
        pulsed: true,
        powered: None,
    };
    assert!(!s.working(reading, true));
}

// --------------------------------------------------- sources that read nothing

#[test]
fn none_and_relay_never_read_the_input() {
    for source in [Source::None, Source::Relay] {
        let mut s = Sense::new(source, Powered::Always);
        let busy = Reading {
            active: true,
            pulsed: true,
            powered: Some(true),
        };
        assert!(!s.working(busy, true), "{source:?} read the input");
        assert!(!source.senses());
    }
}
