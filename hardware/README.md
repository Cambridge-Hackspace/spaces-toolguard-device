# Hardware

The boards this firmware runs on, as source: the schematic as
[SKiDL](https://github.com/devbisme/skidl) (Python), and the KiCad layout it
builds into. SKiDL rather than atopile because atopile's builds now need a
signed-in hosted service to pick parts (#25), and these sources have to build
for anyone, offline, under the licence below.

| Board | What it is |
|---|---|
| [`carrier/`](carrier/) | The ToolGuard carrier: a socketed XIAO ESP32-C5, the reader, the LCD and everything around them (#25) |

## Rules for this directory

- **The pin map is the firmware's.** `toolguard-fw/boards/<board>.toml` says
  which GPIO does what. A board here must agree with it, and a check holds the
  two together (`carrier/check.py`, in CI), so a change to either lands in the
  same pull request.
- **Sources are committed; outputs are not.** The SKiDL source, its
  `pyproject.toml` and `uv.lock`, and the KiCad layout belong in git. The
  netlist, Gerbers, drill files, the BOM and pick-and-place files are built from
  them -- the fab outputs for each release, attached to it the way firmware
  images are.
- **No mains on these boards.** A board's outputs are low-voltage dry contacts
  that close a circuit the machine already has -- a contactor's coil, an
  interlock loop, a strike's own supply. It never switches the machine's supply
  itself.

## Licence

Copyright Cambridge Hackspace.

Everything in this directory is licensed under the
[CERN Open Hardware Licence Version 2 - Strongly Reciprocal](LICENSE)
(SPDX: `CERN-OHL-S-2.0`). The firmware elsewhere in this repository is
AGPL-3.0-or-later; see the top-level [README](../README.md).

Source Location: https://github.com/Cambridge-Hackspace/spaces-toolguard-device

Under the licence, anyone who makes and conveys a board from these sources must
give its recipients the complete source or tell them where to find it (section
4). Each board's silkscreen carries the Source Location above for that reason.
