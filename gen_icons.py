#!/usr/bin/env python3
"""Generate the menubar tray template icon (monochrome) and the colored app
source icon. Marvis-style: rounded square with a robot visor (eyes).

No external SVG tooling required — drawn directly with Pillow.
"""
import os
from PIL import Image, ImageDraw

ICONS = os.path.join(os.path.dirname(__file__), "src-tauri", "icons")
os.makedirs(ICONS, exist_ok=True)


def draw_robot(d, size, body, visor, eye, *, body_fills_canvas=False, mouth=False):
    """Draw the rounded-square robot face.

    body_fills_canvas=True draws the body edge-to-edge (for the app icon, which
    gets its own corner mask from the .icns pipeline); otherwise insets 1px.
    """
    if body_fills_canvas:
        x0, y0, x1, y1 = 0, 0, size - 1, size - 1
    else:
        x0, y0, x1, y1 = 1, 1, size - 2, size - 2
    r = int(size / 5)
    d.rounded_rectangle([x0, y0, x1, y1], radius=r, fill=body)

    # Visor: horizontal rounded bar.
    bw = int(size * 0.64)
    bh = int(size * 0.27)
    bx = (size - bw) // 2
    by = int(size * 0.36)
    d.rounded_rectangle([bx, by, bx + bw, by + bh], radius=bh // 2, fill=visor)

    # Left eye (circle) + right eye (bar) → "wink" robot face.
    ey = by + bh // 2
    ex = bx + int(bw * 0.20)
    er = int(size * 0.10)
    d.ellipse([ex - er, ey - er, ex + er, ey + er], fill=eye)
    rx = bx + int(bw * 0.52)
    rw = int(bw * 0.30)
    rh = max(2, int(size * 0.08))
    d.rounded_rectangle([rx, ey - rh, rx + rw, ey + rh], radius=rh, fill=eye)

    if mouth:
        mw = int(size * 0.22)
        mh = max(2, int(size * 0.03))
        mx = (size - mw) // 2
        my = int(size * 0.70)
        d.rounded_rectangle([mx, my, mx + mw, my + mh], radius=mh, fill=visor)


def make_tray():
    """macOS template icon: opaque black shape on transparent bg; OS recolors.

    Generates tray.png (22) and tray@2x.png (44). lib.rs embeds tray.png.
    """
    for size, name in [(22, "tray.png"), (44, "tray@2x.png")]:
        img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
        d = ImageDraw.Draw(img)
        # Black body, transparent visor cut-out, black eyes → reads as a glyph.
        draw_robot(
            d,
            size,
            body=(0, 0, 0, 255),
            visor=(0, 0, 0, 0),
            eye=(0, 0, 0, 255),
        )
        img.save(os.path.join(ICONS, name))


def make_app_icon():
    """Colored 1024px source icon for the .app / Dock (Marvis green)."""
    size = 1024
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    draw_robot(
        d,
        size,
        body=(90, 158, 47, 255),     # Marvis-ish green (#5a9e2f)
        visor=(45, 90, 22, 255),     # dark green visor (#2d5a16)
        eye=(255, 255, 255, 255),    # white eyes
        body_fills_canvas=True,
        mouth=True,
    )
    img.save(os.path.join(ICONS, "icon.png"))
    img.save(os.path.join(ICONS, "source.png"))


if __name__ == "__main__":
    make_tray()
    make_app_icon()
    print("icons written to", ICONS)
