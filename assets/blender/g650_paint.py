# Paints the Gulfstream G650 and bakes its paint into assets/g650_paint.jpg (plain Python, with Pillow and NumPy):
#
#   python assets/blender/g650_paint.py [--bake-only] [--plain] [<dir of pictures>] [<out.jpg>]
#
# --plain paints the same skin without its scheme (all white, no belly, no
# cheatline, no fin flash, no registration) into <view>_plain.png and bakes
# assets/g650_plain.jpg: the base the game paints its other liveries on
# (g650::LIVERIES, plane.wgsl `livery_g650`). Its pictures are not kept:
# --plain --bake-only needs a full --plain run first.
#
# As Concorde's (concorde_paint.py, whose machinery this is): the paint is four
# pictures of the aircraft, from straight above, from straight below, from its
# left side and from its right side (assets/blender/g650_paint/top.png,
# bottom.png, side.png, right.png), which the game projects back onto the skin
# (plane.wgsl, `hornet_paint`). They are painted here, by rule, over the guides that
#
#   blender -b --factory-startup --python assets/blender/g650.py -- <scratch dir> --guides assets/blender/g650_paint
#
# renders (<view>_guide.png, <view>_mask.png, <view>_geo.npz: where on the
# aircraft each pixel is): a pixel's colour comes from where on the aircraft it
# is, so the livery and the panel lines meet across the views.
#
# The livery is FICTIONAL (no operator's): a private owner's white jet with a
# pearl-grey belly, parted from the white by a navy cheatline with a gold
# pinstripe that sweeps up under the engines into a navy flash on the fin.
# QX-GLF is no country's registration (Q is no country's prefix).
#
# A picture's pixel is 17.7 mm of aircraft (31.8 m in 1800 pixels), so the
# fastener rows are drawn as faint dotted lines four pixels apart (the real
# ones, 25 mm apart, cannot be told apart from across the apron), the seams
# as lines a pixel wide. The skin is aluminium (the G650's fuselage and wing:
# flush rivets, faint), the winglets, the fin's tip fairing and the control
# surfaces' trailing edges composite (no rivets drawn there).
#
# The pictures can be touched up or replaced in any paint program:
# --bake-only then bakes them as they stand. (The background is magenta: keep
# it so, the bake leaves out what is that colour or outside the masks.)
import os
import sys

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

HERE = os.path.dirname(os.path.abspath(__file__))
ARGS = [a for a in sys.argv[1:] if not a.startswith("--")]
BAKE_ONLY = "--bake-only" in sys.argv
PLAIN = "--plain" in sys.argv
SUFFIX = "_plain" if PLAIN else ""
SRC = ARGS[0] if ARGS else os.path.join(HERE, "g650_paint")
OUT = ARGS[1] if len(ARGS) > 1 else os.path.join(os.path.dirname(HERE), "g650_plain.jpg" if PLAIN else "g650_paint.jpg")

# The frames (as g650.py): drawing station d (m behind the radome's tip),
# height h (m above the ground); Blender: y forward, z up, origin at the CG.
CG_D, CG_H = 14.35, 2.55


def Y(d):
    return CG_D - d


def Z(h):
    return h - CG_H


# The rectangles the pictures cover (GUIDE_* in g650.py): metres across and
# fore and aft, the station and height at their middle, pixels per metre.
SPAN, LEN, MID_Y, MID_Z = 31.4, 31.8, Y(15.2), Z(3.9)
PX = 1800 / LEN

# ---------------------------------------------------------------- the livery (sRGB)
WHITE = (246, 246, 243)
PEARL = (188, 190, 192)        # the belly
NAVY = (24, 36, 66)            # the cheatline, the fin's flash, the lettering
GOLD = (184, 150, 82)          # the pinstripe
METAL = (170, 172, 176)        # bare leading edges
STEEL, STRAW, SOOT = (110, 106, 100), (150, 128, 92), (52, 50, 48)
WINDOW = (36, 42, 52)
DOOR_LINE = (120, 124, 130)
GRIME = (96, 90, 80)
REGISTRATION = "QX-GLF"        # (made up: Q is no country's prefix)
REG_AT = (17.95, 3.12, 0.36)   # on the rear fuselage, behind the last window: station, height of its middle, letter height
WING_REG = (8.0, 17.3, 0.75)   # |x|, station, letter height: under the left wing
# The cheatline: the height of its middle along the cabin; how it runs down
# to the nose and up behind the wing; its half width; the pinstripe under it.
CHEAT = ((0.0, 2.10), (2.0, 2.18), (6.0, 2.32), (16.5, 2.32), (19.0, 2.55), (21.5, 3.05), (24.5, 3.60), (27.0, 3.95))
CHEAT_HALF = 0.07
PIN_GAP, PIN_WIDTH = 0.05, 0.025
FIN_FLASH = 0.62               # the fin is navy aft of this fraction of its chord, below...
FIN_FLASH_TOP = (4.2, 6.6)     # ...a line rising from this height at the leading edge to this at the trailing edge
# The windows (g650.py): eight a side, 0.71 by 0.51 m, the 5th and 6th the overwing exits.
WIN_D = [6.65 + 1.375 * k for k in range(8)]
WIN_H, WIN_SIZE = 3.04, (0.71, 0.51)
EXITS = (4, 5)
DOOR = (4.55, 5.45, 1.70, 3.80)   # the entry door (left): d from, to; h from, to
SERVICE = (19.10, 19.80, 2.45, 3.30)  # the baggage door, aft on the left (estimate)

# ---------------------------------------------------------------- the structure under the paint (as g650.py)
W_LE0, W_TAN, W_TE0, W_TE_TAN, W_KINK, W_TE_IN, W_X1 = 8.33, 0.826, 15.33, 0.430, 4.6, 0.14, 14.27
FRAMES = tuple(np.round(np.arange(0.85, 27.0, 1.524), 3)) + (4.40, 21.40)   # skin joints every third frame (frames 20 in apart), and where the fuselage's pieces join
FIN_SPARS = (0.15, 0.45, 0.70)
STAB_SPARS = (0.15, 0.45, 0.70)
WING_SPARS = (0.12, 0.42, 0.68)
PANEL_SEED = 650
PANEL_PITCH = (1.10, 1.0, 0.9)       # round the fuselage (m of girth), along the wing (m), up the fin
SEAM, RIVET_PITCH, RIVET_INSET = 0.018, 0.071, 0.040
SEAM_DARK, RIVET_DARK, SEAM_DIRT, PANEL_TONE = (0.07, 0.10), (0.025, 0.045), (0.015, 0.03), (0.008, 0.02)
NOSE_BAY = (0.0, 0.29, 0.95, 3.32)                 # |x| from, to; d from, to
MAIN_BAY = (0.08, 1.22, 14.95, 16.02)
LEG_SLOT = (1.15, 2.45, 14.85, 15.70)
NAC_X, NAC_H, NAC_D = 2.20, 3.75, (18.85, 23.55)
SLEEVE_D = 22.76


def wing_le(x):
    return W_LE0 + W_TAN * x


def wing_te(x):
    return np.where(x >= W_KINK, W_TE0 + W_TE_TAN * x, W_TE0 + W_TE_TAN * W_KINK - W_TE_IN * (W_KINK - x))


def fin_le(h):
    return np.minimum(23.4 + 0.65 * (h - 4.4), 21.3 + (h - 3.90) * 3.19)


def fin_te(h):
    return 27.0 + 0.355 * (h - 3.5)



# ---------------------------------------------------------------- a picture's geometry
# ---------------------------------------------------------------- a picture's geometry
class View:
    """One guide: for each pixel, the point of the model it shows (x, y, z), the
    surface's normal there, and what it is a picture of."""

    def __init__(self, name):
        self.name = name
        g = np.load(os.path.join(SRC, name + "_geo.npz"))
        self.ok = g["piece"] < 255
        h, w = self.ok.shape
        i, j = np.meshgrid((np.arange(w, dtype=np.float32) + 0.5 - w / 2) / PX, (h / 2 - np.arange(h, dtype=np.float32) - 0.5) / PX)
        depth = np.nan_to_num(g["depth"].astype(np.float32))
        if name in ("top", "bottom"):
            pos = np.stack([i if name == "top" else -i, MID_Y + j, depth], -1)
        else:
            pos = np.stack([depth, MID_Y + (-i if name == "side" else i), MID_Z + j], -1)
        pos *= self.ok[..., None]
        self.x, self.y, self.z = pos[..., 0], pos[..., 1], pos[..., 2]
        nrm = g["nrm"].astype(np.float32) / 127.0
        self.nx, self.ny, self.nz = nrm[..., 0], nrm[..., 1], nrm[..., 2]
        self.pos = pos
        pieces, mats = [str(s) for s in g["pieces"]], [str(s) for s in g["mats"]]
        self.piece_id = g["piece"]
        self._piece = lambda test: np.isin(g["piece"], [k for k, n in enumerate(pieces) if test(n)])
        self._mat = lambda *names: np.isin(g["mat"], [mats.index(n) for n in names if n in mats])
        self.h, self.w = self.ok.shape
        guide = np.asarray(Image.open(os.path.join(SRC, name + "_guide.png")).convert("L"), np.float32) / 255.0
        self.shade = guide / np.median(guide[self.ok])

    def piece(self, *words):
        return self._piece(lambda n: any(w in n for w in words))

    def mat(self, *names):
        return self._mat(*names)

    def px(self, a, b):
        """The pixel of a point of the picture's plane: (x, station) seen from above
        or below, (station, height) seen from a side."""
        if self.name == "top":
            return self.w / 2 + a * PX, self.h / 2 - (b - MID_Y) * PX
        if self.name == "bottom":
            return self.w / 2 - a * PX, self.h / 2 - (b - MID_Y) * PX
        s = -1.0 if self.name == "side" else 1.0  # the left side is seen nose to the left
        return self.w / 2 + s * (a - MID_Y) * PX, self.h / 2 - (b - MID_Z) * PX


def smoothstep(a, b, x):
    t = np.clip((x - a) / (b - a), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def noise(pos, size, seed, n=18):
    """Smooth noise over the model, about -2..2 (unit variance): waves of random
    direction, `size` metres long along each axis."""
    rng = np.random.default_rng(seed)
    k = rng.normal(size=(n, 3))
    k = k / np.linalg.norm(k, axis=1)[:, None] * rng.uniform(0.6, 1.5, (n, 1)) * 2 * np.pi / np.asarray(size, np.float32)
    phase = rng.uniform(0, 2 * np.pi, n)
    out = np.zeros(pos.shape[:2], np.float32)
    for ki, p in zip(k.astype(np.float32), phase):
        out += np.cos(pos @ ki + p)
    return out * np.sqrt(2.0 / n)


def mix(a, b, t):
    return a + (np.asarray(b, np.float32) - a) * t[..., None]


def line(d, w=SEAM):
    """1 on a line, falling to 0 `w` metres from it; d the distance from it."""
    return 1.0 - smoothstep(w * 0.45, w, np.abs(d))


def lines_at(v, stations, w=SEAM):
    out = np.zeros(v.shape, np.float32)
    for s in stations:
        out = np.maximum(out, line(v - s, w))
    return out


def blur(a, r):
    return np.asarray(Image.fromarray(np.clip(a * 255, 0, 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(r)), np.float32) / 255.0


def interp(x, table):
    t = np.asarray(table, np.float32)
    return np.interp(x, t[:, 0], t[:, 1]).astype(np.float32)


# ---------------------------------------------------------------- lettering and what is drawn on a sheet
# Capitals: strokes on a grid 4 wide and 6 high.
GLYPHS = {
    "A": [[(0, 0), (2, 6), (4, 0)], [(0.8, 2.1), (3.2, 2.1)]],
    "C": [[(4, 4.9), (3, 6), (1.2, 6), (0, 4.8), (0, 1.2), (1.2, 0), (3, 0), (4, 1.1)]],
    "D": [[(0, 0), (0, 6), (2.6, 6), (4, 4.6), (4, 1.4), (2.6, 0), (0, 0)]],
    "E": [[(4, 6), (0, 6), (0, 0), (4, 0)], [(0, 3.1), (3, 3.1)]],
    "F": [[(4, 6), (0, 6), (0, 0)], [(0, 3.1), (3, 3.1)]],
    "G": [[(4, 4.9), (3, 6), (1.2, 6), (0, 4.8), (0, 1.2), (1.2, 0), (3, 0), (4, 1.2), (4, 2.8), (2.2, 2.8)]],
    "I": [[(2, 0), (2, 6)]],
    "L": [[(0, 6), (0, 0), (4, 0)]],
    "N": [[(0, 0), (0, 6), (4, 0), (4, 6)]],
    "O": [[(1.2, 0), (3, 0), (4, 1.2), (4, 4.8), (3, 6), (1.2, 6), (0, 4.8), (0, 1.2), (1.2, 0)]],
    "Q": [[(1.2, 0), (3, 0), (4, 1.2), (4, 4.8), (3, 6), (1.2, 6), (0, 4.8), (0, 1.2), (1.2, 0)], [(2.4, 1.6), (4.2, -0.4)]],
    "R": [[(0, 0), (0, 6), (3, 6), (4, 5), (4, 4), (3, 3), (0, 3)], [(2.2, 3), (4, 0)]],
    "S": [[(4, 5), (3, 6), (1, 6), (0, 5), (0, 4), (1, 3), (3, 3), (4, 2), (4, 1), (3, 0), (1, 0), (0, 1)]],
    "T": [[(0, 6), (4, 6)], [(2, 6), (2, 0)]],
    "X": [[(0, 0), (4, 6)], [(0, 6), (4, 0)]],
    "-": [[(0.8, 3), (3.2, 3)]],
}
SS = 6  # sheets are drawn this many times too big and scaled down, for clean edges


class Sheet:
    """Something painted over the white: drawn `SS` times too big on a picture
    `w` by `h` pixels; `at(a, b)` is the pixel of a point given in metres."""

    def __init__(self, w, h, at):
        self.w, self.h, self.at = w, h, at
        self.im = Image.new("RGBA", (w * SS, h * SS), (0, 0, 0, 0))
        self.d = ImageDraw.Draw(self.im)

    def p(self, a, b):
        x, y = self.at(a, b)
        return x * SS, y * SS

    def text(self, s, a, b, height, colour, lean=0.0, weight=1.0, pitch=5.4):
        """Letters `height` m tall centred on (a, b), reading left to right in the
        picture; `lean`: how far their tops lean to the right (italics)."""
        cx, cy = self.p(a, b)
        u = height * PX * SS / 6.0
        # Each letter takes its own width (an I is narrow), with `pitch` - 4 between them.
        ext = [(min(g[0] for st in GLYPHS[ch] for g in st), max(g[0] for st in GLYPHS[ch] for g in st)) for ch in s]
        starts = np.cumsum([0.0] + [hi - lo + pitch - 4 for lo, hi in ext])
        x0 = cx - (starts[-1] - (pitch - 4)) * u / 2
        wd = max(int(round(weight * u)), 1)
        for k, ch in enumerate(s):
            for stroke in GLYPHS[ch]:
                pts = [(x0 + (starts[k] - ext[k][0] + gx + lean * (gy - 3)) * u, cy + (3 - gy) * u) for gx, gy in stroke]
                self.d.line(pts, fill=colour + (255,), width=wd, joint="curve")
                for q in pts:  # round the ends and the corners
                    self.d.ellipse([q[0] - wd / 2, q[1] - wd / 2, q[0] + wd / 2, q[1] + wd / 2], fill=colour + (255,))

    def rrect(self, a0, a1, b0, b1, r, fill=None, outline=None, width=0.03):
        p0, p1 = self.p(a0, b0), self.p(a1, b1)
        box = [min(p0[0], p1[0]), min(p0[1], p1[1]), max(p0[0], p1[0]), max(p0[1], p1[1])]
        self.d.rounded_rectangle(box, radius=r * PX * SS, fill=fill and fill + (255,), outline=outline and outline + (255,), width=max(int(round(width * PX * SS)), 1))

    def polygon(self, pts, colour):
        self.d.polygon([self.p(*q) for q in pts], fill=colour + (255,))

    def oval(self, a, b, ra, rb, colour):
        cx, cy = self.p(a, b)
        A, B = ra * PX * SS, rb * PX * SS
        self.d.ellipse([cx - A, cy - B, cx + A, cy + B], fill=colour + (255,))

    def ring(self, a, b, r, width, colour):
        cx, cy = self.p(a, b)
        R = r * PX * SS
        self.d.ellipse([cx - R, cy - R, cx + R, cy + R], outline=colour + (255,), width=max(int(round(width * PX * SS)), 1))

    def path(self, pts, width, colour, alpha=255):
        self.d.line([self.p(*q) for q in pts], fill=colour + (alpha,), width=max(int(round(width * PX * SS)), 1), joint="curve")

    def streak(self, a, b, length, width, colour, alpha):
        """A stain run back along the airflow from (a, b), fading out."""
        n = 12
        for k in range(n):
            t0, t1 = k / n, (k + 1) / n
            self.path([(a, b - length * t0), (a, b - length * t1)], width * (1 + 1.5 * t0), colour, int(alpha * (1 - t0) ** 1.5))

    def array(self, soft=0.0):
        im = self.im.filter(ImageFilter.GaussianBlur(soft * SS)) if soft else self.im
        return np.asarray(im.resize((self.w, self.h), Image.LANCZOS), np.float32)


def view_sheet(v):
    return Sheet(v.w, v.h, v.px)


def over(rgb, a, where):
    return mix(rgb, a[..., :3], a[..., 3] / 255.0 * where)


# The fuselage's sides and the nacelles, one sheet for each side: laid out by
# station (the nose to the left on the left side's, to the right on the right
# side's) and height, at the pictures' own scale.
SIDE_TOP, SIDE_BOTTOM = Z(8.0), Z(0.5)
SIDE_W, SIDE_H = 1800, int(round((SIDE_TOP - SIDE_BOTTOM) * PX))


def look_up(sheet, y, z, s):
    """The side sheet's paint at station y, height z (s as side_sheet)."""
    fx = np.clip(SIDE_W / 2 + s * (y - MID_Y) * PX - 0.5, 0, SIDE_W - 1.001)
    fy = np.clip((SIDE_TOP - z) * PX - 0.5, 0, SIDE_H - 1.001)
    x0, y0 = fx.astype(np.int32), fy.astype(np.int32)
    tx, ty = (fx - x0)[..., None], (fy - y0)[..., None]
    return (sheet[y0, x0] * (1 - tx) + sheet[y0, x0 + 1] * tx) * (1 - ty) + (sheet[y0 + 1, x0] * (1 - tx) + sheet[y0 + 1, x0 + 1] * tx) * ty


# ---------------------------------------------------------------- skin panels (as f4_paint.py, at this picture's scale)
def hashf(*ks, seed=0):
    """A number 0..1 for each set of whole numbers, the same every time."""
    h = np.full(np.shape(ks[0]), (seed * 0x9E3779B1 + 0x85EBCA6B) & 0xFFFFFFFF, np.uint32)
    for k in ks:
        h ^= np.asarray(k).astype(np.int64).astype(np.uint32)
        h *= np.uint32(0x9E3779B1)
        h ^= h >> np.uint32(15)
        h *= np.uint32(0x85EBCA6B)
        h ^= h >> np.uint32(13)
    return h.astype(np.float32) / 4294967295.0


def dots(along):
    """Fasteners along a row: on for a pixel and a half every RIVET_PITCH metres."""
    return (np.mod(along, RIVET_PITCH) < 0.065).astype(np.float32)


class Detail:
    """What is drawn into one view's paint: seams, fastener rows, stencilled
    lettering, the inside of access panels, and a number 0..1 for each skin
    panel (its paint aged a little differently)."""

    def __init__(self, v):
        self.seam, self.rivet, self.stencil, self.access = (np.zeros(v.ok.shape, np.float32) for _ in range(4))
        self.tone = np.full(v.ok.shape, 0.5, np.float32)


def panels(out, mask, a, b, breaks, b0, b1, pitch, seed, ma=1.0, mb=1.0):
    """Cut the surface under `mask` into skin panels. Seams run its whole length
    where `a` crosses `breaks`; between each pair of them others cross, about
    `pitch` apart in `b` (from b0 to b1), at places of their own in each bay.
    ma, mb: metres per unit of a and b. Every panel gets fasteners inside its
    edges, the wider ones a stiffener's row down the middle, some an access
    panel with its ring of fasteners, some a block of stencilling."""
    idx = np.nonzero(mask)
    if not len(idx[0]):
        return
    a, b = a[idx], b[idx]
    ma = ma[idx] if isinstance(ma, np.ndarray) else np.float32(ma)
    mb = mb[idx] if isinstance(mb, np.ndarray) else np.float32(mb)
    ab = np.concatenate(([-1e3], np.sort(np.asarray(breaks, np.float32)), [1e3])).astype(np.float32)
    i = np.clip(np.searchsorted(ab, a) - 1, 0, len(ab) - 2)
    lo_a, hi_a = ab[i], ab[i + 1]
    rng = np.random.default_rng(seed)
    n = int((b1 - b0) / (0.6 * pitch)) + 2
    table = np.full((len(ab) - 1, n), 1e3, np.float32)
    for row in table:
        p, k = b0 + rng.uniform(0.2, 1.0) * pitch, 0
        while p < b1 and k < n:
            row[k] = p
            p += rng.uniform(0.6, 1.4) * pitch
            k += 1
    t = table[i]
    below = b[:, None] > t
    j = below.sum(1)
    lo_b, hi_b = np.where(below, t, -1e3).max(1), np.where(below, 1e3, t).min(1)
    da, db = np.minimum(a - lo_a, hi_a - a) * ma, np.minimum(b - lo_b, hi_b - b) * mb   # metres from the nearest seam each way
    ha, hb = (hi_a - lo_a) / 2 * ma, (hi_b - lo_b) / 2 * mb                              # the panel's half size
    along_a, along_b = dots(a * ma), dots(b * mb)
    seam = np.maximum(line(da), line(db))
    rivet = np.maximum(line(da - RIVET_INSET) * along_b, line(db - RIVET_INSET) * along_a)
    rivet = np.maximum(rivet, line(ha - da) * along_b * (ha > 0.7) * (ha < 30) * (db > 0.1))
    rivet = np.maximum(rivet, line(hb - db) * along_a * (hb > 0.7) * (hb < 30) * (da > 0.1))
    h = lambda k: hashf(i, j, seed=seed * 16 + k)
    # An access panel: a rectangle with round corners, or an oval.
    ea, eb = np.minimum(ha * (0.35 + 0.30 * h(2)), 0.55), np.minimum(hb * (0.35 + 0.30 * h(3)), 0.55)
    has = (h(1) > 0.70) & (ha > 0.36) & (hb > 0.36) & (ha < 30) & (hb < 30)
    r = np.minimum(ea, eb) * np.where(h(4) > 0.7, 1.0, 0.3)
    qa, qb = np.abs(a - (lo_a + hi_a) / 2) * ma - ea + r, np.abs(b - (lo_b + hi_b) / 2) * mb - eb + r
    d = np.hypot(np.maximum(qa, 0), np.maximum(qb, 0)) + np.minimum(np.maximum(qa, qb), 0) - r
    seam = np.maximum(seam, 0.7 * line(d) * has)
    inside = (d < 0) & has
    # Stencilling: a row or two of dashes in a corner of the panel.
    la, lb = (a - lo_a) * ma - 0.20, (b - lo_b) * mb - 0.18
    rows, wide = 1 + (h(6) * 2).astype(np.int32), 0.35 + 0.6 * h(7)
    st = (h(5) < 0.34) & ~inside & (la > 0) & (la < wide) & (lb > 0) & (lb < rows * 0.11) & (np.mod(lb, 0.11) < 0.05) & (ha < 30) & (hb < 30)
    st &= hashf(np.floor(la / 0.085), np.floor(lb / 0.11), i, j, seed=seed) > 0.32
    out.seam[idx] = np.maximum(out.seam[idx], seam)
    out.rivet[idx] = np.maximum(out.rivet[idx], rivet)
    out.tone[idx] = h(0)
    out.access[idx] = inside
    out.stencil[idx] = st


def box_d(a, b, box, r=0.12):
    """Distance outside a round-cornered rectangle (a from, to; b from, to); negative inside."""
    a0, a1, b0, b1 = box
    qa = np.abs(a - (a0 + a1) / 2) - abs(a1 - a0) / 2 + r
    qb = np.abs(b - (b0 + b1) / 2) - abs(b1 - b0) / 2 + r
    return np.hypot(np.maximum(qa, 0), np.maximum(qb, 0)) + np.minimum(np.maximum(qa, qb), 0) - r


def smear(src, axis, step, decay, ok):
    """Drag `src` across the picture along `axis` (step 1 or -1), dying away by
    `decay` (a picture of its own) each pixel: what runs back from a leak."""
    out, dec, okm = np.moveaxis(src.copy(), axis, 0), np.moveaxis(decay, axis, 0), np.moveaxis(ok, axis, 0)
    for i in (range(1, len(out)) if step > 0 else range(len(out) - 2, -1, -1)):
        out[i] = np.maximum(out[i], out[i - step] * dec[i]) * okm[i]
    return np.moveaxis(out, 0, axis)




def side_sheet(s):
    """What is drawn on one side (s = -1 the left, 1 the right): windows, doors, the registration."""
    k = Sheet(SIDE_W, SIDE_H, lambda y, z: (SIDE_W / 2 + s * (y - MID_Y) * PX, (SIDE_TOP - z) * PX))
    rx, rz = WIN_SIZE[0] / 2, WIN_SIZE[1] / 2
    for n, d in enumerate(WIN_D):
        y, z = Y(d), Z(WIN_H)
        k.oval(y, z, rx + 0.025, rz + 0.025, (205, 207, 210))   # the window's polished reveal
        k.oval(y, z, rx, rz, WINDOW)
        if n in EXITS:
            # the overwing exits' outlines round their windows, and their handles
            k.rrect(Y(d - 0.36), Y(d + 0.36), Z(2.55), Z(3.52), 0.08, outline=DOOR_LINE, width=0.02)
            k.rrect(Y(d - 0.06), Y(d + 0.06), Z(3.40), Z(3.45), 0.02, fill=DOOR_LINE)
    if s < 0:
        d0, d1, h0, h1 = DOOR
        k.rrect(Y(d0), Y(d1), Z(h0), Z(h1), 0.13, outline=DOOR_LINE, width=0.022)
        k.rrect(Y(d0 + 0.28), Y(d0 + 0.62), Z(2.62), Z(2.74), 0.03, outline=DOOR_LINE, width=0.015)   # the handle's recess
        d0, d1, h0, h1 = SERVICE
        k.rrect(Y(d0), Y(d1), Z(h0), Z(h1), 0.06, outline=DOOR_LINE, width=0.02)
    # The registration, aft on each side.
    d, h, ht = REG_AT
    if not PLAIN:
        k.text(REGISTRATION, Y(d), Z(h), ht, NAVY, weight=0.9)
    # Small print: the static ports' red rings, below and ahead of the windshield.
    for d in (1.55, 1.70):
        k.oval(Y(d), Z(2.45), 0.035, 0.035, (180, 40, 36))
        k.oval(Y(d), Z(2.45), 0.022, 0.022, (200, 200, 200))
    return k.array()


def paint(v, sheets):
    sides = v.name in ("side", "right")
    rng = np.random.default_rng(("top", "bottom", "side", "right").index(v.name) + 1)

    def grain(r):
        g = blur(rng.random(v.ok.shape).astype(np.float32), r)
        return (g - g.mean()) / (g.std() + 1e-6)

    d = CG_D - v.y
    h = v.z + CG_H
    # (Seen from below, a gear well's roof stands for the door shut under it: paint it as the belly there.)
    h = np.where(v.mat("well") & (v.name == "bottom"), 1.2, h).astype(np.float32)
    body = v.piece("fuselage", "nose section", "radome", "tail cone", "entry door")
    wing = v.piece("wing", "flap", "aileron", "spoiler")
    fin = v.piece("fin", "rudder")
    stab = v.piece("stabiliser", "elevator")
    nacelle = v.piece("nacelle", "reverser")
    skin = v.mat("skin", "well", "door_in")
    ax = np.abs(v.x)
    steep = np.abs(v.nx) > 0.45
    left = (v.x < 0) if not sides else np.full(v.ok.shape, v.name == "side")

    rgb = np.empty(v.ok.shape + (3,), np.float32)
    rgb[:] = WHITE

    # ---- the livery: the pearl belly under the cheatline, which sweeps up behind the wing.
    c = np.asarray(CHEAT, np.float32)
    zc = np.interp(d, c[:, 0], c[:, 1]).astype(np.float32)
    edge = 0.012
    under = smoothstep(edge, -edge, h - (zc - CHEAT_HALF - PIN_GAP - PIN_WIDTH)) * body
    rgb = mix(rgb, PEARL, under)
    band = smoothstep(CHEAT_HALF + edge, CHEAT_HALF - edge, np.abs(h - zc)) * body * (d > 0.9)
    pin = line(h - (zc - CHEAT_HALF - PIN_GAP / 2), PIN_WIDTH / 2 + edge) * body * (d > 1.4)
    # The fin's flash: navy aft of a line, below another; the gold pin along its front edge.
    ff = (d - fin_le(h)) / np.maximum(fin_te(h) - fin_le(h), 0.1)
    top_line = FIN_FLASH_TOP[0] + (FIN_FLASH_TOP[1] - FIN_FLASH_TOP[0]) * np.clip(ff, 0, 1)
    flash = smoothstep(-0.01, 0.01, ff - FIN_FLASH - 0.25 * (h - 4.0) / 3.0) * smoothstep(0.02, -0.02, h - top_line) * fin
    fin_pin = line((ff - FIN_FLASH - 0.25 * (h - 4.0) / 3.0) * (fin_te(h) - fin_le(h)) + 0.08, PIN_WIDTH / 2 + edge) * fin * (h < top_line)
    band = np.maximum(band, flash)
    pin = np.maximum(pin, fin_pin)
    if PLAIN:
        under, band, pin = under * 0, band * 0, pin * 0
        rgb[:] = WHITE
    rgb = mix(rgb, GOLD, pin)
    rgb = mix(rgb, NAVY, band)
    # Windows, doors, the registration: from the side's own sheet.
    a = np.where(left[..., None], look_up(sheets[0], v.y, v.z, -1.0), look_up(sheets[1], v.y, v.z, 1.0))
    rgb = over(rgb, a, (body | nacelle) * skin * (np.abs(v.nz) < 0.85))
    # The registration under the left wing.
    k = view_sheet(v)
    if v.name == "bottom" and not PLAIN:
        k.text(REGISTRATION, -WING_REG[0], Y(WING_REG[1]), WING_REG[2], NAVY, weight=0.95)
    rgb = over(rgb, k.array(), wing * skin)

    # ---- bare metal and heat
    le = wing_le(np.minimum(ax, W_X1))
    metal = wing * smoothstep(0.30, 0.20, (d - le) / 1.22)
    hot = v.mat("exhaust", "nozzle_in").astype(np.float32)
    rgb = mix(rgb, METAL, metal)
    m = np.empty_like(rgb)
    m[:] = STEEL
    m = mix(m, STRAW, 0.4 * smoothstep(0.2, 0.9, noise(v.pos, (0.3, 1.5, 0.3), 31) * 0.3 + 0.5))
    rgb = mix(rgb, m, hot)
    bare = np.clip(metal + hot, 0, 1)
    fresh = rgb.copy()

    # ---- the structure showing through: skin panels, fasteners, doors
    D = Detail(v)
    pan = v.ok & skin
    mid = np.interp(d, [0, 2, 6, 17, 19.4, 21.4, 23.4, 25.4, 27], [2.08, 2.26, 2.75, 2.77, 2.83, 2.97, 3.17, 3.32, 3.43]).astype(np.float32)
    girth = np.maximum(np.hypot(v.x, h - mid), 0.2)
    panels(D, body & pan, d, np.arctan2(h - mid, ax), FRAMES, -1.6, 1.6, PANEL_PITCH[0], PANEL_SEED, 1.0, girth)
    chord = np.maximum(wing_te(np.minimum(ax, W_X1)) - le, 0.5)
    xc = (d - le) / chord
    for n, face in enumerate((v.nz >= 0, v.nz < 0)):
        panels(D, wing & pan & face & (ax < W_X1), xc, ax, WING_SPARS, 0.0, 15.2, PANEL_PITCH[1], PANEL_SEED + 1 + n, chord, 1.0)
    panels(D, fin & pan, ff, h, FIN_SPARS, 3.6, 7.9, PANEL_PITCH[2], PANEL_SEED + 3, np.maximum(fin_te(h) - fin_le(h), 0.5), 1.0)
    sle = 25.45 + 0.679 * ax
    sc = np.maximum(2.70 - 0.2642 * ax, 0.5)
    panels(D, stab & pan, (d - sle) / sc, ax, STAB_SPARS, 0.0, 5.6, 0.8, PANEL_SEED + 4, sc, 1.0)
    ang = np.arctan2(h - NAC_H, ax - NAC_X)
    panels(D, nacelle & pan, d, ang, (19.15, 20.5, 21.6, SLEEVE_D), -3.2, 3.2, 0.9, PANEL_SEED + 5, 1.0, 0.79)
    seam, rivet = D.seam, D.rivet
    # Where two pieces meet (the control surfaces, the doors, the reversers' sleeves).
    joint = np.zeros(v.ok.shape, np.float32)
    depth = v.z if not sides else v.x
    for axis in (0, 1):
        jd = (np.roll(v.piece_id, 1, axis) != v.piece_id) & v.ok & np.roll(v.ok, 1, axis) & (np.abs(np.roll(depth, 1, axis) - depth) < 0.15)
        joint = np.maximum(joint, jd.astype(np.float32))
    seam = np.maximum(seam, np.clip(blur(joint, 0.6) * 1.6, 0, 1))
    seam = np.maximum(seam, 0.7 * line(d - 0.85) * body)          # the radome's joint
    seam = np.maximum(seam, 0.6 * line(d - SLEEVE_D, 0.02) * nacelle)
    # The composite parts carry no rivets: the winglets, the fin's cap, the radome, the control surfaces.
    composite = v.piece("radome", "rudder", "elevator", "aileron", "spoiler") | (wing & (ax > W_X1)) | (fin & (h > 7.4))
    rivet = rivet * ~composite
    if v.name == "bottom":
        for box in (NOSE_BAY, MAIN_BAY, LEG_SLOT):
            x0, x1, d0, d1 = box
            seam = np.maximum(seam, line(box_d(ax, d, (x0 if x0 > 0.05 else -x1, x1, d0, d1), 0.05)))
        seam = np.maximum(seam, line(ax, 0.02) * (d > NOSE_BAY[2]) * (d < NOSE_BAY[3]))
        # Fuel-tank access panels under the wing: ovals in a row along the front spar.
        for x0 in np.arange(2.5, 13.0, 1.05):
            px, pd = x0, wing_le(x0) + 0.30 * (W_TE0 + W_TE_TAN * x0 - wing_le(x0))
            dd = np.hypot((ax - px) / 0.20, (d - pd) / 0.30) - 1.0
            seam = np.maximum(seam, 0.6 * line(dd * 0.25) * wing)
    not_skin = v.mat("glass", "frame", "inlet", "rubber", "exhaust", "nozzle_in", "steel", "strut", "hub", "lip", "fan", "spinner", "cascade")
    seam = np.clip(seam, 0, 1) * ~not_skin
    rivet = np.clip(rivet, 0, 1) * ~not_skin * (1 - seam)

    # ---- weathering (a privately owned jet's: kept clean)
    fade = noise(v.pos, (4.0, 6.0, 3.0), 11)
    low = np.clip(-v.nz, 0, 1)
    rgb *= (1.0 + 0.008 * fade + (PANEL_TONE[0] + PANEL_TONE[1] * low) * (D.tone - 0.5) + 0.006 * noise(v.pos, (0.5, 0.5, 0.5), 13))[..., None]
    rgb *= (1.0 + 0.006 * grain(0.8) + 0.006 * grain(3.0))[..., None]
    streaks = smoothstep(0.6, 1.9, noise(v.pos, (0.30, 9.0, 0.30), 14) + 0.5 * noise(v.pos, (4.0, 12.0, 4.0), 15))
    rgb = mix(rgb, GRIME, streaks * (0.01 + 0.06 * low))
    # Soot behind the nozzles, on the tail cone's sides and under the stabiliser's wake: light.
    soot = 0.25 * body * smoothstep(24.2, 26.5, d) * smoothstep(1.0, 0.3, np.abs(h - 3.5)) * (0.6 + 0.4 * noise(v.pos, (0.3, 2.0, 0.3), 21))
    rgb = mix(rgb, SOOT, np.clip(soot, 0, 0.4))
    # Fluid streaks back from the gear bays.
    if v.name == "bottom":
        ks = view_sheet(v)
        srng = np.random.default_rng(7)
        for _ in range(18):
            a_ = srng.choice([-1, 1]) * srng.uniform(0.1, 1.3)
            ks.streak(a_, Y(srng.uniform(15.9, 16.4)), srng.uniform(0.8, 3.0), srng.uniform(0.02, 0.05), (86, 74, 56), srng.uniform(40, 90))
        rgb = over(rgb, ks.array(soft=1.0), skin.astype(np.float32))

    # Seams and fasteners: dark lines (the game takes them to lie below the skin round them).
    grime = blur(seam, 2.0)
    rgb *= (1.0 - (SEAM_DIRT[0] + SEAM_DIRT[1] * low) * np.clip(grime * 2.0, 0, 1))[..., None]
    rgb = mix(rgb, (64, 68, 80), 0.45 * D.stencil * (1 - bare) * (1 - band))
    rgb *= (1.0 - (RIVET_DARK[0] + RIVET_DARK[1] * np.maximum(bare, low)) * rivet)[..., None]
    rgb *= (1.0 - (SEAM_DARK[0] + SEAM_DARK[1] * np.maximum(bare, low)) * seam)[..., None]
    rgb = mix(rgb, fresh, 0.0 * band)

    rgb = mix(rgb, (34, 38, 44), v.mat("glass").astype(np.float32))
    rgb = mix(rgb, (28, 29, 31), v.mat("frame", "inlet", "rubber", "cascade").astype(np.float32))
    rgb *= (np.clip(v.shade, 0.6, 1.15) ** 0.08)[..., None]
    rgb = np.clip(rgb, 0, 250)
    rgb[~v.ok] = (255, 0, 255)
    Image.fromarray(rgb.astype(np.uint8)).save(os.path.join(SRC, v.name + SUFFIX + ".png"), optimize=True)
    print(f"painted {v.name}")


# ---------------------------------------------------------------- the bake (as fa18_paint.py)
def spread(rgb, ok, rounds=128):
    """Grow the painted pixels outward, a pixel a round, over the ones that are not."""
    rgb = rgb.astype(np.float32) * ok[..., None]
    ok = ok.astype(np.float32)
    for _ in range(rounds):
        s = np.zeros_like(rgb)
        n = np.zeros_like(ok)
        for dy in (-1, 0, 1):
            for dx in (-1, 0, 1):
                if dy or dx:
                    s += np.roll(rgb, (dy, dx), (0, 1))
                    n += np.roll(ok, (dy, dx), (0, 1))
        grow = (ok == 0) & (n > 0)
        if not grow.any():
            break
        rgb[grow] = s[grow] / n[grow][:, None]
        ok[grow] = 1
    rgb[ok == 0] = rgb[ok > 0].mean(0)
    return rgb


def layer(view):
    mask = Image.open(os.path.join(SRC, view + "_mask.png")).convert("L")
    paint_ = Image.open(os.path.join(SRC, view + SUFFIX + ".png")).convert("RGB")
    if paint_.size != mask.size:
        paint_ = paint_.resize(mask.size, Image.LANCZOS)
    rgb = np.asarray(paint_)
    # Painted: inside the silhouette, less a pixel (where the picture's own edge
    # may fall short of the model's), and not the background's magenta (which
    # also shows through the gaps at the hinges).
    ok = (np.asarray(mask.filter(ImageFilter.MinFilter(3))) > 128) & ~((rgb[..., 0] > 240) & (rgb[..., 1] < 40) & (rgb[..., 2] > 240))
    out = Image.fromarray(np.clip(spread(rgb, ok), 0, 255).astype(np.uint8))
    print(f"{view}: {paint_.size[0]}x{paint_.size[1]}, {ok.mean() * 100:.0f}% painted")
    return out


if not BAKE_ONLY:
    for name in ("top", "bottom", "side", "right"):
        for kind, mode in (("guide", "L"), ("mask", "1")):  # as Blender wrote them: pack them
            path = os.path.join(SRC, f"{name}_{kind}.png")
            im = Image.open(path)
            if im.mode != mode:
                im.convert("L").point(lambda c: c if kind == "guide" else 255 * (c > 127)).convert(mode).save(path, optimize=True)
    sheets = (side_sheet(-1.0), side_sheet(1.0))
    for name in ("top", "bottom", "side", "right"):
        paint(View(name), sheets)
top, bottom, side, right = layer("top"), layer("bottom"), layer("side"), layer("right")
# Nose up like the other two, height running left to right in both: the left
# side is seen nose to the left, the right side nose to the right.
side = side.transpose(Image.Transpose.ROTATE_270)
right = right.transpose(Image.Transpose.FLIP_LEFT_RIGHT).transpose(Image.Transpose.ROTATE_270)
assert top.size == bottom.size == side.size == right.size, (top.size, bottom.size, side.size, right.size)
w, h = top.size
atlas = Image.new("RGB", (w * 4, h))
for k, im in enumerate((top, bottom, side, right)):
    atlas.paste(im, (w * k, 0))
atlas.save(OUT, quality=92, subsampling=0)
print("wrote", OUT, atlas.size, os.path.getsize(OUT), "bytes")
