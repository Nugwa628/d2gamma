"""Makes crates/d2-client/art/send-stash.png, the cel of the inventory's
"Send stackables to stash" button (D2RUST's own art: an iron-banded chest
with a brass lock under a gold bevelled down arrow).

    py -3 -I tools/stash-icon/send_stash_icon.py <act 1 palette> [out.png]

<act 1 palette> is `data/global/palette/act1/pal.dat` from the player's own
game files (768 bytes, blue-green-red), or a 16 x 16 swatch image of it.
The palette only picks the indices; it is not written out.

The PNG is 96 x 32, 8-bit greyscale, three 32 x 32 frames side by side:
normal, hover (one step brighter), pressed (darker). Each value is an act 1
palette index (0 transparent), drawn through the game's palette like a
DC6 cel. The client draws it over the game's button frame
(crates/d2-client/src/ui/own_art.rs).
"""
import os
import sys

import numpy as np
from PIL import Image

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import paint as P  # noqa: E402

SIDE = 32
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..",
                   "crates", "d2-client", "art", "send-stash.png")


def read_palette(path):
    if path.lower().endswith(".dat"):
        raw = open(path, "rb").read()
        if len(raw) != 768:
            sys.exit(f"{path}: {len(raw)} bytes, a palette has 768")
        return np.frombuffer(raw, np.uint8).reshape(256, 3)[:, ::-1]
    im = Image.open(path).convert("RGB")
    if im.size != (16, 16):
        sys.exit(f"{path}: {im.size}, a palette swatch is 16 x 16")
    return np.array(im).reshape(256, 3)


def chest_and_arrow():
    """The 32 px art: the chest in the lower part of the content box, the
    arrow from the box top into its lid."""
    x0, y0, x1, y1 = 5.0, 5.0, 28.0, 28.0
    w, h = x1 - x0, y1 - y0
    ch = P.Chest(x0 + w * 0.02, y0 + h * 0.42, x1 - w * 0.02, y1, detail=2)
    cx = x0 + w * 0.02 + ch.fw * 0.5

    def chest(cv):
        ch.body(cv)
        ch.lid_closed(cv)
        ch.lockplate(cv)

    def arrow(cv):
        P.down_arrow(cv, cx, y0 - 0.5, y0 + h * 0.60, w * 0.62)

    out = Image.new("RGBA", (SIDE, SIDE), (0, 0, 0, 0))
    for i, draw in enumerate((chest, arrow)):
        cv = P.Canvas(SIDE, SIDE, 21 + i)
        draw(cv)
        out.alpha_composite(cv.finish())
    return out


def recolor(img, k):
    """The art `k` times as bright, snapped back to the palette; the dark
    outline pixels kept."""
    a = np.array(img).astype(float)
    solid = a[..., 3] > 0
    dark = a[..., :3].sum(-1) < 40
    rgb = P.quantize(np.clip(a[..., :3] * k, 0, 255))
    rgb[dark] = a[..., :3][dark]
    out = a.copy()
    out[..., :3] = np.where(solid[..., None], rgb, a[..., :3])
    return Image.fromarray(out.astype(np.uint8), "RGBA")


def indices(img):
    """Palette indices of an RGBA cel: 0 where transparent, else the first
    index (not 0 or 255) of the pixel's colour."""
    a = np.array(img)
    out = np.zeros(a.shape[:2], np.uint8)
    first = {}
    for i, c in enumerate(P.PALETTE.astype(int)):
        if i not in (0, 255):
            first.setdefault(tuple(c), i)
    for y, x in zip(*np.nonzero(a[..., 3])):
        out[y, x] = first[tuple(int(v) for v in a[y, x, :3])]
    return out


def main():
    if len(sys.argv) not in (2, 3):
        sys.exit(__doc__)
    P.set_palette(read_palette(sys.argv[1]))
    out = sys.argv[2] if len(sys.argv) == 3 else OUT
    art = chest_and_arrow()
    frames = [art, recolor(art, 1.18), recolor(art, 0.85)]
    strip = np.concatenate([indices(f) for f in frames], axis=1)
    Image.fromarray(strip, "L").save(out, optimize=True)
    print(f"{out}: {strip.shape[1]} x {strip.shape[0]}, {len(frames)} frames")


main()
