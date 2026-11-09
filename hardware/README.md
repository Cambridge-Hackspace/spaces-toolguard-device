# Hardware

The boards this firmware runs on, as source: the schematic as
[atopile](https://atopile.io) code, and the KiCad layout it builds into. Nothing
here yet; the first board, a carrier for the modules the box already uses, is
planned in the issue tracker.

## Rules for this directory

- **The pin map is the firmware's.** `toolguard-fw/src/board.rs` says which
  GPIO does what. A board here must agree with it, and a test checks the two
  against each other, so a change to either lands in the same pull request.
- **Sources are committed; outputs are not.** The `.ato` files, `ato.yaml`, the
  lock file and the KiCad project belong in git. Gerbers, drill files, the BOM
  and pick-and-place files for a fab house are built from them for each release
  and attached to it, the way firmware images are.
- **No mains on these boards.** A board drives the low-voltage coil of a rated
  contactor or relay module; it never switches the machine's supply itself.

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
