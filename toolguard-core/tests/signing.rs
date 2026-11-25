//! A reader built around a signing module keeps it signing (CS:S #249).
//!
//! What sealing *means* -- freshness, replay, which device a seal names -- is
//! `spaces_device::Module`'s, and pinned by spaces-firmware's own
//! `tests/signing.rs`. What this crate decides is only that
//! [`Reader::with_module`] keeps the module it is given: that a card goes out
//! sealed, and that the panel says ACCESS GRANTED for a sealed yes and not for
//! an unsigned one.
//!
//! WHAT THIS DOES NOT PROVE: that the firmware builds its reader this way.
//! `toolguard-fw/src/main.rs` calling `Online::module` is board code, compiled
//! by CI's chip jobs and exercised by nothing on the host.

use css_wire_sign::{Peer, Refused};
use spaces_device::wire::{LocalHello, LocalScan, LocalScanReply};
use spaces_device::{wire::topic, Module};
use toolguard_core::reader::{Action, Reader};
use toolguard_core::screen::Health;
use toolguard_core::Config;

const ME: &str = "dev-uuid-me";
const TOOL: &str = "laser-01";
const ALICE: &str = "04A1B2C3";
const KEY: &str = "4f3c-this-devices-command-key";

fn signing_reader() -> Reader {
    let mut n = 0;
    let module = Module::new(Config::switches_nothing(ME, TOOL)).signing(KEY, move || {
        n += 1;
        format!("mod-{n}")
    });
    let mut r = Reader::with_module(module);
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
    r
}

fn scan_request(actions: &[Action]) -> LocalScan {
    let payloads: Vec<&String> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Publish { topic, payload } if *topic == topic::SCAN_REQUEST => Some(payload),
            _ => None,
        })
        .collect();
    assert_eq!(payloads.len(), 1, "one scan request per card: {actions:?}");
    serde_json::from_str(payloads[0]).expect("the scan request parses")
}

fn granted(actions: &[Action]) -> bool {
    actions
        .iter()
        .any(|a| matches!(a, Action::Show(s) if s.top() == "ACCESS GRANTED"))
}

fn reply(authorized: bool) -> LocalScanReply {
    LocalScanReply {
        resource: TOOL.into(),
        authorized,
        reason: "authorized".into(),
        strike: None,
        member: None,
        sign_in: None,
        device_id: Some(ME.into()),
        seal: None,
    }
}

#[test]
fn a_signing_reader_seals_its_card_and_believes_only_a_sealed_yes() {
    let mut edge = Peer::new("edge-1".to_string());
    let mut r = signing_reader();

    // The card goes out sealed with this box's key. The edge has never heard
    // this box, so the seal names no nonce of the edge's: stale, but signed,
    // and it teaches the edge this box's nonce.
    let scan = scan_request(&r.card(ALICE, 100));
    assert!(scan.seal.is_some(), "the card went out unsigned: {scan:?}");
    assert_eq!(
        edge.open(KEY.as_bytes(), &scan, || "edge-2".to_string()),
        Err(Refused::Stale)
    );

    // The edge's hello introduces it, and the box asks again -- this time
    // fresh, so the edge opens it.
    let mut hello = LocalHello {
        device_id: ME.into(),
        seal: None,
    };
    edge.seal(KEY.as_bytes(), &mut hello);
    let again = r.message(topic::HELLO, &serde_json::to_vec(&hello).unwrap(), 120);
    let scan = scan_request(&again);
    edge.open(KEY.as_bytes(), &scan, || "edge-3".to_string())
        .expect("the edge opens the card asked again");

    // An unsigned yes -- anyone on the broker can publish one -- is not a yes.
    let forged = serde_json::to_vec(&reply(true)).unwrap();
    let out = r.message(topic::SCAN_RESPONSE, &forged, 150);
    assert!(!granted(&out), "an unsigned yes opened the tool: {out:?}");

    // The edge's own sealed yes is.
    let mut yes = reply(true);
    edge.seal(KEY.as_bytes(), &mut yes);
    let out = r.message(
        topic::SCAN_RESPONSE,
        &serde_json::to_vec(&yes).unwrap(),
        200,
    );
    assert!(
        granted(&out),
        "the edge's sealed yes was not believed: {out:?}"
    );
}
