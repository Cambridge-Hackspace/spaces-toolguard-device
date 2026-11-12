"""The build guide's figures, drawn from the board itself.

    python3 figures.py        # needs KiCad 10's Python module (pcbnew)

Writes figures/*.svg: each side of the board, parts in outline, with the
parts a step is about filled in. They are generated rather than drawn so
they cannot drift from the layout. They are build outputs, not committed
(figures/ is git-ignored): run this before `quarto render`, as CI does. Also
writes 3D renders of each side with kicad-cli.
"""

import os
import subprocess

import pcbnew

HERE = os.path.dirname(os.path.abspath(__file__))
BOARD = os.path.join(HERE, "..", "carrier.kicad_pcb")
OUT = os.path.join(HERE, "figures")

b = pcbnew.LoadBoard(BOARD)
edge = next(d for d in b.GetDrawings() if d.GetLayer() == pcbnew.Edge_Cuts).GetBoundingBox()
X0, Y0 = pcbnew.ToMM(edge.GetLeft()), pcbnew.ToMM(edge.GetTop())
W, H = pcbnew.ToMM(edge.GetWidth()), pcbnew.ToMM(edge.GetHeight())


def tag_of(f):
    return f.GetFieldText("SKiDL Tag") if f.HasField("SKiDL Tag") else f.GetReference()


def parts():
    for f in b.GetFootprints():
        back = f.IsFlipped()
        poly = f.GetCourtyard(pcbnew.B_CrtYd if back else pcbnew.F_CrtYd)
        bb = poly.BBox() if poly.OutlineCount() else f.GetBoundingBox(False, False)
        yield {
            "ref": f.GetReference(),
            "tag": tag_of(f),
            "back": back,
            "hole": "MountingHole" in f.GetFPIDAsString(),
            "tht": any(p.GetAttribute() == pcbnew.PAD_ATTRIB_PTH for p in f.Pads()),
            "box": (pcbnew.ToMM(bb.GetLeft()) - X0, pcbnew.ToMM(bb.GetTop()) - Y0,
                    pcbnew.ToMM(bb.GetWidth()), pcbnew.ToMM(bb.GetHeight())),
            "pads": [(pcbnew.ToMM(p.GetPosition().x) - X0, pcbnew.ToMM(p.GetPosition().y) - Y0)
                     for p in f.Pads()],
        }


PARTS = list(parts())
INK, FAINT, HI, HOLE = "#3a3f44", "#c9ced3", "#e8742c", "#ffffff"

# Where the two modules sit on the front, as placed (README: their measured
# hole patterns): x, y, width, height in board mm.
MODULES = {
    "LCD 1602": (4.05, 8.0, 79.9, 36.0),
    "RFID-RC522": (23.55, 47.0, 60.45, 39.4),
}


def svg(name, side, highlight=(), caption=None, label_all=False):
    """One side of the board seen from that side: the back is mirrored, as it
    looks when you turn the board over. `highlight`: tags to fill in."""
    m = 3.0
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{-m} {-m} {W + 2 * m} {H + 2 * m}" '
           f'width="{(W + 2 * m) * 4:.0f}" height="{(H + 2 * m) * 4:.0f}" font-family="sans-serif">',
           f'<rect x="0" y="0" width="{W}" height="{H}" rx="3" fill="#2f6b4f"/>']

    def x(v, w=0):
        return (W - v - w) if side == "back" else v

    for p in PARTS:
        bx, by, bw, bh = p["box"]
        if p["hole"]:
            cx, cy = bx + bw / 2, by + bh / 2
            r = 1.6 if bw > 6.5 else 1.35
            out.append(f'<circle cx="{x(cx):.2f}" cy="{cy:.2f}" r="{r}" fill="{HOLE}"/>')
            continue
        on_this_side = (p["back"] == (side == "back"))
        if not on_this_side and not p["tht"]:
            continue  # SMD on the other side: not seen from here
        hi = p["tag"] in highlight
        if on_this_side:
            fill = HI if hi else "#ffffff"
            op = "0.95" if hi else "0.18"
            out.append(f'<rect x="{x(bx, bw):.2f}" y="{by:.2f}" width="{bw:.2f}" height="{bh:.2f}" '
                       f'fill="{fill}" fill-opacity="{op}" stroke="{HI if hi else FAINT}" stroke-width="0.15"/>')
        for px, py in p["pads"] if p["tht"] else []:
            out.append(f'<circle cx="{x(px):.2f}" cy="{py:.2f}" r="0.55" fill="#d9b44a"/>')
        if on_this_side and (hi or label_all):
            size = 1.6 if hi else 1.0
            out.append(f'<text x="{x(bx, bw) + bw / 2:.2f}" y="{by + bh / 2 + size / 3:.2f}" '
                       f'font-size="{size}" text-anchor="middle" fill="{INK if hi else "#ffffff"}" '
                       f'font-weight="{"bold" if hi else "normal"}">{p["ref"]}</text>')
    for module, (mx, my, mw, mh) in MODULES.items():
        out.append(f'<rect x="{x(mx, mw):.2f}" y="{my:.2f}" width="{mw}" height="{mh}" rx="1" fill="none" '
                   f'stroke="#9fd3ff" stroke-width="0.3" stroke-dasharray="1.2 0.8"/>')
        if side == "front":
            out.append(f'<text x="{x(mx, mw) + 1.5:.2f}" y="{my + 3:.2f}" font-size="2" fill="#9fd3ff">{module}</text>')
    title = caption or ("Front: the panel side" if side == "front" else "Back, as seen when turned over")
    out.append(f'<text x="0" y="{H + 2.4:.2f}" font-size="2" fill="{INK}">{title}</text>')
    out.append("</svg>")
    with open(os.path.join(OUT, name + ".svg"), "w") as f:
        f.write("\n".join(out))


os.makedirs(OUT, exist_ok=True)
svg("front", "front", label_all=True)
svg("back", "back", label_all=True)
svg("step-sockets-front", "front", {"J_RC522", "J_LCD"}, "Front: the RC522 and LCD sockets")
svg("step-led", "front", {"LED_STATUS"}, "Front: the status LED (flat side, pin 1, is K)")
svg("step-xiao", "back", {"J_XIAO_L", "J_XIAO_R"}, "Back: the XIAO's two 1x7 sockets")
svg("step-relays", "back", {"K1", "K2", "J_K1", "J_K2"}, "Back: the relays and their output terminals")
svg("step-terminals", "back", {"J_VIN", "J_SENSE", "J_STOP", "J_JOY", "J_QWIIC", "J_USB"},
    "Back: the connectors along the edges")
svg("step-links", "back", {"R_RC_I2C_SDA", "R_RC_I2C_SCL", "R_RC_I2C_A1", "R_RC_I2C_A0",
                           "R_RC_SPI_CS", "R_RC_SPI_MISO", "R_RC_SPI_SCLK", "R_RC_SPI_MOSI"},
    "Back: the reader's 0-ohm links, beside its header")
svg("step-vin", "back", {"J_VIN", "D_REV", "F_VIN", "D_TVS_VIN", "U_BUCK", "C_VIN", "R_SHDN", "C_BOOT",
                         "D_CATCH", "L_BUCK", "C_VOUT", "R_FB_TOP", "R_FB_BOT", "U_OR_VIN"},
    "Back: the 7-60 V input and its converter, fitted only inside a machine")

for side in ("top", "bottom"):
    subprocess.run(["kicad-cli", "pcb", "render", "--side", side, "--width", "1200", "--height", "1300",
                    "-o", os.path.join(OUT, f"render-{'front' if side == 'top' else 'back'}.png"), BOARD],
                   check=True, capture_output=True)
print("figures written to", OUT)
