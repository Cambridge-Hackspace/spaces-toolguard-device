# The ToolGuard carrier board

One board for every ToolGuard box (#24, #25): a socketed Seeed XIAO ESP32-C5,
the RC522 reader, the 16x2 LCD and everything around them, populated to suit
the box. The schematic is [`carrier.py`](carrier.py) (SKiDL); the layout is
done by hand in KiCad. **Nothing here has been built yet.**

## Building it

Needs [uv](https://docs.astral.sh/uv/) and KiCad 10's symbol and footprint
libraries (`/usr/share/kicad/symbols` and `/usr/share/kicad/footprints`, or set
`KICAD10_SYMBOL_DIR` and `KICAD10_FOOTPRINT_DIR`). Nothing needs a login or a
network service.

```sh
uv run python carrier.py   # writes carrier.net, for KiCad's "Import Netlist"
uv run python check.py     # the checks CI runs
```

`check.py` checks three things, and fails on any of them:

- **the pin map**: every pin role in `toolguard-fw/boards/c5-xiao-carrier.toml`
  reaches the XIAO pad that file names, and only that one, and nothing else on
  the XIAO is wired;
- **pins against pads**: every connected pin of every part exists as a pad on
  its footprint, read from KiCad's own library;
- **ERC**: no errors and no warnings. The exceptions that are intended (two ideal
  diodes driving one rail, unused pins) are declared in `carrier.py`.

Every part has a fixed tag, so rebuilding the netlist never unlinks the layout.

**The layout** is `carrier.kicad_pcb` in this folder, done by hand in KiCad:
*File → Import → Netlist* with `carrier.net`, after every change to
`carrier.py`. KiCad also makes an empty `carrier.kicad_sch` with the project;
it stays empty. The schematic is `carrier.py`, and the netlist is the only way
the design reaches the layout.
SKiDL prints warnings about KiCad 5-9 library paths when it is imported; they are
for tool versions this board does not use.

## What is on it

| Block | Parts | Fitted on |
|---|---|---|
| USB-C power in | USB4125 (power only), 5.1 k on both CC pins, polyfuse, TVS | every box |
| 7-60 V power in | pluggable terminal, reverse-polarity Schottky, fuse, SMBJ40 TVS, LV2862 buck to 5.17 V | boxes inside a machine (d); left off otherwise |
| Combining the two | an LM66100 ideal diode from each | each input that is fitted |
| The XIAO | two 1x7 sockets; its 5V pin fed through one Schottky (B5819W) | every box |
| I²C at 3.3 V | 4.7 k pull-ups, a Qwiic port | every box |
| The LCD | PCA9306 level shifter to a 5 V I²C header for the backpack | (a), (b), (c) |
| The reader | RC522 module socket; 0-ohm links choose I²C (fitted) or SPI | (a), (b), (c) |
| The joystick | PCF8574 at 0x20, JST-PH 8 (COM, UP, DOWN, LEFT, RIGHT, MID, SET, RST) | (c) |
| The stop button | pluggable terminal, normally closed; RC filter and ESD | (b), (c) |
| The sense input | pluggable terminal, LTV-817S opto, 2.2 k for a 5 V signal | (d) |
| Two outputs | G5V-1 relays (dry contacts, 1 A at 24 V DC), AO3400A drivers with gate pull-downs, flyback diodes, an LED each, pluggable COM/NO/NC terminals | (e) |
| The status light | a plain LED on D6 | every box |

The letters are the box configurations in #24. Parts marked `DNP` in the
netlist (the 7-60 V input and buck, the SPI links) are left off by default.

## Facts it rests on, and where they come from

- **The XIAO's pads and GPIOs**: Seeed's XIAO ESP32-C5 pinout and schematic
  rev 1.1. Its 5V pin is VBUS behind a load switch, and its power mux drops to
  the battery input below about 4.33 V, so the 5 V path into it allows only one
  ordinary diode drop. That is why the inputs are combined through ideal diodes.
- **The LV2862**: TI's datasheet. 4-60 V in, 65 V transients, 0.765 V
  reference, non-synchronous (so a catch diode rated 25% over the maximum
  input), 22 µH / 10 µF out / 2.2 µF in / 100 nF boot, SHDN to VIN through
  100 k. 57.6 k over 10 k gives 5.17 V.
- **Every symbol, footprint and pin**: KiCad 10.0.8's libraries, read rather
  than remembered. On the G5V-1, NC is pin 1 and NO is pin 10 (the symbol's
  arm rests on pin 1), the coil is 2 and 9, and the common is 5 and 6.
- **The LCD needs the level shifter**: a PCF8574 at 5 V wants about 3.5 V for a
  logic high, which a 3.3 V bus does not give. With the PCA9306, the backpack's
  own pull-ups can stay.

## Not decided yet

- **Part numbers for the fab**: `MPN` is set where the part is certain. LCSC
  numbers, and parts given only by rating (the polyfuse, the 63 V fuse, the
  22 µH inductor), are chosen with the BOM, before the first order.
- **The board's outline, connectors' positions, the antenna clearance** for the
  RC522 and the cable route for the XIAO's antenna: the layout's.
