"""Draws Hushlist's icon sheet: mixins/hushlist/api/data/icons.png.

Each icon is drawn at 8x and scaled down, so edges are smooth. Horizon draws
mod images untinted, so every icon is drawn once per colour: one row per
colour, one column per icon. Cells are 24 px, drawn into 12 GUI pixels.

Usage: python tools/make_icons.py [--preview PATH]
"""
import argparse
import math
from pathlib import Path

from PIL import Image, ImageDraw

CELL = 24
SS = 8  # supersampling
ICONS = ["speaker_high", "speaker_low", "speaker_off", "reset", "timer", "play", "snowflake",
         "close", "chevron_right", "chevron_down", "view_list", "view_groups", "sort", "search"]
COLOURS = {  # row order is the contract with lib.rs
    "text": (0xEE, 0xEC, 0xF4),
    "dim": (0x9D, 0x99, 0xAD),
    "faint": (0x6B, 0x67, 0x79),
    "own": (0x6F, 0xF0, 0xDA),
    "inherited": (0x4E, 0x7A, 0x75),
    "amber": (0xF5, 0xB1, 0x42),
    "ice_text": (0x12, 0x30, 0x3F),
}
OUT = Path(__file__).resolve().parents[1] / "mixins" / "hushlist" / "api" / "data" / "icons.png"


def s(v):
    return v * SS


def line(d, pts, w):
    d.line([(s(x), s(y)) for x, y in pts], fill=255, width=round(s(w)), joint="curve")
    for x, y in (pts[0], pts[-1]):  # round caps
        r = s(w) / 2
        d.ellipse([s(x) - r, s(y) - r, s(x) + r, s(y) + r], fill=255)


def arc(d, cx, cy, r, start, end, w):
    pts = []
    steps = 48
    for i in range(steps + 1):
        a = math.radians(start + (end - start) * i / steps)
        pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
    line(d, pts, w)


def speaker_body(d):
    d.polygon([(s(3), s(9)), (s(7), s(9)), (s(12.5), s(4)), (s(12.5), s(20)), (s(7), s(15)), (s(3), s(15))], fill=255)


def draw(name):
    img = Image.new("L", (s(CELL), s(CELL)), 0)
    d = ImageDraw.Draw(img)
    if name.startswith("speaker"):
        speaker_body(d)
        if name == "speaker_off":
            line(d, [(15.5, 9), (21, 15)], 2.2)
            line(d, [(21, 9), (15.5, 15)], 2.2)
        else:
            arc(d, 12.5, 12, 4.6, -48, 48, 2.0)
            if name == "speaker_high":
                arc(d, 12.5, 12, 8.6, -50, 50, 2.0)
    elif name == "reset":
        arc(d, 12, 12.5, 7, 200, 470, 2.2)  # open at the top left
        d.polygon([(s(2.2), s(6.8)), (s(9.2), s(6.4)), (s(4.8), s(12.2))], fill=255)
    elif name == "timer":
        arc(d, 12, 13.5, 7.5, 0, 360, 2.0)
        line(d, [(12, 13.5), (12, 9)], 2.0)
        line(d, [(9.5, 2.8), (14.5, 2.8)], 2.2)
        line(d, [(12, 3), (12, 5.6)], 2.0)
        line(d, [(18.6, 5.6), (20.2, 7.2)], 2.0)
    elif name == "play":
        d.polygon([(s(7.5), s(5)), (s(19), s(12)), (s(7.5), s(19))], fill=255)
    elif name == "snowflake":
        for deg in (90, 30, 150):
            a = math.radians(deg)
            dx, dy = 8.5 * math.cos(a), 8.5 * math.sin(a)
            line(d, [(12 - dx, 12 - dy), (12 + dx, 12 + dy)], 1.8)
            for sign in (1, -1):
                ex, ey = 12 + sign * dx, 12 + sign * dy
                bx, by = 12 + sign * dx * 0.55, 12 + sign * dy * 0.55
                for turn in (35, -35):
                    b = math.radians(deg + turn + (180 if sign > 0 else 0))
                    line(d, [(bx, by), (bx + 3.2 * math.cos(b), by + 3.2 * math.sin(b))], 1.6)
                _ = (ex, ey)
    elif name == "close":
        line(d, [(6.5, 6.5), (17.5, 17.5)], 2.4)
        line(d, [(17.5, 6.5), (6.5, 17.5)], 2.4)
    elif name == "chevron_right":
        line(d, [(9, 6), (15, 12), (9, 18)], 2.4)
    elif name == "chevron_down":
        line(d, [(6, 9), (12, 15), (18, 9)], 2.4)
    elif name == "view_list":
        for y in (7, 12, 17):
            line(d, [(5, y), (19, y)], 2.0)
    elif name == "view_groups":
        # The same three lines as the list, the two lower ones indented: a group and its sounds.
        line(d, [(5, 7), (19, 7)], 2.0)
        for y in (12, 17):
            line(d, [(10, y), (19, y)], 2.0)
    elif name == "sort":
        line(d, [(8, 4.5), (8, 19)], 1.8)
        line(d, [(4.5, 15.5), (8, 19), (11.5, 15.5)], 1.8)
        line(d, [(16, 19.5), (16, 5)], 1.8)
        line(d, [(12.5, 8.5), (16, 5), (19.5, 8.5)], 1.8)
    elif name == "search":
        arc(d, 10, 10, 5.8, 0, 360, 2.2)
        line(d, [(14.4, 14.4), (19.5, 19.5)], 2.6)
    return img.resize((CELL, CELL), Image.LANCZOS)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--preview", type=Path)
    args = ap.parse_args()
    masks = [draw(n) for n in ICONS]
    sheet = Image.new("RGBA", (CELL * len(ICONS), CELL * len(COLOURS)), (0, 0, 0, 0))
    for row, rgb in enumerate(COLOURS.values()):
        for col, mask in enumerate(masks):
            tile = Image.new("RGBA", (CELL, CELL), rgb + (0,))
            tile.putalpha(mask)
            sheet.paste(tile, (col * CELL, row * CELL))
    OUT.parent.mkdir(parents=True, exist_ok=True)
    sheet.save(OUT, optimize=True)
    print("wrote", OUT, sheet.size)
    if args.preview:
        bg = Image.new("RGBA", sheet.size, (0x14, 0x13, 0x19, 255))
        bg.alpha_composite(sheet)
        bg.resize((sheet.width * 4, sheet.height * 4), Image.NEAREST).save(args.preview)
        print("preview", args.preview)


if __name__ == "__main__":
    main()
