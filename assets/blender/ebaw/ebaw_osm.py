"""EBAW (Antwerp International) ground layout from OpenStreetMap.

Plain Python 3 + shapely (not Blender). Fetches the aerodrome's aeroway
features from the Overpass API, puts them in the scenery frame and writes
ebaw_layout.json next to this file, which 01_ground_layout.py reads.

    python3 assets/blender/ebaw/ebaw_osm.py            # fetch and build
    python3 assets/blender/ebaw/ebaw_osm.py osm.json   # build from a saved answer

Scenery frame (all EBAW modules share it):
  * 1 unit = 1 m. X = east, Y = north (Blender), Z = up, Z = 0 = paved surface.
  * Origin = the midpoint of runway 11/29 (from OSM), ORIGIN_LAT/LON below,
    aerodrome elevation 12 m (39 ft) AMSL.
  * Bearings are true degrees, clockwise from north.

The paved areas are made one partition with no overlaps: the runway, the
taxiways (OSM centrelines widened, junctions filleted) and the aprons, each
cut out of the others, the shoulders a band round them and the grass what is
left of the aerodrome boundary. So nothing overlaps and nothing hides under
anything else.

Map data (c) OpenStreetMap contributors, ODbL.
"""

import json
import math
import sys
import urllib.parse
import urllib.request
from pathlib import Path

from shapely.geometry import LineString, Point, Polygon, mapping
from shapely.ops import unary_union

HERE = Path(__file__).resolve().parent
OUT = HERE / "ebaw_layout.json"
VL = HERE / "ebaw_vl.json"   # Flemish open data (ebaw_vl.py), optional

# Runway 11/29 (OSM way 4310244; length 1510 m, width 45 m, asphalt).
RWY_W_END = (51.1922656, 4.4529535)  # west end (runway 11)
RWY_E_END = (51.1877066, 4.4731530)  # east end (runway 29)
ORIGIN_LAT = (RWY_W_END[0] + RWY_E_END[0]) / 2
ORIGIN_LON = (RWY_W_END[1] + RWY_E_END[1]) / 2
RWY_LEN = 1510.0
RWY_W = 45.0

TWY_W = 15.0        # lettered taxiways (code C width; no published figure at hand)
LINK_W = 12.0       # unnamed links outside the aprons
FILLET_R = 18.0     # junction fillets (closing radius)
SHOULDER = 6.0      # graded shoulder band round the pavement
RED_INSET = 7.0     # red equipment-limit line, inside the apron edge

# Terminal frame: its airside facade line, from two corners of OSM way 4414121
# (the terminal building), local metres. "a" runs along the facade (south-
# south-east), "c" out of it towards the apron; the tower is OSM way 909567225.
TERM_A0 = (-804.55, -53.97)
TERM_A1 = (-783.07, -146.01)
TERM_HALL = (4.0, 90.0, -50.0, 0.0)     # a0, a1, c0, c1 of the main hall
TERM_SITE = (-14.0, 108.0, -84.0, 6.0)  # paved forecourt, under the canopies too

# Airside buildings (step 3): every building within BLD_NEAR m of the pavement,
# bar the ones modelled elsewhere or not airside. Their kind sets the model.
BLD_NEAR = 30.0
BLD_MIN_AREA = 250.0
BLD_SKIP = {4414121, 787077901}          # the terminal (step 2), a school
BLD_KIND = {44961519: "fire",            # Airport Fire station
            44802699: "museum",          # Stampe & Vertongen museum: the long building north of the rotunda
                                         # (orthophoto 2025 and the photographs; assumed, not tagged in OSM)
            44802700: "rotunda",         # the terminal's round building: part of the terminal (step 2)
            41615256: "office"}          # small terminal annex
BLD_OPEN = {44802699, 445909590}         # hangars drawn with their doors open
FENCE_CLEAR = 12.0                       # perimeter fence kept this far from buildings

QUERY = """[out:json][timeout:60];
(way["aeroway"](51.178,4.425,51.202,4.495);
 node["aeroway"~"parking_position|holding_position|windsock|navigationaid"](51.178,4.425,51.202,4.495);
 node["airmark"="beacon"](51.178,4.425,51.202,4.495);
 way["building"](51.183,4.440,51.196,4.476););
out geom tags;"""

# WGS84 radii at the origin, for a local tangent plane (2 km across: < 1 cm error)
_A, _E2 = 6378137.0, 6.69437999014e-3
_S = math.sin(math.radians(ORIGIN_LAT))
_RN = _A / math.sqrt(1 - _E2 * _S * _S)
_RM = _RN * (1 - _E2) / (1 - _E2 * _S * _S)


def local(lat, lon):
    """(lat, lon) in degrees -> (x east, y north) metres from the origin."""
    x = math.radians(lon - ORIGIN_LON) * _RN * math.cos(math.radians(ORIGIN_LAT))
    y = math.radians(lat - ORIGIN_LAT) * _RM
    return (x, y)


def bearing(a, b):
    return math.degrees(math.atan2(b[0] - a[0], b[1] - a[1])) % 360.0


def polys(geom):
    """A shapely (Multi)Polygon -> [[outer, hole, ...], ...], 1 cm rounding."""
    if geom.is_empty:
        return []
    parts = getattr(geom, "geoms", [geom])
    out = []
    for p in parts:
        if p.geom_type != "Polygon" or p.area < 1.0:
            continue
        p = p.simplify(0.05)
        rings = [p.exterior] + list(p.interiors)
        out.append([[[round(x, 2), round(y, 2)] for x, y in list(r.coords)[:-1]] for r in rings])
    return out


def lines(geom):
    if geom.is_empty:
        return []
    parts = getattr(geom, "geoms", [geom])
    return [[[round(x, 2), round(y, 2)] for x, y in g.coords] for g in parts if g.geom_type == "LineString" and g.length > 1.0]


def airside_buildings(ways, paved, tag, geo):
    """Hangars and the other airside buildings: their minimum rectangle, the side
    facing the pavement (doors there) and a kind. Returns them and their union."""
    out, foots = [], []
    # Flemish open data (ebaw_vl.py): GRB footprints and DHMV II roof heights, where found
    vl = json.loads(VL.read_text())["buildings"] if VL.exists() else {}
    for x in ways:
        if "building" not in x.get("tags", {}) or x["id"] in BLD_SKIP:
            continue
        pts = geo(x)
        if len(pts) < 4:
            continue
        pg = Polygon(pts).buffer(0)
        mv = vl.get(str(x["id"]))
        # (a building the LiDAR has not seen may also differ from the cadastre: OSM then)
        if mv and (not mv["lidar_missing"] or abs(Polygon(mv["poly"]).area / pg.area - 1.0) < 0.25):
            pg = Polygon(mv["poly"]).buffer(0)
        if pg.area < BLD_MIN_AREA or pg.distance(paved) > BLD_NEAR:
            continue
        r = list(pg.minimum_rotated_rectangle.exterior.coords)[:4]
        # the door side: the rectangle side nearest the pavement
        sides = [(r[i], r[(i + 1) % 4]) for i in range(4)]
        di = min(range(4), key=lambda i: paved.distance(Point(((sides[i][0][0] + sides[i][1][0]) / 2, (sides[i][0][1] + sides[i][1][1]) / 2))))
        (x0, y0), (x1, y1) = sides[di]
        L = math.dist((x0, y0), (x1, y1))
        D = math.dist(sides[(di + 1) % 4][0], sides[(di + 1) % 4][1])
        cx, cy = pg.minimum_rotated_rectangle.centroid.x, pg.minimum_rotated_rectangle.centroid.y
        ax, ay = (x1 - x0) / L, (y1 - y0) / L
        # local +Y must point out through the door side: flip X if it points inwards
        mx, my = (x0 + x1) / 2 - cx, (y0 + y1) / 2 - cy
        if -ay * mx + ax * my < 0:
            ax, ay = -ax, -ay
        kind = BLD_KIND.get(x["id"])
        if kind is None:
            if min(L, D) < 14.0 and max(L, D) > 3 * min(L, D):
                kind = "t_row"
            elif pg.area < 500.0:
                kind = "shed"
            else:
                kind = "hangar"
        out.append({"id": x["id"], "name": tag(x, "name") or "", "kind": kind,
                    "x": round(cx, 2), "y": round(cy, 2), "rot": round(math.atan2(ay, ax), 5),
                    "L": round(L, 2), "D": round(D, 2), "open": x["id"] in BLD_OPEN,
                    # measured (ebaw_vl.py), else None: the model estimates from the span
                    "measured": None if not mv or mv["lidar_missing"] else {
                        "eave": mv["h"]["p20"] if mv["roof"] == "gable" else mv["h"]["p50"],
                        "ridge": mv["h"]["p95"], "roof": mv["roof"]},
                    "roof_rgb": mv["roof_rgb"] if mv else None, "grb": bool(mv)})
        foots.append(pg.minimum_rotated_rectangle)
    return out, unary_union(foots) if foots else Polygon()


def main():
    if len(sys.argv) > 1:
        osm = json.loads(Path(sys.argv[1]).read_text())
    else:
        req = urllib.request.Request(
            "https://overpass-api.de/api/interpreter",
            data=urllib.parse.urlencode({"data": QUERY}).encode(),
            headers={"User-Agent": "fableflight-scenery"},
        )
        osm = json.load(urllib.request.urlopen(req, timeout=90))
    els = osm["elements"]

    # Runway, centred on the OSM midpoint along the OSM bearing.
    w, e = local(*RWY_W_END), local(*RWY_E_END)
    hdg = bearing(w, e)  # runway 11's true heading
    ux, uy = math.sin(math.radians(hdg)), math.cos(math.radians(hdg))
    vx, vy = -uy, ux     # to the left of runway 11
    hl, hw = RWY_LEN / 2, RWY_W / 2
    rwy = Polygon([(-hl * ux - hw * vx, -hl * uy - hw * vy), (hl * ux - hw * vx, hl * uy - hw * vy),
                   (hl * ux + hw * vx, hl * uy + hw * vy), (-hl * ux + hw * vx, -hl * uy + hw * vy)])

    ways = [x for x in els if x["type"] == "way" and "geometry" in x]
    tag = lambda x, k: x.get("tags", {}).get(k)
    geo = lambda x: [local(p["lat"], p["lon"]) for p in x["geometry"]]

    aprons = [Polygon(geo(x)).buffer(0) for x in ways if tag(x, "aeroway") in ("apron", "helipad")]
    apron = unary_union(aprons)
    boundary = [Polygon(geo(x)).buffer(0) for x in ways if tag(x, "aeroway") == "aerodrome"][0]

    twy_lines, lane_lines, named = [], [], []
    for x in ways:
        if tag(x, "aeroway") != "taxiway":
            continue
        ln = LineString(geo(x))
        inside = ln.intersection(apron.buffer(2.0)).length / max(ln.length, 1e-6)
        if inside > 0.6 and not tag(x, "ref"):
            lane_lines.append(ln)           # an apron taxilane: paint only
        else:
            twy_lines.append(ln.buffer((TWY_W if tag(x, "ref") else LINK_W) / 2, cap_style="flat"))
            lane_lines.append(ln)
            if tag(x, "ref"):
                named.append((tag(x, "ref"), ln))

    # Everything paved, the junctions filleted by a closing, then split by kind.
    paved = unary_union([rwy, apron] + twy_lines)
    paved = paved.buffer(FILLET_R, quad_segs=6).buffer(-FILLET_R, quad_segs=6)
    paved = unary_union([paved, rwy, apron]).buffer(0)
    taxi = paved.difference(rwy).difference(apron)
    apr = apron.difference(rwy)
    shoulder = paved.buffer(SHOULDER, quad_segs=4).difference(paved).intersection(boundary.buffer(SHOULDER))
    grass = boundary.difference(paved.buffer(SHOULDER, quad_segs=4))

    # Terminal site: paved round the building, the hall itself left to the building's floor.
    ta, tb = TERM_A0, TERM_A1
    ln = math.dist(ta, tb)
    da = ((tb[0] - ta[0]) / ln, (tb[1] - ta[1]) / ln)
    dc = (-da[1], da[0]) if (-da[1]) > 0 else (da[1], -da[0])  # towards the apron (east)

    def tpoly(a0, a1, c0, c1):
        pt = lambda a, c: (ta[0] + da[0] * a + dc[0] * c, ta[1] + da[1] * a + dc[1] * c)
        return Polygon([pt(a0, c0), pt(a1, c0), pt(a1, c1), pt(a0, c1)])
    site, hall = tpoly(*TERM_SITE), tpoly(*TERM_HALL)
    plaza = site.difference(hall).difference(paved)
    grass = grass.difference(site)
    shoulder = shoulder.difference(site)
    buildings, foot = airside_buildings(ways, paved, tag, geo)
    grass = grass.difference(foot)
    shoulder = shoulder.difference(foot)
    plaza = plaza.difference(foot)
    tower = [x for x in ways if tag(x, "aeroway") == "tower"]
    tc = Polygon(geo(tower[0])).centroid if tower else hall.centroid
    rel = (tc.x - ta[0], tc.y - ta[1])
    terminal = {"origin": [round(ta[0], 3), round(ta[1], 3)],
                "a_axis": [round(da[0], 5), round(da[1], 5)], "c_axis": [round(dc[0], 5), round(dc[1], 5)],
                "hall": TERM_HALL, "site": TERM_SITE,
                "tower": [round(rel[0] * da[0] + rel[1] * da[1], 2), round(rel[0] * dc[0] + rel[1] * dc[1], 2)]}

    # Paint: taxiway centrelines stop at the runway edge.
    rwy_keep = rwy.buffer(0.5)
    centre = unary_union([ln.difference(rwy_keep) for ln in lane_lines])

    # Red equipment-limit line: inside the apron edge, broken where taxilanes come in.
    red = apron.buffer(-RED_INSET).boundary.difference(unary_union([ln.buffer(12.0) for ln in lane_lines]))

    def nearest_dir(pt, cands):
        best, bd = None, 1e9
        for ln in cands:
            d = ln.distance(pt)
            if d < bd:
                bd, best = d, ln
        s = best.project(pt)
        a, b = best.interpolate(max(s - 2, 0)), best.interpolate(min(s + 2, best.length))
        return bearing((a.x, a.y), (b.x, b.y)), bd

    stands, holds, socks, navaids = [], [], [], []
    for x in els:
        if x["type"] != "node":
            continue
        a = tag(x, "aeroway")
        p = local(x["lat"], x["lon"])
        pt = Point(p)
        if a == "parking_position":
            # The stand's own lead-in is the taxilane that ends at it.
            ends = [ln for ln in lane_lines if Point(ln.coords[-1]).distance(pt) < 1.5 or Point(ln.coords[0]).distance(pt) < 1.5]
            if ends:
                ln = ends[0]
                c = list(ln.coords)
                if Point(c[0]).distance(pt) < 1.5:
                    c = c[::-1]
                hd = bearing(c[-2], c[-1])  # nose-in heading at the stop point
            else:
                hd, _ = nearest_dir(pt, lane_lines)
            stands.append({"ref": tag(x, "ref") or "", "x": round(p[0], 2), "y": round(p[1], 2), "hdg": round(hd, 1)})
        elif a == "holding_position":
            hd, _ = nearest_dir(pt, lane_lines)
            # Point the heading towards the runway: the solid bars face away from it.
            to_rwy = bearing(p, (rwy.centroid.x, rwy.centroid.y))
            if math.cos(math.radians(hd - to_rwy)) < 0 and rwy.distance(pt) > 1:
                hd = (hd + 180) % 360
            near = rwy.exterior.interpolate(rwy.exterior.project(pt))
            if math.cos(math.radians(hd - bearing(p, (near.x, near.y)))) < 0:
                hd = (hd + 180) % 360
            # the taxiway it stands on, for the signs (nearest lettered taxiway)
            twy = min(named, key=lambda r: r[1].distance(pt))[0] if named else ""
            holds.append({"x": round(p[0], 2), "y": round(p[1], 2), "hdg": round(hd, 1), "twy": twy})
        elif a == "windsock":
            socks.append([round(p[0], 2), round(p[1], 2)])
        elif a == "navigationaid" or tag(x, "airmark") == "beacon":
            # PAPIs, and the radio beacons: localizer, glide slope, DVOR, markers
            navaids.append({"x": round(p[0], 2), "y": round(p[1], 2), "tags": x.get("tags", {})})

    # fuel station (aeroway=fuel): its minimum rectangle
    fuel = []
    for x in ways:
        if tag(x, "aeroway") == "fuel":
            r = Polygon(geo(x)).minimum_rotated_rectangle
            c = list(r.exterior.coords)
            ax = math.atan2(c[1][1] - c[0][1], c[1][0] - c[0][0])
            fuel.append({"x": round(r.centroid.x, 2), "y": round(r.centroid.y, 2), "rot": round(ax, 5),
                         "L": round(math.dist(c[0], c[1]), 2), "D": round(math.dist(c[1], c[2]), 2),
                         "name": tag(x, "name") or ""})
    # perimeter fence: the aerodrome boundary, kept clear of buildings and the terminal site
    keep_out = unary_union([foot, site]).buffer(FENCE_CLEAR)
    fence = boundary.exterior.difference(keep_out)

    # blue edge lights: along the taxiway edges (every 30 m, 15 m on curves) and the
    # apron edges, not where they meet the runway, an apron or a building
    def edge_points(area, avoid, step):
        pts = []
        geoms = getattr(area, "geoms", [area])
        for pg in geoms:
            for ring in [pg.exterior] + list(pg.interiors):
                L = ring.length
                s = 0.0
                while s < L:
                    p = ring.interpolate(s)
                    a, c = ring.interpolate(max(s - 3, 0)), ring.interpolate(min(s + 3, L))
                    turn = abs(math.atan2(c.y - p.y, c.x - p.x) - math.atan2(p.y - a.y, p.x - a.x))
                    if avoid.distance(p) > 2.0:
                        pts.append([round(p.x, 2), round(p.y, 2)])
                    s += step / 2 if min(turn, 2 * math.pi - turn) > 0.05 else step
        return pts
    twy_lights = edge_points(taxi, unary_union([rwy.buffer(3.0), apron, site, foot]), 30.0)
    apron_lights = edge_points(apr, unary_union([paved.difference(apr).buffer(1.0), site, foot]), 30.0)

    helipads = []
    for x in ways:
        if tag(x, "aeroway") == "helipad":
            pg = Polygon(geo(x))
            helipads.append({"name": tag(x, "name") or "", "x": round(pg.centroid.x, 2), "y": round(pg.centroid.y, 2),
                             "r": round(math.sqrt(pg.area / math.pi), 2)})

    layout = {
        "about": "EBAW ground layout from OpenStreetMap (c) OpenStreetMap contributors, ODbL. Built by ebaw_osm.py.",
        "frame": {"origin_lat": ORIGIN_LAT, "origin_lon": ORIGIN_LON, "elevation_m": 12.0,
                  "axes": "x east, y north, z up, metres", "bearings": "true degrees"},
        "runway": {"ref": "11/29", "hdg_11": round(hdg, 3), "length": RWY_LEN, "width": RWY_W,
                   "centre": [0.0, 0.0], "outline": polys(rwy)[0]},
        "areas": {"taxiway": polys(taxi), "apron": polys(apr), "shoulder": polys(shoulder), "grass": polys(grass),
                  "plaza": polys(plaza)},
        "terminal": terminal,
        "terminal_vl": json.loads(VL.read_text())["terminal"] if VL.exists() else None,
        "sources_vl": json.loads(VL.read_text())["sources"] if VL.exists() else [],
        "buildings": buildings,
        "paint": {"taxi_centre": lines(centre), "red_limit": lines(red)},
        "taxiway_names": [{"ref": r, "x": round(ln.interpolate(0.5, normalized=True).x, 2),
                           "y": round(ln.interpolate(0.5, normalized=True).y, 2)} for r, ln in named],
        "stands": stands, "holds": holds, "helipads": helipads, "windsocks": socks, "navaids": navaids,
        "fuel": fuel, "fence": lines(fence),
        "lights": {"taxiway_edge": twy_lights, "apron_edge": apron_lights},
    }
    OUT.write_text(json.dumps(layout, separators=(",", ":")))
    print(f"runway 11 true heading {hdg:.2f}, {len(stands)} stands, {len(holds)} holds -> {OUT}")
    for k, v in layout["areas"].items():
        print(f"  {k}: {len(v)} polygons, {sum(len(r) for p in v for r in p)} vertices")


if __name__ == "__main__":
    main()
