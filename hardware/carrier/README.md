# The ToolGuard carrier board

One board for every ToolGuard box (#24, #25): a socketed Seeed XIAO ESP32-C5,
the RC522 reader, the 16x2 LCD and everything around them, populated to suit
the box. The schematic is [`carrier.py`](carrier.py) (SKiDL); the layout is
done by hand in KiCad. **Nothing here has been built yet.**

**The build guide** -- what to fit for each kind of box, the soldering order,
the mechanical stack, wiring and bring-up -- is [`guide/`](guide/): Quarto,
rendered to PDF (through Typst) and HTML by CI, with figures generated from
the board by `guide/figures.py`.

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
| The status light | a plain 3 mm through-hole LED on D6, on the front | every box |
| Mounting | 4 M3 enclosure holes; 4 M2.5 for the LCD's standoffs; 4 M3 for the RC522 (nylon standoffs: metal detunes the antenna) | every box |

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

## Layout guidance

- **Keep the RC522's antenna clear**: no copper, pour, tracks or metal standoffs
  under or close to the module's antenna end, on either layer.
- **Keep the buck converter's loop tight** (LV2862, input capacitor, catch diode,
  inductor) in one corner over solid ground, far from the antenna and I²C.
- **Field wiring on one edge**: the terminals, with K1, K2 and their LEDs.
- **Power on a side edge**: USB-C, the ideal diodes and the 7-60 V terminal,
  with the XIAO's own USB-C facing the same edge, and a route for its antenna
  cable to a vent or plastic.
- **SMD parts on the back** for one-sided assembly; the front carries only the
  LCD header, the RC522 socket and the status LED. Ground on the bottom layer,
  except under the antenna.
- **Every hole and part comes from `carrier.py`**: a re-import deletes footprints
  that are not in the netlist, so add mechanical parts there, not by hand.

## The modules' hole patterns, as measured

Measured with calipers on the Hackspace's own modules (2026-11), centres worked
out from edge-to-edge gaps. Place the hole footprints from these.

**LCD 1602 (QAPASS):** board 79.9 x 36.0 mm; four holes of 2.9 mm on a
**75.0 x 30.9 mm** rectangle, about 2.5 mm in from each edge. The standard 1602
pattern. Takes M2.5.

**RFID-RC522 (blue):** board 39.4 x 60.45 mm, viewed from the front with the
header at the top and the antenna in the lower part. Hole centres:

| hole | from the left edge | from the bottom edge |
|---|---|---|
| top-left | 2.7 mm | 44.3 mm |
| top-right | 36.85 mm | 44.3 mm |
| bottom-left | 7.5 mm | 6.9 mm |
| bottom-right | 32.4 mm | 6.9 mm |

That is a 34.15 mm top pair and a 24.95 mm bottom pair, 37.4 mm apart. It
assumes 3.2 mm holes (3.5 mm was measured, but only 3.2 makes the top row's
widths add up) and a level bottom row. To confirm when the module is to hand,
measure square-on from the bottom-left hole to the left edge and to the bottom
edge. The antenna is between and below the bottom pair: keep the carrier's
copper clear of that whole area plus a few millimetres.

## Ordering

```sh
python3 fab.py        # KiCad 10's pcbnew and kicad-cli; no uv environment needed
```

writes `fab/` (git-ignored: it is built from `carrier.kicad_pcb`):

| File | For |
|---|---|
| `carrier-r1-gerbers.zip` | the bare board: Gerbers and drill files, origin at the bottom-left corner |
| `usb/bom.csv`, `usb/cpl.csv` | assembly of every box except (d): what the fab fits, and where |
| `vin/bom.csv`, `vin/cpl.csv` | assembly for box (d), with the 7-60 V input fitted |
| `*/hand-solder.csv` | the through-hole parts you solder yourself |
| `*/not-fitted.csv` | the parts that variant leaves off |

The BOM uses PCBWay's columns. The fab assembles the back only (every
surface-mount part, the USB-C included); the through-hole parts come loose and
are soldered by hand ([the build guide](guide/guide.qmd)). Run DRC before an
order: `kicad-cli pcb drc carrier.kicad_pcb` reports 0 violations and 0
unconnected items.

## Not decided yet

- **Second sources**: every assembled part has an `MPN` except the generic
  resistors and capacitors, which go by value and package. Where the fab
  offers an equivalent it holds in stock, check it against the rating in the
  part's value (`fab.py` lists any part that has neither).
- **The RC522's header**: its position and pin order on the module were
  assumed, not measured (the guide's "Putting it together").
