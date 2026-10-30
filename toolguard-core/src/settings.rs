//! What this box is, as an administrator sets it: the settings read into a
//! [`Config`], and the roles the box registers with.
//!
//! The roles are independent — read cards, switch power, sense the machine,
//! drive a door's latch — and a ToolGuard may fill any combination. None of that
//! is a build-time choice: one image is flashed and the same board becomes a
//! reader beside a laser, a reader that also switches it, a plug with no reader
//! at all, or a door box (CS:S #182: a door, a tool and a room are the same
//! resource with different bindings).
//!
//! Here rather than in the firmware so that it is tested: what a typo reads as,
//! what a blank field defaults to, and which roles a combination registers are
//! all decided from strings, and none of that needs a board. The firmware
//! keeps the setup page's fields and the store, and hands this a lookup.
//!
//! **Booleans are typed as words** because the setup page has only text and
//! password inputs. The spellings a person actually uses are accepted rather
//! than one blessed form, and anything unrecognised reads as the default rather
//! than as `false` — a typo must not silently disable a safety-relevant output.
//!
//! The timings are seconds where a person would say seconds, and milliseconds
//! only for the glitch filter, where a person would not.

use spaces_device::Config;

/// Store keys. **At most fifteen characters each**, which is the NVS limit and
/// the reason some of these are abbreviated.
pub mod key {
    pub const READS_CARDS: &str = "reads_cards";
    pub const GATES_POWER: &str = "gates_power";
    pub const SENSES_RUN: &str = "senses_run";
    pub const SENSE_LOW: &str = "sense_low";
    pub const USE_HOLD_S: &str = "use_hold_s";
    pub const USE_ASSERT: &str = "use_asrt_ms";
    pub const STOP_2TAP: &str = "stop_2tap";
    pub const IDLE_END_S: &str = "idle_end_s";
    pub const MAX_SESS_S: &str = "max_sess_s";
    pub const USAGE_INT_S: &str = "usage_int_s";
    pub const STOP_BTN: &str = "stop_button";
    pub const REPORTS_USAGE: &str = "report_usage";
    pub const LEASE_WAIT_S: &str = "lease_wait_s";
    pub const DRIVES_LATCH: &str = "drives_latch";
}

/// How this box is configured, read once at boot.
#[derive(Debug, Clone)]
pub struct Settings {
    pub config: Config,
    /// Whether the sensed input reads the machine as working when LOW.
    ///
    /// Applied in the firmware, where the pin is read, and nowhere else.
    /// `Module::running` takes a *semantic* bool, so exactly one inversion
    /// happens in the whole firmware and it is that one.
    pub sense_active_low: bool,
    /// Whether a stop button is wired to the `stop` pin.
    ///
    /// Momentary, to GND, read with the internal pull-up; so there is no
    /// polarity to configure, and an unconnected pin reads as "not pressed".
    pub stop_button: bool,
    /// What could not be read, and what was used instead, for the log.
    pub warnings: Vec<String>,
}

impl Settings {
    /// Read the settings through `get`, falling back to a plain badge reader.
    ///
    /// The ids are left empty for the caller to fill: [`Self::roles`] is needed
    /// *before* registration, because it is what the device registers as, and
    /// `device_id` does not exist until registration has happened. Reading the
    /// store twice to break that circle would mean two places that decide what
    /// this box is.
    pub fn read(get: impl Fn(&str) -> Option<String>) -> Self {
        let mut r = Reading {
            get: &get,
            warnings: Vec::new(),
        };
        let reads_cards = r.flag(key::READS_CARDS, true);
        let gates_power = r.flag(key::GATES_POWER, false);
        let senses_running = r.flag(key::SENSES_RUN, false);

        let mut config = Config::new("", "");
        config.reads_cards = reads_cards;
        config.gates_power = gates_power;
        config.senses_running = senses_running;
        config.use_hold_ms = clamp_u32(r.secs_ms(key::USE_HOLD_S, 10));
        config.use_assert_ms = clamp_u32(r.number(key::USE_ASSERT, 100));
        config.idle_end_ms = r.secs_ms(key::IDLE_END_S, 0);
        config.max_session_ms = r.secs_ms(key::MAX_SESS_S, 0);
        config.usage_interval_ms = r.secs_ms(key::USAGE_INT_S, 30);
        config.reports_usage = r.flag(key::REPORTS_USAGE, true);
        config.first_lease_timeout_ms = r.secs_ms(key::LEASE_WAIT_S, 5);
        config.drives_latch = r.flag(key::DRIVES_LATCH, false);
        // Defaults to on for a box that cannot stop a session any other way,
        // and off for one that can: where a lease lapses or a relay opens, a
        // re-tap meaning "done" would surprise somebody who simply swiped
        // twice.
        config.stop_on_second_tap = r.flag(key::STOP_2TAP, !gates_power);

        Settings {
            config,
            sense_active_low: r.flag(key::SENSE_LOW, true),
            stop_button: r.flag(key::STOP_BTN, false),
            warnings: r.warnings,
        }
    }

    /// The roles to register with, derived from the hardware rather than
    /// typed in twice.
    ///
    /// A device declares what it can do and an administrator binds it to a tool
    /// in a role it declared. Claiming `power` here when nothing is wired to a
    /// relay is how a reader with no relay becomes the thing a tool's safety
    /// rests on, and the mistake surfaces at the far end of the system as a
    /// machine that never comes on.
    pub fn roles(&self) -> Vec<&'static str> {
        let mut roles = Vec::new();
        if self.config.reads_cards {
            roles.push("reader");
        }
        if self.config.gates_power {
            roles.push("power");
        }
        if self.config.senses_running {
            roles.push("sensor");
        }
        if self.config.drives_latch {
            roles.push("strike");
        }
        roles
    }

    /// One line for the log, so a boot record says what this box thinks it is.
    ///
    /// Worth the line: almost every way this firmware can be misconfigured
    /// shows up as a tool that quietly never works, and the first question is
    /// always what the box believes about itself.
    pub fn describe(&self) -> String {
        format!(
            "reads_cards={} gates_power={} senses_running={} (active {}) \
             hold={}ms assert={}ms stop_2tap={} idle_end={}ms max_session={}ms usage_every={}ms \
             reports_usage={} lease_wait={}ms stop_button={} drives_latch={}",
            self.config.reads_cards,
            self.config.gates_power,
            self.config.senses_running,
            if self.sense_active_low { "low" } else { "high" },
            self.config.use_hold_ms,
            self.config.use_assert_ms,
            self.config.stop_on_second_tap,
            self.config.idle_end_ms,
            self.config.max_session_ms,
            self.config.usage_interval_ms,
            self.config.reports_usage,
            self.config.first_lease_timeout_ms,
            self.stop_button,
            self.config.drives_latch,
        )
    }
}

/// The lookup, and what it could not make sense of.
struct Reading<'a> {
    get: &'a dyn Fn(&str) -> Option<String>,
    warnings: Vec<String>,
}

impl Reading<'_> {
    /// A yes/no setting. Anything unrecognised, including empty, is `default`.
    fn flag(&mut self, key: &str, default: bool) -> bool {
        let Some(v) = (self.get)(key) else {
            return default;
        };
        match v.trim().to_ascii_lowercase().as_str() {
            "yes" | "y" | "true" | "t" | "1" | "on" => true,
            "no" | "n" | "false" | "f" | "0" | "off" => false,
            // Not `false`: a typo must not quietly turn an output off, or on.
            "" => default,
            other => {
                self.warnings.push(format!(
                    "{key}: {other:?} is not yes or no; using {default}"
                ));
                default
            }
        }
    }

    /// A whole number, or `default` if it is absent or not one.
    fn number(&mut self, key: &str, default: u64) -> u64 {
        let Some(v) = (self.get)(key) else {
            return default;
        };
        let v = v.trim();
        if v.is_empty() {
            return default;
        }
        match v.parse::<u64>() {
            Ok(n) => n,
            Err(_) => {
                self.warnings
                    .push(format!("{key}: {v:?} is not a number; using {default}"));
                default
            }
        }
    }

    /// A number of seconds, as milliseconds.
    fn secs_ms(&mut self, key: &str, default_secs: u64) -> u64 {
        self.number(key, default_secs).saturating_mul(1_000)
    }
}

/// Saturating, where `as` would wrap: a huge hold must not become a small one.
fn clamp_u32(n: u64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}
