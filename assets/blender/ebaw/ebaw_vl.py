"""EBAW from Flemish open data: real footprints, roof heights and roof colours.

Plain Python 3 + shapely + numpy + Pillow (not Blender). Reads the airside
buildings ebaw_osm.py found (ebaw_layout.json), and writes ebaw_vl.json, which
ebaw_osm.py then folds into the layout:

  * footprints from the GRB (Grootschalig Referentiebestand, layer GBG), the
    cadastral base map, in place of OpenStreetMap's
  * roof heights from the DHMV II (Digitaal Hoogtemodel Vlaanderen II) 1 m
    surface model minus its terrain model (LiDAR flown 2013-2015: a building
    put up since has no height, and keeps the estimate)
  * roof colours from the 2025 orthophoto mosaic (0.3 m)
  * the terminal as blocks of measured height (its roof levels, the rotunda,
    the tower), from the height model inside its GRB footprint

    python3 assets/blender/ebaw/ebaw_vl.py           # fetch (cached in data/) and build
    python3 assets/blender/ebaw/ebaw_vl.py --debug   # also writes data/vl_check.jpg

All three are open data under the Flemish "Modellicentie voor gratis
hergebruik" (free reuse, the source credited):
  Bron: Grootschalig Referentie Bestand Vlaanderen, Digitaal Vlaanderen
  Bron: Digitaal Vlaanderen (DHMV II)
  Bron: Luchtopnamen Digitaal Vlaanderen
Services: geo.api.vlaanderen.be GRB/wfs, DHMV/wcs (DHMVII_DSM_1m, DHMVII_DTM_1m),
OMW/wmts (omwrgb25vl).
"""

import io
import json
import math
import subprocess
import sys
from pathlib import Path

import numpy as np
from PIL import Image
from shapely.geometry import Point, Polygon, box, shape
from shapely.ops import unary_union
from shapely.prepared import prep

HERE = Path(__file__).resolve().parent
DATA = HERE / "data"
LAYOUT = json.loads((HERE / "ebaw_layout.json").read_text())
OUT = HERE / "ebaw_vl.json"
API = "https://geo.api.vlaanderen.be"
UA = "fableflight-scenery"
BBOX = (4.438, 51.181, 4.480, 51.198)   # lon/lat round the aerodrome
SOURCES = [
    "Bron: Grootschalig Referentie Bestand Vlaanderen, Digitaal Vlaanderen (GRB, footprints)",
    "Bron: Digitaal Vlaanderen (DHMV II, roof heights)",
    "Bron: Luchtopnamen Digitaal Vlaanderen (orthophoto 2025, roof colours)",
]
TERMINAL_OSM = 4414121
TOWER_OSM_XY = None  # from the layout's terminal frame

FR = LAYOUT["frame"]
LAT0, LON0 = FR["origin_lat"], FR["origin_lon"]
_A, _E2 = 6378137.0, 6.69437999014e-3
_S = math.sin(math.radians(LAT0))
_RN = _A / math.sqrt(1 - _E2 * _S * _S)
_RM = _RN * (1 - _E2) / (1 - _E2 * _S * _S)


def local(lon, lat):
    """lon/lat -> local metres (as ebaw_osm.local)."""
    return (math.radians(lon - LON0) * _RN * math.cos(math.radians(LAT0)), math.radians(lat - LAT0) * _RM)


def lonlat(x, y):
    """local metres -> lon/lat (the exact inverse)."""
    return (LON0 + math.degrees(x / (_RN * math.cos(math.radians(LAT0)))), LAT0 + math.degrees(y / _RM))


def get(url, path):
    if not path.exists():
        path.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(["curl", "-s", "-m", "300", "-A", UA, url, "-o", str(path)], check=True)
    return path.read_bytes()


# ---------------------------------------------------------------- GRB and the Lambert 72 fit

def grb():
    q = (f"{API}/GRB/wfs?service=WFS&version=2.0.0&request=GetFeature&typeNames=GRB:GBG&count=10000"
         f"&outputFormat=application/json&bbox={BBOX[0]},{BBOX[1]},{BBOX[2]},{BBOX[3]},EPSG:4326")
    wgs = json.loads(get(q + "&srsName=EPSG:4326", DATA / "gbg_wgs84.json"))["features"]
    l72 = json.loads(get(q, DATA / "gbg_l72.json"))["features"]
    return wgs, l72


def ring(f):
    g = f["geometry"]
    return g["coordinates"][0] if g["type"] == "Polygon" else g["coordinates"][0][0]


def fit(wgs, l72):
    """A local quadratic map lon/lat <-> Lambert 72 from the same GRB vertices in both (~0.3 m)."""
    by = {f["properties"]["OIDN"]: f for f in wgs}
    P, Q = [], []
    for f in l72:
        g = by.get(f["properties"]["OIDN"])
        if g and len(ring(g)) == len(ring(f)):
            P += [p[:2] for p in ring(g)]
            Q += [q[:2] for q in ring(f)]
    P, Q = np.array(P), np.array(Q)

    def design(X):
        return np.c_[X[:, 0], X[:, 1], np.ones(len(X)), X[:, 0] ** 2, X[:, 1] ** 2, X[:, 0] * X[:, 1]]
    # centre the inputs, for a well-conditioned fit
    pm, qm = P.mean(0), Q.mean(0)
    fwd = [np.linalg.lstsq(design(P - pm), Q[:, i] - qm[i], rcond=None)[0] for i in range(2)]
    inv = [np.linalg.lstsq(design(Q - qm), P[:, i] - pm[i], rcond=None)[0] for i in range(2)]
    res = np.abs(np.c_[design(P - pm) @ fwd[0] + qm[0] - Q[:, 0], design(P - pm) @ fwd[1] + qm[1] - Q[:, 1]]).max()

    def to_l72(lon, lat):
        d = design(np.array([[lon - pm[0], lat - pm[1]]]))[0]
        return float(d @ fwd[0] + qm[0]), float(d @ fwd[1] + qm[1])

    def from_l72(x, y):
        d = design(np.array([[x - qm[0], y - qm[1]]]))[0]
        return float(d @ inv[0] + pm[0]), float(d @ inv[1] + pm[1])
    print(f"Lambert 72 fit on {len(P)} vertices, worst {res:.2f} m")
    return to_l72, from_l72


# ---------------------------------------------------------------- heights

class Heights:
    """nDSM (surface - terrain) on the 1 m DHMV II grid, sampled in local metres."""

    def __init__(self, to_l72):
        xs, ys = zip(*[to_l72(lo, la) for lo in (BBOX[0], BBOX[2]) for la in (BBOX[1], BBOX[3])])
        self.x0, self.y1 = math.floor(min(xs)), math.ceil(max(ys))
        x1, y0 = math.ceil(max(xs)), math.floor(min(ys))
        grids = []
        for cov in ("DHMVII_DSM_1m", "DHMVII_DTM_1m"):
            url = (f"{API}/DHMV/wcs?service=WCS&version=2.0.1&request=GetCoverage&CoverageId={cov}"
                   f"&format=image/tiff&subset=x({self.x0},{x1})&subset=y({y0},{self.y1})")
            raw = get(url, DATA / f"{cov}_{self.x0}_{y0}.wcs")
            tif = raw[raw.find(b"II*\x00"):raw.rfind(b"--wcs")].rstrip(b"\r\n")  # multipart: GML, then the TIFF
            grids.append(np.array(Image.open(io.BytesIO(tif)), dtype=np.float32))
        self.n = np.clip(grids[0] - grids[1], 0.0, 200.0)
        self.to_l72 = to_l72
        print(f"height model {self.n.shape[1]} x {self.n.shape[0]} m")

    def cells(self, poly_local, inset=0.8):
        """(x, y local, height) of the 1 m cells inside a local polygon."""
        p = poly_local.buffer(-inset) if poly_local.buffer(-inset).area > 4 else poly_local
        pp = prep(p)
        minx, miny, maxx, maxy = p.bounds
        out = []
        for y in np.arange(miny, maxy, 1.0):
            for x in np.arange(minx, maxx, 1.0):
                if pp.contains(Point(x, y)):
                    lx, ly = self.to_l72(*lonlat(x, y))
                    r, c = int(self.y1 - ly), int(lx - self.x0)
                    if 0 <= r < self.n.shape[0] and 0 <= c < self.n.shape[1]:
                        out.append((x, y, float(self.n[r, c])))
        return np.array(out) if out else np.zeros((0, 3))


# ---------------------------------------------------------------- orthophoto

class Ortho:
    """The 2025 orthophoto, by web-mercator tiles at zoom z (cached)."""

    def __init__(self, z=18):
        self.z = z
        self.tiles = {}

    def tile(self, tx, ty):
        if (tx, ty) not in self.tiles:
            url = (f"{API}/OMW/wmts?SERVICE=WMTS&REQUEST=GetTile&VERSION=1.0.0&LAYER=omwrgb25vl&STYLE=&FORMAT=image/png"
                   f"&TILEMATRIXSET=GoogleMapsVL&TILEMATRIX={self.z}&TILEROW={ty}&TILECOL={tx}")
            raw = get(url, DATA / "ortho" / f"{self.z}_{tx}_{ty}.png")
            try:
                self.tiles[(tx, ty)] = np.array(Image.open(io.BytesIO(raw)).convert("RGB"))
            except Exception:
                self.tiles[(tx, ty)] = np.zeros((256, 256, 3), np.uint8)
        return self.tiles[(tx, ty)]

    def px(self, lon, lat):
        n = 2 ** self.z
        x = (lon + 180) / 360 * n
        y = (1 - math.asinh(math.tan(math.radians(lat))) / math.pi) / 2 * n
        t = self.tile(int(x), int(y))
        return t[int((y % 1) * 256), int((x % 1) * 256)]

    def grid(self, lons, lats):
        """RGB (0..255) at many points at once."""
        n = 2 ** self.z
        x = (np.asarray(lons) + 180) / 360 * n
        y = (1 - np.arcsinh(np.tan(np.radians(np.asarray(lats)))) / np.pi) / 2 * n
        tx, ty = x.astype(int), y.astype(int)
        px, py = ((x % 1) * 256).astype(int), ((y % 1) * 256).astype(int)
        out = np.zeros((len(x), 3), np.float32)
        for key in set(zip(tx.tolist(), ty.tolist())):
            m = (tx == key[0]) & (ty == key[1])
            out[m] = self.tile(*key)[py[m], px[m]]
        return out

    def colour(self, poly_local, step=1.5):
        """Median colour (0..1, sRGB) inside a local polygon, its edges left out."""
        p = poly_local.buffer(-1.5) if poly_local.buffer(-1.5).area > 4 else poly_local
        pp = prep(p)
        minx, miny, maxx, maxy = p.bounds
        cs = [self.px(*lonlat(x, y)) for y in np.arange(miny, maxy, step) for x in np.arange(minx, maxx, step) if pp.contains(Point(x, y))]
        return (np.median(np.array(cs), axis=0) / 255.0).round(3).tolist() if cs else [0.5, 0.5, 0.5]


# ---------------------------------------------------------------- build

def building_rect(b):
    ca, sa = math.cos(b["rot"]), math.sin(b["rot"])
    return Polygon([(b["x"] + x * ca - y * sa, b["y"] + x * sa + y * ca) for x, y in
                    ((-b["L"] / 2, -b["D"] / 2), (b["L"] / 2, -b["D"] / 2), (b["L"] / 2, b["D"] / 2), (-b["L"] / 2, b["D"] / 2))])


def terminal_blocks(foot, H, tower_hint):
    """The terminal as blocks of one roof level each, from the height model inside its footprint."""
    c = H.cells(foot, inset=0.0)
    roof = c[c[:, 2] > 2.0]
    # Roof levels: peaks of the height histogram (0.5 m bins, smoothed), at
    # least 120 cells each; peaks closer than 1.2 m are one terrace. (Kept
    # apart, the LiDAR's few centimetres between neighbouring roofs made the
    # building step like a staircase.)
    hist, edges = np.histogram(roof[:, 2], bins=np.arange(2.0, 30.0, 0.5))
    sm = np.convolve(hist, [1, 2, 3, 2, 1], "same")
    peaks = []
    for i in range(1, len(sm) - 1):
        if sm[i] >= sm[i - 1] and sm[i] > sm[i + 1] and hist[max(0, i - 2):i + 3].sum() >= 120:
            peaks.append((float((edges[i] + edges[i + 1]) / 2), float(hist[max(0, i - 2):i + 3].sum())))
    clusters = []   # [weighted height, highest peak in it, cells]
    for h, n in sorted(peaks):
        if clusters and h - clusters[-1][1] < 1.2:
            w = clusters[-1][2]
            clusters[-1] = [(clusters[-1][0] * w + h * n) / (w + n), h, w + n]
        else:
            clusters.append([h, h, n])
    levels = [round(v[0], 1) for v in clusters]
    # each cell above 2 m to its nearest terrace; lower cells (courtyards) stay open
    lv = np.array(levels)
    near_lv = np.abs(roof[:, 2:3] - lv[None, :]).argmin(1)

    def outline(sel):
        """The cells as polygons with straight walls: specks dropped and what
        paint and vents cut out closed, then the LiDAR's 1 m saw edge taken
        off (simplified to 1.8 m), the corners kept."""
        g = unary_union([box(x - 0.5, y - 0.5, x + 0.5, y + 0.5) for x, y, _ in sel])
        g = g.buffer(1.5, join_style="mitre").buffer(-2.5, join_style="mitre").buffer(1.0, join_style="mitre")
        return g.simplify(1.8, preserve_topology=True).buffer(0)

    blocks = []
    taken = Polygon()
    for k in sorted(range(len(lv)), key=lambda k: -lv[k]):
        sel = roof[near_lv == k]
        if len(sel) < 40:
            continue
        g = outline(sel).intersection(foot).difference(taken).buffer(0)
        for part in getattr(g, "geoms", [g]):
            if part.geom_type == "Polygon" and part.area >= 40:
                # (cut by the taller terraces and the footprint: straighten again)
                part = part.simplify(1.0, preserve_topology=True)
                # one height for the whole terrace, so its parts meet flush
                h = float(lv[k])
                blocks.append({"poly": [[round(x, 2), round(y, 2)] for x, y in list(part.exterior.coords)[:-1]], "h": round(h, 1), "area": round(part.area)})
        taken = taken.union(g)
    # What the footprint still has without a block was built after the LiDAR
    # flight (2013-2015: the apron-side block with the brown roof): the
    # middle level, flagged as an estimate.
    rest = foot.difference(unary_union([Polygon(b["poly"]) for b in blocks]).buffer(0.6)).buffer(-1.0).buffer(1.0)
    mid = round(float(sorted(levels)[len(levels) // 2]), 1) if levels else 7.0
    for part in getattr(rest, "geoms", [rest]):
        if part.geom_type == "Polygon" and part.area >= 60:
            part = part.simplify(1.2, preserve_topology=True)
            blocks.append({"poly": [[round(x, 2), round(y, 2)] for x, y in list(part.exterior.coords)[:-1]], "h": mid,
                           "area": round(part.area), "estimated": True})
    # the tower: the highest point, and its cab roof height
    top = c[np.argmax(c[:, 2])]
    near = c[(np.hypot(c[:, 0] - top[0], c[:, 1] - top[1]) < 6.0) & (c[:, 2] > top[2] - 2.0)]
    tx, ty = near[:, 0].mean(), near[:, 1].mean()
    tower = {"x": round(float(tx), 2), "y": round(float(ty), 2), "top": round(float(np.percentile(near[:, 2], 60)), 2),
             "osm_offset_m": round(math.dist((tx, ty), tower_hint), 2) if tower_hint else None}
    return blocks, levels, tower


def apron_concrete(O19, aprons, step=0.5):
    """The concrete stand pads on the asphalt aprons: the brighter, greyer
    pavement on the orthophoto (two-class Otsu threshold on its luminance
    over each apron), cleaned and turned into polygons with their slab
    direction. Returns (pads, asphalt rgb, concrete rgb)."""
    pads, tones = [], {"asphalt": [], "concrete": []}
    for ring_set in aprons:
        ap = Polygon(ring_set[0], ring_set[1:]).buffer(-1.0)
        if ap.area < 2000:
            continue
        minx, miny, maxx, maxy = ap.bounds
        xs, ys = np.meshgrid(np.arange(minx, maxx, step), np.arange(miny, maxy, step))
        xs, ys = xs.ravel(), ys.ravel()
        inside = np.array([prep(ap).contains(Point(x, y)) for x, y in zip(xs, ys)])
        xs, ys = xs[inside], ys[inside]
        ll = np.array([lonlat(x, y) for x, y in zip(xs, ys)])
        rgb = O19.grid(ll[:, 0], ll[:, 1]) / 255.0
        lum = rgb @ np.array([0.2126, 0.7152, 0.0722])
        sat = rgb.max(1) - rgb.min(1)
        # Otsu on the luminance
        hist, edges = np.histogram(lum, bins=64, range=(0, 1))
        w = np.cumsum(hist); m = np.cumsum(hist * (edges[:-1] + edges[1:]) / 2)
        between = (m[-1] * w / w[-1] - m) ** 2 / np.maximum(w * (w[-1] - w), 1)
        t = (edges[:-1] + edges[1:])[np.argmax(between)] / 2
        conc = (lum > t) & (sat < 0.12)
        tones["asphalt"].append(np.median(rgb[~conc], axis=0))
        if conc.any():
            tones["concrete"].append(np.median(rgb[conc], axis=0))
        h = step / 2
        g = unary_union([box(x - h, y - h, x + h, y + h) for x, y in zip(xs[conc], ys[conc])])
        # close the joints and paint, drop specks
        # (aircraft, paint and stains on the photo read as dark: close over them)
        g = g.buffer(3.0, join_style="mitre").buffer(-5.0, join_style="mitre").buffer(2.0, join_style="mitre").intersection(ap)
        for part in getattr(g, "geoms", [g]):
            if part.geom_type != "Polygon" or part.area < 40:
                continue
            part = part.simplify(1.2)   # (the edge runs along slab lines: no pixel steps)
            r = list(part.minimum_rotated_rectangle.exterior.coords)
            e = max(((r[i], r[i + 1]) for i in range(2)), key=lambda q: math.dist(*q))
            pads.append({"ring": [[round(x, 2), round(y, 2)] for x, y in list(part.exterior.coords)[:-1]],
                         "holes": [[[round(x, 2), round(y, 2)] for x, y in list(hh.coords)[:-1]] for hh in part.interiors if Polygon(hh).area > 300],
                         "axis": round(math.atan2(e[1][1] - e[0][1], e[1][0] - e[0][0]), 4), "area": round(part.area)})
    avg = {k: (np.mean(v, axis=0).round(3).tolist() if v else None) for k, v in tones.items()}
    return pads, avg


def main():
    debug = "--debug" in sys.argv
    wgs, l72 = grb()
    to_l72, _ = fit(wgs, l72)
    H = Heights(to_l72)
    O = Ortho(18)
    gpolys = []
    for f in wgs:
        try:
            g = shape(f["geometry"])
        except Exception:
            continue
        g = g if g.geom_type == "Polygon" else max(g.geoms, key=lambda q: q.area)
        gpolys.append((f["properties"]["OIDN"], Polygon([local(*p[:2]) for p in g.exterior.coords])))
    out = {"sources": SOURCES, "buildings": {}, "terminal": None}
    rotunda = None
    for b in LAYOUT["buildings"]:
        osm = building_rect(b)
        parts = [g for _, g in gpolys if g.intersection(osm).area > 0.3 * min(g.area, osm.area)]
        if not parts:
            continue
        # One GRB polygon can hold two buildings (the rotunda and the museum): keep
        # what lies on this OSM building, so no two buildings share a footprint.
        g = unary_union(parts).intersection(osm.buffer(3.0, join_style="mitre")).buffer(0.3).buffer(-0.3)
        if g.is_empty:
            continue
        g = g if g.geom_type == "Polygon" else max(g.geoms, key=lambda q: q.area)
        if b["kind"] == "rotunda":
            rotunda = g
        c = H.cells(g)
        hs = c[:, 2] if len(c) else np.zeros(1)
        p05, p20, p50, p95, mx = (float(v) for v in np.percentile(hs, [5, 20, 50, 95, 100]))
        roof_rgb = O.colour(g)
        missing = p50 < 2.0
        roof = "flat" if (p95 - p05) < 1.2 else "gable"
        out["buildings"][str(b["id"])] = {
            "poly": [[round(x, 2), round(y, 2)] for x, y in list(g.exterior.coords)[:-1]],
            "area": round(g.area), "h": {"p05": round(p05, 2), "p20": round(p20, 2), "p50": round(p50, 2), "p95": round(p95, 2), "max": round(mx, 2)},
            "roof": roof, "roof_rgb": roof_rgb, "lidar_missing": bool(missing)}
        print(f"{b['id']:>11} {b['kind']:<6} {b['name'][:12]:<12} GRB {g.area:6.0f} m2 (OSM {osm.area:6.0f})  "
              f"h p05 {p05:5.1f} p50 {p50:5.1f} p95 {p95:5.1f}  {roof:<5} roof {roof_rgb}{'  NO LIDAR' if missing else ''}")
    # the terminal: GRB parts under OSM's terminal way (from the layout's terminal frame: hall + canopies + tower)
    T = LAYOUT["terminal"]
    o, A, C = T["origin"], T["a_axis"], T["c_axis"]
    # wider than the modelled site: the real building runs on along the apron
    a0, a1, c0, c1 = -70.0, 130.0, -95.0, 12.0
    site = Polygon([(o[0] + A[0] * a + C[0] * cc, o[1] + A[1] * a + C[1] * cc) for a, cc in ((a0, c0), (a1, c0), (a1, c1), (a0, c1))])
    parts = [g for _, g in gpolys if g.intersection(site).area > 0.5 * g.area and g.area > 150]
    if rotunda is not None:
        parts.append(rotunda)
    foot = unary_union(parts).buffer(0.3).buffer(-0.3)
    hint = (o[0] + A[0] * T["tower"][0] + C[0] * T["tower"][1], o[1] + A[1] * T["tower"][0] + C[1] * T["tower"][1])
    blocks, levels, tower = terminal_blocks(foot, H, hint)
    out["terminal"] = {"footprint": [[[round(x, 2), round(y, 2)] for x, y in list(p.exterior.coords)[:-1]] for p in getattr(foot, "geoms", [foot])],
                       "levels": levels, "blocks": blocks, "tower": tower,
                       "roof_rgb": O.colour(foot)}
    print(f"terminal: {len(parts)} GRB parts, {foot.area:.0f} m2; roof levels {levels}; {len(blocks)} blocks; tower {tower}")
    # the aprons' concrete pads (orthophoto 2025, 0.3 m)
    pads, tones = apron_concrete(Ortho(19), LAYOUT["areas"]["apron"])
    out["ground"] = {"concrete": pads, "tones": tones}
    print(f"aprons: {len(pads)} concrete pads, {sum(p['area'] for p in pads)} m2; tones {tones}")
    OUT.write_text(json.dumps(out, separators=(",", ":")))
    print("->", OUT)
    if debug:
        check_image(out, O)


def check_image(out, O):
    """The GRB footprints and the terminal's blocks drawn over the orthophoto (data/vl_check.jpg)."""
    from PIL import ImageDraw
    T = out["terminal"]
    pts = [p for b in T["blocks"] for p in b["poly"]]
    minx, miny = min(p[0] for p in pts) - 60, min(p[1] for p in pts) - 60
    maxx, maxy = max(p[0] for p in pts) + 60, max(p[1] for p in pts) + 60
    s = 4  # px per metre
    W, Hh = int((maxx - minx) * s), int((maxy - miny) * s)
    img = Image.new("RGB", (W, Hh))
    for j in range(0, Hh, 2):
        for i in range(0, W, 2):
            img.paste(tuple(int(v) for v in O.px(*lonlat(minx + i / s, maxy - j / s))), (i, j, i + 2, j + 2))
    d = ImageDraw.Draw(img)
    cols = [(255, 60, 60), (255, 200, 0), (0, 220, 255), (120, 255, 120), (255, 0, 255), (255, 255, 255)]
    lv = sorted({b["h"] for b in T["blocks"]})
    for b in T["blocks"]:
        xy = [((x - minx) * s, (maxy - y) * s) for x, y in b["poly"]]
        d.polygon(xy, outline=cols[lv.index(b["h"]) % len(cols)], width=3)
        cx, cy = sum(p[0] for p in xy) / len(xy), sum(p[1] for p in xy) / len(xy)
        d.text((cx, cy), f"{b['h']:.1f}", fill=(255, 255, 255))
    tw = T["tower"]
    d.ellipse([((tw["x"] - minx) * s - 12, (maxy - tw["y"]) * s - 12), ((tw["x"] - minx) * s + 12, (maxy - tw["y"]) * s + 12)], outline=(255, 0, 0), width=4)
    img.save(DATA / "vl_check.jpg", quality=85)
    print("->", DATA / "vl_check.jpg")
    # the concrete pads over the orthophoto, Apron 1
    pads = out.get("ground", {}).get("concrete", [])
    if pads:
        pts = [p for pd in pads for p in pd["ring"]]
        minx, miny = min(p[0] for p in pts) - 30, min(p[1] for p in pts) - 30
        maxx, maxy = max(p[0] for p in pts) + 30, max(p[1] for p in pts) + 30
        s = 1.5
        W, Hh = int((maxx - minx) * s), int((maxy - miny) * s)
        img = Image.new("RGB", (W, Hh))
        for j in range(0, Hh, 2):
            for i in range(0, W, 2):
                img.paste(tuple(int(v) for v in O.px(*lonlat(minx + i / s, maxy - j / s))), (i, j, i + 2, j + 2))
        d = ImageDraw.Draw(img)
        for pd in pads:
            d.polygon([((x - minx) * s, (maxy - y) * s) for x, y in pd["ring"]], outline=(255, 0, 255), width=2)
        img.save(DATA / "pads_check.jpg", quality=85)
        print("->", DATA / "pads_check.jpg")


if __name__ == "__main__":
    main()
