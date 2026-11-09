//! The badge box as a `Device`: what spaces-firmware's loop relies on (#18).

use spaces_device::wire::topic;
use spaces_device::{Device, Effect, Link, Output};
use toolguard_core::reader::{Action, Reader};
use toolguard_core::screen::Health;
use toolguard_core::{Config, Input};

const ME: &str = "dev-uuid-me";
const TOOL: &str = "laser-01";

fn started() -> Reader {
    let mut r = Reader::new(Config::switches_nothing(ME, TOOL));
    Device::start(&mut r, 0);
    r.health(
        Health {
            wifi: false,
            broker: false,
            address: Some("192.168.1.50".into()),
            clock: None,
        },
        0,
    );
    r
}

fn up() -> Link {
    Link {
        wifi: true,
        broker: true,
    }
}

#[test]
fn the_link_updates_wifi_and_broker_and_keeps_the_address() {
    // The loop knows the link and not the address; a link change must not
    // wipe what the panel's diagnostic screen shows someone with a laptop.
    let mut r = started();
    Device::link(&mut r, up(), 10);
    assert!(r.ready());
    let mut r2 = started();
    Device::link(
        &mut r2,
        Link {
            wifi: false,
            broker: false,
        },
        10,
    );
    let screen = r2.screen().expect("a screen");
    assert_eq!(screen.top(), "No Wi-Fi");
    assert_eq!(
        screen.bottom(),
        "192.168.1.50",
        "the address was lost: {screen:?}"
    );
}

#[test]
fn a_card_goes_to_the_edge_once_the_link_is_up() {
    let mut r = started();
    Device::link(&mut r, up(), 10);
    let out = r.input(Input::Card("04A1B2C3".into()), 20);
    assert!(out
        .iter()
        .any(|a| matches!(a, Action::Publish { topic, .. } if *topic == topic::SCAN_REQUEST)));
    assert!(Device::in_session(&r), "asking holds an update off");
}

#[test]
fn publishes_and_notes_are_the_loops_and_the_rest_is_the_boxs() {
    assert!(matches!(
        Reader::effect(Action::Publish {
            topic: topic::SCAN_REQUEST,
            payload: "{}".into()
        }),
        Ok(Effect::Publish { .. })
    ));
    assert_eq!(
        Reader::effect(Action::Note("hi".into())),
        Ok(Effect::Note("hi".into()))
    );
    let set = Action::Set {
        output: Output::Tool,
        on: true,
    };
    assert_eq!(Reader::effect(set.clone()), Err(set));
}

#[test]
fn the_clock_reaches_the_resting_screen() {
    let mut r = started();
    Device::link(&mut r, up(), 10);
    r.input(Input::Clock(Some("14:05".into())), 20);
    assert_eq!(r.screen().expect("a screen").bottom(), "14:05");
}
