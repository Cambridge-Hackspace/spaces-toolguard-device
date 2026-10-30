//! Whether the machine is working, from what is wired to tell.
//!
//! How a box decides that is a setting, because machines say it differently
//! (#10). The firmware reads its pins and hands over a [`Reading`] every pass;
//! this turns it into the one semantic bool `spaces_device::Module::running`
//! takes, and `RunFilter`'s *ignore blips* and *bridge gaps* stay downstream of
//! it, unchanged.
//!
//! ## The laser, which is why there is more than one way
//!
//! On the Hackspace laser's Ruida (RDC6442S-B(EC)) the signal is `L-ON1`,
//! active low, measured against GND:
//!
//! | machine | L-ON1 |
//! |---|---|
//! | off | ~0 V |
//! | on, not firing | 4.4 V |
//! | firing | pulses towards 0 V |
//!
//! Two consequences, each a source of wrong bills:
//!
//! - **Firing is pulses, not a level** (#9). During an engrave the line drops
//!   per dot and per scan line, for milliseconds. Read as a level once a pass,
//!   most of them are never seen. [`Source::Pulses`] counts a pass as working
//!   if the line pulsed at all since the last one, which the firmware latches
//!   by interrupt.
//! - **Off reads the same as firing** (#8), since the line's pull-up is powered
//!   by the machine. [`Powered`] qualifies the source: by a second input that
//!   says the machine is on, or, with nothing else to go on, by requiring the
//!   line to have read idle since the session opened.

/// What the box measures usage by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Nothing is wired. No working time is reported.
    None,
    /// The input's level: working while it is asserted.
    Level,
    /// Activity on the input: working while it is asserted, or if it pulsed
    /// since the last read. For a line that toggles while the machine works.
    Pulses,
    /// The time this box's own relay was on. What a box that switches the
    /// tool and senses nothing reports anyway; on one with no relay, nothing.
    Relay,
}

impl Source {
    /// Whether this source reads the sense input, and so is a `sensor`.
    pub fn senses(self) -> bool {
        matches!(self, Source::Level | Source::Pulses)
    }
}

/// What has to be true, besides the source, for the machine to be working.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Powered {
    /// Nothing: the source alone decides.
    Always,
    /// A second input says whether the machine is on.
    Input,
    /// The source must have read not-working since the session opened. For a
    /// signal that reads as working when the machine is off, and no other
    /// signal to tell.
    ///
    /// **It covers a machine that is off when the session opens, and not one
    /// switched off during it** -- after a session's first idle reading,
    /// nothing tells the two apart again. Prefer [`Powered::Input`], or wiring
    /// that reads idle when the machine is off.
    SeenIdle,
}

/// One pass's worth of what the pins said.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Reading {
    /// The sense input is asserted now, its polarity already applied.
    pub active: bool,
    /// It became asserted at least once since the last reading, however
    /// briefly.
    pub pulsed: bool,
    /// What the powered input says, if one is wired: `Some(true)` for on.
    pub powered: Option<bool>,
}

/// The state behind [`Powered::SeenIdle`], and the decision.
#[derive(Debug, Clone)]
pub struct Sense {
    source: Source,
    powered: Powered,
    /// Whether a session was open at the last reading, to see one open.
    was_open: bool,
    /// Whether the source has read not-working since the session opened.
    seen_idle: bool,
}

impl Sense {
    pub fn new(source: Source, powered: Powered) -> Self {
        Sense {
            source,
            powered,
            was_open: false,
            seen_idle: false,
        }
    }

    pub fn source(&self) -> Source {
        self.source
    }

    /// Whether the machine is working, for a box whose session is or is not
    /// `open`. `false` for a source that reads no input.
    pub fn working(&mut self, reading: Reading, open: bool) -> bool {
        let raw = match self.source {
            Source::Level => reading.active,
            Source::Pulses => reading.active || reading.pulsed,
            Source::None | Source::Relay => false,
        };
        if open && !self.was_open {
            // A new session: what the line did before it says nothing about
            // whether the machine is on now.
            self.seen_idle = false;
        }
        self.was_open = open;
        if !raw {
            self.seen_idle = true;
        }
        let on = match self.powered {
            Powered::Always => true,
            // Unknown is off: the failure that bills nobody.
            Powered::Input => reading.powered.unwrap_or(false),
            Powered::SeenIdle => self.seen_idle,
        };
        raw && on
    }
}
