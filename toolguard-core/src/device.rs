//! The badge box as a [`Device`], so spaces-firmware's loop runs it (#18).
//!
//! This box used to run its own copy of the loop (`toolguard-fw`'s
//! `broker.rs`), because the library's `run()` drove only a `Module`. Its copy
//! then missed what the library's loop did besides -- holding an update during
//! a session, the BROKER milestone, Wi-Fi state -- and each was patched back in
//! by hand (#1, #2, #5). With the [`Device`] trait (spaces-firmware #8) the
//! library's loop drives this box too, and does all of that once, for every
//! device.

use spaces_device::wire::topic;
use spaces_device::{Device, Effect, Link};

use crate::reader::{Action, Reader};
use crate::sense::Reading;

/// What the box's hardware and clock tell it.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    /// A card in the field, by UID.
    Card(String),
    /// The stop button, pressed.
    Stop,
    /// What the sense pins said this pass. Sent every pass.
    Sense(Reading),
    /// Something the box needs somebody to know, for the resting screen, or
    /// `None` to clear it.
    Notice(Option<String>),
    /// The wall clock as `HH:MM`, once SNTP has answered.
    Clock(Option<String>),
}

impl Device for Reader {
    type Action = Action;
    type Input = Input;

    fn subscriptions(&self) -> &'static [&'static str] {
        topic::SUBSCRIPTIONS
    }

    fn start(&mut self, now: u64) -> Vec<Action> {
        Reader::start(self, now)
    }

    fn input(&mut self, input: Input, now: u64) -> Vec<Action> {
        match input {
            Input::Card(uid) => self.card(&uid, now),
            Input::Stop => self.tool_off(now),
            Input::Sense(reading) => self.sense(reading, now),
            Input::Notice(notice) => self.notice(notice.as_deref(), now),
            Input::Clock(clock) => self.clock(clock, now),
        }
    }

    fn message(&mut self, topic: &str, payload: &[u8], now: u64) -> Vec<Action> {
        Reader::message(self, topic, payload, now)
    }

    fn link(&mut self, link: Link, now: u64) -> Vec<Action> {
        Reader::link(self, link.wifi, link.broker, now)
    }

    fn tick(&mut self, now: u64) -> Vec<Action> {
        Reader::tick(self, now)
    }

    fn in_session(&self) -> bool {
        Reader::in_session(self)
    }

    fn effect(action: Action) -> Result<Effect, Action> {
        match action {
            Action::Publish { topic, payload } => Ok(Effect::Publish { topic, payload }),
            Action::Note(text) => Ok(Effect::Note(text)),
            other => Err(other),
        }
    }
}
