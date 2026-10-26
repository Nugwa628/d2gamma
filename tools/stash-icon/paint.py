"""The painter of the inventory's "Send stackables to stash" icon (D2RUST's
own art; `send_stash_icon.py` drives it).

Shapes are rendered 8x supersampled with light from the top left,
box-filtered down, given grain, then snapped to the act 1 palette's colours
(no colour outside the palette, no alpha blending: each pixel is opaque or
transparent like a DC6 cel) and given a dark item-style outline. Nothing is
copied or traced from game art. The palette comes from the player's own
files at generation time ([`set_palette`]); it is never stored here.
"""
import numpy as np
from PIL import Image, ImageDraw

SS = 8
W8 = np.array([2.0, 4.0, 3.0])
# The palette's 256 colours, and the ones a cel may use: index 0 is
# transparent in cels, 255 is a pure white the art never uses.
PALETTE = None
PAL = None
OUTLINE = None


def set_palette(colors):
    """Sets the 256 RGB colours (index order) the art snaps to."""
    global PALETTE, PAL, OUTLINE
    PALETTE = np.asarray(colors, float).reshape(256, 3)
    PAL = np.array([c for i, c in enumerate(PALETTE) if i not in (0, 255)])
    OUTLINE = near((10, 8, 4))


def quantize(rgb):
    """Nearest palette colour per pixel (weighted RGB)."""
    flat = rgb.reshape(-1, 3)
    d = (((flat[:, None, :] - PAL[None, :, :]) ** 2) * W8).sum(-1)
    return PAL[d.argmin(1)].reshape(rgb.shape)


def near(c):
    return tuple(int(v) for v in quantize(np.array([[c]], float))[0, 0])


# Materials (linear-ish RGB before palette snap).
WOOD = np.array([80, 52, 28.0])
WOOD_D = np.array([46, 30, 16.0])
IRON = np.array([92, 90, 84.0])
BRASS = np.array([176, 142, 76.0])
GOLD = np.array([212, 172, 92.0])
INSIDE = np.array([22, 14, 8.0])
LEATHER = np.array([112, 78, 48.0])


class Canvas:
    def __init__(self, w, h, seed=1):
        self.w, self.h = w, h
        H, W = h * SS, w * SS
        self.rgb = np.zeros((H, W, 3))
        self.a = np.zeros((H, W))
        yy, xx = np.mgrid[0:H, 0:W]
        self.X = (xx + 0.5) / SS
        self.Y = (yy + 0.5) / SS
        self.rng = np.random.default_rng(seed)
        self.dark = np.zeros((h, w), bool)  # pixels forced to outline colour

    def _mask(self, draw_fn):
        m = Image.new("L", (self.w * SS, self.h * SS), 0)
        draw_fn(ImageDraw.Draw(m))
        return np.array(m) > 127

    def _paint(self, mask, color):
        c = color(self.X, self.Y) if callable(color) else np.broadcast_to(np.array(color, float), self.rgb.shape)
        self.rgb[mask] = c[mask]
        self.a[mask] = 1.0

    def poly(self, pts, color):
        P = [(x * SS, y * SS) for x, y in pts]
        self._paint(self._mask(lambda d: d.polygon(P, fill=255)), color)

    def ellipse(self, x0, y0, x1, y1, color):
        self._paint(self._mask(lambda d: d.ellipse((x0 * SS, y0 * SS, x1 * SS, y1 * SS), fill=255)), color)

    def rect(self, x0, y0, x1, y1, color):
        self.poly([(x0, y0), (x1, y0), (x1, y1), (x0, y1)], color)

    def line(self, pts, width, color):
        P = [(x * SS, y * SS) for x, y in pts]
        self._paint(self._mask(lambda d: d.line(P, fill=255, width=max(1, int(width * SS)))), color)

    def finish(self, grain=0.07, outline=True):
        h, w = self.h, self.w
        a = self.a.reshape(h, SS, w, SS).mean((1, 3))
        rgb = (self.rgb * self.a[..., None]).reshape(h, SS, w, SS, 3).sum((1, 3))
        rgb = rgb / np.maximum(a * SS * SS, 1e-6)[..., None]
        solid = a >= 0.5
        rgb = rgb * (1 + grain * self.rng.standard_normal((h, w, 1)))
        rgb = quantize(np.clip(rgb, 0, 255))
        rgb[self.dark] = OUTLINE
        if outline:
            pad = np.pad(solid, 1)
            nb = pad[:-2, 1:-1] | pad[2:, 1:-1] | pad[1:-1, :-2] | pad[1:-1, 2:]
            ring = nb & ~solid
            rgb[ring] = OUTLINE
            solid = solid | ring
        out = np.zeros((h, w, 4), np.uint8)
        out[..., :3] = rgb.astype(np.uint8)
        out[..., 3] = np.where(solid, 255, 0)
        return Image.fromarray(out, "RGBA")


# ---- shading helpers -------------------------------------------------------
def lit(base, x0, y0, x1, y1, k=0.35, mul=1.0):
    """Base colour brighter toward the top left of the box (x0,y0)-(x1,y1)."""
    base = np.array(base, float)

    def f(X, Y):
        t = ((X - x0) / max(x1 - x0, 1e-3) + (Y - y0) / max(y1 - y0, 1e-3)) / 2
        return base * mul * (1 + k * (0.5 - np.clip(t, 0, 1)))[..., None]
    return f


def grain_wood(base, x0, y0, x1, y1, mul=1.0, seed=0, k=0.35):
    sh = lit(base, x0, y0, x1, y1, k=k, mul=mul)
    ph = np.random.default_rng(seed).uniform(0, 6.28, 3)

    def f(X, Y):
        g = np.sin(Y * 3.1 + 1.3 * np.sin(X * 0.9 + ph[0]) + ph[1]) * 0.5 + np.sin(Y * 7.3 + X * 0.4 + ph[2]) * 0.25
        return sh(X, Y) * (1 + 0.16 * g)[..., None]
    return f


def strap(base, x0, x1):
    """Metal strap lit across its width: bright left edge, dark right edge."""
    base = np.array(base, float)

    def f(X, Y):
        t = np.clip((X - x0) / max(x1 - x0, 1e-3), 0, 1)
        return base * (1.35 - 0.75 * t)[..., None]
    return f


def hstrap(base, y0, y1):
    base = np.array(base, float)

    def f(X, Y):
        t = np.clip((Y - y0) / max(y1 - y0, 1e-3), 0, 1)
        return base * (1.35 - 0.75 * t)[..., None]
    return f


def rivet(cv, x, y, r, base=(200, 196, 180)):
    cv.ellipse(x - r, y - r, x + r, y + r, lit(base, x - r, y - r, x + r, y + r, k=1.4))


# ---- chest -----------------------------------------------------------------
class Chest:
    """Oblique 3/4 chest: front face toward the viewer, depth going up-right."""

    def __init__(self, x0, y0, x1, y1, detail=2, metal=IRON, lock=BRASS, seed=3):
        self.x0, self.y0, self.x1, self.y1 = x0, y0, x1, y1
        w, h = x1 - x0, y1 - y0
        self.dx, self.dy = w * 0.20, -h * 0.20
        self.fw = w - self.dx                 # front face width
        self.seam = y0 + h * 0.46             # lid/body seam on the front
        self.detail, self.metal, self.lock, self.seed = detail, metal, lock, seed

    def body(self, cv, open_=False):
        x0, x1f, y1, seam = self.x0, self.x0 + self.fw, self.y1, self.seam
        dx, dy = self.dx, self.dy
        # right side
        cv.poly([(x1f, seam), (x1f + dx, seam + dy), (x1f + dx, y1 + dy), (x1f, y1)],
                grain_wood(WOOD, x1f, seam, x1f + dx, y1, mul=0.55, seed=self.seed + 1))
        # front
        cv.poly([(x0, seam), (x1f, seam), (x1f, y1), (x0, y1)],
                grain_wood(WOOD, x0, seam, x1f, y1, mul=0.95, seed=self.seed))
        if open_:
            # rim and dark inside seen from above
            cv.poly([(x0, seam), (x1f, seam), (x1f + dx, seam + dy), (x0 + dx, seam + dy)],
                    lit(INSIDE, x0, seam + dy, x1f + dx, seam, k=-0.6))
            cv.line([(x0, seam), (x1f, seam), (x1f + dx, seam + dy)], 0.7, lit(self.metal, x0, seam, x1f, seam, k=0.6))
        self._front_metal(cv, seam, y1)
        # bottom iron band
        cv.rect(x0, y1 - max(0.9, (y1 - seam) * 0.16), x1f, y1, hstrap(self.metal, y1 - 1.2, y1))
        if self.detail >= 2:
            # plank seam
            ys = seam + (y1 - seam) * 0.48
            cv.line([(x0 + 0.3, ys), (x1f - 0.3, ys)], 0.45, WOOD_D * 0.6)

    def _front_metal(self, cv, ya, yb, rivets=True):
        x0, fw = self.x0, self.fw
        sw = max(1.2, fw * 0.12)
        for u in (0.16, 0.84):
            cx = x0 + fw * u
            cv.rect(cx - sw / 2, ya, cx + sw / 2, yb, strap(self.metal, cx - sw / 2, cx + sw / 2))
            if rivets and self.detail >= 2 and yb - ya > 4:
                rivet(cv, cx, ya + (yb - ya) * 0.35, 0.42)
                rivet(cv, cx, ya + (yb - ya) * 0.72, 0.42)

    def lid_closed(self, cv):
        x0, x1f, y0, seam = self.x0, self.x0 + self.fw, self.y0, self.seam
        dx, dy = self.dx, self.dy
        top = y0 - dy  # front top of the dome's flat part
        r = (seam - top) * 0.55
        # dome surface: hull of the front lid outline and its copy at depth
        front = [(x0, seam), (x0, top + r), (x0 + r * 0.35, top + r * 0.25), (x0 + fw_q(self, 0.5), top - r * 0.15),
                 (x1f - r * 0.35, top + r * 0.25), (x1f, top + r), (x1f, seam)]
        back = [(x + dx, y + dy) for x, y in front]
        hull = convex_hull(front + back)
        cv.poly(hull, grain_wood(WOOD, x0, top + dy, x1f + dx, seam, mul=1.25, seed=self.seed + 2, k=0.6))
        # right end of the lid (darker)
        cv.poly([(x1f, top + r), (x1f + dx, top + r + dy), (x1f + dx, seam + dy), (x1f, seam)],
                grain_wood(WOOD, x1f, top, x1f + dx, seam, mul=0.6, seed=self.seed + 3))
        # lid front
        cv.poly(front, grain_wood(WOOD, x0, top, x1f, seam, mul=1.0, seed=self.seed + 4))
        # straps over the dome
        sw = max(1.2, self.fw * 0.12)
        for u in (0.16, 0.84):
            cx = x0 + self.fw * u
            ytop = arc_y(front, cx)
            cv.poly([(cx - sw / 2, ytop), (cx - sw / 2 + dx, ytop + dy), (cx + sw / 2 + dx, ytop + dy), (cx + sw / 2, ytop)],
                    lit(self.metal, cx, ytop + dy, cx + dx, ytop, k=0.8, mul=1.25))
            cv.rect(cx - sw / 2, ytop, cx + sw / 2, seam, strap(self.metal, cx - sw / 2, cx + sw / 2))
        # lid rim band at the seam
        cv.rect(x0, seam - max(0.8, (seam - top) * 0.2), x1f, seam + 0.35, hstrap(self.metal, seam - 1, seam))
        cv.poly([(x1f, seam - 0.9), (x1f + dx, seam - 0.9 + dy), (x1f + dx, seam + dy + 0.35), (x1f, seam + 0.35)],
                lambda X, Y: self.metal * 0.55 + 0 * X[..., None])

    def lid_open(self, cv):
        """Lid swung back past upright: we see its dark underside above the
        back edge and the lit edge of its top."""
        x0, x1f, seam = self.x0, self.x0 + self.fw, self.seam
        dx, dy = self.dx, self.dy
        lh = (self.y1 - seam) * 0.80
        bx0, bx1, by = x0 + dx, x1f + dx, seam + dy
        lean = dx * 0.6
        quad = [(bx0, by), (bx1, by), (bx1 + lean, by - lh), (bx0 + lean, by - lh)]
        cv.poly(quad, lambda X, Y: lit(WOOD_D, bx0, by - lh, bx1, by, k=-0.5, mul=0.75)(X, Y))
        # lid edge (curved top, lit from the top left)
        cv.line([(bx0 + lean, by - lh), (bx1 + lean, by - lh)], max(0.8, lh * 0.18),
                lit(WOOD, bx0, by - lh - 1, bx1, by - lh, mul=1.5, k=0.6))
        cv.line([(bx1, by), (bx1 + lean, by - lh)], 0.6, WOOD * 0.6)

    def lockplate(self, cv, arrow=False, size=1.0):
        cx = self.x0 + self.fw * 0.5
        seam = self.seam
        pw = max(2.2, self.fw * 0.22) * size
        ph = max(2.4, (self.y1 - seam) * 0.55) * size
        if arrow:
            # brass down-arrow plate: shaft from the lid, head on the body
            sh = pw * 0.42
            ytop = seam - ph * 0.9
            yhead = seam + ph * 0.05
            ytip = seam + ph * 0.95
            pts = [(cx - sh, ytop), (cx + sh, ytop), (cx + sh, yhead), (cx + pw * 0.95, yhead),
                   (cx, ytip), (cx - pw * 0.95, yhead), (cx - sh, yhead)]
            cv.poly(pts, gold_facets(self.lock, cx, ytop, ytip))
            cv.line([(cx - sh * 0.4, ytop + 0.4), (cx - sh * 0.4, yhead)], 0.35, self.lock * 1.45)
        else:
            cv.rect(cx - pw / 2, seam - ph * 0.25, cx + pw / 2, seam + ph * 0.75,
                    lit(self.lock, cx - pw / 2, seam - ph * 0.25, cx + pw / 2, seam + ph * 0.75, k=0.9))
            cv.rect(cx - 0.35, seam + ph * 0.15, cx + 0.35, seam + ph * 0.55, (14, 10, 6))


def fw_q(ch, u):
    return ch.fw * u


def arc_y(poly, x):
    """Top y of the lid front outline at x (piecewise linear)."""
    pts = poly[1:-1]
    for (xa, ya), (xb, yb) in zip(pts, pts[1:]):
        if xa <= x <= xb:
            t = (x - xa) / max(xb - xa, 1e-6)
            return ya + (yb - ya) * t
    return pts[0][1]


def convex_hull(pts):
    pts = sorted(set(pts))
    if len(pts) <= 2:
        return pts

    def cross(o, a, b):
        return (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    lower, upper = [], []
    for p in pts:
        while len(lower) >= 2 and cross(lower[-2], lower[-1], p) <= 0:
            lower.pop()
        lower.append(p)
    for p in reversed(pts):
        while len(upper) >= 2 and cross(upper[-2], upper[-1], p) <= 0:
            upper.pop()
        upper.append(p)
    return lower[:-1] + upper[:-1]


# ---- arrow and small things ------------------------------------------------
def gold_facets(base, cx, ytop, ytip):
    """Bevelled metal arrow: left facet lit, right facet in shade, light ridge."""
    base = np.array(base, float)

    def f(X, Y):
        left = X < cx
        m = np.where(left, 1.18, 0.68)
        m = m * (1 + 0.18 * (0.5 - np.clip((Y - ytop) / max(ytip - ytop, 1e-3), 0, 1)))
        ridge = np.abs(X - cx) < 0.30
        m = np.where(ridge, 1.38, m)
        return base * m[..., None]
    return f


def down_arrow(cv, cx, ytop, ytip, width, base=GOLD, shaft=0.36):
    sh = width * shaft / 2
    yhead = ytop + (ytip - ytop) * 0.40
    pts = [(cx - sh, ytop), (cx + sh, ytop), (cx + sh, yhead), (cx + width / 2, yhead),
           (cx, ytip), (cx - width / 2, yhead), (cx - sh, yhead)]
    cv.poly(pts, gold_facets(base, cx, ytop, ytip))
