//! What this box is, as an administrator sets it: the setup page's fields.
//!
//! What the values mean -- the defaults, the yes/no spellings, the roles they
//! add up to -- is decided in `toolguard_core::settings`, where it is tested.
//! This holds the fields the setup page renders, and reads the store.
//!
//! **Every field here is `required: false`, and that is load-bearing.**
//! `Store::missing` puts a device into the setup portal and keeps it there
//! until every required field is filled, so adding a required field to this
//! firmware would drop every already-deployed board into its captive portal on
//! the next boot — a fleet of readers waiting for somebody with a phone. A new
//! setting must therefore have a sane default in code, which is also the right
//! answer for an upgrade: a box that was working keeps working.

use spaces_device_espidf::store::{Field, Store};
use toolguard_core::settings::key;
pub use toolguard_core::Settings;

/// The settings this firmware adds to the shared set, in the order they are
/// rendered.
pub const FIELDS: &[Field] = &[
    Field {
        key: key::READS_CARDS,
        label: "Has a card reader",
        hint: "yes (default) if an RC522 is fitted. no makes this a plug: it switches the \
               tool on the edge's lease and never reads a badge",
        secret: false,
        required: false,
    },
    Field {
        key: key::GATES_POWER,
        label: "Switches the tool",
        hint: "no (default) if this box only reads badges and shows answers. yes if it \
               drives a relay -- it must then be bound to the tool in the power role, or \
               the edge will never lease it",
        secret: false,
        required: false,
    },
    Field {
        key: key::SENSES_RUN,
        label: "Senses the machine",
        hint: "no (default). yes if an input is wired to something that says the machine \
               is working. Leave it no when nothing is: a device that reports zero working \
               time is believed, and on a tool billed by working time that bills nothing",
        secret: false,
        required: false,
    },
    Field {
        key: key::SENSE_LOW,
        label: "Sense is active low",
        hint: "yes (default) -- a Ruida laser's L-ON1 pulls low to enable, and most dry \
               contacts close to ground. Check the machine: on a Ruida this is the \
               'Laser Output Signal' vendor setting and it can be either way round",
        secret: false,
        required: false,
    },
    Field {
        key: key::USE_HOLD_S,
        label: "Bridge gaps (seconds)",
        hint: "10 (default). How long the machine may pause and still count as working. A \
               laser's enable line drops on every scan-line return, so 0 here meters one \
               engrave as hundreds of fragments. 0 measures beam-on time instead",
        secret: false,
        required: false,
    },
    Field {
        key: key::USE_ASSERT,
        label: "Ignore blips under (ms)",
        hint: "100 (default). The signal must hold this long before it counts. Rejects \
               noise, which matters next to a laser's HV supply",
        secret: false,
        required: false,
    },
    Field {
        key: key::STOP_2TAP,
        label: "Tap again to finish",
        hint: "yes (default) when this box does not switch the tool. The member's own card \
               ends their session -- on a box with no relay and no sensor it is the only \
               stop there is, and without it the tool stays in use after they leave",
        secret: false,
        required: false,
    },
    Field {
        key: key::IDLE_END_S,
        label: "Finish after idle (s)",
        hint: "0 (default, never). End the session when the machine has not worked for this \
               long. Needs a sensor; ignored without one",
        secret: false,
        required: false,
    },
    Field {
        key: key::MAX_SESS_S,
        label: "Longest session (s)",
        hint: "0 (default, none). A backstop for a member who walks away without tapping \
               off. Worth setting on any box that does not switch the tool",
        secret: false,
        required: false,
    },
    Field {
        key: key::STOP_BTN,
        label: "Has a stop button",
        hint: "no (default). yes if a momentary switch is wired between the stop pin and \
               GND. Pressing it ends the session -- usage reported, tool released, relay \
               open if this box switches the tool. Anyone may press it; it is not an \
               emergency stop",
        secret: false,
        required: false,
    },
    Field {
        key: key::DRIVES_LATCH,
        label: "Drives a latch",
        hint: "no (default). yes if a door strike's driver is wired to the strike pin. On a \
               yes the pin is driven for exactly the pulse the platform names -- the \
               resource's strike binding, set in Resource Wiring -- then released; this \
               box then registers in the strike role. A box that drives a latch and \
               switches nothing is a door box: its session is the pulse. Not an egress \
               or fire-safety device",
        secret: false,
        required: false,
    },
    Field {
        key: key::REPORTS_USAGE,
        label: "Reports usage",
        hint: "yes (default). no for a box that must not meter: a second plug on a tool \
               another device already meters -- an extractor that follows the laser -- \
               would count the same minutes twice. Everything else is unchanged",
        secret: false,
        required: false,
    },
    Field {
        key: key::LEASE_WAIT_S,
        label: "Wait for lease up to (s)",
        hint: "5 (default). After a yes, how long a box that switches the tool waits for its \
               first lease before giving up. Must exceed the tool's whole start sequence: a \
               stage behind another plug gets no lease until that plug has confirmed it is \
               on and its start delay has run. Ignored by a box that switches nothing",
        secret: false,
        required: false,
    },
    Field {
        key: key::USAGE_INT_S,
        label: "Report usage every (s)",
        hint: "30 (default). Usage is sent in increments, so a reboot mid-session costs one \
               interval rather than the whole session. 0 reports once at the end",
        secret: false,
        required: false,
    },
];

/// Read the settings from the store, logging anything that could not be read.
pub fn read(store: &Store) -> Settings {
    let settings = Settings::read(|k| store.get(k));
    for w in &settings.warnings {
        log::warn!("{w}");
    }
    settings
}
