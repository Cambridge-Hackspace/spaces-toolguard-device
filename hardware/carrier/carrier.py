"""The ToolGuard carrier board, as SKiDL (#25).

`uv run python carrier.py` writes `carrier.net`, a KiCad netlist the layout
(in KiCad, by hand) imports. `uv run python check.py` checks this circuit
against the firmware's board file.

One board for every box (#24): a door reader, a tool reader with a stop
button and a joystick, a usage sensor inside a machine, a power box. Parts a
box does not need are left off the board, not designed out. Which blocks a
configuration fits is in README.md.

Facts this rests on, each checked rather than remembered:

- The XIAO ESP32-C5's edge pins and their GPIOs: Seeed's pinout
  (XIAO_ESP32C5_Pinout.xlsx) and schematic rev 1.1. Its 5V pin is VBUS
  behind a TPS22916C load switch; its power mux falls back to the battery
  input when VBUS is under ~4.33 V, so only one ordinary diode drop is
  allowed on the 5 V path into it.
- The LV2862 buck: TI datasheet (4-60 V in, 0.765 V reference,
  non-synchronous, 22 uH / 10 uF / 2.2 uF / 100 nF, SHDN via 100 k to VIN).
- Every symbol and footprint name, and every pin used: KiCad 10.0.8's
  libraries, read rather than assumed. The G5V-1's NC is pin 1 and NO is
  pin 10 (the symbol's arm rests on pin 1); coil 2-9; common 5 and 6.
- The pin map: toolguard-fw/boards/c5-xiao-carrier.toml, the firmware's
  source of truth. Nets the firmware drives are named for their roles there
  (SDA, SCL, SENSE, RELAY, STRIKE, STOP, LIGHT, SCLK, MISO, MOSI, CS), and
  check.py holds the two together.

Mains never reaches this board: the relays are low-voltage dry contacts.
"""

import os
import sys

os.environ.setdefault("KICAD10_SYMBOL_DIR", "/usr/share/kicad/symbols")
os.environ.setdefault("KICAD10_FOOTPRINT_DIR", "/usr/share/kicad/footprints")

import builtins  # noqa: E402

from skidl import (  # noqa: E402
    KICAD10,
    POWER,
    Net,
    Part,
    footprint_search_paths,
    set_default_tool,
)

# SKiDL's no-connect net. It is installed as a builtin on import, not exported.
NC = builtins.NC

set_default_tool(KICAD10)
# This folder's fp-lib-table: KiCad's own (its template, every library under
# ${KICAD10_FOOTPRINT_DIR}), so nothing depends on a KiCad GUI having run here.
HERE = os.path.dirname(os.path.abspath(__file__))
footprint_search_paths[KICAD10] = [HERE]

R0603 = "Resistor_SMD:R_0603_1608Metric"
C0603 = "Capacitor_SMD:C_0603_1608Metric"


def part(lib, name, tag, value=None, footprint=None, mpn=None, note=None, dnp=False):
    """A part with a FIXED tag: KiCad matches netlist parts to layout
    footprints by it, so a random one would unlink the layout every rebuild.

    And an explicit footprint, always. A symbol's default is not a choice:
    KiCad's 1N4148W defaults to a through-hole DO-41, and three SOD-123
    diodes reached the first layout as DO-41s before this refused it.
    """
    assert footprint, f"{tag}: give its footprint; a symbol's default is not a choice"
    p = Part(lib, name, tag=tag)
    if value is not None:
        p.value = value
    if footprint is not None:
        p.footprint = footprint
    if mpn is not None:
        p.fields["MPN"] = mpn
    if note is not None:
        p.fields["Note"] = note
    if dnp:
        p.fields["DNP"] = "DNP"
    return p


def pin(p, key):
    """The one pin of `p` named (or numbered) exactly `key`.

    Not `p[key]`: SKiDL matches a string loosely against names AND numbers,
    so on a PCF8574 `pin(p, "P1")` returns P1 and pin 1 (A0) both, and wires the
    joystick to a grounded address pin without a word.
    """
    found = [q for q in p.pins if q.name == key] or [q for q in p.pins if str(q.num) == key]
    assert len(found) == 1, f"{p.ref}: {len(found)} pins match {key!r}"
    return found[0]


def r(tag, value, a, b, footprint=R0603, **kw):
    res = part("Device", "R", tag, value=value, footprint=footprint, **kw)
    a += res[1]
    b += res[2]
    return res


def c(tag, value, a, b, footprint=C0603, **kw):
    cap = part("Device", "C", tag, value=value, footprint=footprint, **kw)
    a += cap[1]
    b += cap[2]
    return cap


def build():
    global NC  # `NC += pin` rebinds it
    gnd = Net("GND")
    v5 = Net("+5V")  # the carrier's rail: relay coils, LCD, level shifter
    v3 = Net("+3V3")  # from the XIAO's own regulator
    # Rails that come in through a connector or a passive, which ERC cannot
    # see as a source: declared as driven, so its warnings stay meaningful.
    for rail in (gnd, v3):
        rail.drive = POWER

    # --- Power in: USB-C (power only) -----------------------------------
    usb = part(
        "Connector", "USB_C_Receptacle_PowerOnly_6P", "J_USB",
        footprint="Connector_USB:USB_C_Receptacle_GCT_USB4125-xx-x_6P_TopMnt_Horizontal",
        mpn="USB4125-GF-A",
    )
    vbus = Net("VBUS")
    vbus.drive = POWER
    vbus += pin(usb, "A9"), pin(usb, "B9")
    gnd += pin(usb, "A12"), pin(usb, "B12"), pin(usb, "SH")
    # Both CC pins pulled down 5.1 k: without them a USB-C charger gives 0 V.
    r("R_CC1", "5.1k", pin(usb, "A5"), gnd)
    r("R_CC2", "5.1k", pin(usb, "B5"), gnd)
    usb_fused = Net("VBUS_F")
    usb_fused.drive = POWER
    f_usb = part("Device", "Polyfuse", "F_USB", value="1.1A hold, 6V+",
                 footprint="Fuse:Fuse_1206_3216Metric", mpn="MF-NSMF110-2")
    vbus += f_usb[1]
    usb_fused += f_usb[2]
    tvs_usb = part("Device", "D_TVS", "D_TVS_USB", value="5V standoff",
                   footprint="Diode_SMD:D_SMA", mpn="SMAJ5.0CA")
    usb_fused += tvs_usb[1]
    gnd += tvs_usb[2]

    # --- Power in: 7-60 V terminal and the optional buck ------------------
    # Fitted only on boxes that live inside a machine (#25): a Ruida's
    # controller gives +24 V on CN1/CN2 pin 6, and can run at 36 V.
    j_vin = part("Connector_Generic", "Conn_01x02", "J_VIN",
                 footprint="Connector_Phoenix_MC:PhoenixContact_MC_1,5_2-G-3.81_1x02_P3.81mm_Horizontal",
                 note="VIN+ (7-36 V nominal), GND", dnp=True)
    vin_raw = Net("VIN_RAW")
    vin_raw += j_vin[1]
    gnd += j_vin[2]
    vin_rp = Net("VIN_RP")
    d_rev = part("Device", "D_Schottky", "D_REV", value="100V 1A",
                 footprint="Diode_SMD:D_SMA", mpn="SS110", note="reverse polarity", dnp=True)
    vin_raw += pin(d_rev, "A")
    vin_rp += pin(d_rev, "K")
    vin = Net("VIN")
    vin.drive = POWER
    f_vin = part("Device", "Fuse", "F_VIN", value="500mA 63V",
                 footprint="Fuse:Fuse_1206_3216Metric", mpn="0466.500NR", dnp=True)
    vin_rp += f_vin[1]
    vin += f_vin[2]
    # 40 V standoff: clamps under the LV2862's 65 V transient rating, and
    # conducts nothing from a 36 V supply.
    tvs_vin = part("Device", "D_TVS", "D_TVS_VIN", value="40V standoff",
                   footprint="Diode_SMD:D_SMB", mpn="SMBJ40CA", dnp=True)
    vin += tvs_vin[1]
    gnd += tvs_vin[2]

    buck = part("Regulator_Switching", "LV2862XDDC", "U_BUCK",
                footprint="Package_TO_SOT_SMD:SOT-23-6", mpn="LV2862XDDCR", dnp=True)
    vin += pin(buck, "VIN")
    gnd += pin(buck, "GND")
    c("C_VIN", "2.2uF 100V", vin, gnd, footprint="Capacitor_SMD:C_1206_3216Metric", dnp=True)
    r("R_SHDN", "100k", vin, pin(buck, "~{SHDN}"), dnp=True)
    sw = Net("BUCK_SW")
    sw += pin(buck, "SW")
    c("C_BOOT", "100nF", pin(buck, "CB"), sw, dnp=True)
    d_catch = part("Device", "D_Schottky", "D_CATCH", value="100V 1A",
                   footprint="Diode_SMD:D_SMA", mpn="SS110",
                   note="rated 25% over the 60 V maximum input", dnp=True)
    sw += pin(d_catch, "K")
    gnd += pin(d_catch, "A")
    v5_buck = Net("+5V2_BUCK")
    v5_buck.drive = POWER
    ind = part("Device", "L", "L_BUCK", value="22uH, Isat >= 1.6A",
               footprint="Inductor_SMD:L_Bourns_SRN6045TA", mpn="SRN6045TA-220M", dnp=True)
    sw += ind[1]
    v5_buck += ind[2]
    c("C_VOUT", "10uF 16V", v5_buck, gnd, footprint="Capacitor_SMD:C_0805_2012Metric", dnp=True)
    # VOUT = 0.765 V x (1 + 57.6 k / 10 k) = 5.17 V: a little over 5 V, so
    # the XIAO's 5V pin stays clear of its 4.33 V brownout behind the diode.
    fb = Net("BUCK_FB")
    fb += pin(buck, "FB")
    r("R_FB_TOP", "57.6k 1%", v5_buck, fb, dnp=True)
    r("R_FB_BOT", "10k 1%", fb, gnd, dnp=True)

    # --- Combine the two inputs: ideal diodes -----------------------------
    # Two ordinary diodes in a row would leave the XIAO's 5V pin at or under
    # its 4.33 V brownout. An LM66100 drops tens of mV.
    for tag, src, dnp in (("U_OR_USB", usb_fused, False), ("U_OR_VIN", v5_buck, True)):
        ideal = part("Power_Management", "LM66100DCK", tag,
                     footprint="Package_TO_SOT_SMD:SOT-363_SC-70-6", mpn="LM66100DCKR", dnp=dnp)
        src += pin(ideal, "VIN")
        v5 += pin(ideal, "VOUT")
        gnd += pin(ideal, "GND"), pin(ideal, "~{CE}")  # always on
        NC += pin(ideal, "ST")  # status output: not used
        # Two outputs on one rail is the point of an OR: each blocks reverse
        # current, so ERC's power-out conflict does not apply here.
        pin(ideal, "VOUT").do_erc = False
    c("C_5V", "10uF 10V", v5, gnd, footprint="Capacitor_SMD:C_0805_2012Metric")

    # --- The XIAO ESP32-C5, socketed -----------------------------------------
    # Two 1x7 sockets. Rows, top view with USB up, from Seeed's pinout:
    # D0|5V, D1|GND, D2|3V3, D3|D10, D4|D9, D5|D8, D6|D7.
    xl = part("Connector_Generic", "Conn_01x07", "J_XIAO_L",
              footprint="Connector_PinSocket_2.54mm:PinSocket_1x07_P2.54mm_Vertical",
              note="XIAO D0..D6")
    xr = part("Connector_Generic", "Conn_01x07", "J_XIAO_R",
              footprint="Connector_PinSocket_2.54mm:PinSocket_1x07_P2.54mm_Vertical",
              note="XIAO 5V, GND, 3V3, D10, D9, D8, D7")
    xiao = {
        "D0": xl[1], "D1": xl[2], "D2": xl[3], "D3": xl[4], "D4": xl[5], "D5": xl[6], "D6": xl[7],
        "5V": xr[1], "GND": xr[2], "3V3": xr[3], "D10": xr[4], "D9": xr[5], "D8": xr[6], "D7": xr[7],
    }
    gnd += xiao["GND"]
    v3 += xiao["3V3"]
    # One ordinary diode into the XIAO's 5V pin, so a laptop on the XIAO's
    # own USB never backfeeds the carrier, and the carrier never pushes
    # current into the laptop (#25).
    xiao_5v = Net("XIAO_5V")
    xiao_5v += xiao["5V"]
    d_xiao = part("Device", "D_Schottky", "D_XIAO", value="40V 1A",
                  footprint="Diode_SMD:D_SOD-123", mpn="B5819W")
    v5 += pin(d_xiao, "A")
    xiao_5v += pin(d_xiao, "K")

    nets = {}
    for role, edge in (("SENSE", "D0"), ("RELAY", "D1"), ("STRIKE", "D2"), ("CS", "D3"),
                      ("SDA", "D4"), ("SCL", "D5"), ("LIGHT", "D6"), ("STOP", "D7"),
                      ("SCLK", "D8"), ("MISO", "D9"), ("MOSI", "D10")):
        n = Net(role)
        n += xiao[edge]
        nets[role] = n

    # --- I2C at 3.3 V, and the Qwiic port ----------------------------------
    sda, scl = nets["SDA"], nets["SCL"]
    r("R_SDA_PU", "4.7k", sda, v3)
    r("R_SCL_PU", "4.7k", scl, v3)
    qwiic = part("Connector_Generic_MountingPin", "Conn_01x04_MountingPin", "J_QWIIC",
                 footprint="Connector_JST:JST_SH_SM04B-SRSS-TB_1x04-1MP_P1.00mm_Horizontal",
                 mpn="SM04B-SRSS-TB", note="Qwiic: GND, 3V3, SDA, SCL")
    gnd += qwiic[1]
    v3 += qwiic[2]
    sda += qwiic[3]
    scl += qwiic[4]
    gnd += pin(qwiic, "MP")  # its two solder tabs, grounded as is usual

    # --- The LCD, at 5 V behind a level shifter ------------------------------
    # A PCF8574 backpack at 5 V needs ~3.5 V for a logic high, which a 3.3 V
    # bus does not give. The PCA9306 translates, and the backpack's own
    # pull-ups can stay.
    lvl = part("Interface", "PCA9306", "U_LVL",
               footprint="Package_SO:SOIC-8_3.9x4.9mm_P1.27mm", mpn="PCA9306DR")
    gnd += pin(lvl, "GND")
    v3 += pin(lvl, "VREF1")
    sda += pin(lvl, "SDA1")
    scl += pin(lvl, "SCL1")
    sda5, scl5, en_lvl = Net("SDA_5V"), Net("SCL_5V"), Net("LVL_EN")
    sda5 += pin(lvl, "SDA2")
    scl5 += pin(lvl, "SCL2")
    en_lvl += pin(lvl, "VREF2"), pin(lvl, "EN")
    en_lvl.drive = POWER  # VREF2 is fed through 200 k, as the PCA9306 wants
    r("R_LVL_EN", "200k", en_lvl, v5)
    c("C_LVL_EN", "100nF", en_lvl, gnd)
    r("R_SDA5_PU", "4.7k", sda5, v5)
    r("R_SCL5_PU", "4.7k", scl5, v5)
    lcd = part("Connector_Generic", "Conn_01x04", "J_LCD",
               footprint="Connector_PinSocket_2.54mm:PinSocket_1x04_P2.54mm_Vertical",
               note="LCD backpack: GND, VCC (5 V), SDA, SCL")
    gnd += lcd[1]
    v5 += lcd[2]
    sda5 += lcd[3]
    scl5 += lcd[4]

    # --- The card reader: an RC522 module, on I2C or SPI -------------------
    # The blue module's header: SDA, SCK, MOSI, MISO, IRQ, GND, RST, 3.3V.
    # A converted module (I2C, #23) puts SDA on its SDA pin and SCL on MISO,
    # with SCK and MOSI as address bits tied to ground. An unconverted one is
    # SPI. 0-ohm links pick one; the I2C set is fitted by default.
    rc = part("Connector_Generic", "Conn_01x08", "J_RC522",
              footprint="Connector_PinSocket_2.54mm:PinSocket_1x08_P2.54mm_Vertical",
              note="RC522: SDA, SCK, MOSI, MISO, IRQ, GND, RST, 3.3V")
    m_sda, m_sck, m_mosi, m_miso = Net("RC_SDA"), Net("RC_SCK"), Net("RC_MOSI"), Net("RC_MISO")
    m_sda += rc[1]
    m_sck += rc[2]
    m_mosi += rc[3]
    m_miso += rc[4]
    NC += rc[5]  # IRQ: not used, the firmware polls every 75 ms
    gnd += rc[6]
    v3 += rc[7], rc[8]  # RST tied high: no pin for it, never in power-down
    for tag, a, b, dnp in (
        ("R_RC_I2C_SDA", m_sda, sda, False),
        ("R_RC_I2C_SCL", m_miso, scl, False),
        ("R_RC_I2C_A1", m_sck, gnd, False),
        ("R_RC_I2C_A0", m_mosi, gnd, False),
        ("R_RC_SPI_CS", m_sda, nets["CS"], True),
        ("R_RC_SPI_MISO", m_miso, nets["MISO"], True),
        ("R_RC_SPI_SCLK", m_sck, nets["SCLK"], True),
        ("R_RC_SPI_MOSI", m_mosi, nets["MOSI"], True),
    ):
        r(tag, "0R", a, b, dnp=dnp)
    c("C_RC522", "10uF 10V", v3, gnd, footprint="Capacitor_SMD:C_0805_2012Metric")

    # --- The joystick, on a PCF8574 at 0x20 --------------------------------
    joy_x = part("Interface_Expansion", "PCF8574T", "U_JOY",
                 footprint="Package_SO:SOIC-16W_7.5x10.3mm_P1.27mm", mpn="PCF8574T/3")
    v3 += pin(joy_x, "VDD")
    gnd += pin(joy_x, "GND"), pin(joy_x, "A0"), pin(joy_x, "A1"), pin(joy_x, "A2")  # 0x20
    sda += pin(joy_x, "SDA")
    scl += pin(joy_x, "SCL")
    c("C_JOY", "100nF", v3, gnd)
    joy = part("Connector_Generic", "Conn_01x08", "J_JOY",
               footprint="Connector_JST:JST_PH_B8B-PH-K_1x08_P2.00mm_Vertical",
               mpn="B8B-PH-K-S", note="joystick: COM, UP, DOWN, LEFT, RIGHT, MID, SET, RST")
    gnd += joy[1]
    for i, (name, port) in enumerate(
        (("UP", "P0"), ("DOWN", "P1"), ("LEFT", "P2"), ("RIGHT", "P3"),
         ("MID", "P4"), ("SET", "P5"), ("RST", "P6")), start=2):
        n = Net(f"JOY_{name}")
        n += joy[i], pin(joy_x, port)
    NC += pin(joy_x, "P7"), pin(joy_x, "~{INT}")  # spare; the firmware polls

    # --- The stop button, normally closed ------------------------------------
    stop_in = Net("STOP_IN")
    j_stop = part("Connector_Generic", "Conn_01x02", "J_STOP",
                  footprint="Connector_Phoenix_MC:PhoenixContact_MC_1,5_2-G-3.81_1x02_P3.81mm_Horizontal",
                  note="stop button, normally closed: STOP, GND")
    stop_in += j_stop[1]
    gnd += j_stop[2]
    stop = nets["STOP"]
    r("R_STOP", "1k", stop_in, stop)
    r("R_STOP_PU", "10k", stop, v3)
    c("C_STOP", "100nF", stop, gnd)
    esd_stop = part("Device", "D_TVS", "D_ESD_STOP", value="3.3V ESD",
                    footprint="Diode_SMD:D_SOD-323", mpn="PESD3V3S1UB")
    stop_in += esd_stop[1]
    gnd += esd_stop[2]

    # --- The sense input, opto-isolated --------------------------------------
    # S+ to a supply the machine has, S- to the signal (on the laser: L-ON1,
    # active low). 2.2 k suits a 5 V signal (1.7 mA); fit more for 24 V.
    j_sense = part("Connector_Generic", "Conn_01x02", "J_SENSE",
                   footprint="Connector_Phoenix_MC:PhoenixContact_MC_1,5_2-G-3.81_1x02_P3.81mm_Horizontal",
                   note="sense: S+, S-")
    s_plus, s_minus, s_led = Net("SENSE_P"), Net("SENSE_N"), Net("SENSE_LED")
    s_plus += j_sense[1]
    s_minus += j_sense[2]
    opto = part("Isolator", "LTV-817S", "U_SENSE",
                footprint="Package_DIP:SMDIP-4_W9.53mm", mpn="LTV-817S")
    r("R_SENSE_LED", "2.2k", s_plus, s_led, footprint="Resistor_SMD:R_1206_3216Metric",
      note="2.2k for 5 V signals; 10k for 24 V")
    s_led += opto[1]
    s_minus += opto[2]
    d_rev_sense = part("Diode", "1N4148W", "D_SENSE_REV", footprint="Diode_SMD:D_SOD-123", mpn="1N4148W")
    s_led += pin(d_rev_sense, "K")
    s_minus += pin(d_rev_sense, "A")
    sense = nets["SENSE"]
    sense += opto[4]
    gnd += opto[3]
    r("R_SENSE_PU", "10k", sense, v3)

    # --- Two dry-contact relay outputs ----------------------------------------
    # Low-side AO3400A with a gate pull-down, so a relay is OFF while the
    # ESP32 resets or boots; a flyback diode across each coil (the atopile
    # relay package had none). Contacts rated 1 A at 24 V DC: never mains.
    for n_out, role in ((1, "RELAY"), (2, "STRIKE")):
        drive = nets[role]
        gate, coil_lo = Net(f"K{n_out}_GATE"), Net(f"K{n_out}_COIL")
        fet = part("Transistor_FET", "AO3400A", f"Q_K{n_out}",
                   footprint="Package_TO_SOT_SMD:SOT-23", mpn="AO3400A")
        r(f"R_K{n_out}_GATE", "100R", drive, gate)
        r(f"R_K{n_out}_PD", "100k", gate, gnd)
        gate += pin(fet, "G")
        gnd += pin(fet, "S")
        coil_lo += pin(fet, "D")
        relay = part("Relay", "G5V-1", f"K{n_out}",
                     footprint="Relay_THT:Relay_SPDT_Omron_G5V-1", mpn="G5V-1-DC5")
        v5 += relay[2]
        coil_lo += relay[9]
        fly = part("Diode", "1N4148W", f"D_K{n_out}_FLY", footprint="Diode_SMD:D_SOD-123", mpn="1N4148W")
        coil_lo += pin(fly, "A")
        v5 += pin(fly, "K")
        led_k = Net(f"K{n_out}_LED")
        led = part("Device", "LED", f"LED_K{n_out}", value="green",
                   footprint="LED_SMD:LED_0603_1608Metric", mpn="LTST-C191KGKT",
                   note="on when the coil is")
        v5 += pin(led, "A")
        led_k += pin(led, "K")
        r(f"R_K{n_out}_LED", "2.2k", led_k, coil_lo)
        com, no, nc = Net(f"K{n_out}_COM"), Net(f"K{n_out}_NO"), Net(f"K{n_out}_NC")
        com += relay[5], relay[6]
        nc += relay[1]
        no += relay[10]
        term = part("Connector_Generic", "Conn_01x03", f"J_K{n_out}",
                    footprint="Connector_Phoenix_MC:PhoenixContact_MC_1,5_3-G-3.81_1x03_P3.81mm_Horizontal",
                    note=f"output {n_out}: COM, NO, NC")
        com += term[1]
        no += term[2]
        nc += term[3]

    # --- The status light: a plain LED on the panel --------------------------
    # Through-hole, on the FRONT, so it shows through the panel while every
    # SMD part stays on the back for one-sided assembly. Pad 1 is K, as on
    # the symbol.
    status = part("Device", "LED", "LED_STATUS", value="red, 3 mm",
                  footprint="LED_THT:LED_D3.0mm", note="front side; lit when D6 is high")
    r("R_STATUS", "1k", nets["LIGHT"], pin(status, "A"))
    gnd += pin(status, "K")

    # --- Mounting --------------------------------------------------------------
    # The import deletes any footprint not in the netlist, so every hole is
    # here; where each one goes is the layout's, measured against the real
    # modules.
    for i in range(1, 5):
        part("Mechanical", "MountingHole", f"H{i}", footprint="MountingHole:MountingHole_3.2mm_M3",
             note="enclosure, M3")
    # The LCD on four standoffs: a 1602's holes are usually on a 75 x 31 mm
    # rectangle. M2.5, which also takes the modules with smaller holes.
    for i in range(1, 5):
        part("Mechanical", "MountingHole", f"H_LCD{i}",
             footprint="MountingHole:MountingHole_2.7mm_M2.5", note="LCD standoff, M2.5")
    # The RC522 module, all four of its holes: held only at the header end,
    # a 60 mm module flexes at every tap. NYLON standoffs: metal near the
    # antenna detunes it.
    for i in range(1, 5):
        part("Mechanical", "MountingHole", f"H_RC{i}",
             footprint="MountingHole:MountingHole_3.2mm_M3", note="RC522 standoff, M3, NYLON")

    # For check.py: the nets named for the firmware's roles, and which socket
    # pin is which of the XIAO's pads.
    return {"roles": nets, "xiao": xiao}


if __name__ == "__main__":
    from skidl import ERC, generate_netlist

    build()
    ERC()
    out = sys.argv[1] if len(sys.argv) > 1 else "carrier.net"
    generate_netlist(file_=out)
