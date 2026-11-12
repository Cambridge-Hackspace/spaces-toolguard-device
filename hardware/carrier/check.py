"""Checks the carrier's schematic against the firmware, and against its own
footprints (#25). `uv run python check.py`; exits non-zero on any failure.

1. **The pin map.** For every pin role in the firmware's board file
   (toolguard-fw/boards/c5-xiao-carrier.toml), the net named for that role
   must reach the XIAO pad whose GPIO the board file names -- and only that
   pad. Every XIAO pad the board wires must be a role the firmware has.
   Either side moving without the other fails here.
2. **Pins against pads.** Every connected pin of every part must exist as a
   pad on its footprint, read from KiCad's own .kicad_mod. A symbol and a
   footprint numbered differently would otherwise make a board whose wires
   go to the wrong legs.
3. **ERC is clean**: no errors and no warnings. Intentional exceptions are
   declared in carrier.py, so a new warning always means something.
"""

import io
import os
import re
import sys
import tomllib
from contextlib import redirect_stderr, redirect_stdout

HERE = os.path.dirname(os.path.abspath(__file__))
BOARD = os.path.join(HERE, "..", "..", "toolguard-fw", "boards", "c5-xiao-carrier.toml")
FOOTPRINTS = os.environ.get("KICAD10_FOOTPRINT_DIR", "/usr/share/kicad/footprints")

failures = []


def fail(msg):
    failures.append(msg)


def pin_map(design, board):
    roles, xiao = design["roles"], design["xiao"]
    pins, pads = board["pins"], board["pads"]
    gpio_to_pad = {gpio: pad for pad, gpio in pads.items()}
    seen_pads = set()
    for role, gpio in pins.items():
        pad = gpio_to_pad.get(gpio)
        if pad is None:
            # On the XIAO itself, not an edge pad: setup is its BOOT button.
            continue
        net = roles.get(role.upper())
        if net is None:
            fail(f"pin map: the firmware's `{role}` (GPIO{gpio}, XIAO {pad}) has no net "
                 f"`{role.upper()}` on the board")
            continue
        reached = sorted(p for p, socket_pin in xiao.items()
                         if socket_pin.net is not None and socket_pin.net is net)
        if reached != [pad]:
            fail(f"pin map: `{role}` is GPIO{gpio}, XIAO {pad}, in the firmware, but its net "
                 f"reaches XIAO {reached or 'nothing'} on the board")
        seen_pads.add(pad)
    for pad, socket_pin in xiao.items():
        if pad in ("5V", "GND", "3V3") or pad in seen_pads:
            continue
        if socket_pin.net is not None and socket_pin.is_connected():
            fail(f"pin map: XIAO {pad} (GPIO{pads.get(pad)}) is wired on the board "
                 f"(net {socket_pin.net.name}) but is no role in the firmware")


def footprint_pads(fp):
    lib, name = fp.split(":", 1)
    path = os.path.join(FOOTPRINTS, f"{lib}.pretty", f"{name}.kicad_mod")
    with open(path) as f:
        text = f.read()
    return set(re.findall(r'\(pad\s+"([^"]*)"', text))


def pins_against_pads(circuit):
    for part in circuit.parts:
        if not part.footprint:
            fail(f"pads: {part.ref} ({part.name}) has no footprint")
            continue
        try:
            pads = footprint_pads(part.footprint)
        except FileNotFoundError:
            fail(f"pads: {part.ref}'s footprint {part.footprint} is not in KiCad's library")
            continue
        for p in part.pins:
            if p.is_connected() and str(p.num) not in pads:
                fail(f"pads: {part.ref} pin {p.num} ({p.name}) is connected, but "
                     f"{part.footprint} has no pad {p.num} (pads: {sorted(pads)})")


def main():
    with open(BOARD, "rb") as f:
        board = tomllib.load(f)

    sink = io.StringIO()
    with redirect_stdout(sink), redirect_stderr(sink):
        import builtins

        import carrier
        from skidl import erc_logger

        # Installed as a builtin by SKiDL, like NC; not exported.
        default_circuit = builtins.default_circuit

        design = carrier.build()
        default_circuit.ERC()

    pin_map(design, board)
    pins_against_pads(default_circuit)
    errors, warnings = erc_logger.error.count, erc_logger.warning.count
    if errors or warnings:
        fail(f"ERC: {errors} errors and {warnings} warnings; run carrier.py to see them")

    if failures:
        print(f"{len(failures)} problem(s):")
        for msg in failures:
            print("  -", msg)
        sys.exit(1)
    print(f"ok: pin map matches {os.path.relpath(BOARD, HERE)}; "
          f"{len(default_circuit.parts)} parts' pins all exist as pads; ERC clean")


if __name__ == "__main__":
    main()
