//! A ToolGuard badge reader.
//!
//! Scan a card, ask the edge about it, show the answer. Every decision is in
//! `toolguard_core`; this is the shell that gives them hardware. Nothing in
//! this file names a GPIO — the pin table is in [`board`], per chip.
//!
//! ## What it reuses rather than rebuilds
//!
//! `spaces-device-espidf::boot` does everything that is not reader-specific,
//! and does it with scars worth inheriting: a setup portal that handles an
//! eight-emoji invite without length-checking or normalising it, a Wi-Fi join
//! that ends a failed attempt on the disconnect event instead of waiting out a
//! ten-second timeout, the NVS store, register-once-and-persist-before-anything
//! -else, the OTA endpoint with milestone rollback, and the web log.
//!
//! ## What is reader-specific
//!
//! The roles it declares, the one setting it adds (its tool), the peripherals,
//! and [`Hardware`]: what the core's decisions are carried out on. The loop
//! that runs the box is spaces-device-espidf's `run()`, the same one every
//! device runs (#18).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use esp_idf_svc::hal::gpio::{Input, InputPin, InterruptType, Output, PinDriver, Pull};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::{eventloop::EspSystemEventLoop, nvs::EspDefaultNvsPartition};
use spaces_device::input::Debounced;
use spaces_device_espidf::run::Event;
use spaces_device_espidf::status_light::{Status, StatusLight};
use spaces_device_espidf::store::{key, Field, Store};
use spaces_device_espidf::update::Updates;
use spaces_device_espidf::{boot, watch_setup_button, weblog, Firmware};
use toolguard_core::screen::Screen;
use toolguard_core::sense::{Powered, Reading, Source};
use toolguard_core::{Action, Reader};

mod board;
mod lcd;
mod rc522;
mod settings;

/// How often the antenna is asked whether a card is present.
///
/// 75 ms, which is what `nfc-522` settled on: fast enough that a tap by a
/// person moving at human speed is never missed, slow enough that the SPI bus
/// is mostly idle.
const POLL_MS: u64 = 75;

/// This device reads cards. It switches nothing.
///
/// `capabilities.roles` is the device's own claim about itself, and the server
/// lets an administrator bind it in any role it declares — so claiming `power`
/// here would let someone make a reader with no relay the thing that energizes
/// a machine, and the mistake surfaces at the far end of the system as a tool
/// that never comes on.
/// The tool this box guards. The rest of what it is lives in [`settings`],
/// which is read from the same store.
const TOOL_FIELD: &[Field] = &[Field {
    key: key::TOOL_ID,
    label: "Tool",
    hint: "the machine this reader guards: its external id on the platform, e.g. laser-01",
    secret: false,
    required: true,
}];

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    // Before any other logging call: this installs the tee that also captures
    // ESP-IDF's own C-side output, so `/log` holds the whole story.
    weblog::start();

    let peripherals = Peripherals::take()?;
    // Taken apart immediately, because the status light and the setup button are
    // wanted before `boot()` and the two peripherals after it, and
    // `peripherals.pins` cannot be moved from twice.
    // --- every pin this firmware uses, per chip -----------------------------
    //
    // Concrete pins rather than GPIO numbers: that is what esp-idf-hal's
    // generic bounds want, and it makes using one pin twice a COMPILE error
    // rather than a runtime surprise. Which matters -- an earlier draft of this
    // had the S3 using GPIO8 for both the status LED and I2C SDA, and only the
    // move checker would have caught it, on a chip nobody here can build.
    //
    // All of them in one tuple so the conflicts are visible in one place. The
    // reasoning per choice is in `board`.
    let pins = peripherals.pins;

    // C6 Super Mini: GPIO8 is the RGB LED and a strapping pin, GPIO9 is BOOT,
    // GPIO12/13 are native USB and the only console there is.
    #[cfg(esp32c6)]
    let (
        light_pin,
        setup_pin,
        sclk,
        mosi,
        miso,
        cs,
        rst,
        sda,
        scl,
        sense,
        relay,
        stop,
        strike,
        powered,
    ) = (
        pins.gpio8,
        pins.gpio9,
        pins.gpio0,
        pins.gpio1,
        pins.gpio2,
        pins.gpio3,
        pins.gpio18,
        pins.gpio22,
        pins.gpio23,
        // All four from the freely usable set (IO0-3, IO14, IO18-23), which
        // SPI and I2C leave 14, 19, 20 and 21 of -- and these take all four,
        // so a C6 Super Mini is full. None can do ADC: on the C6 that is
        // GPIO0-6 only, and SPI has all of those -- so a sensed input here is
        // digital, which is what an optoisolated enable line gives anyway. The
        // stop button is a switch to GND on the internal pull-up; the strike
        // is an output to a latch driver (CS:S #182), driven low at boot.
        pins.gpio14,
        pins.gpio19,
        pins.gpio20,
        pins.gpio21,
        // The powered input (#8): an edge pin on the Super Mini, and neither a
        // strapping pin nor one of the ADC pins (GPIO0-6) an analog source
        // would want.
        pins.gpio7,
    );

    // UNTESTED: no C3 here, so this compiles and nothing more. Same strapping
    // pair (8 and 9) as the C6; USB is on 18/19 rather than 12/13.
    #[cfg(esp32c3)]
    let (light_pin, setup_pin, sclk, mosi, miso, cs, rst, sda, scl, sense, relay, stop, strike) = (
        pins.gpio8,
        pins.gpio9,
        pins.gpio10,
        pins.gpio7,
        pins.gpio6,
        pins.gpio5,
        // rst is GPIO21, U0TXD, and the strike is not (#20). The console is
        // native USB on 18/19, but that does not free UART0's TX at boot: by
        // default (EFUSE_UART_PRINT_CONTROL = 0) the ROM prints its messages
        // on UART0 whatever the application's console is, and the datasheet
        // lists U0TXD as output-enabled with a pull-up after reset. A latch
        // driver there would pulse the door open on every power-up, crash
        // and update, before this code runs. The RC522's reset is the one
        // output here that does not care: toggling it during boot only
        // resets a reader the firmware resets again anyway.
        pins.gpio21,
        pins.gpio1,
        pins.gpio0,
        pins.gpio2,
        pins.gpio3,
        // GPIO20 is U0RXD: an input with a pull-up after reset, which the ROM
        // does not drive, so the stop button is safe there.
        pins.gpio20,
        // The strike is GPIO4 (MTMS): input-enabled with no pull at reset and
        // not driven by the ROM, so the driver's gate pull-down holds it off
        // until the firmware drives it low. Not GPIO6 (MTCK), which has a
        // pull-up after reset unless JTAG is disabled in eFuse.
        pins.gpio4,
    );
    // No powered input on the C3: every usable pin is taken. "Machine powered
    // by: input" falls back to seen-idle there, and says so.

    // UNTESTED: no C5 here, so this compiles and nothing more (#15). The C5's
    // strapping pair is 27 and 28 -- "GPIO27 = 0 and GPIO28 = 0 is invalid",
    // the C6's GPIO8/GPIO9 rule under new numbers -- and on the DevKitC-1
    // GPIO27 is the RGB LED and GPIO28 is BOOT, so they take the same two
    // roles they do on the C6. USB is 13/14, UART0 is 11/12 (and the DevKitC-1
    // has a USB-UART bridge on those, so an output there fights the bridge),
    // and 15-22 are the module's flash and PSRAM. What is left is exactly the
    // eleven pins the other roles need: 0, 1, 3, 4, 5, 6, 8, 9, 10, 23, 24.
    //
    // sense is GPIO1 on purpose: ADC1_CH0. This is the first board on which
    // the sensed input COULD be analog; it is digital today, like the others.
    // stop is GPIO3 (MTDI) because the DevKit guide calls it a strapping pin
    // and the IDF GPIO page does not -- an input on a pull-up never fights a
    // strap, so the doubt lands on the input and not on an output.
    #[cfg(esp32c5)]
    let (light_pin, setup_pin, sclk, mosi, miso, cs, rst, sda, scl, sense, relay, stop, strike) = (
        pins.gpio27,
        pins.gpio28,
        pins.gpio6,
        pins.gpio5,
        pins.gpio4,
        pins.gpio10,
        pins.gpio8,
        pins.gpio23,
        pins.gpio24,
        pins.gpio1,
        pins.gpio9,
        pins.gpio3,
        pins.gpio0,
    );
    // No powered input on the C5 either: the eleven pins above are the whole
    // budget on the DevKitC-1. As on the C3, "input" falls back to seen-idle.

    // UNTESTED: no S3 here. Its addressable LED is on GPIO48 on the boards that
    // have one, flash and PSRAM occupy 26-32, and USB is 19/20 -- so this stays
    // clear of all of them. GPIO9 is kept for BOOT for consistency with the
    // other two rather than because the S3 needs it there.
    #[cfg(esp32s3)]
    let (
        light_pin,
        setup_pin,
        sclk,
        mosi,
        miso,
        cs,
        rst,
        sda,
        scl,
        sense,
        relay,
        stop,
        strike,
        powered,
    ) = (
        pins.gpio48,
        pins.gpio0,
        pins.gpio12,
        pins.gpio11,
        pins.gpio13,
        pins.gpio10,
        pins.gpio14,
        pins.gpio4,
        pins.gpio5,
        pins.gpio15,
        pins.gpio16,
        pins.gpio17,
        pins.gpio18,
        pins.gpio21,
    );
    // THE OUTPUTS FIRST, low, whatever the settings say. Until a pin is
    // driven it is an input at its reset state -- floating, or weakly pulled --
    // and `boot()` below can take seconds to join Wi-Fi and register, or
    // forever while the setup portal is open. A relay driver that reads a
    // floating input as on would energize the tool, or release a door, for all
    // of that time.
    let mut relay = PinDriver::output(relay)?;
    relay.set_low()?;
    let mut strike = PinDriver::output(strike)?;
    strike.set_low()?;

    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;
    let version = env!("CARGO_PKG_VERSION");

    log::info!("spaces-toolguard-device {version} on {}", board::CHIP);

    // Not on the critical path: `start` is infallible and degrades to a no-op,
    // because a reader with a dead LED should still read cards.
    let light = StatusLight::start(light_pin, Status::Connecting);
    // Early, so an image on probation is watched from the first moment rather
    // than from whenever the loop happens to start.
    let updates = Updates::start(
        nvs.clone(),
        Store::new(nvs.clone()),
        light.clone(),
        env!("CARGO_PKG_NAME"),
        version,
    );
    watch_setup_button(
        PinDriver::input(setup_pin, Pull::Up)?,
        Store::new(nvs.clone()),
    )?;

    // The shared settings plus ours. `Box::leak` is the idiom the demo
    // establishes: the portal wants a `&'static [Field]`, and this list is
    // built once at boot and lives as long as the program.
    let fields: &'static [Field] = Box::leak(
        spaces_device_espidf::store::FIELDS
            .iter()
            .copied()
            .chain(TOOL_FIELD.iter().copied())
            .chain(settings::FIELDS.iter().copied())
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );

    // Before boot, because `roles` is what this device registers AS: a device
    // declares what it can do, and an administrator may then bind it to a tool
    // in a role it declared. Claiming `power` with nothing wired to a relay is
    // how a box with no relay becomes what a machine's safety rests on.
    let mut settings = settings::read(&Store::new(nvs.clone()));
    let roles = settings.roles();
    // Before boot, because boot registers a device that is not yet registered,
    // and after it there is no telling whether the roles just went to the
    // platform or went years ago.
    let was_registered = Store::new(nvs.clone()).credentials().is_some();
    log::info!("this device: {}", settings.describe());
    log::info!("registering as {roles:?}");

    let online = match boot(
        peripherals.modem,
        sysloop.clone(),
        nvs,
        Firmware {
            fields,
            roles: &roles,
            software_version: version,
        },
        light.clone(),
        updates,
    ) {
        Ok(online) => online,
        // No panel yet: it is started after boot, so this is the light alone.
        Err(e) => give_up(&e, &light, None),
    };

    let tool_id = online
        .store
        .get(key::TOOL_ID)
        .unwrap_or_default()
        .trim()
        .to_string();
    settings.config.device_id = online.credentials.device_id.clone();
    settings.config.tool_id = tool_id.clone();
    let roles_notice = roles_check(&online.store, &roles, was_registered);

    // --- the two peripherals ------------------------------------------------
    //
    // The panel first, so a reader fault has somewhere to be displayed.
    let mut panel = match lcd::start(peripherals.i2c0, sda, scl, board::LCD_ADDRESSES) {
        Ok(p) => Some(p),
        Err(e) => {
            // Degraded, not broken: the reader can still authorize a member,
            // who sees the result on the machine rather than on a display. So
            // it complains and carries on.
            log::error!("no panel: {e:#}");
            None
        }
    };

    // Only when this box is supposed to have one. A plug has no antenna and
    // failing on its absence would make the one configuration that needs no
    // reader impossible to run.
    let card_reader = if settings.config.reads_cards {
        match rc522::start(peripherals.spi2, sclk, mosi, miso, cs, rst) {
            Ok(r) => Some(r),
            Err(e) => {
                // On a box that should have read cards, no antenna is the
                // entire function of the device missing, so this is terminal —
                // and it says so ON the panel, which is where a member
                // standing there will look.
                let fault = Screen::new("Reader fault", "see /log");
                give_up(&e, &light, panel.as_mut().map(|p| (p, fault)));
            }
        }
    } else {
        log::info!("configured with no card reader; not starting the antenna");
        None
    };

    // The two outputs, now that it is known which are wired. Both were driven
    // low before `boot()`; an output that is not configured keeps its driver
    // held anyway, because dropping one resets the pin and lets it float.
    let (relay, _relay_unused) = if settings.config.gates_power {
        (Some(relay), None)
    } else {
        (None, Some(relay))
    };
    let (strike, _strike_unused) = if settings.config.drives_latch {
        (Some(strike), None)
    } else {
        (None, Some(strike))
    };

    // The sensed input. Pulled up, so an unconnected pin reads as "not
    // working" when the sense is active-low, which is the quiet default rather
    // than a machine that looks permanently busy.
    // The sensed input. Pulled up, so an unconnected pin reads as "not
    // working" when the sense is active-low, which is the quiet default rather
    // than a machine that looks permanently busy.
    let sense_active_low = settings.sense_active_low;
    // Set by the interrupt when the input becomes asserted, however briefly,
    // and taken by the loop: a pulse between two passes is not missed (#9).
    let pulsed = Arc::new(AtomicBool::new(false));
    let sense = if settings.source.senses() {
        match sense_input(sense, settings.source, sense_active_low, &pulsed) {
            Ok(p) => Some(p),
            Err(e) => {
                log::error!("configured to sense the machine, but the input pin failed: {e:#}");
                None
            }
        }
    } else {
        None
    };

    // The powered input (#8), on the chips that have a pin for it.
    let powered_active_low = settings.powered_active_low;
    #[cfg(any(esp32c6, esp32s3))]
    let powered = if settings.powered == Powered::Input {
        match PinDriver::input(powered, Pull::Up) {
            Ok(p) => Some(p),
            Err(e) => {
                // Absent reads as off, which bills nobody rather than everyone.
                log::error!("configured with a powered input, but the pin failed: {e:#}");
                None
            }
        }
    } else {
        None
    };
    #[cfg(any(esp32c3, esp32c5))]
    let powered: Option<PinDriver<'_, Input>> = {
        if settings.powered == Powered::Input {
            log::warn!(
                "machine powered by input, but an {} has no pin left for one; using seen-idle",
                board::CHIP
            );
            settings.powered = Powered::SeenIdle;
        }
        None
    };

    // The stop button: a momentary switch to GND, so the pull-up makes an
    // unconnected or open contact read "not pressed". Debounced in the core;
    // the pin is just sampled.
    let stop = if settings.stop_button {
        match PinDriver::input(stop, Pull::Up) {
            Ok(p) => Some(p),
            Err(e) => {
                log::error!("configured with a stop button, but the input pin failed: {e:#}");
                None
            }
        }
    } else {
        None
    };

    // --- the loop: spaces-firmware's, as every device runs it (#18) ----------
    //
    // This box used to run its own copy of the loop, and that copy missed what
    // the library's did besides -- holding an update during a session, the
    // BROKER milestone, Wi-Fi state -- until each was patched back in by hand.
    // Now the library's loop runs it: the broker, the update guard and the
    // Wi-Fi state are handled there, once, for every device, and what is left
    // here is the hardware.
    let reader = Reader::new(settings.config.clone())
        .sensing(settings.source, settings.powered)
        .joined(online.address.clone());
    let (sender, events) = mpsc::channel();
    // Ahead of everything the loop does, so the resting screen says it from the
    // first moment.
    let _ = sender.send(Event::Input(toolguard_core::Input::Notice(
        roles_notice.map(str::to_string),
    )));
    // Shared with the hardware so that a loop that cannot start can still say
    // so on the panel, which is where a member standing there will look.
    let panel = Rc::new(RefCell::new(panel));
    let hardware = Hardware {
        panel: panel.clone(),
        relay,
        strike,
        card_reader,
        last_card_poll: None,
        sense,
        sense_active_low,
        pulsed,
        powered,
        powered_active_low,
        stop,
        stop_button: Debounced::button(),
    };
    // Returns only if the broker client cannot be set up -- the usual cause is
    // an address it cannot parse -- and that is fatal like the others: not a
    // `?`, which would end the program but not the board, leaving it on the
    // network serving updates with nothing reading cards and nothing saying why.
    let e = match spaces_device_espidf::run::run(&online, reader, hardware, sender, events) {
        Err(e) => e,
        Ok(never) => match never {},
    };
    let fault = Screen::new("Broker failed", "see /log");
    let mut panel = panel.borrow_mut();
    give_up(&e, &light, panel.as_mut().map(|p| (p, fault)))
}

/// The box's hardware: what the core's decisions are carried out on, and the
/// inputs read on the loop's own thread.
struct Hardware<'d> {
    panel: Rc<RefCell<Option<lcd::Panel<'d>>>>,
    relay: Option<PinDriver<'d, Output>>,
    strike: Option<PinDriver<'d, Output>>,
    card_reader: Option<rc522::Reader<'d>>,
    /// When the antenna was last asked; see [`POLL_MS`].
    last_card_poll: Option<u64>,
    sense: Option<PinDriver<'d, Input>>,
    sense_active_low: bool,
    /// Set by the sense interrupt, taken here: a pulse between two passes is
    /// not missed (#9).
    pulsed: Arc<AtomicBool>,
    powered: Option<PinDriver<'d, Input>>,
    powered_active_low: bool,
    stop: Option<PinDriver<'d, Input>>,
    /// spaces-device's debouncer, the one every device uses: a press counts
    /// once the contact has settled, and holds long enough that no bounce or
    /// lingering finger is a second press.
    stop_button: Debounced,
}

impl spaces_device_espidf::run::Hardware<Reader> for Hardware<'_> {
    /// Carry out what the core decided. Publishes and log lines are the loop's;
    /// what reaches here is the panel and the outputs.
    fn apply(&mut self, action: Action) {
        match action {
            Action::Show(screen) => {
                if let Some(p) = self.panel.borrow_mut().as_mut() {
                    if let Err(e) = p.show(&screen) {
                        // Not fatal and not silent: the panel may recover on the
                        // next change, and the log says that it did not now.
                        log::warn!("panel write failed: {e:#}");
                    }
                }
            }
            Action::Set { output, on } => {
                log::info!("{output:?} {}", if on { "ON" } else { "off" });
                // The tool output and the strike reach hardware here.
                // `Output::Running` is an indicator and this box has no lamp
                // for it -- the panel is what tells a member the machine is
                // working, and the status LED is already spoken for by the
                // network state.
                let (pin, what) = match output {
                    toolguard_core::Output::Tool => (self.relay.as_mut(), "relay"),
                    toolguard_core::Output::Strike => (self.strike.as_mut(), "strike"),
                    toolguard_core::Output::Running => return,
                };
                match pin {
                    Some(pin) => {
                        let r = if on { pin.set_high() } else { pin.set_low() };
                        if let Err(e) = r {
                            // Loud: a write that failed means a machine or a
                            // door is not where the core believes it is.
                            log::error!("{what} write failed: {e:#}");
                        }
                    }
                    // Unreachable by construction -- the core emits no Tool or
                    // Strike action unless the box is configured with one, and
                    // the pin is opened whenever it is. Logged rather than
                    // ignored, because if it ever happens the thing that did
                    // not happen is a machine switching off.
                    None => log::error!("asked to drive the {what} with none configured"),
                }
            }
            // The loop's (Effect): never routed here.
            Action::Publish { .. } | Action::Note(_) => {}
        }
    }

    fn poll(&mut self, now: u64) -> Vec<toolguard_core::Input> {
        let mut inputs = Vec::new();
        // The antenna first, which is the thing with a person waiting on it.
        if self
            .last_card_poll
            .is_none_or(|t| now.saturating_sub(t) >= POLL_MS)
        {
            self.last_card_poll = Some(now);
            if let Some(uid) = self.card_reader.as_mut().and_then(|r| r.poll()) {
                inputs.push(toolguard_core::Input::Card(uid));
            }
        }
        // Then the machine. Sampled every pass rather than on a change, which
        // the filter in the core is built for: it only moves its clock when the
        // value actually changes, so handing it every reading costs nothing and
        // means no edge can be missed between passes.
        if let Some(pin) = self.sense.as_mut() {
            // The inversions, here and nowhere else: the core takes what the
            // machine is doing, and the polarity of this installation's wiring
            // is applied as the pins are read.
            let latched = self.pulsed.swap(false, Ordering::AcqRel);
            if latched {
                // The driver disables the interrupt each time it fires.
                if let Err(e) = pin.enable_interrupt() {
                    log::warn!("could not re-arm the sense interrupt: {e}");
                }
            }
            inputs.push(toolguard_core::Input::Sense(Reading {
                active: pin.is_low() == self.sense_active_low,
                pulsed: latched,
                powered: self
                    .powered
                    .as_ref()
                    .map(|p| p.is_low() == self.powered_active_low),
            }));
        }
        // Then the stop button, which anyone may press. One stop per press,
        // however long the finger stays; the core turns the bounce into that.
        if let Some(pin) = self.stop.as_mut() {
            if self.stop_button.sample(now, pin.is_low()) == Some(true) {
                inputs.push(toolguard_core::Input::Stop);
            }
        }
        inputs
    }
}

/// Give up on this boot: say why, in the log and on the panel if there is one,
/// show the failure on the status light, and start again in thirty seconds.
///
/// Every failure the box cannot run without comes here, so that none of them
/// leaves it up and doing nothing. The wait keeps a box that fails at every
/// boot from restarting so fast that nobody can read the panel, or reach
/// `/log`, in between.
fn give_up(
    why: &anyhow::Error,
    light: &StatusLight,
    panel: Option<(&mut lcd::Panel<'_>, Screen)>,
) -> ! {
    log::error!("{why:#}");
    if let Some((p, screen)) = panel {
        let _ = p.show(&screen);
    }
    light.set(Status::Failed);
    std::thread::sleep(Duration::from_secs(30));
    esp_idf_svc::hal::reset::restart()
}

/// The sense input, and for [`Source::Pulses`] an interrupt that sets `pulsed`
/// whenever it becomes asserted.
///
/// The interrupt does nothing but store to an atomic, which is all that is
/// safe in one. The loop takes the flag each pass and re-arms it.
fn sense_input<'d>(
    pin: impl InputPin + 'd,
    source: Source,
    active_low: bool,
    pulsed: &Arc<AtomicBool>,
) -> anyhow::Result<PinDriver<'d, Input>> {
    let mut driver = PinDriver::input(pin, Pull::Up)?;
    if source == Source::Pulses {
        driver.set_interrupt_type(if active_low {
            InterruptType::NegEdge
        } else {
            InterruptType::PosEdge
        })?;
        let flag = pulsed.clone();
        // SAFETY: the callback only stores to an atomic, which is ISR-safe, and
        // the driver is held for the life of the program, so it is never
        // forgotten with the callback still registered.
        unsafe {
            driver.subscribe(move || flag.store(true, Ordering::Release))?;
        }
        driver.enable_interrupt()?;
    }
    Ok(driver)
}

/// Whether the platform has the roles this box now claims, recording them
/// whenever they are known to have just been sent. A notice for the panel if
/// not.
///
/// A device registers once, and a registered one never sends its roles again,
/// so a setting changed afterwards changes what the box does and not what the
/// platform thinks it is. Nothing here can fix that; it can only say so.
fn roles_check(store: &Store, roles: &[&str], was_registered: bool) -> Option<&'static str> {
    use toolguard_core::settings::{check_roles, key, roles_record, RolesCheck};
    let record = |what: &str| {
        if let Err(e) = store.set(key::REG_ROLES, &roles_record(roles)) {
            log::warn!("could not record the roles {what}: {e:#}");
        }
    };
    if !was_registered {
        // Registered on this boot, with exactly these.
        record("just registered with");
        return None;
    }
    match check_roles(store.get(key::REG_ROLES).as_deref(), roles) {
        RolesCheck::Same => None,
        RolesCheck::Unknown => {
            log::info!(
                "registered before roles were recorded; assuming the platform has {roles:?}"
            );
            record("assumed");
            None
        }
        RolesCheck::Changed { registered, now } => {
            log::warn!(
                "ROLES CHANGED since registration: the platform has [{registered}], the \
                 settings now claim [{now}]. A registered device does not send its roles \
                 again, so it cannot be bound in the new ones. Change the settings back, \
                 or erase this board's settings and register it again with a new invite \
                 (docs/INSTALLING.md §6)"
            );
            Some("Roles changed")
        }
    }
}
