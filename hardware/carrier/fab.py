"""Everything a board house needs to make and assemble the carrier.

    python3 fab.py            # needs KiCad 10 (pcbnew and kicad-cli)

Writes, under fab/ (git-ignored: these are build outputs, the board file is
the source):

    carrier-r1-gerbers.zip      Gerbers and drill files, for the bare board
    <variant>/bom.csv           what the fab assembles: PCBWay's BOM columns
    <variant>/cpl.csv           where it goes: the centroid (pick-and-place) file
    <variant>/hand-solder.csv   the through-hole parts you solder yourself
    <variant>/not-fitted.csv    the parts left off in this variant

Two variants, differing only in what is fitted:

    usb   every box except (d): power from USB-C, the 7-60 V input left off
    vin   box (d), inside a machine: the 7-60 V input and its converter fitted

The I2C reader links are fitted in both; the SPI ones never are (guide,
"Which reader").

The source board is not changed: the script works on a copy, marks that
variant's do-not-fit parts on it (the schematic's DNP field, which the
netlist import does not turn into KiCad's own flag), and puts the
drill/place origin at the board's bottom-left corner.
"""

import csv
import os
import re
import shutil
import subprocess
import sys
import zipfile
from collections import defaultdict

import pcbnew

HERE = os.path.dirname(os.path.abspath(__file__))
BOARD = os.path.join(HERE, "carrier.kicad_pcb")
OUT = os.path.join(HERE, "fab")
NAME = "carrier-r1"

LAYERS = ["F.Cu", "B.Cu", "F.Paste", "B.Paste", "F.SilkS", "B.SilkS", "F.Mask", "B.Mask", "Edge.Cuts"]

# The DNP parts that the vin variant fits: the 7-60 V input. Everything else
# marked DNP (the SPI links) stays off in both.
VIN_TAGS = {"J_VIN", "D_REV", "F_VIN", "D_TVS_VIN", "U_BUCK", "C_VIN", "R_SHDN", "C_BOOT",
            "D_CATCH", "L_BUCK", "C_VOUT", "R_FB_TOP", "R_FB_BOT", "U_OR_VIN"}
VARIANTS = {"usb": set(), "vin": VIN_TAGS}

KIND = {"R": "Resistor", "C": "Capacitor", "L": "Inductor", "D": "Diode", "F": "Fuse",
        "U": "IC", "Q": "MOSFET", "J": "Connector", "K": "Relay"}


def cli(*args):
    r = subprocess.run(["kicad-cli", *args], capture_output=True, text=True)
    if r.returncode:
        sys.exit(f"kicad-cli {args[:3]} failed:\n{r.stdout}{r.stderr}")


def field(f, name):
    return f.GetFieldText(name) if f.HasField(name) else ""


def tag(f):
    return field(f, "SKiDL Tag") or f.GetReference()


def package(f):
    """R_0603_1608Metric -> 0603; SOT-23-6 stays SOT-23-6."""
    name = str(f.GetFPID().GetLibItemName())
    m = re.match(r"^[A-Za-z]+_(\d{4})_\d{4}Metric", name)
    return m.group(1) if m else name


def ref_key(ref):
    m = re.match(r"([A-Z]+)(\d+)", ref)
    return (m.group(1), int(m.group(2))) if m else (ref, 0)


def prepare(variant):
    """A copy of the board for one variant: DNP flags set, origin at the
    bottom-left corner. Returns its path and the loaded board."""
    path = os.path.join(OUT, variant, f"{NAME}.kicad_pcb")
    b = pcbnew.LoadBoard(BOARD)
    edge = b.GetBoardEdgesBoundingBox()
    b.GetDesignSettings().SetAuxOrigin(pcbnew.VECTOR2I(edge.GetLeft(), edge.GetBottom()))
    for f in b.GetFootprints():
        f.SetDNP(bool(field(f, "DNP")) and tag(f) not in VARIANTS[variant])
    pcbnew.SaveBoard(path, b)
    return path, b


def write_csv(path, header, rows):
    with open(path, "w", newline="") as fh:
        w = csv.writer(fh)
        w.writerow(header)
        w.writerows(rows)


def grouped(parts):
    """Lines of a BOM: parts alike in value, package and MPN, one line."""
    lines = defaultdict(list)
    for f in parts:
        lines[(f.GetValue(), package(f), field(f, "MPN"), field(f, "Note"))].append(f)
    for (value, pkg, mpn, note), fs in sorted(lines.items(), key=lambda kv: ref_key(min(
            (f.GetReference() for f in kv[1]), key=ref_key))):
        refs = sorted((f.GetReference() for f in fs), key=ref_key)
        kind = KIND.get(ref_key(refs[0])[0], "")
        yield refs, value, pkg, mpn, kind, note


def bom(b, variant, d):
    fitted = [f for f in b.GetFootprints() if not f.IsDNP() and "MountingHole" not in f.GetFPIDAsString()]
    smd = [f for f in fitted if f.GetAttributes() & pcbnew.FP_SMD]
    tht = [f for f in fitted if f.GetAttributes() & pcbnew.FP_THROUGH_HOLE]
    unmarked = [f.GetReference() for f in fitted if not f.GetAttributes() & (pcbnew.FP_SMD | pcbnew.FP_THROUGH_HOLE)]
    assert not unmarked, f"neither SMD nor THT: {unmarked}"

    rows, need_mpn = [], []
    for i, (refs, value, pkg, mpn, kind, note) in enumerate(grouped(smd), 1):
        if not mpn and kind not in ("Resistor", "Capacitor"):
            need_mpn.append(f"{','.join(refs)} ({value})")
        desc = f"{kind} {value}".strip()
        if not mpn and kind == "Resistor":
            desc += "" if "%" in value or value == "0R" else ", 5% or better"
        rows.append([i, ",".join(refs), len(refs), "", mpn, desc, pkg, "SMD", note])
    write_csv(os.path.join(d, "bom.csv"),
              ["Item #", "Designator", "Qty", "Manufacturer", "Mfg Part #", "Description / Value",
               "Package/Footprint", "Type", "Your Instructions / Notes"], rows)

    write_csv(os.path.join(d, "hand-solder.csv"),
              ["Designator", "Qty", "Value", "Footprint", "Mfg Part #", "Notes"],
              [[",".join(refs), len(refs), value, pkg, mpn, note]
               for refs, value, pkg, mpn, kind, note in grouped(tht)])

    off = [f for f in b.GetFootprints() if f.IsDNP()]
    write_csv(os.path.join(d, "not-fitted.csv"), ["Designator", "Value", "Footprint"],
              [[f.GetReference(), f.GetValue(), package(f)]
               for f in sorted(off, key=lambda f: ref_key(f.GetReference()))])
    return len(smd), len(tht), len(off), need_mpn


def main():
    shutil.rmtree(OUT, ignore_errors=True)
    os.makedirs(OUT)

    for variant in VARIANTS:
        d = os.path.join(OUT, variant)
        os.makedirs(d)
        path, b = prepare(variant)

        if variant == "usb":  # the bare board is the same for both
            g = os.path.join(OUT, "gerbers")
            cli("pcb", "export", "gerbers", "--layers", ",".join(LAYERS), "--subtract-soldermask",
                "--use-drill-file-origin", "--check-zones", "-o", g + os.sep, path)
            cli("pcb", "export", "drill", "--excellon-separate-th", "--drill-origin", "plot",
                "--generate-map", "--map-format", "pdf", "-o", g + os.sep, path)
            with zipfile.ZipFile(os.path.join(OUT, f"{NAME}-gerbers.zip"), "w", zipfile.ZIP_DEFLATED) as z:
                for name in sorted(os.listdir(g)):
                    z.write(os.path.join(g, name), name)

        cpl = os.path.join(d, "cpl.csv")
        cli("pcb", "export", "pos", "--format", "csv", "--units", "mm", "--side", "both",
            "--smd-only", "--exclude-dnp", "--use-drill-file-origin", "-o", cpl, path)

        n_smd, n_tht, n_off, need_mpn = bom(b, variant, d)
        with open(cpl) as fh:
            placed = sum(1 for _ in fh) - 1
        assert placed == n_smd, f"{variant}: {placed} placements for {n_smd} BOM parts"
        for ext in (".kicad_pcb", ".kicad_pro", ".kicad_prl"):
            os.remove(path.removesuffix(".kicad_pcb") + ext)
        print(f"{variant}: fab fits {n_smd} parts, you solder {n_tht}, {n_off} left off")
        if need_mpn:
            print(f"  no part number yet: {'; '.join(need_mpn)}")
    print("written to", OUT)


if __name__ == "__main__":
    main()
