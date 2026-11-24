// The aircraft (and its wreckage): paint scheme, panel seams and rivets,
// weathering, the cockpit's instruments and radios, and the transparent pass
// (windows and the propeller blur). Every vertex carries its body-frame
// position at rest (L), so whatever is painted here stays put on moving parts.
// Material, pattern, light and instrument ids follow airframe.rs / cabin.rs.

// tear: up to two breaks through the body frame (plane normal, offset); dmg:
// the mode of each (see damage.rs), scrapes 0..1, raggedness of the edge (m);
// dmg2: oil film on the windshield, crack centre z and y, crack size;
// crumple: a lasting dent's centre (body frame) and radius, crumple_dir the
// way it was pushed in and its depth (damage.rs); pose: the top three rows of
// the piece's pose, mesh frame to body frame; flex.x: wing bending, the tip's
// deflection over the half-span squared (main.rs, plane.rs `wing_flex`).
struct Obj { model: mat4x4f, tint: vec4f, glow: vec4f, params: array<vec4f, 6>, tear: array<vec4f, 2>, dmg: vec4f, dmg2: vec4f, crumple: vec4f, crumple_dir: vec4f, pose: array<vec4f, 3>, flex: vec4f };
@group(1) @binding(0) var<uniform> O: Obj;
@group(2) @binding(0) var<uniform> CS: Cascade;
// The glass pass only: the lit scene before the glass went on, and its depth.
@group(2) @binding(1) var t_scene: texture_2d<f32>;
@group(2) @binding(2) var t_gdepth: texture_depth_2d;

// params[0] airspeed kt, altitude ft, vertical speed fpm, heading deg
// params[1] pitch deg, bank deg, turn rate deg/s, slip
// params[2] rpm, manifold pressure, fuel left, fuel right (gal)
// params[3] oil temp F, oil pressure psi, EGT 0..1, amps
// params[4] suction, clock (s), gear position, flaps deg
// params[5] nav lights (on: 1000 + the compass card's heading), strobe flash, beacon flash, landing light

const M_PAINT = 0u; const M_LIVERY = 1u; const M_ALU = 2u; const M_STEEL = 3u; const M_RUBBER = 4u;
const M_PLASTIC = 5u; const M_FABRIC = 6u; const M_GLASS = 7u; const M_LAMP = 8u; const M_GAUGE = 9u;
const M_BLACK = 10u; const M_EXHAUST = 11u; const M_SCREW = 12u; const M_PROP = 13u; const M_DISPLAY = 14u;
const M_SATIN = 15u; const M_PROPDISC = 16u; const M_PLUME = 17u; const M_TEX = 18u;
const P_FUSE = 1u; const P_WING = 2u; const P_CTRL = 3u; const P_COWL = 4u; const P_TAIL = 5u; const P_TREAD = 6u;
const P_LINING = 7u; const P_SEAT = 8u; const P_CARPET = 9u; const P_PANEL = 10u; const P_PLACARD = 11u; const P_KNURL = 12u;
const P_NACELLE = 13u; const P_LEGEND = 14u; const P_FLEECE = 15u; const P_SPINNER = 16u; const P_HORNET = 17u;
// The Hornet's cockpit (aircraft/fa18/pit.rs, pit18.wgsl).
const P_PIT = 18u; const P_MESH = 19u; const P_WEBBING = 20u; const P_NONSLIP = 21u; const P_STRIPE = 22u; const P_HOSE = 23u;

// The paint of the aircraft being flown (aircraft::Paint, e.g. the Hornet's
// assets/fa18_paint.jpg): it seen from above, from below and from the left,
// layers 0 to 2, and for some from the right, layer 3. PT.box: the rectangle
// each picture covers, across (and up the side views) and fore and aft; how
// far the pictures' edges are from the datum, ahead and below. PT.view.x: how
// many pictures (0: it has no paint); .y: how much relief its lines stand
// for; .z: how much its sheen follows its tone (a dark paint's panels).
@group(1) @binding(1) var t_paint: texture_2d_array<f32>;
// A downloaded aircraft's meshes bind its own textures there instead (fgmodel.rs):
// layer `layer` (< 0: none), repeating.
fn tex_at(uv: vec2f, layer: f32, dx: vec2f, dy: vec2f) -> vec4f {
  if (layer < -0.5) { return vec4f(1.0); }
  return textureSampleGrad(t_paint, s_aniso, fract(uv), i32(layer + 0.5), dx, dy);
}
// PT.a .. PT.d, PT.title: the paint scheme picked in the aircraft picker
// (aircraft::livery_uniform, each type's LIVERIES): four colours, the style in
// a.w (all zero when the type has none), and an airline's name, 8 bits a
// glyph with its length in the top byte (`title_ink`). b.w is the flown
// type's kind + 1: only that type reads the scheme (`livery_ours`).
struct Paint { box: vec4f, view: vec4f, a: vec4f, b: vec4f, c: vec4f, d: vec4f, title: vec4u };
@group(1) @binding(2) var<uniform> PT: Paint;

// Which aircraft is being drawn (aircraft::Spec::id, carried in glow.y): 0 the
// light single, 1 the 747, 2 the Hornet, 3 the Phantom, 4 the Raptor, 5 the Starship rocket. Its windows, wells,
// paint and panel lines differ. tint.y and tint.z carry the engines: how far the afterburners
// are lit, and how hard they are running dry (0 idle .. 1).
// specialisation, render.rs `specialise` rewrites these two lines. a kind under 99 is fixed.
const KIND: u32 = 99u;
// instrument faces and displays, 0 left out (drawn by their own pipeline), 1 drawn.
const INSTR: u32 = 1u;
fn kind() -> u32 { if (KIND < 99u) { return KIND; } return u32(O.glow.y + 0.5); }
// The fighters: modelled in Blender, their paint a picture, one blown canopy over a tub.
fn fighter() -> bool { return (kind() >= 2u && kind() <= 4u) || kind() == 7u; }
// The engines, of the aircraft being flown only (wreckage and parts that have come off carry a glow.x).
fn burner() -> f32 { return O.tint.y * step(O.glow.x, 0.001); }
fn dry_power() -> f32 { return O.tint.z * step(O.glow.x, 0.001); }

const WHITE_PAINT = vec3f(0.80, 0.80, 0.78);

struct VOut {
  @builtin(position) clip: vec4f,
  @location(0) P: vec3f,
  @location(1) N: vec3f,
  @location(2) col: vec4f,
  @location(3) uv: vec2f,
  @location(4) @interpolate(flat) mat: vec4f,
  @location(5) wear: f32,
  @location(6) L: vec3f,
  @location(7) Nl: vec3f,
};
struct GOut { @location(0) c: vec4f, @location(1) n: vec4f, @location(2) amb: f32 };
struct SOut { @builtin(position) clip: vec4f, @location(0) L: vec3f, @location(1) @interpolate(flat) mat: vec4f };

// The airframe's shape under load and after a hard hit, worked out in the
// body frame (so a deflected aileron bends with its wing about the bent hinge):
// the wing bent up in a parabola from the centreline to the tip's deflection,
// and a lasting dent, a smooth bowl pushed in along the blow, only on the
// faces turned to it (the far side of a fuselage stays put). The normal is
// tilted by the same displacement so the light shows the shape. Glass and a
// rocket's plume are left as they are.
struct Bent { p: vec3f, n: vec3f };
fn deform(p: vec3f, n: vec3f, mat: vec4f) -> Bent {
  let m = u32(mat.x + 0.5);
  let depth = O.crumple_dir.w;
  let dented = O.crumple.w > 0.001 && depth > 0.001;
  if (m == M_GLASS || m == M_PLUME || (!dented && O.flex.x == 0.0)) { return Bent(p, n); }
  let r0 = O.pose[0]; let r1 = O.pose[1]; let r2 = O.pose[2];
  let b = vec3f(dot(r0, vec4f(p, 1.0)), dot(r1, vec4f(p, 1.0)), dot(r2, vec4f(p, 1.0)));
  let nb = vec3f(dot(r0.xyz, n), dot(r1.xyz, n), dot(r2.xyz, n));
  var d = vec3f(0.0);
  // For a displacement d(b) the normal goes (to first order) to n - J^T n,
  // J the displacement's gradient.
  var jn = vec3f(0.0);
  let bend = O.flex.x;
  d.y += bend * b.x * b.x;
  jn.x += 2.0 * bend * b.x * nb.y;
  if (dented) {
    let rel = b - O.crumple.xyz;
    let dist = length(rel);
    let rad = O.crumple.w;
    if (dist < rad) {
      let t = 1.0 - dist / rad;
      let inward = normalize(O.crumple_dir.xyz);
      let facing = smoothstep(-0.1, 0.65, dot(nb, -inward));
      let wrinkle = nz(b * 5.5).x - 0.5;
      let k = depth * facing;
      d += inward * (k * t * t * (3.0 - 2.0 * t) * (1.0 + 0.12 * wrinkle));
      // (d/d dist of the smoothstep bowl, along the way out from its centre)
      jn += rel / max(dist, 1e-4) * (-k * 6.0 * t * (1.0 - t) / rad) * dot(inward, nb);
    }
  }
  // Back into the mesh frame: the pose's rotation is orthonormal, so its
  // transpose takes body-frame vectors back.
  let back = d.x * r0.xyz + d.y * r1.xyz + d.z * r2.xyz;
  let nn = nb - jn;
  return Bent(p + back, normalize(nn.x * r0.xyz + nn.y * r1.xyz + nn.z * r2.xyz));
}

@vertex fn vs(@location(0) pos: vec3f, @location(1) nrm: vec3f, @location(2) col: vec4f, @location(3) uv: vec2f, @location(4) mat: vec4f) -> VOut {
  // (A rocket's plume is as wide as the engines burning make it, and spreads in thin air.)
  let bent = deform(pos, nrm, mat);
  var at = bent.p;
  if (kind() == 5u && u32(mat.x + 0.5) == M_PLUME) { at = vec3f(pos.xy * rocket_spread(pos), pos.z); }
  let P = (O.model * vec4f(at, 1.0)).xyz;
  let N = (O.model * vec4f(bent.n, 0.0)).xyz;
  return VOut(G.view_proj * vec4f(P, 1.0), P, N, col, uv, mat, mat.z, pos, nrm);
}
@vertex fn vs_shadow(@location(0) pos: vec3f, @location(1) nrm: vec3f, @location(4) mat: vec4f) -> SOut {
  let at = deform(pos, nrm, mat).p;
  return SOut(CS.vp * vec4f((O.model * vec4f(at, 1.0)).xyz, 1.0), pos, mat);
}
@fragment fn fs_shadow(in: SOut) {
  // Sunlight comes in through the windows, the gear wells stay open, and
  // nothing is cast by what has been torn off.
  if (cut_here(in.L, u32(in.mat.y + 0.5)) || u32(in.mat.x + 0.5) == M_GLASS || u32(in.mat.x + 0.5) == M_PROPDISC || u32(in.mat.x + 0.5) == M_PLUME || tear_d(in.L) > 0.0) { discard; }
}

// ---------------------------------------------------------------- helpers

fn sat(x: f32) -> f32 { return clamp(x, 0.0, 1.0); }
fn nz(p: vec3f) -> vec4f { return textureSampleLevel(t_noise, s_repeat, p, 0.0); }
fn seg_d(p: vec2f, a: vec2f, b: vec2f) -> f32 {
  let pa = p - a;
  let ba = b - a;
  let h = clamp(dot(pa, ba) / max(dot(ba, ba), 1e-9), 0.0, 1.0);
  return length(pa - ba * h);
}
fn line(d: f32, w: f32) -> f32 { return exp(-d * d / (w * w)); }
fn rot2(p: vec2f, a: f32) -> vec2f { let c = cos(a); let s = sin(a); return vec2f(c * p.x - s * p.y, s * p.x + c * p.y); }
// A row of round-headed rivets along `along`, `across` from the row: 0..1 dome.
fn rivets(along: f32, across: f32, pitch: f32, r: f32) -> f32 {
  let a = (fract(along / pitch + 0.5) - 0.5) * pitch;
  return max(0.0, 1.0 - (a * a + across * across) / (r * r));
}

// Exact signed distance to a polygon of n <= 5 points (negative inside).
fn sd_poly(p: vec2f, vin: array<vec2f, 5>, n: i32) -> f32 {
  var v = vin;
  var d = dot(p - v[0], p - v[0]);
  var s = 1.0;
  var j = n - 1;
  for (var i = 0; i < n; i++) {
    let e = v[j] - v[i];
    let w = p - v[i];
    let b = w - e * clamp(dot(w, e) / dot(e, e), 0.0, 1.0);
    d = min(d, dot(b, b));
    let c1 = p.y >= v[i].y;
    let c2 = p.y < v[j].y;
    let c3 = e.x * w.y > e.y * w.x;
    if ((c1 && c2 && c3) || (!c1 && !c2 && !c3)) { s = -s; }
    j = i;
  }
  return s * sqrt(d);
}
fn quad(a: vec2f, b: vec2f, c: vec2f, d: vec2f) -> array<vec2f, 5> { return array<vec2f, 5>(a, b, c, d, a); }

// Window openings (negative inside); matches c172::window / b747::window.
fn window(p: vec3f) -> f32 {
  if (kind() == 1u) { return window747(p); }
  if (kind() == 10u) { return window_bonanza(p); }
  // (The Robin's canopy is a glass piece of its own, all of it clear.)
  if (kind() == 8u) { return -1.0; }
  // A fighter's canopy is blown glass: all of it is window.
  if (fighter()) { return -1.0; }
  // (Concorde's windscreen, side windows and visor are glass pieces of their own.)
  if (kind() == 6u) { return -1.0; }
  // (So are the G650's windshield, side windows and cabin windows: its skin has the openings cut in it.)
  if (kind() == 9u) { return -1.0; }
  let zy = vec2f(p.z, p.y);
  var d = sd_poly(zy, array<vec2f, 5>(vec2f(-1.72, 0.31), vec2f(-1.52, 0.07), vec2f(-1.03, 0.64), vec2f(-1.0, 0.80), vec2f(-1.75, 0.80)), 5) + 0.01;
  if (abs(p.x) > 0.3 && p.y < 0.6) {
    d = min(d, sd_poly(zy, quad(vec2f(-1.36, 0.12), vec2f(-0.60, 0.12), vec2f(-0.60, 0.48), vec2f(-0.98, 0.48)), 4) - 0.04);
    d = min(d, sd_poly(zy, quad(vec2f(-0.42, 0.12), vec2f(0.62, 0.12), vec2f(0.42, 0.48), vec2f(-0.42, 0.48)), 4) - 0.04);
  }
  if (p.y > 0.35) {
    d = min(d, sd_poly(vec2f(p.z, p.x), quad(vec2f(0.84, -0.30), vec2f(1.48, -0.17), vec2f(1.48, 0.17), vec2f(0.84, 0.30)), 4) - 0.04);
  }
  return d;
}
// Landing gear wells (only the skin has them); matches c172::well / b747::well.
fn well(p: vec3f) -> f32 {
  if (kind() == 1u) { return well747(p); }
  if (kind() == 10u) { return well_bonanza(p); }
  if (kind() == 8u) { return 1.0; }
  // (The G650's wells are cut into its exported skin.)
  if (kind() == 9u) { return 1.0; }
  var d = 1.0;
  if (p.y < -0.40) {
    d = min(d, sd_poly(vec2f(p.z, p.x), quad(vec2f(-3.0, -0.135), vec2f(-1.915, -0.135), vec2f(-1.915, 0.135), vec2f(-3.0, 0.135)), 4) + 0.005);
    let q = vec2f(p.z, abs(p.x));
    let trough = sd_poly(q, quad(vec2f(0.215, 0.37), vec2f(1.15, 0.183), vec2f(1.15, 0.427), vec2f(0.215, 0.55)), 4);
    let bay = sd_poly(q, array<vec2f, 5>(vec2f(1.14, 0.03), vec2f(1.545, 0.03), vec2f(1.545, 0.43), vec2f(1.27, 0.505), vec2f(1.14, 0.44)), 5);
    d = min(d, min(trough, bay) + 0.005);
  }
  return d;
}
// The Robin's two round cooling-air inlets in the nose bowl (dr400::INLET,
// body frame): negative inside, where the skin is cut away to the ducts.
fn inlet_dr400(L: vec3f) -> f32 {
  if (L.z > -1.62) { return 1.0; }
  return length(vec2f(abs(L.x) - 0.225, L.y + 0.19)) - 0.060;
}
fn cut_here(L: vec3f, pt: u32) -> bool {
  // Nothing else is cut out of the Robin's skin: it simply stops under the canopy.
  if (kind() == 8u) { return pt == P_COWL && inlet_dr400(L) < 0.0; }
  // The 747's wing gear passes through the lower wing skin as well.
  let wing_well = kind() == 1u && pt == P_WING && well(L) < 0.0;
  return wing_well || ((pt == P_FUSE || pt == P_COWL || pt == P_LINING) && (window(L) < 0.0 || (pt != P_LINING && well(L) < 0.0)));
}
fn torn() -> bool { return O.dmg.x != 0.0 || O.dmg.y != 0.0; }
// Signed distance past the piece's break(s): positive where it has been torn
// away. The edge wanders by dmg.w, the same way on both sides of the break,
// so the stump and the part that left share one ragged line.
fn tear_d(L: vec3f) -> f32 {
  if (!torn()) { return -1e3; }
  let jag = O.dmg.w;
  // A few big jags, torn skin between them, and fine shredding along the edge.
  let n1 = (nz(L * 1.4).r - 0.5) * 1.6 + (nz(L * 4.5).b - 0.5) * 0.7;
  let n2 = nz(L * 16.0).g - 0.5;
  var s = -1e3;
  for (var i = 0; i < 2; i++) {
    let m = select(O.dmg.x, O.dmg.y, i == 1);
    if (m == 0.0) { continue; }
    let t = O.tear[i];
    var q = L;
    if (abs(m) > 1.5) { q.x = abs(q.x); }
    var v = dot(q, t.xyz) - t.w + n1 * jag + n2 * jag * 0.35;
    if (m < 0.0) { v = -v; }
    s = max(s, v);
  }
  return s;
}
// Cabin door outline in (z, y).
fn door(L: vec3f) -> f32 {
  return sd_poly(vec2f(L.z, L.y), array<vec2f, 5>(vec2f(-1.47, -0.42), vec2f(-0.47, -0.42), vec2f(-0.47, 0.59), vec2f(-0.99, 0.59), vec2f(-1.47, 0.03)), 5);
}
fn baggage_door(L: vec3f) -> f32 {
  let q = vec2f(L.z - 1.25, L.y + 0.14);
  let b = abs(q) - vec2f(0.20, 0.17) + 0.03;
  return length(max(b, vec2f(0.0))) + min(max(b.x, b.y), 0.0) - 0.03;
}

// ---------------------------------------------------------------- lettering

// Keep dynamically indexed font data in a uniform. Inlining the tables at
// every label can exhaust the Asahi shader compiler's register allocator.
struct StrokeFont {
  segments: array<vec4f, 237>,
  starts: array<vec4u, 14>,
  strings: array<vec4u, STRING_COUNT>,
};
@group(1) @binding(3) var<uniform> font: StrokeFont;

fn glyph_start(g: u32) -> u32 { return font.starts[g / 4u][g % 4u]; }
fn glyph_d(g: u32, p: vec2f) -> f32 {
  var d = 1e3;
  let s = glyph_start(g);
  let e = glyph_start(g + 1u);
  for (var i = s; i < e; i++) {
    let q = font.segments[i];
    d = min(d, seg_d(p, q.xy, q.zw));
  }
  return d;
}
// A packed string (4 bits a glyph, first lowest) at p in glyph units: glyphs
// are 1 wide, 2 tall, 1.45 apart. Returns ink coverage.
fn text(p: vec2f, code: u32, n: u32, stroke: f32, aa: f32) -> f32 {
  if (p.y < -0.6 || p.y > 2.6 || p.x < -0.6) { return 0.0; }
  let k = i32(floor((p.x + 0.225) / 1.45));
  if (k < 0 || k >= i32(n)) { return 0.0; }
  let g = (code >> (4u * u32(k))) & 15u;
  let d = glyph_d(g, vec2f(p.x - f32(k) * 1.45, p.y));
  return 1.0 - smoothstep(stroke - aa, stroke + aa, d);
}
fn num_code(v: u32) -> vec2u {
  var digs = array<u32, 6>(0u, 0u, 0u, 0u, 0u, 0u);
  var x = v;
  var n = 0u;
  loop {
    digs[n] = x % 10u;
    x = x / 10u;
    n = n + 1u;
    if (x == 0u || n >= 6u) { break; }
  }
  var code = 0u;
  for (var i = 0u; i < n; i++) { code = code | (digs[n - 1u - i] << (4u * i)); }
  return vec2u(code, n);
}
// A string centred on c, glyph height h (same units as p).
fn label_code(p: vec2f, c: vec2f, code: u32, n: u32, h: f32, aa: f32) -> f32 {
  let s = h * 0.5;
  let w = (f32(n) * 1.45 - 0.45) * s;
  return text((p - c + vec2f(w * 0.5, h * 0.5)) / s, code, n, 0.17, aa / s + 0.02);
}
fn pack(a: u32, b: u32, c: u32, d: u32, e: u32, f: u32) -> u32 {
  return a | (b << 4u) | (c << 8u) | (d << 12u) | (e << 16u) | (f << 20u);
}

// ---------------------------------------------------------------- paint scheme

fn livery(L: vec3f, aa: f32, pt: u32) -> vec3f {
  if (kind() == 1u) { return livery747(L, aa, pt); }
  if (kind() == 8u) { return livery_dr400(L, aa, pt); }
  if (kind() == 10u) { return livery_bonanza(L, aa, pt); }
  // The Cessna: white, and the scheme picked for it (PT.a: main colour, w 1
  // if painted; PT.b: the pinstripe).
  var c = WHITE_PAINT;
  let ax = abs(L.x);
  // (Another type flown: the default navy and red.)
  let ours = livery_ours();
  let painted = !ours || PT.a.w > 0.5;
  let prim = select(vec3f(0.010, 0.024, 0.075), PT.a.rgb, ours);
  let trim = select(vec3f(0.34, 0.018, 0.012), PT.b.rgb, ours);
  let fin = ax < 0.15 && L.y > 0.33 && L.z > 2.8;
  // No scheme: no paint at all, the airframe in bare aluminium (made metallic
  // in the material switch, `bare_c172`), only the registration painted on.
  if (!painted) { c = vec3f(0.62, 0.63, 0.65); }
  if (painted) {
    // Cheat line along the fuselage, sweeping up the tail cone.
    if (!fin && ax < 0.62 && L.y < 0.62 && L.z > -3.16 && L.z < 4.7) {
      let zz = max(L.z - 1.0, 0.0);
      let yc = -0.18 + zz * zz * 0.045;
      let dy = L.y - yc;
      let taper = smoothstep(-3.16, -2.85, L.z);
      let hw = 0.065 * taper;
      let band = smoothstep(hw + aa, hw - aa, abs(dy));
      let pin = smoothstep(0.009 * taper + aa, 0.009 * taper - aa, abs(dy - 0.095 * taper));
      c = mix(c, prim, band);
      c = mix(c, trim, pin);
    }
    if (fin) {
      let band = smoothstep(aa, -aa, abs(L.y - 1.09) - 0.07);
      let pin = smoothstep(aa, -aa, abs(L.y - 1.205) - 0.014);
      c = mix(c, prim, max(band, smoothstep(1.69 - aa, 1.69 + aa, L.y)));
      c = mix(c, trim, pin);
    }
    // Wing and stabiliser tips, spinner.
    if (ax > 5.26 && L.y > 0.5) { c = prim; }
    if (ax > 1.53 && L.z > 3.3 && L.y > -0.1 && L.y < 0.3) { c = prim; }
    if (L.z < -3.178) { c = prim; }
  }
  // Registration on both sides of the tail cone, reading nose-to-tail on the
  // left and tail-to-nose on the right as painted: in the scheme's colour, or
  // black on a bare aeroplane.
  if (ax > 0.1 && L.z > 1.6 && L.z < 3.0 && L.y > 0.0 && L.y < 0.3) {
    let tx = (2.25 - L.z) * sign(L.x) + 0.44;
    let reg = pack(10u, 4u, 1u, 9u, 6u, 12u);
    let ink = text(vec2f(tx, L.y - 0.045) / 0.09, reg, 6u, 0.13, aa / 0.09 + 0.01);
    c = mix(c, select(vec3f(0.012, 0.012, 0.014), prim, painted), ink);
  }
  return c;
}

// The Cessna with no paint scheme: its skin is bare aluminium.
fn bare_c172() -> bool { return kind() == 0u && livery_ours() && PT.a.w < 0.5; }

// Whether the scheme in PT is this aircraft's: it is the flown type's, and
// the uniform is shared by everything drawn (multiplayer aircraft, wreckage).
fn livery_ours() -> bool { return u32(PT.b.w + 0.5) == kind() + 1u; }
// The style of the scheme picked (PT.a.w; 0 when none was sent, or for
// another type than the flown one).
fn livery_style() -> u32 { return select(0u, u32(PT.a.w + 0.5), livery_ours()); }
// The airline's name (PT.title) centred on c, glyph height h, stroke half
// width w in glyph units: ink coverage.
fn title_ink(p: vec2f, c: vec2f, h: f32, w: f32, aa: f32) -> f32 {
  let n = PT.title.w >> 24u;
  if (n == 0u) { return 0.0; }
  let s = h * 0.5;
  let q = (p - c + vec2f((f32(n) * 1.45 - 0.45) * s * 0.5, s)) / s;
  if (q.x < -0.6 || q.x > f32(n) * 1.45 || q.y < -0.6 || q.y > 2.6) { return 0.0; }
  let k = u32(max(floor((q.x + 0.225) / 1.45), 0.0));
  if (k >= n) { return 0.0; }
  let g = (PT.title[k / 4u] >> (8u * (k % 4u))) & 255u;
  if (g == 15u) { return 0.0; }
  let e = aa / s + 0.02;
  return 1.0 - smoothstep(w - e, w + e, glyph_d(g, vec2f(q.x - f32(k) * 1.45, q.y)));
}

// The Robin DR 400: white, a blue cheat line from the spinner to the tail
// with a red pinstripe under it, the top of the fin blue, red wing tips and
// spinner (a classic Robin scheme, made up; no registration). The picker's
// other schemes (dr400::LIVERIES) recolour the same design.
const ROBIN_BLUE = vec3f(0.012, 0.045, 0.20);
const ROBIN_RED = vec3f(0.42, 0.02, 0.015);
// Coverage of the walkway on the top of a wing root (body frame; 0 off it).
fn walkway_dr400(Lb: vec3f, aa: f32) -> f32 {
  let L = Lb - vec3f(0.0, 0.0, 0.55);
  let ax = abs(L.x);
  let xc = (L.z + 1.0) / 1.71;
  if (ax < 0.56 || ax > 1.0 || xc < 0.05 || xc > 0.75 || L.y < -0.66 - 0.0895 * xc) { return 0.0; }
  let e = min(min(ax - 0.585, 0.975 - ax), min(xc - 0.08, 0.72 - xc) * 1.71);
  return smoothstep(-aa, aa, e);
}
fn livery_dr400(Lb: vec3f, aa: f32, pt: u32) -> vec3f {
  // (In dr400.rs's design frame, 0.55 m ahead of the body frame.)
  let L = Lb - vec3f(0.0, 0.0, 0.55);
  // The scheme picked (dr400::LIVERIES): cheat line, pinstripe, tips, airframe.
  let picked = livery_style() > 0u;
  let line_c = select(ROBIN_BLUE, PT.a.rgb, picked);
  let pin_c = select(ROBIN_RED, PT.b.rgb, picked);
  let tip_c = select(ROBIN_RED, PT.c.rgb, picked);
  let body_c = select(WHITE_PAINT, PT.d.rgb, picked);
  var c = body_c;
  let ax = abs(L.x);
  // The black anti-skid walkway on top of each wing root, greyer where worn.
  if (pt == P_WING) {
    let wk = walkway_dr400(Lb, aa);
    if (wk > 0.0) {
      let worn = smoothstep(0.55, 0.85, nz(L * vec3f(9.0, 9.0, 3.0)).g) * 0.5;
      return mix(c, mix(vec3f(0.022, 0.022, 0.024), vec3f(0.055, 0.052, 0.05), worn), wk);
    }
  }
  let fin = ax < 0.12 && L.y > 0.12 && L.z > 3.1;
  if (!fin && ax < 0.6 && L.y < 0.5 && L.y > -0.5 && L.z > -2.42 && L.z < 4.4) {
    let zz = max(L.z - 0.9, 0.0);
    let yc = -0.07 + zz * 0.025;
    let taper = 1.0 - 0.5 * smoothstep(1.0, 4.3, L.z);
    let hw = 0.045 * taper;
    let blue = smoothstep(hw + aa, hw - aa, abs(L.y - yc));
    let red = smoothstep(0.010 + aa, 0.010 - aa, abs(L.y - (yc - hw - 0.028)));
    c = mix(c, line_c, blue);
    c = mix(c, pin_c, red);
  }
  if (fin) {
    let edge = 0.78 - (L.z - 3.6) * 0.25;
    c = mix(c, line_c, smoothstep(edge - aa, edge + aa, L.y));
    c = mix(c, pin_c, smoothstep(aa, -aa, abs(L.y - (edge - 0.05)) - 0.012));
  }
  if (ax > 4.12 && L.y > -0.2) { c = tip_c; }
  if (ax > 1.45 && L.z > 3.4 && abs(L.y - 0.06) < 0.15) { c = line_c; }
  if (L.z < -2.41) { c = tip_c; }
  return c;
}

// Grime, exhaust soot, oil and fuel stains, fading: everything a few hundred
// hours of flying leaves on the paint. Returns (colour, extra roughness).
fn weather(L: vec3f, N: vec3f, base: vec3f) -> vec4f {
  var c = base;
  let n = nz(L * 0.35);
  let belly = sat(-N.y * 1.5 + 0.2);
  c *= 1.0 - (0.03 + 0.07 * belly) * n.g;
  if (kind() == 1u) { return weather747(L, N, c, belly, n); }
  if (kind() == 8u) { return weather_dr400(L, N, c, belly); }
  if (kind() == 10u) { return weather_bonanza(L, N, c, belly); }
  // Soot from the exhaust stack streaking back along the belly.
  let d = L.z + 2.1;
  if (d > 0.0 && L.y < -0.2) {
    let cx = 0.335 - min(d, 1.8) * 0.12;
    let spread = 0.05 + d * 0.07;
    let streak = nz(vec3f(L.x * 7.0, L.y * 7.0, L.z * 0.35)).r;
    let soot = exp(-pow((L.x - cx) / spread, 2.0)) * exp(-d / 1.6) * sat(belly * 1.5) * (0.55 + 0.6 * streak);
    c = mix(c, vec3f(0.035, 0.03, 0.026), sat(soot * 1.2));
  }
  // Oil mist from the breather along the belly centreline.
  if (L.y < -0.45 && L.z > -2.2 && L.z < 1.5) {
    let oil = exp(-L.x * L.x / 0.03) * exp(-(L.z + 2.1) / 1.4) * (0.4 + 0.6 * nz(vec3f(L.x * 5.0, 0.0, L.z * 0.5)).b);
    c = mix(c, c * vec3f(0.62, 0.52, 0.36), sat(oil * 0.6));
  }
  // Blue streaks of spilt avgas behind the fuel caps.
  if (L.y > 0.6 && abs(abs(L.x) - 1.55) < 0.12 && L.z > -0.5 && L.z < 0.3) {
    let f = exp(-pow((abs(L.x) - 1.55) / 0.035, 2.0)) * smoothstep(-0.47, -0.42, L.z) * exp(-(L.z + 0.42) / 0.25) * nz(vec3f(L.x * 18.0, 0.0, L.z * 2.0)).r;
    c = mix(c, c * vec3f(0.72, 0.84, 1.0), sat(f * 1.6));
  }
  return vec4f(c, 0.08 * belly * n.b);
}


// The Robin's: exhaust soot from the two stubs under the cowl streaking
// back along the belly, an oil mist from the breather, grey grime on the
// walkways and round the step, avgas stains under the wing fillers.
fn weather_dr400(Lb: vec3f, N: vec3f, base: vec3f, belly: f32) -> vec4f {
  let L = Lb - vec3f(0.0, 0.0, 0.55);
  var c = base;
  let ax = abs(L.x);
  let d = L.z + 1.56;
  if (d > -0.05 && L.y < -0.30 && L.z < 3.0 && ax < 0.62) {
    let cx = 0.15 + min(max(d, 0.0), 1.2) * 0.07;
    let spread = 0.035 + max(d, 0.0) * 0.06;
    let streak = nz(vec3f(L.x * 9.0, L.y * 9.0, L.z * 0.4)).r;
    let soot = exp(-pow((ax - cx) / spread, 2.0)) * exp(-max(d, 0.0) / 1.3) * smoothstep(-0.05, 0.03, d) * sat(belly * 1.6) * (0.5 + 0.7 * streak);
    c = mix(c, vec3f(0.04, 0.035, 0.03), sat(soot * 1.3));
  }
  if (L.y < -0.5 && L.z > -1.6 && L.z < 2.5) {
    let oil = exp(-L.x * L.x / 0.012) * exp(-(L.z + 1.6) / 1.2) * (0.4 + 0.6 * nz(vec3f(L.x * 6.0, 0.0, L.z * 0.6)).b);
    c = mix(c, c * vec3f(0.6, 0.5, 0.34), sat(oil * 0.7));
  }
  // Scuffed paint where shoes go: either side of the walkway, the root fairing, the step.
  if (L.y > -0.75 && L.y < -0.35 && ax > 0.5 && ax < 1.05 && L.z > -0.9 && L.z < 0.95) {
    let scuff = smoothstep(0.55, 0.9, nz(L * vec3f(5.0, 5.0, 2.0)).g) * sat(N.y * 2.0);
    c = mix(c, c * vec3f(0.78, 0.76, 0.72), scuff * 0.6);
  }
  // Avgas round the wing tank fillers (a faint blue tide mark).
  let fl = length(vec2f(ax - 1.2, L.z + 0.49));
  if (fl < 0.35 && L.y > -0.6) {
    let f = exp(-fl / 0.08) * (0.4 + 0.6 * nz(vec3f(L.x * 14.0, 0.0, L.z * 14.0)).r) * smoothstep(0.04, 0.05, fl);
    c = mix(c, c * vec3f(0.74, 0.84, 1.0), sat(f * 1.5));
  }
  return vec4f(c, 0.08 * belly);
}

// ---------------------------------------------------------------- surface relief

fn chord_at(s: f32) -> f32 { return select(1.65 - 0.5 * (s - 2.55) / 2.95, 1.65, s <= 2.55); }


// ---------------------------------------------------------------- the Robin's skin
// Wood under doped fabric: plywood ahead of the spars, bare fabric shrunk
// over the ribs behind them, every rib under a pinked tape; a glass-fibre
// cowling in pieces held by fasteners. (dr400.rs's design frame is 0.55 m
// ahead of the body frame L is in; uv is in the design frame.)

// A pinked fabric tape `hw` wide centred on d (m), its edges zigzag cut.
fn tape(d: f32, along: f32, hw: f32, px: f32) -> f32 {
  let zig = abs(fract(along / 0.007) - 0.5) * 0.0024;
  let e = hw + zig;
  let a = max(px, 0.0006);
  return smoothstep(e + a, e - a, abs(d));
}
// Fabric over ribs `pitch` apart from s0 along s, spanning chord distance x
// (m) from x0 to x1 (sagging between them most halfway), rib tapes on top.
fn fabric(s: f32, x: f32, s0: f32, pitch: f32, x0: f32, x1: f32, px: f32) -> vec2f {
  if (x < x0 - 0.02 || x > x1 + 0.02) { return vec2f(0.0); }
  let d = (fract((s - s0) / pitch + 0.5) - 0.5) * pitch;
  let f = sat((x - x0) / max(x1 - x0, 1e-3));
  let env = sin(f * 3.14159);
  let fade = 1.0 - smoothstep(0.004, 0.02, px);
  var h = -0.0028 * env * (0.5 - 0.5 * cos(6.2831853 * (d / pitch))) * fade;
  // (The tape: raised, with the rib stitching under it in a row of small bumps.)
  let t = tape(d, x, 0.016, px) * step(x0, x);
  let stitch = line(fract(x / 0.065) - 0.5, 0.06) * line(d, 0.004) * (1.0 - smoothstep(0.0005, 0.002, px));
  h += 0.0005 * t + 0.0003 * stitch;
  let dirt = 0.45 * line(abs(d) - 0.017, max(px, 0.0012)) * step(x0, x) + 0.25 * stitch;
  return vec2f(h, dirt * fade);
}
fn relief_dr400(pt: u32, uv: vec2f, L: vec3f, px: f32) -> vec3f {
  var h = 0.0;
  var dirt = 0.0;
  let gw = max(0.0007, px * 0.9);
  let ga = 0.00045 * min(1.0, 0.0012 / gw);
  // Dope and paint over fabric: a faint weave, and the slight waviness of
  // a hand-finished wooden airframe.
  let weave = (1.0 - smoothstep(0.0003, 0.0012, px)) * 0.00004 * (sin(uv.x * 9000.0) + sin(uv.y * 9000.0));
  h += 0.00035 * (nz(L * 1.7).r - 0.5) + weave;
  switch pt {
    case 2u: { // wing
      let s = abs(uv.x);
      let x = abs(uv.y);
      let c = select(1.71 - 0.56 * (s - 1.95) / 2.49, 1.71, s < 1.95);
      let xc = x / c;
      if (s > 0.56 && s < 4.3) {
        let f = fabric(s, x, 0.45, 0.29, 0.30 * c, 0.985 * c, px);
        h += f.x;
        dirt += f.y;
      }
      // The spar cap tape along the edge of the plywood, the trailing edge tape.
      h += 0.0003 * tape(xc - 0.30, s, 0.022 / c, px / c) + 0.0003 * tape(xc - 0.985, s, 0.025 / c, px / c);
      // The joint between the centre section and the outer panel (under its strip).
      let dk = s - 1.95;
      h -= ga * line(abs(dk) - 0.006, gw);
      dirt += line(abs(dk) - 0.006, gw * 2.0);
      // Black anti-skid walkway on the root of the centre section.
      if (uv.y > 0.0 && s > 0.58 && s < 0.98 && xc > 0.08 && xc < 0.72) {
        h += 0.0005 * nz(L * 180.0).r * (1.0 - smoothstep(0.0003, 0.0012, px));
      }
      // Access panel round the wing tank's gauge and the aileron cable pulleys.
      if (uv.y < 0.0) {
        for (var i = 0; i < 2; i++) {
          let q = vec2f(s - select(1.35, 2.85, i == 1), x - 0.62);
          let b = abs(q) - vec2f(0.09, 0.07);
          let od = length(max(b, vec2f(0.0))) + min(max(b.x, b.y), 0.0) - 0.015;
          h -= ga * 1.2 * line(od, gw);
          dirt += line(od, gw * 2.0);
        }
      }
    }
    case 3u: { // control surfaces: fabric over ribs
      let s = abs(uv.x);
      let x = abs(uv.y);
      let f = fabric(s, x, 0.0, 0.16, -10.0, 12.0, px);
      h += f.x * 0.6;
      dirt += f.y;
    }
    case 5u: { // fin: plywood ahead of its spar, fabric behind
      let s = uv.x;
      let x = abs(uv.y);
      let f = fabric(s, x, 0.05, 0.2, 0.3, 1.2, px);
      h += f.x * 0.8;
      dirt += f.y;
    }
    case 1u: { // fuselage: plywood under fabric
      let z = uv.x;
      // Inspection hatches on the tail cone, the access panel under the belly.
      let ax = abs(L.x);
      if (z > 2.2 && z < 3.2 && ax > 0.1) {
        let d = length(vec2f(z - 2.65, L.y + 0.12)) - 0.055;
        h -= ga * 1.5 * line(d, gw);
        dirt += line(d, gw * 2.5);
      }
      if (L.y < -0.6 && z > -0.3 && z < 0.4) {
        let q = vec2f(z - 0.05, L.x);
        let b = abs(q) - vec2f(0.20, 0.14);
        let od = length(max(b, vec2f(0.0))) + min(max(b.x, b.y), 0.0) - 0.02;
        h -= ga * 1.4 * line(od, gw);
        dirt += line(od, gw * 2.5);
      }
      // The static port, both sides of the tail cone.
      if (z > 1.5 && z < 1.7) {
        let d = length(vec2f(z - 1.6, L.y + 0.05)) ;
        h -= 0.0008 * smoothstep(0.004 + px, 0.004 - px, d);
        dirt += smoothstep(0.012, 0.004, d) * 0.6;
      }
    }
    case 4u: { // cowling (and the gear fairings, glass fibre too)
      let z = L.z - 0.55;
      if (L.y > -0.72 && z < -1.45) {
        // The split between the upper and lower halves, the nose bowl, the firewall edge.
        var lines = array<f32, 3>(select(1.0, L.y + 0.12, abs(L.x) > 0.15), z + 2.22, z + 1.52);
        for (var i = 0; i < 3; i++) {
          h -= ga * 1.5 * line(lines[i], gw);
          dirt += line(lines[i], gw * 2.5);
        }
        // The oil door on top.
        if (L.y > 0.05) {
          let q = vec2f(z + 1.86, L.x);
          let b = abs(q) - vec2f(0.10, 0.075);
          let od = length(max(b, vec2f(0.0))) + min(max(b.x, b.y), 0.0) - 0.02;
          h -= ga * 1.5 * line(od, gw);
          dirt += line(od, gw * 2.0);
        }
      } else if (L.y < -0.72) {
        // A wheel fairing's halves join down its middle, a mud scraper cut-out underneath.
        let cx = select(1.30, 0.0, abs(L.x) < 0.6);
        let d = abs(L.x) - cx;
        h -= ga * 1.3 * line(d, gw);
        dirt += line(d, gw * 2.5);
      }
    }
    case 6u: { // tyre tread: circumferential grooves
      var c = vec3f(sign(L.x) * 1.30, -1.01, 0.15);
      var r = 0.19;
      var g = vec2f(0.020, 0.044);
      if (abs(L.x) < 0.6) { c = vec3f(0.0, -1.035, -1.43); r = 0.165; g = vec2f(0.016, 0.036); }
      let ax = abs(L.x - c.x);
      let rad = length(L.yz - c.yz);
      if (rad > r * 0.9) {
        let gd = min(abs(ax - g.x), abs(ax - g.y));
        h -= 0.002 * line(gd, max(0.003, px));
        dirt += line(gd, max(0.004, px));
      }
    }
    default: {}
  }
  return vec3f(h, sat(dirt) * min(1.0, 0.0022 / gw), 0.0);
}

// Height (m), seam dirt, rivet coverage, for the surface pattern.
// constant tables live in constant memory, not copied per pixel.
const FUSE_STATIONS = array<f32, 8>(-1.98, 0.92, 1.52, 2.12, 2.72, 3.32, 3.92, 4.40);
const WIN747_BAYS = array<vec4f, 2>(vec4f(5.30, 5.86, 0.30, 0.55), vec4f(5.98, 6.40, 0.36, 0.55));
const LAPS747 = array<f32, 4>(1.7, 3.6, 5.6, 7.7);
const CHORDS747 = array<f32, 5>(0.44, 1.36, 2.48, 4.94, 6.06);
fn relief(pt: u32, uv: vec2f, L: vec3f, Nl: vec3f, px: f32) -> vec3f {
  if (kind() == 1u) { return relief747(pt, uv, L, Nl, px); }
  // The Robin is wood and fabric: no rivets, no laps (its cabin keeps its
  // upholstery, carpet, crinkle paint and knurling).
  if (kind() == 8u && pt <= 6u) { return relief_dr400(pt, uv, L, px); }
  // The Bonanza: flush-riveted aluminium (its cabin keeps the shared patterns below).
  if (kind() == 10u && pt <= 6u) { return relief_bonanza(pt, uv, L, Nl, px); }
  var h = 0.0;
  var dirt = 0.0;
  var rv = 0.0;
  let gw = max(0.0007, px * 0.9);
  let ga = 0.00045 * min(1.0, 0.0012 / gw);
  let rr = 0.0024;
  let rf = 1.0 - smoothstep(rr * 0.5, rr * 1.6, px);
  switch pt {
    case 1u: { // fuselage
      let z = uv.x;
      let s = abs(uv.y);
      for (var i = 0; i < 8; i++) {
        let dz = z - FUSE_STATIONS[i];
        if (abs(dz) < 0.04) {
          h -= ga * line(dz, gw);
          dirt += line(dz, gw * 2.5);
          rv = max(rv, rivets(s, abs(abs(dz) - 0.012), 0.027, rr) * rf);
        }
      }
      if (L.y < -0.4 && z > -1.9 && z < 0.9) {
        for (var i = 0; i < 3; i++) {
          let dz = z - (-1.25 + f32(i) * 0.75);
          h -= ga * line(dz, gw);
          dirt += line(dz, gw * 2.5);
          rv = max(rv, rivets(s, abs(abs(dz) - 0.012), 0.027, rr) * rf);
        }
      }
      if (z > 0.92) {
        for (var i = 0; i < 2; i++) {
          let ds = s - select(1.25, 0.6, i == 0);
          h -= ga * line(ds, gw);
          dirt += line(ds, gw * 2.0);
          rv = max(rv, rivets(z, abs(ds - 0.011), 0.027, rr) * rf);
        }
        // Single rows of rivets on the stringers between the laps.
        let k = (fract(s / 0.21) - 0.5) * 0.21;
        rv = max(rv, rivets(z, k, 0.04, rr * 0.9) * rf * 0.8);
      }
      if (abs(L.x) > 0.35) {
        let dd = door(L);
        h -= ga * 1.8 * line(dd, gw * 1.3);
        dirt += line(dd, gw * 3.0);
        if (L.x < 0.0) {
          let bd = baggage_door(L);
          h -= ga * 1.6 * line(bd, gw * 1.2);
          dirt += line(bd, gw * 2.5);
        }
      }
      h += rv * 0.0006;
    }
    case 2u, 5u: { // wing, tail surfaces
      let s = abs(uv.x);
      let av = abs(uv.y);
      let tail = pt == 5u;
      let c = select(chord_at(s), 1.0, tail);
      let xc = av / c;
      let rib_p = select(0.305, 0.21, tail);
      let rib0 = select(0.62, 0.15, tail);
      if (s > rib0 && s < select(5.24, 1.6, tail)) {
        let d = (fract((s - rib0) / rib_p + 0.5) - 0.5) * rib_p;
        rv = max(rv, rivets(av, d, 0.03, rr) * rf);
      }
      let spar = select(0.25 * c, 0.20, tail);
      rv = max(rv, rivets(s, av - spar, 0.03, rr) * rf);
      if (!tail) {
        rv = max(rv, rivets(s, av - 0.66 * c, 0.03, rr) * rf);
        let le = av - 0.13 * c;
        h -= ga * line(le, gw);
        dirt += line(le, gw * 2.0);
        rv = max(rv, rivets(s, abs(le + 0.012), 0.025, rr) * rf);
        for (var i = 0; i < 3; i++) {
          let ds = s - select(select(5.28, 3.9, i == 1), 2.55, i == 0);
          h -= ga * line(ds, gw);
          dirt += line(ds, gw * 2.0);
          if (i < 2) { rv = max(rv, rivets(av, abs(ds) - 0.012, 0.027, rr) * rf); }
        }
      }
      h += rv * 0.0006;
    }
    case 3u: { // control surfaces: stiffening beads across the span
      let d = (fract(abs(uv.x) / 0.075) - 0.5) * 0.075;
      h += 0.0009 * line(d, max(0.0045, px));
      dirt += 0.3 * line(abs(d) - 0.007, gw * 2.0);
    }
    case 4u: { // cowling
      let lines = array<f32, 2>(L.z + 2.975, L.y + 0.085);
      for (var i = 0; i < 2; i++) {
        let d = lines[i];
        h -= ga * 1.4 * line(d, gw);
        dirt += line(d, gw * 2.5);
      }
      if (L.y > 0.15) {
        let q = vec2f(L.z + 2.36, L.x);
        let b = abs(q) - vec2f(0.12, 0.10);
        let od = length(max(b, vec2f(0.0))) + min(max(b.x, b.y), 0.0);
        h -= ga * 1.5 * line(od, gw);
        dirt += line(od, gw * 2.0);
      }
    }
    case 7u: { // cabin lining: door gaps, headliner seams, grain
      if (kind() == 8u) {
        // The Robin's vinyl side panels: a stitched seam under the sill and a coarser grain.
        let d = L.y + 0.035;
        h -= 0.0012 * line(d, max(0.0018, px));
        let st = step(0.5, fract(L.z / 0.006)) * line(abs(d) - 0.006, max(0.0006, px));
        h -= 0.0002 * st;
        dirt += 0.6 * line(d, max(0.004, px * 2.0)) + 0.3 * st;
        h += 0.00025 * nz(L * 70.0).r * (1.0 - smoothstep(0.0004, 0.0016, px));
      }
      if (kind() != 8u && abs(L.x) > 0.35) {
        let dd = door(L);
        h -= 0.0015 * line(dd, max(0.0015, px));
        dirt += line(dd, max(0.003, px * 2.0));
      }
      if (kind() != 8u && L.y > 0.46) {
        let d = (fract((L.z + 0.6) / 0.42) - 0.5) * 0.42;
        h -= 0.001 * line(d, max(0.004, px));
        dirt += 0.5 * line(d, 0.006);
      }
      h += 0.00015 * nz(L * 40.0).r * (1.0 - smoothstep(0.0005, 0.002, px));
    }
    case 8u: { // pleated upholstery
      if (abs(Nl.x) < 0.7) {
        let t = fract(L.x / 0.065 + 0.5) * 2.0 - 1.0;
        h += 0.004 * sqrt(max(1.0 - t * t, 0.0));
        dirt += smoothstep(0.85, 1.0, abs(t));
      }
      h += 0.0004 * nz(L * 60.0).g * (1.0 - smoothstep(0.0005, 0.002, px));
    }
    case 9u: { h += 0.0015 * nz(L * 45.0).r * (1.0 - smoothstep(0.001, 0.004, px)); }
    case 18u: { // cockpit paint: orange peel
      h += 0.00010 * nz(L * 110.0).r * (1.0 - smoothstep(0.0002, 0.0009, px));
    }
    case 19u: { // open-weave mesh fabric: raised threads round small holes
      let q = uv / 0.0026;
      let f = abs(fract(q) - 0.5) * 2.0;
      let thread = max(smoothstep(0.55, 0.9, f.x), smoothstep(0.55, 0.9, f.y));
      let fade = 1.0 - smoothstep(0.0003, 0.0012, px);
      h += 0.0004 * thread * fade + 0.0012 * nz(L * 18.0).g;
      dirt += (1.0 - thread) * 0.7 * fade;
    }
    case 20u: { // webbing: a twill weave, stitching along both edges
      let tw = sin((uv.y + uv.x * 0.6) * 2400.0);
      let fade = 1.0 - smoothstep(0.0002, 0.0008, px);
      h += 0.00012 * tw * fade;
      let st = step(0.5, fract(uv.y / 0.004)) * line(abs(abs(uv.x) - 0.017), 0.0006);
      h -= 0.0002 * st * fade;
      dirt += st * 0.5 * fade;
    }
    case 21u: { // anti-skid grit
      h += 0.0005 * nz(L * 190.0).r * (1.0 - smoothstep(0.0003, 0.0012, px));
    }
    case 23u: { // corrugated hose
      let r = sin(uv.y * 6.2831853 / 0.0065);
      h += 0.0011 * r * (1.0 - smoothstep(0.0006, 0.0025, px));
      dirt += smoothstep(0.6, 1.0, -r) * 0.4;
    }
    case 10u: { h += select(0.0002, 0.00006, fighter()) * nz(L * 160.0).r * (1.0 - smoothstep(0.0003, 0.0012, px)); }
    case 12u: { h += 0.00025 * sin((L.x + L.y * 1.3 + L.z * 0.7) * 2600.0) * (1.0 - smoothstep(0.0002, 0.0008, px)); }
    case 6u: { // tyre tread: circumferential grooves
      var c = vec3f(sign(L.x) * 1.25, -1.31, 0.30);
      var r = 0.19;
      var g = vec2f(0.022, 0.048);
      if (L.z < -1.0) { c = vec3f(0.0, -1.335, -2.87); r = 0.165; g = vec2f(0.018, 0.038); }
      let ax = abs(L.x - c.x);
      let rad = length(L.yz - c.yz);
      if (rad > r * 0.9) {
        let gd = min(abs(ax - g.x), abs(ax - g.y));
        h -= 0.002 * line(gd, max(0.003, px));
        dirt += line(gd, max(0.004, px));
      }
    }
    default: {}
  }
  // Fine seams fade with distance rather than widen into dark bands.
  return vec3f(h, sat(dirt) * min(1.0, 0.0022 / gw), rv);
}

// ---------------------------------------------------------------- instruments
// Each instrument only sets up what is on its face below; the graduations,
// arcs, needles and lettering are then drawn once, so the shader stays small.

fn ang_of(uv: vec2f) -> f32 { return atan2(uv.x, uv.y); }
fn unwrap(th: f32, a0: f32) -> f32 { return a0 + fract((th - a0) / 6.2831853) * 6.2831853; }
fn wrapa(a: f32) -> f32 { return a - round(a / 6.2831853) * 6.2831853; }
// Graduations: n intervals between angles a0 and a1, every `per`th long.
fn ticks(uv: vec2f, a0: f32, a1: f32, n: f32, per: f32, px: f32) -> f32 {
  let r = length(uv);
  let th = unwrap(ang_of(uv), min(a0, a1) - 0.05);
  let k = clamp(round((th - a0) / (a1 - a0) * n), 0.0, n);
  let ak = a0 + (a1 - a0) * k / n;
  let major = abs(k - round(k / per) * per) < 0.5;
  let len = select(0.085, 0.17, major);
  let w = select(0.011, 0.022, major);
  let d = abs(wrapa(th - ak)) * r;
  let inr = smoothstep(0.95 - len - px, 0.95 - len + px, r) * smoothstep(0.95 + px, 0.95 - px, r);
  return inr * (1.0 - smoothstep(w - px, w + px, d));
}
fn arc(uv: vec2f, a0: f32, a1: f32, r0: f32, r1: f32, px: f32) -> f32 {
  let r = length(uv);
  let th = unwrap(ang_of(uv), a0 - 0.3);
  let inside = step(a0, th) * step(th, a1);
  return inside * smoothstep(r0 - px, r0 + px, r) * smoothstep(r1 + px, r1 - px, r);
}
fn needle(uv: vec2f, a: f32, len: f32, w: f32, tail: f32, px: f32) -> f32 {
  let d = vec2f(sin(a), cos(a));
  let t = clamp(dot(uv, d), -tail, len);
  let r = w * mix(1.0, 0.3, sat(t / len));
  return 1.0 - smoothstep(-px, px, length(uv - d * t) - r);
}
fn dial(v: f32, vmin: f32, vmax: f32, a0: f32, a1: f32) -> f32 { return a0 + sat((v - vmin) / (vmax - vmin)) * (a1 - a0); }
fn rad(d: f32) -> f32 { return d * 0.017453293; }
// Compass card lettering: N, 3, 6, E, 12 ... for card index 0..11.
fn card_code(k: i32) -> vec2u {
  let ki = u32((k % 12 + 12) % 12);
  if (ki == 0u) { return vec2u(10u, 1u); }
  if (ki == 3u) { return vec2u(11u, 1u); }
  if (ki == 6u) { return vec2u(12u, 1u); }
  if (ki == 9u) { return vec2u(13u, 1u); }
  return num_code(ki * 3u);
}

// The face of an instrument: rgb albedo, a = how much of it is luminous paint.
fn gauge(id: u32, uv: vec2f, px: f32) -> vec4f {
  if (id == 31u || id == 32u) {
    let value = select(G.cockpit[2].y, O.params[4].w / 30.0, id == 32u);
    let notch = clamp(round((0.85-uv.y)/1.7*3.0),0.0,3.0);
    let mark = 0.85-1.7*notch/3.0;
    let ticks = (1.0 - smoothstep(0.02, 0.04, abs(uv.y-mark))) * step(0.2, uv.x);
    var ink = ticks;
    if (id == 32u) { let code=num_code(u32(notch)*10u); ink=max(ink,label_code(uv,vec2f(0.0,mark+0.08),code.x,code.y,0.18,px)); }
    let pointer = (1.0 - smoothstep(0.025, 0.055, abs(uv.y - (0.85 - 1.7 * value)))) * step(uv.x, -0.2);
    let col = mix(vec3f(0.025), vec3f(0.8), ink);
    return vec4f(mix(col, vec3f(0.9,0.55,0.08), pointer), pointer);
  }
  let P0 = O.params[0];
  let P1 = O.params[1];
  let P2 = O.params[2];
  let P3 = O.params[3];
  let P4 = O.params[4];
  let r = length(uv);
  var bg = vec3f(0.012, 0.012, 0.013);
  var ink = 0.0;
  var col = vec3f(0.0);
  var cov = 0.0;
  var ndl = 0.0;
  // Dial: angles, value range, graduations.
  var a0 = 0.0; var a1 = 0.0; var vmin = 0.0; var vmax = 1.0; var tn = 0.0; var tper = 1.0;
  var q = uv;                       // where the dial is drawn (small gauges sit low)
  // Numbers round the dial: count, first value, step, divisor, radius, height.
  var ln = 0.0; var lfirst = 0.0; var lstep = 1.0; var ldiv = 1.0; var lr = 0.6; var lh = 0.15;
  // Coloured arcs (value from, to; colour) and a red line.
  var arcs = array<vec4f, 3>(vec4f(0.0), vec4f(0.0), vec4f(0.0));
  var arc_col = array<vec3f, 3>(vec3f(0.0), vec3f(0.0), vec3f(0.0));
  var redline = -1e9;
  // Needles: angle, length, width, tail.
  var nd = array<vec4f, 3>(vec4f(0.0), vec4f(0.0), vec4f(0.0));
  var nn = 0;
  // One more piece of lettering: point in its frame, centre, code, glyphs, height.
  var tp = uv; var tc = vec2f(9.0); var tcode = 0u; var tlen = 0u; var th = 0.1;
  var hub = true;
  let green = vec3f(0.05, 0.4, 0.08);
  switch id {
    case 0u: { // airspeed
      a0 = 0.0; a1 = rad(330.0); vmax = 200.0; tn = 40.0; tper = 4.0;
      ln = 7.0; lfirst = 40.0; lstep = 20.0; lr = 0.62;
      // A 172's markings (indicated): white Vso..Vfe, green Vs1..Vno, yellow to Vne.
      arcs[0] = vec4f(50.0, 129.0, 0.86, 0.95); arc_col[0] = green;
      arcs[1] = vec4f(129.0, 163.0, 0.86, 0.95); arc_col[1] = vec3f(0.6, 0.45, 0.02);
      arcs[2] = vec4f(45.0, 85.0, 0.79, 0.85); arc_col[2] = vec3f(0.6);
      redline = 163.0;
      nd[0] = vec4f(dial(P0.x, 0.0, 200.0, a0, a1), 0.86, 0.035, 0.18); nn = 1;
      if (kind() == 1u) {
        // The 747's standby airspeed: 60 to 450 kt, no arcs, the barber pole at Vmo.
        a0 = rad(10.0); vmin = 60.0; vmax = 450.0; tn = 39.0; tper = 3.0;
        ln = 8.0; lfirst = 100.0; lstep = 50.0;
        arcs[0] = vec4f(0.0); arcs[1] = vec4f(0.0); arcs[2] = vec4f(0.0);
        redline = 365.0;
        nd[0] = vec4f(dial(P0.x, 60.0, 450.0, a0, a1), 0.86, 0.035, 0.18);
      }
      if (kind() == 10u) {
        // The A36's markings, KIAS (G36/A36 limits, aircraft/bonanza): white
        // 51-122, green 61-165, yellow 165-203, the red line at 203.
        vmax = 220.0; tn = 44.0; ln = 10.0;
        arcs[0] = vec4f(61.0, 165.0, 0.86, 0.95); arcs[1] = vec4f(165.0, 203.0, 0.86, 0.95); arcs[2] = vec4f(51.0, 122.0, 0.79, 0.85);
        redline = 203.0;
        nd[0] = vec4f(dial(P0.x, 0.0, 220.0, a0, a1), 0.86, 0.035, 0.18);
      }
      if (fighter() || kind() == 6u) {
        // A fighter's airspeed (and Concorde's): 50 to 850 kt.
        a0 = rad(10.0); vmin = 50.0; vmax = 850.0; tn = 32.0; tper = 4.0;
        ln = 8.0; lfirst = 100.0; lstep = 100.0; ldiv = 10.0;
        arcs[0] = vec4f(0.0); arcs[1] = vec4f(0.0); arcs[2] = vec4f(0.0);
        redline = -1e9;
        nd[0] = vec4f(dial(P0.x, vmin, vmax, a0, a1), 0.86, 0.035, 0.18);
      }
    }
    case 2u: { // altimeter: hundreds, thousands, ten thousands, pressure window
      a0 = 0.0; a1 = rad(360.0); vmax = 10.0; tn = 50.0; tper = 5.0;
      ln = 10.0; lfirst = 0.0; lstep = 1.0; lr = 0.64; lh = 0.17;
      let alt = P0.y;
      nd[0] = vec4f(fract(alt / 1000.0) * 6.2831853, 0.8, 0.03, 0.15);
      nd[1] = vec4f(fract(alt / 10000.0) * 6.2831853, 0.5, 0.06, 0.1);
      nd[2] = vec4f(fract(alt / 100000.0) * 6.2831853, 0.93, 0.012, 0.0);
      nn = 3;
      let kb = abs(uv - vec2f(0.52, 0.0)) - vec2f(0.2, 0.07);
      if (max(kb.x, kb.y) < 0.0) { bg = vec3f(0.0); ln = 0.0; tn = 0.0; }
      tc = vec2f(0.52, 0.0);
      let pressure = u32(round(G.cockpit[0].x));
      tcode = pack(pressure / 1000u, pressure / 100u % 10u, 14u, pressure / 10u % 10u, pressure % 10u, 0u); tlen = 5u; th = 0.08;
    }
    case 5u: { // vertical speed
      a0 = rad(-260.0); a1 = rad(80.0); vmin = -2000.0; vmax = 2000.0; tn = 40.0; tper = 5.0;
      ln = 9.0; lfirst = -2000.0; lstep = 500.0; ldiv = 100.0;
      nd[0] = vec4f(dial(P0.z, vmin, vmax, a0, a1), 0.86, 0.035, 0.18); nn = 1;
      if (fighter() || kind() == 6u) {
        vmin = -6000.0; vmax = 6000.0; lfirst = -6000.0; lstep = 2000.0; ln = 7.0; ldiv = 1000.0; tn = 24.0; tper = 4.0;
        nd[0] = vec4f(dial(P0.z, vmin, vmax, a0, a1), 0.86, 0.035, 0.18);
      }
    }
    case 21u: { // the Hornet's brake accumulator pressure, psi x 1000
      a0 = rad(-120.0); a1 = rad(120.0); vmin = 0.0; vmax = 4.0; tn = 8.0; tper = 2.0;
      ln = 5.0; lfirst = 0.0; lstep = 1.0; lr = 0.58; lh = 0.2;
      arcs[0] = vec4f(2.0, 3.2, 0.88, 0.95); arc_col[0] = vec3f(0.05, 0.4, 0.08);
      nd[0] = vec4f(dial(3.0, vmin, vmax, a0, a1), 0.84, 0.045, 0.15); nn = 1;
    }
    // The Phantom's engine instruments (aircraft/f4): a jet's readings, see main.rs `instruments`.
    case 22u: { // engine speed, percent
      a0 = rad(-135.0); a1 = rad(135.0); vmax = 110.0; tn = 22.0; tper = 2.0;
      ln = 6.0; lfirst = 0.0; lstep = 20.0; ldiv = 10.0; lr = 0.60; lh = 0.17;
      arcs[0] = vec4f(60.0, 100.0, 0.88, 0.95); arc_col[0] = green;
      redline = 103.0;
      nd[0] = vec4f(dial(P2.x * 100.0, vmin, vmax, a0, a1), 0.86, 0.035, 0.18); nn = 1;
    }
    case 23u: { // exhaust gas temperature, degrees C x 100
      a0 = rad(-135.0); a1 = rad(135.0); vmax = 1000.0; tn = 20.0; tper = 2.0;
      ln = 6.0; lfirst = 0.0; lstep = 200.0; ldiv = 100.0; lr = 0.60; lh = 0.17;
      arcs[0] = vec4f(200.0, 650.0, 0.88, 0.95); arc_col[0] = green;
      redline = 700.0;
      nd[0] = vec4f(dial(P2.y, vmin, vmax, a0, a1), 0.86, 0.035, 0.18); nn = 1;
    }
    case 24u: { // fuel flow of one engine, lb/h x 1000, the afterburner's included (a J79: 10 at military power, 35 lit)
      a0 = rad(-135.0); a1 = rad(135.0); vmax = 40.0; tn = 40.0; tper = 5.0;
      ln = 5.0; lfirst = 0.0; lstep = 10.0; lr = 0.60; lh = 0.17;
      nd[0] = vec4f(dial(P2.z * 2.20462 / 1000.0, vmin, vmax, a0, a1), 0.86, 0.035, 0.18); nn = 1;
      // (Concorde's: kg/h x 1000; an Olympus burns 16 flat out, 23 with reheat.)
      if (kind() == 6u) { vmax = 30.0; tn = 30.0; nd[0] = vec4f(dial(P2.z / 1000.0, vmin, vmax, a0, a1), 0.86, 0.035, 0.18); }
    }
    case 25u: { // nozzle position: wide open idling, closing to military power, open again as the burners light
      a0 = rad(-120.0); a1 = rad(120.0); vmax = 4.0; tn = 8.0; tper = 2.0;
      ln = 5.0; lfirst = 0.0; lstep = 1.0; lr = 0.58; lh = 0.2;
      let open = max(1.0 - smoothstep(0.05, 0.9, dry_power()), burner());
      nd[0] = vec4f(dial(0.6 + 3.2 * open, vmin, vmax, a0, a1), 0.84, 0.045, 0.15); nn = 1;
    }
    case 26u: { // fuel on board, lb x 1000
      a0 = rad(-135.0); a1 = rad(135.0); vmax = 14.0; tn = 28.0; tper = 2.0;
      ln = 8.0; lfirst = 0.0; lstep = 2.0; lr = 0.60; lh = 0.17;
      arcs[0] = vec4f(0.0, 2.0, 0.88, 0.95); arc_col[0] = vec3f(0.6, 0.02, 0.01);
      // (The Blackbird carries 80,000 lb: its scale runs to 90.)
      if (kind() == 7u) { vmax = 90.0; tn = 18.0; ln = 10.0; lstep = 10.0; arcs[0] = vec4f(0.0, 10.0, 0.88, 0.95); }
      nd[0] = vec4f(dial(P2.w * 2.20462 / 1000.0, vmin, vmax, a0, a1), 0.86, 0.035, 0.18); nn = 1;
      // (Concorde's: tonnes, 96 of them with the tanks full.)
      if (kind() == 6u) {
        vmax = 100.0; tn = 20.0; ln = 6.0; lstep = 20.0;
        arcs[0] = vec4f(0.0, 10.0, 0.88, 0.95);
        nd[0] = vec4f(dial(P2.w / 1000.0, vmin, vmax, a0, a1), 0.86, 0.035, 0.18);
      }
    }
    case 27u: { // Mach number, tenths
      a0 = rad(20.0); a1 = rad(340.0); vmin = 0.4; vmax = 2.4; tn = 20.0; tper = 2.0;
      ln = 6.0; lfirst = 0.4; lstep = 0.4; ldiv = 0.1; lr = 0.62; lh = 0.15;
      // (The Blackbird's runs to Mach 3.6.)
      if (kind() == 7u) { vmax = 3.6; tn = 32.0; tper = 4.0; ln = 9.0; lh = 0.13; }
      nd[0] = vec4f(dial(P3.x, vmin, vmax, a0, a1), 0.86, 0.035, 0.18); nn = 1;
    }
    // The Blackbird's own (aircraft/sr71).
    case 28u: { // an inlet spike's position, inches aft: forward below Mach 1.6, 26 in back by Mach 3.2 (as sr71 `anim` moves it)
      a0 = rad(-120.0); a1 = rad(120.0); vmax = 26.0; tn = 13.0; tper = 5.0;
      ln = 3.0; lfirst = 0.0; lstep = 10.0; lr = 0.56; lh = 0.22;
      arcs[0] = vec4f(0.0, 1.0, 0.88, 0.95); arc_col[0] = green;
      nd[0] = vec4f(dial(26.0 * smoothstep(1.6, 3.2, P3.x), vmin, vmax, a0, a1), 0.84, 0.05, 0.15); nn = 1;
    }
    case 29u: { // compressor inlet temperature (the air's total temperature), degrees C x 100: what limits its speed
      a0 = rad(-135.0); a1 = rad(135.0); vmax = 450.0; tn = 18.0; tper = 2.0;
      ln = 5.0; lfirst = 0.0; lstep = 100.0; ldiv = 100.0; lr = 0.58; lh = 0.2;
      arcs[0] = vec4f(0.0, 400.0, 0.88, 0.95); arc_col[0] = green;
      redline = 427.0;
      nd[0] = vec4f(dial(P3.w, vmin, vmax, a0, a1), 0.86, 0.04, 0.18); nn = 1;
    }
    case 6u: { // tachometer with its hour meter
      a0 = rad(-135.0); a1 = rad(135.0); vmax = 3500.0; tn = 35.0; tper = 5.0;
      ln = 8.0; lfirst = 0.0; lstep = 500.0; ldiv = 100.0;
      arcs[0] = vec4f(2100.0, 2700.0, 0.86, 0.95); arc_col[0] = green;
      redline = 2700.0;
      nd[0] = vec4f(dial(P2.x, 0.0, vmax, a0, a1), 0.86, 0.035, 0.18); nn = 1;
      let hb = abs(uv - vec2f(0.0, -0.45)) - vec2f(0.2, 0.065);
      if (max(hb.x, hb.y) < 0.0) { bg = vec3f(0.0); tn = 0.0; ln = 0.0; }
      tc = vec2f(0.0, -0.45); tcode = num_code(u32(G.cockpit[2].w)).x; tlen = 4u; th = 0.085;
    }
    case 7u: { // manifold pressure
      a0 = rad(-135.0); a1 = rad(135.0); vmin = 10.0; vmax = 35.0; tn = 25.0; tper = 5.0;
      ln = 6.0; lfirst = 10.0; lstep = 5.0; lh = 0.16;
      arcs[0] = vec4f(15.0, 25.0, 0.86, 0.95); arc_col[0] = green;
      // (The Bonanza's IO-550-B: green 15 to 29.6 in Hg.)
      if (kind() == 10u) { arcs[0] = vec4f(15.0, 29.6, 0.86, 0.95); }
      nd[0] = vec4f(dial(P2.y, vmin, vmax, a0, a1), 0.86, 0.04, 0.18); nn = 1;
    }
    case 8u, 9u, 10u, 11u, 12u, 13u, 16u, 18u, 34u: { // small engine gauges, dial low on the face
      a0 = rad(-60.0); a1 = rad(60.0); tn = 10.0; tper = 5.0;
      q = (uv + vec2f(0.0, 0.35)) * 0.78;
      var v = 0.0; var g0 = 0.0; var g1 = 0.0;
      if (id == 8u) { v = P2.z; vmax = select(26.0, max(G.cockpit[0].w, 1.0), kind() == 0u || kind() == 10u); g0 = 2.0; g1 = vmax; }
      if (id == 9u) { v = P2.w; vmax = select(26.0, max(G.cockpit[0].w, 1.0), kind() == 0u || kind() == 10u); g0 = 2.0; g1 = vmax; }
      if (id == 10u) { v = P3.x; vmin = 100.0; vmax = 250.0; g0 = 100.0; g1 = 240.0; }
      if (id == 11u) { v = P3.y; vmax = 115.0; g0 = 60.0; g1 = 90.0; redline = 114.0; }
      if (id == 12u) { v = P3.w; vmin = -60.0; vmax = 60.0; g0 = -5.0; g1 = 60.0; }
      if (id == 13u) { v = P4.x; vmin = 3.0; vmax = 7.0; g0 = 4.5; g1 = 5.5; }
      if (id == 16u) { v = P3.z; g0 = 0.5; g1 = 0.8; }
      // (fuel flow, US gal/h: the Bonanza's, main.rs `cockpit_globals` row 3 w)
      if (id == 34u) { v = G.cockpit[3].w; vmax = 30.0; g0 = 6.0; g1 = 27.0; }
      if (id == 18u) { v = select(P3.x * 1.6 + 30.0, G.cockpit[2].x, kind() == 0u || kind() == 10u); vmin = 200.0; vmax = 500.0; g0 = 200.0; g1 = 460.0; }
      arcs[0] = vec4f(g0, g1, 0.86, 0.95); arc_col[0] = green;
      if (id == 8u || id == 9u) {
        arcs[1] = vec4f(vmin, 2.0, 0.86, 0.95); arc_col[1] = vec3f(0.6, 0.02, 0.01);
        tc = vec2f(-0.55, 0.35); tcode = 0u; tlen = 1u; th = 0.16;
      }
      nd[0] = vec4f(dial(v, vmin, vmax, a0, a1), 0.9, 0.05, 0.0); nn = 1;
      ndl = 1.0 - smoothstep(0.06 - px, 0.06 + px, length(q));
      hub = false;
    }
    case 14u: { // clock, with a red sweep second hand
      a0 = 0.0; a1 = rad(360.0); vmax = 12.0; tn = 60.0; tper = 5.0;
      ln = 4.0; lfirst = 3.0; lstep = 3.0; lh = 0.2;
      let t = P4.y;
      nd[0] = vec4f(fract(t / 43200.0) * 6.2831853, 0.5, 0.05, 0.1);
      nd[1] = vec4f(fract(t / 3600.0) * 6.2831853, 0.8, 0.035, 0.1);
      nn = 2;
      let sec = needle(uv, floor(t) * 0.10471976, 0.9, 0.012, 0.2, px);
      col = vec3f(0.6, 0.05, 0.02) * sec;
      cov = sec;
    }
    case 1u: { // attitude indicator
      let roll = rad(P1.y);
      let pitch = P1.x;
      let qq = rot2(uv, roll);
      let hz = -pitch * 0.028;
      bg = mix(vec3f(0.2, 0.09, 0.03), vec3f(0.06, 0.2, 0.5), smoothstep(hz - px, hz + px, qq.y));
      if (r < 0.74) {
        ink = 1.0 - smoothstep(0.0, px * 1.5, abs(qq.y - hz) - 0.01);
        let k = clamp(round((qq.y - hz) / 0.14), -4.0, 4.0);
        let w = select(0.1, 0.2, abs(k - round(k / 2.0) * 2.0) < 0.5);
        let m = (1.0 - smoothstep(0.0, px, abs(qq.y - hz - k * 0.14) - 0.008)) * step(abs(qq.x), w) * step(0.5, abs(k));
        ink = max(ink, m);
      } else {
        // Bank scale on the rotating ring: 10, 20, 30, 45, 60, 90 degrees.
        let a = abs(ang_of(qq)) * 57.29578;
        let mk = select(select(select(round(a / 10.0) * 10.0, 45.0, a > 37.5 && a < 52.5), 60.0, a >= 52.5 && a < 75.0), 90.0, a >= 75.0);
        let d = abs(a - mk) * 0.017453 * r;
        let len = select(0.1, 0.18, mk == 30.0 || mk == 60.0 || mk == 90.0);
        ink = (1.0 - smoothstep(0.012, 0.012 + px, d)) * step(0.97 - len, r) * step(mk, 90.5) * select(0.0, 1.0, mk != 40.0 && mk != 50.0 && mk != 70.0 && mk != 80.0);
      }
      let tri = step(abs(uv.x), (0.74 - uv.y) * 0.5) * step(0.62, uv.y) * step(uv.y, 0.74);
      let wing = (1.0 - smoothstep(0.0, px, abs(uv.y) - 0.022)) * step(0.16, abs(uv.x)) * step(abs(uv.x), 0.5);
      let fixed_ = max(max(tri, wing), 1.0 - smoothstep(0.04, 0.04 + px, r));
      col = vec3f(0.75, 0.35, 0.02) * fixed_;
      cov = fixed_;
      hub = false;
    }
    case 4u: { // heading indicator: the card turns under a fixed lubber line
      let hdg = rad(P0.w);
      a0 = 0.0; a1 = rad(360.0); tn = 72.0; tper = 2.0;
      q = rot2(uv, -hdg);
      let k = round((ang_of(uv) + hdg) / rad(30.0));
      let a = k * rad(30.0) - hdg;
      tp = rot2(uv - vec2f(sin(a), cos(a)) * 0.6, a);
      tc = vec2f(0.0);
      let cc = card_code(i32(k));
      tcode = cc.x; tlen = cc.y; th = 0.17;
      let lub = step(abs(uv.x), (0.98 - uv.y) * 0.4) * step(0.8, uv.y);
      let plane = max((1.0 - smoothstep(0.0, px, abs(uv.x) - 0.02)) * step(abs(uv.y + 0.05), 0.3), (1.0 - smoothstep(0.0, px, abs(uv.y - 0.05) - 0.02)) * step(abs(uv.x), 0.25));
      col = vec3f(0.75, 0.35, 0.02) * lub + vec3f(0.6) * plane;
      cov = max(lub, plane);
      hub = false;
    }
    case 3u: { // turn coordinator
      let qq = rot2(uv, clamp(P1.z / 3.0, -1.6, 1.6) * rad(20.0));
      let wing = (1.0 - smoothstep(0.0, px, abs(qq.y) - 0.025)) * step(abs(qq.x), 0.62);
      let body = 1.0 - smoothstep(0.07, 0.07 + px, length(qq));
      let fin = (1.0 - smoothstep(0.0, px, abs(qq.x) - 0.018)) * step(0.0, qq.y) * step(qq.y, 0.2);
      ink = max(max(wing, body), fin);
      // Level and standard-rate marks either side.
      let a = abs(ang_of(uv));
      let mk = select(rad(110.0), rad(90.0), a < rad(100.0));
      ink = max(ink, (1.0 - smoothstep(0.02, 0.02 + px, abs(a - mk) * r)) * step(0.66, r) * step(r, 0.85));
      // Inclinometer: a ball in a curved glass tube.
      let tube = vec2f(uv.x, uv.y + 0.52 - uv.x * uv.x * 0.25);
      if (abs(tube.y) < 0.09 && abs(tube.x) < 0.45) {
        let lines = 1.0 - smoothstep(0.0, px, abs(abs(tube.x) - 0.1) - 0.008);
        let ball = 1.0 - smoothstep(0.075 - px, 0.075 + px, length(tube - vec2f(clamp(P1.w, -1.0, 1.0) * 0.32, 0.0)));
        bg = mix(vec3f(0.45, 0.45, 0.42), vec3f(0.01), max(ball, lines));
      }
      hub = false;
    }
    case 15u: { // VOR / ILS course deviation indicator, on NAV 1 (G.nav, main.rs `nav_globals`)
      let nv = G.nav;
      let flags = u32(nv.w + 0.5);
      let live = (flags & 1u) != 0u;
      // The OBS card turns under the index at the top: ticks every 5°, figures every 30°.
      let obs = rad(nv.z);
      a0 = 0.0; a1 = rad(360.0); tn = 72.0; tper = 2.0;
      q = rot2(uv, -obs);
      if (r < 0.8) { tn = 0.0; }
      let k = round((ang_of(uv) + obs) / rad(30.0));
      let a = k * rad(30.0) - obs;
      tp = rot2(uv - vec2f(sin(a), cos(a)) * 0.68, a);
      tc = vec2f(0.0);
      let cc = card_code(i32(k));
      tcode = cc.x; tlen = cc.y; th = 0.13;
      if (r < 0.6) { tlen = 0u; }
      let index = step(abs(uv.x), (0.99 - uv.y) * 0.35) * step(0.88, uv.y);
      // Five dots each side of centre for the course, and for the glide slope on the right.
      let kx = clamp(round(uv.x / 0.1), -5.0, 5.0);
      var dots = (1.0 - smoothstep(0.016, 0.016 + px, length(uv - vec2f(kx * 0.1, 0.0)))) * step(0.5, abs(kx));
      let gsl = (flags & 8u) != 0u;
      let ky = clamp(round(uv.y / 0.1), -5.0, 5.0);
      dots = max(dots, (1.0 - smoothstep(0.013, 0.013 + px, length(uv - vec2f(0.6, ky * 0.1)))) * step(0.5, abs(ky)));
      ink = max(ink, dots);
      // The course needle hangs from the top; full scale at the fifth dot.
      let dev = select(0.0, clamp(nv.x, -1.0, 1.0) * 0.5, live);
      ndl = (1.0 - smoothstep(0.0, px, abs(uv.x - dev) - 0.016)) * step(abs(uv.y), 0.62);
      // The glide slope needle, across, while it is alive (stowed below otherwise).
      let gy = select(-0.66, clamp(nv.y, -1.0, 1.0) * 0.5, gsl);
      ndl = max(ndl, (1.0 - smoothstep(0.0, px, abs(uv.y - gy) - 0.014)) * step(abs(uv.x), 0.5));
      // TO / FROM: a white triangle pointing up or down beside the centre.
      let tri_up = step(abs(uv.x + 0.3), (0.32 - uv.y) * 0.6) * step(0.2, uv.y);
      let tri_dn = step(abs(uv.x + 0.3), (uv.y + 0.32) * 0.6) * step(uv.y, -0.2);
      var tf = 0.0;
      if ((flags & 2u) != 0u) { tf = tri_up; }
      if ((flags & 4u) != 0u) { tf = tri_dn; }
      // The red NAV flag while the receiver hears nothing usable; the GS flag the same.
      let flag = step(abs(uv.x - 0.3), 0.12) * step(abs(uv.y - 0.32), 0.06) * select(1.0, 0.0, live);
      let gflag = step(abs(uv.x - 0.6), 0.06) * step(abs(uv.y - 0.6), 0.1) * select(1.0, 0.0, gsl);
      col = vec3f(0.75, 0.35, 0.02) * index + vec3f(0.7) * tf + vec3f(0.6, 0.02, 0.01) * max(flag, gflag);
      cov = max(max(index, tf), max(flag, gflag));
      hub = false;
    }
    case 30u: { // ADF: bearing relative to the nose, independent of NAV 1's OBS.
      a0 = 0.0; a1 = rad(360.0); vmax = 360.0; tn = 36.0; tper = 3.0;
      ln = 12.0; lfirst = 0.0; lstep = 30.0; ldiv = 10.0; lr = 0.68; lh = 0.14;
      if (G.cockpit[1].w > 0.5) {
        nd[0] = vec4f(rad(G.cockpit[1].z), 0.78, 0.025, 0.48); nn = 1;
      } else {
        col = vec3f(0.65,0.02,0.01) * step(abs(uv.x),0.16) * step(abs(uv.y-0.25),0.07);
        cov = max(col.r,0.0);
      }
    }
    case 17u: { // magnetic compass: a card seen edge-on through a window
      let hdg = O.params[5].x % 1000.0;
      let x = uv.x * 32.0 + hdg;
      let k = round(x / 5.0);
      let major = abs(k - round(k / 2.0) * 2.0) < 0.5;
      ink = (1.0 - smoothstep(0.008, 0.008 + px, abs(x - k * 5.0) / 32.0)) * step(select(0.3, 0.05, major), uv.y) * step(uv.y, 0.7);
      let kl = round(x / 30.0);
      tc = vec2f((kl * 30.0 - hdg) / 32.0, -0.35);
      let cc = card_code(i32(kl));
      tcode = cc.x; tlen = cc.y; th = 0.5;
      let lub = 1.0 - smoothstep(0.0, px, abs(uv.x) - 0.02);
      col = vec3f(0.6, 0.05, 0.02) * lub;
      cov = lub;
      bg = vec3f(0.02, 0.02, 0.022) * (1.0 - 0.4 * uv.y * uv.y);
      hub = false;
    }
    case 20u: { // outside air temperature, degrees C (the standard atmosphere's at this height)
      a0 = rad(-135.0); a1 = rad(135.0); vmin = -40.0; vmax = 50.0; tn = 18.0; tper = 2.0;
      ln = 5.0; lfirst = -40.0; lstep = 20.0; lr = 0.58; lh = 0.2;
      arcs[0] = vec4f(-40.0, 0.0, 0.88, 0.95); arc_col[0] = vec3f(0.05, 0.15, 0.5);
      nd[0] = vec4f(dial(15.0 - 1.98 * P0.y / 1000.0, vmin, vmax, a0, a1), 0.84, 0.045, 0.15); nn = 1;
    }
    case 19u, 33u: { // 172RG and DR400 fuel selector plates
      ink = 1.0 - smoothstep(0.0, px, abs(r - 0.92) - 0.02);
      ink = max(ink, str_at(uv, vec2f(0.0,0.62), select(S_BOTH, S_MAIN, id == 33u), 0.17, px, 0.0));
      ink = max(ink, str_at(uv, vec2f(-0.62,0.0), S_L, 0.17, px, 0.0));
      ink = max(ink, str_at(uv, vec2f(0.62,0.0), S_R, 0.17, px, 0.0));
      ink = max(ink, str_at(uv, vec2f(0.0,-0.65), S_OFF, 0.17, px, 0.0));
      hub = false;
    }
    default: {}
  }
  // The dial, drawn once whatever the instrument.
  if (tn > 0.0) {
    ink = max(ink, ticks(q, a0, a1, tn, tper, px));
    for (var i = 0; i < 3; i++) {
      if (arcs[i].x != arcs[i].y) {
        let m = arc(q, dial(arcs[i].x, vmin, vmax, a0, a1), dial(arcs[i].y, vmin, vmax, a0, a1), arcs[i].z, arcs[i].w, px);
        col += arc_col[i] * m;
        cov = max(cov, m);
      }
    }
    if (redline > -1e8) {
      let m = arc(q, dial(redline, vmin, vmax, a0, a1) - 0.012, dial(redline, vmin, vmax, a0, a1) + 0.012, 0.78, 0.97, px);
      col += vec3f(0.6, 0.02, 0.01) * m;
      cov = max(cov, m);
    }
  }
  for (var i = 0; i < nn; i++) {
    ndl = max(ndl, needle(q, nd[i].x, nd[i].y, nd[i].z, nd[i].w, px));
  }
  // Lettering: the nearest number round the dial, or the instrument's own.
  if (ln > 0.0) {
    let ang = unwrap(ang_of(q), min(a0, a1) - 0.3);
    let x = vmin + (ang - a0) / (a1 - a0) * (vmax - vmin);
    let k = clamp(round((x - lfirst) / lstep), 0.0, ln - 1.0);
    let v = lfirst + k * lstep;
    let a = a0 + (v - vmin) / (vmax - vmin) * (a1 - a0);
    let c = vec2f(sin(a), cos(a)) * lr;
    if (length(q - c) < length(tp - tc)) {
      let nc = num_code(u32(round(abs(v) / ldiv)));
      tp = q; tc = c; tcode = nc.x; tlen = nc.y; th = lh;
    }
  }
  if (tlen > 0u) { ink = max(ink, label_code(tp, tc, tcode, tlen, th, px)); }
  if (hub) {
    let hb = 1.0 - smoothstep(0.075 - px, 0.075 + px, r);
    ndl = ndl * (1.0 - hb);
    col = mix(col, vec3f(0.02), hb);
    cov = max(cov, hb);
  }
  var c = mix(bg, col / max(cov, 1e-3), sat(cov));
  c = mix(c, vec3f(0.78, 0.78, 0.74), ink * (1.0 - cov * 0.5));
  c = mix(c, vec3f(0.82, 0.82, 0.78), ndl);
  c *= 1.0 - 0.6 * smoothstep(0.8, 1.0, r) * step(r, 1.01) * select(1.0, 0.0, id == 17u);
  return vec4f(c, max(ink, ndl) * 0.8 + cov * 0.2);
}

// Radio readouts (emissive): up to two strings per display.
fn display(id: u32, uv: vec2f, px: f32) -> vec3f {
  var size = vec2f(0.048, 0.0085);
  var c0 = vec2f(9.0); var k0 = 0u; var n0 = 0u; var h0 = 0.0105; var col0 = vec3f(0.0);
  var c1 = vec2f(9.0); var k1 = 0u; var n1 = 0u; var h1 = 0.0105; var col1 = vec3f(0.0);
  var bg = vec3f(0.0);
  switch id {
    case 0u: { // nav-com: active and standby
      let freq = u32(round(G.cockpit[1].x * 100.0));
      c0 = vec2f(-0.023, 0.0); k0 = pack(freq / 10000u, freq / 1000u % 10u, freq / 100u % 10u, 14u, freq / 10u % 10u, freq % 10u); n0 = 6u; col0 = vec3f(0.35, 1.0, 0.45);
      if (freq == 0u) {
        n0 = 0u;
        bg = vec3f(0.2,0.7,0.3) * str_at(uv,vec2f(-0.45,0.0),select(S_OFF,S_ILS,G.cockpit[3].z>0.5),0.75,px*100.0,0.0);
      }
      let adf = num_code(u32(G.cockpit[1].y));
      c1 = vec2f(0.024, 0.0); k1 = adf.x; n1 = adf.y; col1 = vec3f(0.2, 0.55, 0.25);
    }
    case 1u: { // No simulated GA autopilot: do not display a fictitious selected altitude.
      bg = vec3f(0.25,0.14,0.025) * str_at(uv,vec2f(0.0),S_OFF,0.8,px*100.0,0.0);
    }
    case 2u: { // transponder: code and pressure altitude
      c0 = vec2f(0.012, 0.0); k0 = pack(1u, 2u, 0u, 0u, 0u, 0u); n0 = 4u; col0 = vec3f(1.0, 0.55, 0.1);
      let a = num_code(u32(max(G.cockpit[0].y, 0.0)));
      c1 = vec2f(-0.025, 0.0); k1 = a.x; n1 = a.y; h1 = 0.007; col1 = vec3f(0.6, 0.33, 0.06);
    }
    case 3u: { // GPS: a heading-up moving map, ground speed in the corner
      size = vec2f(0.052, 0.027);
      // Heading-up navigation plot, 10 NM to the edge: actual relative station positions.
      let heading = rad(O.params[0].w);
      let grid = 1.0 - smoothstep(0.005, 0.012, min(abs(fract(uv.x * 2.0 + 0.5)-0.5), abs(fract(uv.y * 2.0 + 0.5)-0.5)));
      bg = vec3f(0.015,0.035,0.045) + vec3f(0.025) * grid;
      let waypoint = rot2(G.cockpit[3].xy, -heading);
      if (G.cockpit[3].z > 0.5) {
        let delta = uv - waypoint;
        let dotmark = 1.0 - smoothstep(0.025,0.045,length(delta));
        let along = clamp(dot(uv,waypoint)/max(dot(waypoint,waypoint),0.0001),0.0,1.0);
        let route = 1.0-smoothstep(0.008,0.014,length(uv-waypoint*along));
        bg = mix(bg,vec3f(0.8,0.1,0.8),max(dotmark,route)*0.9);
      }
      for (var i=4u; i<8u; i++) {
        let station = rot2(G.cockpit[i].xy,-heading);
        let dotmark = (1.0-smoothstep(0.015,0.025,length(uv-station))) * G.cockpit[i].z;
        bg = mix(bg,vec3f(0.3,0.7,0.9),dotmark);
      }
      let own = step(abs(uv.x),(0.12-uv.y)*0.5)*step(-0.08,uv.y);
      bg = mix(bg,vec3f(0.9),own);
      let gs = num_code(u32(G.cockpit[0].z));
      c0 = vec2f(-0.04,0.021); k0=gs.x; n0=gs.y; h0=0.006; col0=vec3f(1.0);
    }
    case 5u: { // hour meter
      size = vec2f(0.021, 0.0065);
      c0 = vec2f(0.0); k0 = num_code(u32(G.cockpit[2].w * 10.0)).x; n0 = 5u; h0 = 0.008; col0 = vec3f(0.9);
    }
    default: {}
  }
  let q = uv * size;
  var c = bg;
  if (n0 > 0u) { c = mix(c, col0, label_code(q, c0, k0, n0, h0, px)); }
  if (n1 > 0u) { c = mix(c, col1, label_code(q, c1, k1, n1, h1, px)); }
  return c;
}

// ---------------------------------------------------------------- lighting

fn ggx(ndh: f32, a: f32) -> f32 {
  let a2 = a * a;
  let d = ndh * ndh * (a2 - 1.0) + 1.0;
  return a2 / (PI * d * d);
}
fn vis(ndv: f32, ndl: f32, a: f32) -> f32 {
  let k = a * 0.5;
  return 0.25 / ((ndv * (1.0 - k) + k) * (ndl * (1.0 - k) + k));
}

// ---------------------------------------------------------------- the Hornet (aircraft/fa18)

// Its paint: three pictures of it (from above, from below and from the left
// side, the layers of t_paint) projected back onto the skin, each where the
// surface faces that way. The rectangles they cover are GUIDE_* in
// assets/blender/fa18.py. rgb the paint; a 0 where no picture faces the
// surface (the lips of the intakes, the ends of things).
// The other painted types' work the same way, each with its own rectangle
// (PT, from its aircraft::Paint); those with a picture of each side have
// their lettering the right way round on both.
fn hornet_paint(L: vec3f, Nl: vec3f, dLx: vec3f, dLy: vec3f) -> vec4f {
  // across (and up the side views), fore and aft; then how far the pictures' edges are from the datum: to the side, ahead, below
  let box = PT.box.xy;
  let off = vec3f(PT.box.x * 0.5, PT.box.zw);
  let n = Nl * inverseSqrt(max(dot(Nl, Nl), 1e-12));
  var w = vec3f(pow(max(n.y, 0.0), 4.0), pow(max(-n.y, 0.0), 4.0), pow(abs(n.x), 4.0));
  let sum = w.x + w.y + w.z;
  if (sum < 0.02) { return vec4f(0.0); }
  w /= sum;
  let v = (L.z + off.y) / box.y;
  let dv = vec2f(dLx.z, dLy.z) / box.y;
  var c = vec3f(0.0);
  if (w.x > 0.004) { c += w.x * textureSampleGrad(t_paint, s_aniso, vec2f((L.x + off.x) / box.x, v), 0, vec2f(dLx.x / box.x, dv.x), vec2f(dLy.x / box.x, dv.y)).rgb; }
  if (w.y > 0.004) { c += w.y * textureSampleGrad(t_paint, s_aniso, vec2f((off.x - L.x) / box.x, v), 1, vec2f(-dLx.x / box.x, dv.x), vec2f(-dLy.x / box.x, dv.y)).rgb; }
  if (w.z > 0.004) { c += w.z * textureSampleGrad(t_paint, s_aniso, vec2f((L.y + off.z) / box.x, v), 2 + select(0, 1, PT.view.x > 3.5 && n.x > 0.0), vec2f(dLx.y / box.x, dv.x), vec2f(dLy.y / box.x, dv.y)).rgb; }
  return vec4f(c, 1.0);
}

// Where the engines' exhaust leaves: how far out the engines' centrelines
// are and their height, and the station of the nozzle exits. The Hornet's:
// 0.52 m either side, 0.05 m below the datum, at z = 8.0.
// The Phantom's (assets/blender/f4.py): 0.50 m either side, 0.30 m below, at z = 4.8.
const F4_NOZZLE = vec3f(0.50, -0.30, 4.8);
// The Raptor's (assets/blender/f22.py): 0.78 m either side, 0.12 m below, at z = 7.3. Its
// nozzles are flat, 1.3 m long from their hinge with the flame holder 0.9 m
// in, and its jets leave them wider than they are tall (PLUME_SQUASH there).
const F22_NOZZLE = vec3f(0.78, -0.12, 7.3);
const F22_SQUASH = vec2f(1.30, 0.65);
// The Blackbird's (assets/blender/sr71.py, moved 3.3 m forward onto its centre of gravity: sr71/mod.rs `CG`): out on the wings, 4.12 m either side, 0.10 m below, at z = 9.1.
// Its nozzles are the Hornet's 1.85 times as wide and 1.5 m long, its plumes as much wider and 2.5 times as long.
const SR71_NOZZLE = vec3f(4.12, -0.10, 9.1);
const SR71_PLUME = vec2f(1.853, 2.5);
fn nozzle() -> vec3f { return select(select(select(vec3f(0.52, -0.05, 8.0), F4_NOZZLE, kind() == 3u), F22_NOZZLE, kind() == 4u), SR71_NOZZLE, kind() == 7u); }
// How long the nozzle is, shroud to lip, and how deep inside it is lit.
fn nozzle_len() -> vec2f { return select(select(vec2f(0.85, 0.58), vec2f(1.3, 0.9), kind() == 4u), vec2f(1.5, 1.0), kind() == 7u); }
fn plume_r(L: vec3f) -> f32 { return length(vec2f(abs(L.x) - nozzle().x, L.y - nozzle().y) / select(select(vec2f(1.0), F22_SQUASH, kind() == 4u), vec2f(SR71_PLUME.x), kind() == 7u)); }

// The Phantom's two cockpits under their canopy, as the Hornet's tub below:
// half width, floor, front and back; and where an eye inside them is: half
// width, lowest, highest, front, back.
const F4_TUB = vec4f(0.56, 0.2, -6.56, -2.75);
const F4_CABIN = array<f32, 5>(0.6, 0.2, 1.5, -6.4, -2.5);
// The Raptor's cockpit, likewise.
const F22_TUB = vec4f(0.48, 0.1, -7.1, -4.1);
const F22_CABIN = array<f32, 5>(0.55, 0.1, 1.35, -7.1, -3.9);
// Its HUD's combiner (f22/pit.rs HUD_BASE): half width, lowest, highest, front, back.
const F22_HUD = array<f32, 5>(0.125, 0.86, 1.07, -6.20, -6.08);
// Concorde (assets/blender/concorde.py, kind 6): four engines, their centrelines 4.755 m and
// 6.105 m either side and 1.92 m below the datum, the nozzles' exits at z = 19.5; the
// buckets are 1.6 m long with the flame holders 1.2 m in; its reheat plumes are 1.4 times
// the Hornet's. Its flight deck: half width, floor, front and back; and an eye inside it.
const CONCORDE_NOZZLE = vec4f(4.755, 6.105, -1.82, 18.9);
const CONCORDE_PLUME = 1.4;
const CONCORDE_TUB = vec4f(1.36, -0.75, -25.3, -20.85);
const CONCORDE_CABIN = array<f32, 5>(1.3, -0.75, 1.5, -25.3, -20.85);
// The G650's flight deck (g650/mod.rs: the nose section's skin, FLOOR_Y, DECK_Z), likewise.
const G650_TUB = vec4f(1.20, -0.36, -12.75, -9.95);
const G650_CABIN = array<f32, 5>(1.15, -0.36, 1.55, -12.75, -9.95);
// The nozzle a point belongs to: `nozzle()`, or of two engines a side the nearer one.
fn nozzle_at(L: vec3f) -> vec3f {
  if (kind() == 6u) { return vec3f(select(CONCORDE_NOZZLE.x, CONCORDE_NOZZLE.y, abs(L.x) > 0.5 * (CONCORDE_NOZZLE.x + CONCORDE_NOZZLE.y)), CONCORDE_NOZZLE.zw); }
  return nozzle();
}
// How many times the Hornet's size the plume is, and its radius about its own engine's jet in the Hornet's measure.
fn plume_scale() -> f32 { return select(1.0, CONCORDE_PLUME, kind() == 6u); }
fn plume_r_at(L: vec3f) -> f32 {
  if (kind() == 6u) { let n = nozzle_at(L); return length(vec2f(abs(L.x) - n.x, L.y - n.y)) / CONCORDE_PLUME; }
  return plume_r(L);
}
// The Blackbird's two cockpits, likewise.
const SR71_TUB = vec4f(0.46, -0.02, -15.8, -12.3);
const SR71_CABIN = array<f32, 5>(0.5, 0.0, 1.25, -15.4, -12.2);

// The rocket (aircraft/starship, kind 5): each stage's engines make one plume on
// the centreline. Its radius at the exit, the station of the exit plane and
// the length of the flame, for the booster and for the ship (starship.py).
// tint.z says which stage is burning (0 the booster, 1 the ship, 2 both: the
// ship lit on top of the booster's centre engines as they part) and, in its
// fraction, how many of its engines (0.9 = all); tint.w how thin the air is
// (0 sea level .. 1 a vacuum); glow.z how hot the air makes the ship's tiles.
const ROCKET_BOOSTER = vec3f(4.3, 48.0, 64.0);
const ROCKET_SHIP = vec3f(3.8, -23.2, 28.0);
// How hard the engines of the stage this point belongs to are burning.
fn rocket_lit(L: vec3f) -> f32 {
  let stage = floor(O.tint.z + 0.01);
  return burner() * select(step(0.5, stage), 1.0 - step(0.5, stage) + step(1.5, stage), L.z > 40.0);
}
// The share of that stage's engines that are (all of the ship's while staging).
fn rocket_some(L: vec3f) -> f32 {
  return select(sat(fract(O.tint.z + 0.01) / 0.9), 1.0, O.tint.z > 1.99 && L.z < 40.0);
}
// How wide their flame stands, as a share of all of them burning: three in
// the middle make a thin core, all 33 the full fan.
fn rocket_width(L: vec3f) -> f32 {
  return 0.12 + 0.88 * pow(rocket_some(L), select(1.3, 0.6, L.z > 40.0));
}
// And how far out a point of the plume's shells is pushed for that: wider
// still the thinner the air and the further behind the engines, where
// nothing holds the exhaust in.
fn rocket_spread(L: vec3f) -> f32 {
  let S = select(ROCKET_SHIP, ROCKET_BOOSTER, L.z > 40.0);
  let thin = O.tint.w;
  return rocket_width(L) * (1.0 + thin * thin * (0.5 + 5.0 * max(L.z - S.y, 0.0) / S.z));
}
// Is the engine whose bell this is one of those burning? (The booster's rim
// of 20 only with all 33, its ring of 10 with the inner 13; the ship's three
// vacuum engines only with all six.)
fn rocket_bell(L: vec3f) -> f32 {
  let r = length(L.xy);
  let some = rocket_some(L);
  return select(select(1.0, step(0.75, some), r > 1.6), select(select(1.0, step(0.3, some), r > 1.5), step(0.9, some), r > 3.1), L.z > 40.0);
}
// Methane and oxygen burn almost clear: a blue-white core going violet, shock
// diamonds a couple of plume-widths apart, an orange tail where it meets the
// air; fainter and wider the thinner the air.
fn rocket_plume(in: VOut) -> vec4f {
  let layer = u32(in.mat.w + 0.5);
  let L = in.L;
  let lit = rocket_lit(L);
  if (lit < 0.01) { discard; }
  let S0 = select(ROCKET_SHIP, ROCKET_BOOSTER, L.z > 40.0);
  let wide = rocket_width(L);
  // (The diamonds stand as far apart as the flame is wide.)
  let S = vec3f(S0.x * wide, S0.yz);
  let some = mix(0.55, 1.0, rocket_some(L));
  let thin = O.tint.w;
  let z = L.z - S.y;
  let s = z / S.z;
  let ndv = abs(dot(normalize(in.N), normalize(in.P)));
  let t = G.time.x;
  let n1 = nz(vec3f(L.x * 0.22, L.y * 0.22, z * 0.045 - t * 5.0));
  let n2 = nz(vec3f(L.x * 0.8, L.y * 0.8, z * 0.15 - t * 13.0));
  if (layer == 0u) {
    let k = 0.7 * lit * (1.0 - thin) * (0.3 + 0.7 * smoothstep(0.0, 0.6, ndv)) * smoothstep(-0.5, 6.0, z) * (1.0 - smoothstep(0.8, 1.35, s));
    if (k < 0.01) { discard; }
    let off = (vec2f(n1.g, n1.b) - 0.5 + (vec2f(n2.r, n2.a) - 0.5) * 0.6) * k * 0.5 / max(length(in.P), 20.0);
    let uv = in.clip.xy * G.screen.zw + off;
    var seen = textureLoad(t_scene, vec2i(in.clip.xy), 0).rgb;
    let q = vec2i(clamp(uv * G.screen.xy, vec2f(0.0), G.screen.xy - 1.0));
    if (textureLoad(t_gdepth, q, 0) <= in.clip.z) { seen = textureSampleLevel(t_scene, s_clamp, uv, 0.0).rgb; }
    let a = sat(k * 1.6);
    return vec4f(seen * a, a);
  }
  let l = light_at_nv(in.P);
  let night = 1.0 - smoothstep(-0.08, 0.1, dot(l.upn, G.sun_dir.xyz));
  let dia = pow(0.5 + 0.5 * cos((z / (S.x * 2.6) - 0.35) * 6.2832), 3.0) * exp(-max(z, 0.0) / (S.z * 0.45)) * smoothstep(1.0, 4.0, z) * (1.0 - 0.8 * thin);
  let cone = exp(-max(z, 0.0) / (S.z * 0.12));
  let flick = 0.75 + 0.5 * n1.r;
  var col = vec3f(0.0);
  var glow = 0.0;
  if (layer == 2u) {
    col = mix(vec3f(0.80, 0.86, 1.0), vec3f(0.72, 0.55, 1.0), smoothstep(0.0, 0.5, s));
    col = mix(col, vec3f(1.0, 0.88, 0.72), sat(dia * 1.3));
    glow = (0.25 + 1.0 * cone + 1.7 * dia) * pow(ndv, 1.3) * (1.0 - smoothstep(0.35, 0.62, s)) * flick;
  } else if (layer == 1u) {
    col = mix(vec3f(0.52, 0.58, 1.0), vec3f(1.0, 0.50, 0.22), smoothstep(0.3, 0.95, s));
    glow = (0.22 + 0.3 * cone + 0.3 * dia) * pow(ndv, mix(1.6, 0.7, thin)) * (1.0 - smoothstep(0.5, 1.0, s)) * (0.5 + n2.g);
  } else {
    let r = length(L.xy) / max(S0.x * (1.0 - 0.6 * s), 0.3);
    col = mix(vec3f(0.80, 0.85, 1.0), vec3f(0.85, 0.55, 0.80), sat(smoothstep(0.0, 0.6, s) * 0.6 + 0.5 * r));
    glow = 0.22 * pow(ndv, 3.0) * pow(sat(1.0 - r * r), 1.5) * (1.0 - smoothstep(0.45, 0.85, s)) * (0.6 + 1.2 * dia + 0.6 * cone) * flick;
  }
  glow *= lit * some * smoothstep(-0.5, 1.0, z) * mix(1.0, 0.4, thin);
  if (glow < 0.0005) { discard; }
  return vec4f(col * glow * G.sun_dir.w * mix(0.15, 0.012, night), 0.0);
}

// The exhaust plume (material 17, drawn in the transparent pass): shells and
// discs round each engine's jet that glow by how squarely they are looked
// through, so the whole reads as a volume. in.mat.w: 0 the heat haze, 1 the
// flame's sheath, 2 its core, 3 discs across it (what is seen looking up the
// jet). Dry, the jet only makes the air shimmer; with the afterburners lit
// it is a flame five metres long with shock diamonds standing in it.
fn plume(in: VOut) -> vec4f {
  if (kind() == 5u) { return rocket_plume(in); }
  let layer = u32(in.mat.w + 0.5);
  let ab = burner();
  let dry = dry_power();
  let L = in.L;
  // (how far aft of the nozzle, in the Hornet's plume's lengths)
  let z = (L.z - nozzle_at(L).z) / select(plume_scale(), SR71_PLUME.y, kind() == 7u);
  let V = normalize(in.P);
  let ndv = abs(dot(normalize(in.N), V));
  let t = G.time.x;
  let side = sign(L.x);
  // Turbulence streaming aft with the jet.
  let n1 = nz(vec3f(L.x * 1.7 + side * 3.1, L.y * 1.7, z * 0.30 - t * 9.0));
  let n2 = nz(vec3f(L.x * 5.0 + side, L.y * 5.0, z * 0.9 - t * 21.0));
  if (layer == 0u) {
    // Heat haze: what lies behind the jet shimmers through it.
    let k = (0.10 + 0.55 * dry + 0.6 * ab) * (0.3 + 0.7 * smoothstep(0.0, 0.6, ndv)) * smoothstep(-0.05, 0.8, z) * (1.0 - smoothstep(5.0, 11.5, z));
    if (k < 0.01) { discard; }
    let off = (vec2f(n1.g, n1.b) - 0.5 + (vec2f(n2.r, n2.a) - 0.5) * 0.6) * k * 0.05 / max(length(in.P), 2.0);
    let uv = in.clip.xy * G.screen.zw + off;
    var seen = textureLoad(t_scene, vec2i(in.clip.xy), 0).rgb;
    let q = vec2i(clamp(uv * G.screen.xy, vec2f(0.0), G.screen.xy - 1.0));
    if (textureLoad(t_gdepth, q, 0) <= in.clip.z) { seen = textureSampleLevel(t_scene, s_clamp, uv, 0.0).rgb; }
    let a = sat(k * 1.6);
    return vec4f(seen * a, a);
  }
  let l = light_at_nv(in.P);
  let night = 1.0 - smoothstep(-0.08, 0.1, dot(l.upn, G.sun_dir.xyz));
  // Shock diamonds: bright knots where the pressure waves in the jet cross,
  // closer together and fainter aft; and the cone of flame off the nozzle.
  let zd = z * (1.0 + 0.035 * z);
  let dia = pow(0.5 + 0.5 * cos((zd - 0.62) * 6.2832 / 0.88), 3.0) * exp(-max(z, 0.0) / 2.6) * smoothstep(0.15, 0.5, z);
  let cone = exp(-max(z, 0.0) / 0.45);
  let flick = 0.72 + 0.56 * n1.r;
  var col = vec3f(0.0);
  var glow = 0.0;
  if (layer == 2u) {
    // The core: blue-white off the nozzle, the diamonds yellow-white, orange further back.
    let s = z / 4.5;
    col = mix(vec3f(0.55, 0.70, 1.0), vec3f(1.0, 0.72, 0.36), smoothstep(0.0, 0.6, s));
    col = mix(col, vec3f(1.0, 0.90, 0.70), sat(dia * 1.4));
    glow = ab * (0.10 + 1.1 * cone + 2.3 * dia) * pow(ndv, 1.4) * (1.0 - smoothstep(0.5, 1.0, s)) * flick;
  } else if (layer == 1u) {
    // The sheath: a thin orange going to red, with a violet edge near the nozzle.
    let s = z / 6.9;
    col = mix(vec3f(1.0, 0.46, 0.14), vec3f(0.85, 0.18, 0.04), smoothstep(0.1, 0.9, s));
    col = mix(col, vec3f(0.50, 0.40, 1.0), 0.45 * (1.0 - ndv) * (1.0 - smoothstep(0.0, 0.45, s)));
    glow = ab * (0.20 + 0.25 * cone + 0.40 * dia) * pow(ndv, 2.0) * (1.0 - smoothstep(0.25, 1.0, s)) * (0.5 + 1.0 * n2.g);
    // Dry, at full power: a faint glow off the nozzle, only seen at dusk.
    glow += dry * dry * 0.012 * pow(ndv, 2.0) * (1.0 - smoothstep(0.0, 0.3, s));
  } else {
    // Discs across the jet: its glow seen from behind, down its length.
    let s = z / 5.6;
    let r = plume_r_at(L) / max(0.36 * (1.0 - 0.75 * s), 0.05);
    col = mix(vec3f(0.75, 0.80, 1.0), vec3f(1.0, 0.55, 0.20), sat(smoothstep(0.0, 0.7, s) * 0.6 + 0.6 * r));
    glow = ab * 0.20 * pow(ndv, 3.0) * pow(sat(1.0 - r * r), 1.5) * (1.0 - smoothstep(0.4, 1.0, s)) * (0.6 + 1.2 * dia + 0.6 * cone) * flick;
  }
  glow *= smoothstep(-0.14, 0.06, z);
  if (glow < 0.0005) { discard; }
  // (Turned down at night, when the eye has opened up; it still blooms.)
  return vec4f(col * glow * G.sun_dir.w * mix(0.06, 0.007, night), 0.0);
}

// Lit colour, normal and SSR weight, and whether the fragment lies in an opening.
struct Shaded { c: vec4f, n: vec4f, kill: f32, amb: f32 };

// Pieces with openings cut in them (skin, lining) discard; everything else
// goes through fs_solid, which never does, so early depth testing stays on.
@fragment fn fs(in: VOut) -> GOut {
  let s = surface(in);
  if (s.kill > 0.5) { discard; }
  return GOut(s.c, s.n, s.amb);
}
@fragment fn fs_solid(in: VOut) -> GOut {
  let s = surface(in);
  return GOut(s.c, s.n, s.amb);
}

fn surface(in: VOut) -> Shaded {
  let id = u32(in.mat.x + 0.5);
  let pt = u32(in.mat.y + 0.5);
  // Everything that needs screen derivatives first, while control flow is uniform.
  let dPx = dpdx(in.P);
  let dPy = dpdy(in.P);
  let px = max(length(dPx), length(dPy));
  let duvx = dpdx(in.uv);
  let duvy = dpdy(in.uv);
  let duv = max(length(duvx), length(duvy));
  let dLx = dpdx(in.L);
  let dLy = dpdy(in.L);
  // low preset skips panel lines and rivets, G.quality.x is 1.8 there.
  var rel = vec3f(0.0);
  if (G.quality.x <= 1.5) { rel = relief(pt, in.uv, in.L, in.Nl, px); }
  // The Hornet's paint; its panel lines and rivets, darker than the skin
  // round them, are taken to lie a little below it.
  var paint = vec4f(0.0);
  if (PT.view.x > 0.5 && pt == P_HORNET) {
    paint = hornet_paint(in.L, in.Nl, dLx, dLy);
    rel.x += luma(paint.rgb) * 0.004 * PT.view.y * paint.a * (1.0 - smoothstep(0.003, 0.012, px));
  }
  let hx = dpdx(rel.x);
  let hy = dpdy(rel.x);
  let cuts = ((pt == P_FUSE || pt == P_COWL || pt == P_LINING) && kind() != 8u) || (pt == P_COWL && kind() == 8u);
  var wd = 1.0;
  var wl = 1.0;
  if (cuts) { wd = select(window(in.L), inlet_dr400(in.L), kind() == 8u); }
  // The 747's lining is cut back round its windows by the width of the trim (b747::deck::lining_window).
  if (kind() == 1u && pt == P_LINING) {
    // Measured on the skin straight outside (radially from 0.3 m up), less the trim.
    let rr = normalize(vec2f(in.L.x, in.L.y - 0.3));
    wd = window(in.L + vec3f(rr * 0.10, 0.0)) - 0.03;
  }
  if (pt == P_FUSE || pt == P_COWL) { wl = well(in.L); }
  // The well outlines are drawn in plan (z, x); on the steep sides of the
  // belly that stretches distances along the skin, so measure the edge there.
  let wl_s = wl * px / max(length(vec2f(dpdx(wl), dpdy(wl))), 1e-7);
  let td = tear_d(in.L);
  // A downloaded aircraft's own texture (fgmodel.rs), its see-through parts cut out.
  var tex = vec4f(1.0);
  if (id == M_TEX) { tex = tex_at(in.uv, in.mat.w, duvx, duvy); }
  let kill = (cuts && (wd < 0.0 || wl < 0.0)) || id == M_GLASS || id == M_PROPDISC || id == M_PLUME || td > 0.0 || tex.a < 0.5;

  var n0 = in.N;
  if (dot(n0, n0) < 1e-10) { n0 = -in.P; }
  n0 = normalize(n0);
  let r1 = cross(dPy, n0);
  let r2 = cross(n0, dPx);
  let det = dot(dPx, r1);
  var N = n0;
  if (abs(det) > 1e-14) { N = normalize(abs(det) * n0 - sign(det) * (hx * r1 + hy * r2)); }
  let V = normalize(in.P);
  var Ng = n0;
  let back = dot(n0, V) > 0.0;
  if (back) { N = -N; Ng = -n0; }
  let l = light_at(in.P);
  let night = 1.0 - smoothstep(-0.08, 0.1, dot(l.upn, G.sun_dir.xyz));
  let L = in.L;
  let ns = nz(L * 2.1);
  let fine = nz(L * 13.0);

  var albedo = in.col.rgb;
  var rough = 0.5;
  var metal = 0.0;
  var emit = vec3f(0.0);
  var spec_k = 1.0;
  var ssr = 0.0;
  var ao = in.col.a;
  let cav = 1.0 - rel.y * 0.4;

  switch id {
    case 0u, 1u, 15u: {
      if (id == 1u) { albedo = livery(L, px * 1.2, pt); }
      rough = select(0.13 + 0.08 * ns.g, 0.5 + 0.15 * ns.g, id == 15u);
      if (id == 1u && bare_c172()) {
        // Bare aluminium, as `case 2u`; the black registration stays paint.
        metal = smoothstep(0.08, 0.3, luma(albedo));
        rough = mix(rough, 0.22 + 0.18 * fine.g, metal);
        albedo *= mix(1.0, 0.9 + 0.12 * ns.r, metal);
      }
      if (pt == P_HORNET && paint.a > 0.5) {
        // The Hornet's matte tactical greys, as painted; fluid stains (darker and browner) are a little glossier.
        albedo = paint.rgb * 0.85;
        // (The G650 in an airline's scheme: painted over its plain skin.)
        if (kind() == 9u && livery_style() > 0u) { albedo = livery_g650(L, albedo, px * 1.2); }
        // (Only the Hornet's: on a camouflage the tans and greens are not stains.)
        rough = 0.48 + 0.14 * ns.g - select(0.0, 0.12, kind() == 2u) * smoothstep(0.02, 0.10, paint.r - paint.b);
        // (A dark paint's panels: the lighter, baked ones flatter, the darker, fresher ones with more of a sheen.)
        rough += PT.view.z * (sat(luma(paint.rgb) * 25.0) - 0.5);
        // (Concorde's is an airliner's gloss white, kept clean.)
        if (kind() == 6u) { rough = 0.16 + 0.08 * ns.g; }
        // (The G650's: a business jet's deep gloss, polished.)
        if (kind() == 9u) { rough = 0.12 + 0.06 * ns.g; }
      }
      // Each skin panel's paint aged a little differently.
      albedo *= 0.985 + 0.03 * fine.a * select(0.4, 1.0, id == 1u);
      if (id == 1u || pt == P_FUSE) {
        if (G.quality.x <= 1.5) {
          let w = weather(L, in.Nl, albedo);
          albedo = w.rgb;
          rough += w.a;
        }
        // Chipped paint where things get knocked: leading edges, around fasteners.
        let chip = smoothstep(0.80, 0.83, nz(L * 7.0).b + in.wear * 0.07) * sat(in.wear * 1.5);
        albedo = mix(albedo, vec3f(0.55, 0.56, 0.58), chip);
        metal = max(metal, chip);
        rough = mix(rough, 0.35, chip);
        // Bugs on the leading edges.
        let bugs = smoothstep(0.86, 0.9, nz(L * 9.0).a) * smoothstep(0.75, 0.97, in.wear) * sat(-in.Nl.z * 2.0) * (1.0 - smoothstep(0.002, 0.008, px));
        albedo = mix(albedo, vec3f(0.12, 0.09, 0.03), bugs * 0.7);
        ssr = 1.0;
      }
      if (pt == P_PIT) {
        // The Hornet's cockpit grey: satin, worn where boots and elbows go,
        // grimier low down, scuffed through to lighter primer here and there.
        rough = 0.55 + 0.12 * fine.g;
        albedo *= 0.9 + 0.2 * nz(L * 3.1).r;
        albedo *= mix(0.7, 1.0, sat((L.y - FLOOR18) / 0.35));
        let sc = smoothstep(0.80, 0.9, nz(vec3f(L.x * 34.0, L.y * 34.0, L.z * 5.0)).r) * smoothstep(0.45, 0.7, nz(L * 4.3).b);
        albedo = mix(albedo, albedo * 1.8 + vec3f(0.03), sc * 0.6);
        rough = mix(rough, 0.4, sc);
        albedo = mix(albedo, albedo * 0.6, smoothstep(0.62, 0.85, nz(L * 9.0).g) * 0.4);
      }
      if (kind() == 10u && pt == P_WING) {
        let wk = walkway_bonanza(L, px);
        rough = mix(rough, 0.85, wk);
        spec_k = mix(spec_k, 0.5, wk);
      }
      if (kind() == 8u && pt == P_WING) {
        let wk = walkway_dr400(L, px);
        rough = mix(rough, 0.85, wk);
        spec_k = mix(spec_k, 0.5, wk);
      }
      if (pt == P_NONSLIP) {
        rough = 0.92;
        spec_k = 0.4;
        albedo *= 0.8 + 0.4 * nz(L * 60.0).r;
        albedo = mix(albedo, vec3f(0.06, 0.055, 0.05), smoothstep(0.5, 0.8, nz(L * 7.0).g) * 0.5);
      }
      if (pt == P_STRIPE) {
        // Yellow and black: diagonal on a face, rings along a tube or strap.
        let w = in.mat.w;
        let t = select(in.uv.x + in.uv.y, in.uv.y, w < 0.0) / max(abs(w), 1e-4);
        let blk = smoothstep(0.47, 0.53, abs(fract(t) - 0.5) * 2.0 + 0.0);
        albedo = mix(albedo, vec3f(0.012), blk) * (0.85 + 0.25 * fine.r);
        rough = 0.35 + 0.15 * fine.g;
      }
      if (kind() == 1u && (id == 1u || pt == P_SPINNER)) {
        let fn_ = skin747(Fin(albedo, rough, metal), pt, L, in.Nl, in.uv, in.wear, px);
        albedo = fn_.c;
        rough = fn_.rough;
        metal = fn_.metal;
        // At night the cabin shows through its windows (not the wreck's):
        // warm, each a little different, a few dimmer and some shades down.
        if (id == 1u && night > 0.001 && O.glow.x < 0.001) {
          let sd = cabin_win_sd747(L);
          if (sd < 0.01) {
            let d = L.z + 31.6;
            let upper = step(2.5, L.y);
            let iw = floor((d - select(2.4, 9.6, upper > 0.5)) / 0.508);
            let h = hash2(vec2f(iw + sign(L.x) * 131.0, upper * 17.0 + 3.0));
            let h2 = hash2(vec2f(iw * 0.37 + sign(L.x) * 7.0, upper * 5.0 + 11.0));
            // Shades down on about one in seven; a block of rows dimmed now and then.
            let blind = step(0.86, h);
            let zone = 0.55 + 0.45 * step(0.25, hash1(floor(d / 7.5) + sign(L.x) * 3.0));
            let tint = mix(vec3f(1.0, 0.80, 0.56), vec3f(1.0, 0.90, 0.78), h2);
            // Brightest in the middle of the pane, the reveal round it darker;
            // dimmer looked at slantwise (the far wall seen instead of the lamps).
            let inner = smoothstep(0.012, -0.05, sd);
            let ndv = abs(dot(normalize(in.N), V));
            let k = inner * (0.35 + 0.65 * ndv) * zone * mix(1.0, 0.12, blind) * (0.75 + 0.5 * h2);
            emit += tint * k * night * G.sun_dir.w * 0.006;
            albedo *= 1.0 - 0.6 * inner * night;
          }
        }
      }
      // Window sealant and the edges of the gear wells.
      if (pt == P_FUSE || pt == P_COWL) {
        let gk = smoothstep(0.016 + px, 0.016 - px, wd) * step(-0.001, wd) * select(1.0, 0.0, kind() == 8u);
        let we = smoothstep(0.004 + px, 0.004 - px, wl_s) * step(-0.001, wl);
        albedo = mix(albedo, vec3f(0.012), max(gk, we * 0.6));
        rough = mix(rough, 0.6, gk);
        ssr *= 1.0 - gk;
        if (pt == P_COWL && L.z < -2.97 && kind() == 0u) {
          // Cooling air inlets in the nose bowl.
          let e1 = length((vec2f(abs(L.x), L.y) - vec2f(0.235, 0.05)) / vec2f(0.105, 0.068));
          let e2 = length((L.xy - vec2f(0.0, -0.33)) / vec2f(0.11, 0.045));
          let inlet = smoothstep(1.0 + px * 10.0, 1.0 - px * 10.0, min(e1, e2));
          albedo = mix(albedo, vec3f(0.006) * (0.4 + 0.6 * smoothstep(0.0, 1.0, min(e1, e2))), inlet);
          rough = mix(rough, 0.9, inlet);
          ao *= 1.0 - 0.6 * inlet;
          ssr *= 1.0 - inlet;
        }
      }
      albedo *= cav;
      // Rivet heads catch a little grime round their edges.
      albedo *= 1.0 - rel.z * (1.0 - rel.z) * 0.35 - rel.z * 0.03;
    }
    case 2u: { // bare aluminium
      metal = 1.0;
      rough = 0.22 + 0.18 * fine.g;
      albedo *= 0.9 + 0.12 * ns.r;
      ssr = 0.6;
    }
    case 3u: { // steel: 0 machined, 1 chrome, 2 cast iron / heat-tinted
      metal = 1.0;
      rough = select(select(0.25, 0.05, in.mat.w > 0.5 && in.mat.w < 1.5), 0.45, in.mat.w > 1.5);
      if (in.mat.w > 1.5) {
        albedo = mix(albedo, vec3f(0.30, 0.16, 0.07), smoothstep(0.4, 0.7, fine.r) * 0.6);
        metal = 0.8;
      }
      ssr = 0.5;
    }
    case 4u: { // rubber: dusty, the tread worn lighter
      rough = 0.85;
      albedo = mix(albedo, vec3f(0.07, 0.065, 0.058), sat(rel.y * 0.0 + smoothstep(0.3, 0.8, ns.g) * 0.4 + select(0.0, 0.45, pt == P_TREAD && rel.y < 0.3)));
      spec_k = 0.4;
      if (kind() == 1u) {
        // An airliner's tyres: near-black, greyed by brake dust and rubber
        // scrubbed off on landing; the sidewalls a touch glossier.
        let tread = select(0.0, 1.0, pt == P_TREAD && abs(in.Nl.x) < 0.5);
        albedo = mix(vec3f(0.022, 0.022, 0.023), vec3f(0.05, 0.049, 0.048), sat(smoothstep(0.3, 0.8, ns.g) * 0.5 + tread * 0.5));
        rough = mix(0.62, 0.85, tread);
      }
    }
    case 5u: {
      rough = 0.5 + 0.2 * fine.b;
      spec_k = 0.6;
      albedo *= cav;
      if (pt == P_LEGEND) {
        // Printed legends, button faces, scales (deck747.wgsl): white paint
        // lit from behind at night; annunciators that are on glow by day too.
        let code = u32(floor(in.mat.w));
        let lg = legend(code % 16u, code / 16u, in.uv, max(in.wear, 0.05), max(duv, 0.002), fract(in.mat.w) * 100.0);
        albedo = mix(albedo, lg.col, lg.a);
        rough = mix(rough, 0.35, lg.a);
        emit = (lg.glow * mix(0.0, 0.004, night) + lg.lit * mix(0.05, 0.012, night)) * G.sun_dir.w;
        // (The Hornet's panels are edge-lit in NVG green.)
        if (kind() == 2u) { emit *= vec3f(0.55, 1.0, 0.62); }
        // (The Phantom's in red.)
        if (kind() == 3u) { emit *= vec3f(1.0, 0.22, 0.12); }
        // (The Raptor's in NVG green, as the Hornet's.)
        if (kind() == 4u) { emit *= vec3f(0.55, 1.0, 0.62); }
        // (The Blackbird's in red, as the Phantom's.)
        if (kind() == 7u) { emit *= vec3f(1.0, 0.22, 0.12); }
      }
      if (pt == P_LINING && kind() == 1u) {
        // The flight deck's lining: matte, stippled plastic.
        rough = 0.8 + 0.08 * fine.b;
        spec_k = 0.45;
      }
      if (pt == P_PANEL && kind() == 1u) {
        // Boeing's grey-brown: a fine texture, scuffs and grime where hands go.
        rough = 0.55 + 0.15 * fine.g;
        albedo *= 0.92 + 0.12 * nz(L * 37.0).r;
        albedo = mix(albedo, albedo * 0.7, smoothstep(0.62, 0.85, nz(L * 6.0).g) * 0.5);
      }
      if (pt == P_PANEL && fighter()) {
        // Black lacquered panels: satin, dusty where they face up, rubbed shiny here and there.
        rough = 0.45 + 0.12 * fine.g - 0.12 * smoothstep(0.75, 0.92, nz(L * 23.0).r);
        albedo *= 0.94 + 0.12 * nz(L * 41.0).r;
        albedo = mix(albedo, vec3f(0.055, 0.054, 0.052), smoothstep(0.55, 0.85, nz(L * 13.0).g) * sat(in.Nl.y * 2.0) * 0.18);
      }
      if (pt == P_PLACARD) {
        let t = fract(in.uv * vec2f(220.0, 340.0));
        let txt = step(0.3, t.y) * step(t.y, 0.72) * step(0.45, nz(vec3f(in.uv * 300.0, 0.5)).r);
        albedo = mix(albedo, vec3f(0.02), txt * 0.8 * (1.0 - smoothstep(0.0003, 0.001, px)));
      }
    }
    case 6u: { // fabric: rough with a soft sheen at grazing angles
      rough = 0.95;
      spec_k = 0.15;
      albedo *= cav * (0.9 + 0.2 * fine.r);
      if (pt == P_MESH) {
        // Olive mesh: the holes dark, the weave faded by the sun unevenly.
        albedo *= (1.0 - rel.y * 0.55) * (0.85 + 0.3 * nz(L * 3.7).g);
        albedo = mix(albedo, albedo * vec3f(1.15, 1.1, 1.25), smoothstep(0.5, 0.8, nz(L * 1.3).r) * 0.5);
      }
      if (pt == P_WEBBING) {
        albedo *= (1.0 - rel.y * 0.4) * (0.9 + 0.2 * nz(L * 9.0).r);
        spec_k = 0.3;
      }
      if (pt == P_FLEECE) {
        // Sheepskin: tufts of wool, lighter at their tips.
        let tuft = nz(L * 70.0);
        albedo *= 0.65 + 0.6 * tuft.r * tuft.g + 0.25 * nz(L * 9.0).b;
        ao *= 0.75 + 0.25 * tuft.g;
      }
      let ndv = max(dot(N, -V), 0.0);
      albedo *= 1.0 + 0.35 * pow(1.0 - ndv, 3.0);
    }
    case 8u: { // light lens
      rough = 0.08;
      let ch = u32(in.mat.w + 0.5);
      let P4 = O.params[4];
      let P5 = O.params[5];
      var on = 0.0;
      var k = 0.0;
      switch ch {
        case 0u, 1u, 2u: { on = step(999.0, P5.x); k = mix(0.08, 0.9, night); }
        case 3u: { on = P5.y; k = mix(3.0, 3.5, night); }
        case 4u: { on = P5.z; k = mix(0.5, 1.2, night); }
        case 5u: { on = P5.w; k = mix(0.3, 2.0, night); }
        case 6u: { on = step(0.999, P4.z) * select(1.0, G.cockpit[2].z, kind() == 0u || kind() == 9u); k = mix(0.05, 0.01, night); }
        case 7u: { on = step(P4.z, 0.001) * select(1.0, G.cockpit[2].z, kind() == 0u || kind() == 9u); k = mix(0.05, 0.01, night); }
        // Formation light strips: pale panels by day, a soft green glow with the navigation lights at night.
        case 9u: { on = step(999.0, P5.x); k = 0.012 * night; }
        default: { on = 1.0; k = 0.004; }
      }
      emit = in.col.rgb * on * k * G.sun_dir.w;
      // Not a flat glowing block: the bulb sits behind a moulded lens, so
      // the light is hottest (and burns towards white) where the glass faces
      // the eye, falls off towards its edges, and the lens's prisms break
      // it into fine bright and dark ribs.
      if (ch <= 5u) {
        let fv = abs(dot(normalize(in.N), V));
        let ribs = 0.6 + 0.8 * nz(L * 140.0).r * (0.5 + 0.5 * sin(dot(L, vec3f(310.0, 0.0, 190.0))));
        let core = pow(fv, 6.0);
        emit *= (0.12 + 0.88 * pow(fv, 2.0)) * ribs;
        emit = mix(emit, vec3f(luma(emit)) * 1.2, core * 0.6);
      }
      // (A landing light's lens is dark seen from behind, from the cabin.)
      if (ch == 5u) { emit *= (0.6 + 0.4 * smoothstep(1.0, 0.0, length(in.uv))) * select(1.0, 0.0, back); }
      albedo = in.col.rgb * 0.15;
      if (ch == 9u) { albedo = vec3f(0.40, 0.43, 0.27); rough = 0.4; }
      ssr = 0.5;
    }
    case 9u: { // instrument face under glass
      if (INSTR != 0u) {
        let g = gauge(u32(in.mat.w + 0.5), in.uv, max(duv * 1.2, 0.004));
        albedo = g.rgb;
        rough = 0.04;
        spec_k = 0.5;
        emit = g.rgb * g.a * G.sun_dir.w * 0.0035 * night;
      }
    }
    case 10u: { // matte black: scuffed
      rough = 0.65 + 0.2 * fine.g;
      albedo *= 0.8 + 0.5 * smoothstep(0.6, 0.9, fine.a);
      spec_k = 0.5;
      if (kind() == 5u && O.glow.z > 0.01 && O.glow.x < 0.001) {
        // The rocket's heat shield coming back through the air: the tiles
        // glow, most where they face it, dull red going to orange-white.
        let hot = O.glow.z * (0.35 + 0.65 * sat(-in.Nl.y)) * (0.8 + 0.4 * fine.g);
        emit = mix(vec3f(1.0, 0.14, 0.02), vec3f(1.0, 0.62, 0.30), sat(hot * hot)) * hot * hot * G.sun_dir.w * mix(0.06, 0.012, night);
      }
    }
    case 11u: { // exhaust: heat-tinted, sooted at the outlet
      metal = 0.75;
      if (fighter() || kind() == 6u) {
        // A fighter's nozzles (and Concorde's reheated ones). in.mat.w: 0 the petals outside, 1 inside the
        // nozzle, 2 the flame holder at the bottom of it.
        let part = u32(in.mat.w + 0.5);
        let nzl = nozzle_at(L);
        let nl = select(nozzle_len(), vec2f(1.4, 1.2), kind() == 6u);
        let s = sat((L.z - nzl.z + nl.x) / nl.x);  // 0 at the shroud .. 1 at the lip
        if (part == 0u) {
          // Titanium petals: straw and blue heat tints toward the lip, soot streaked along them.
          rough = 0.38 + 0.2 * fine.g;
          metal = 0.9;
          let streak = nz(vec3f(atan2(L.y - nzl.y, abs(L.x) - nzl.x) * 2.4, 0.3, L.z * 0.6)).r;
          albedo = mix(vec3f(0.36, 0.34, 0.32), vec3f(0.30, 0.23, 0.16), smoothstep(0.05, 0.55, s));
          albedo = mix(albedo, vec3f(0.17, 0.165, 0.20), smoothstep(0.5, 0.9, s) * 0.7);
          albedo *= (0.55 + 0.6 * streak) * (0.8 + 0.3 * fine.r);
          albedo = mix(albedo, vec3f(0.03), smoothstep(0.93, 1.0, s) * 0.7);
          if (kind() == 4u) {
            // The Raptor's flat nozzle flaps: dark coated metal, heat-tinted toward their sawtooth edges, streaked fore and aft.
            let st = nz(vec3f(L.x * 3.0, 0.3, L.z * 0.5)).r;
            albedo = mix(vec3f(0.115, 0.118, 0.125), vec3f(0.17, 0.14, 0.11), smoothstep(0.3, 0.9, s)) * (0.7 + 0.5 * st) * (0.85 + 0.3 * fine.r);
            rough = 0.45 + 0.2 * fine.g;
            metal = 0.8;
          }
        } else {
          // Sooted black inside; glowing when the burners are lit, and a dull
          // red from the turbine at full dry power, deep in.
          rough = 0.85;
          metal = 0.2;
          albedo = vec3f(0.035, 0.032, 0.03) * (0.6 + 0.8 * fine.g);
          let deep = sat((nzl.z - L.z) / nl.y);
          let fl = 0.8 + 0.4 * nz(vec3f(L.xy * 4.0, G.time.x * 7.0)).r;
          let dry = dry_power();
          var heat = burner() * fl * mix(0.35, 1.3, deep) + dry * dry * 0.03 * deep;
          if (part == 2u) { heat = burner() * fl * 2.4 + dry * dry * 0.07; }
          emit = mix(vec3f(1.0, 0.30, 0.06), vec3f(1.0, 0.72, 0.38), sat(heat * 0.6)) * heat * G.sun_dir.w * mix(0.08, 0.014, night);
          ao = max(ao, 0.3);
        }
        break;
      }
      if (kind() == 5u) {
        // A rocket engine's bell (in.mat.w: 0 outside, 1 inside, 2 the injector
        // at its throat): dark, heat-stained metal; inside it the flame when
        // its stage is burning.
        let part = u32(in.mat.w + 0.5);
        rough = 0.5 + 0.2 * fine.g;
        albedo = mix(vec3f(0.20, 0.17, 0.14), vec3f(0.09, 0.09, 0.10), sat(fine.r * 0.7 + 0.15)) * (0.7 + 0.5 * ns.g);
        if (part > 0u) {
          albedo = vec3f(0.04, 0.036, 0.034) * (0.6 + 0.8 * fine.g);
          rough = 0.85;
          metal = 0.2;
          let heat = rocket_lit(L) * rocket_bell(L) * (0.8 + 0.4 * nz(vec3f(L.xy * 2.0, G.time.x * 7.0)).r) * select(1.0, 2.2, part == 2u);
          emit = mix(vec3f(0.55, 0.50, 1.0), vec3f(0.90, 0.92, 1.0), sat(heat * 0.5)) * heat * G.sun_dir.w * mix(0.08, 0.014, night);
          ao = max(ao, 0.3);
        }
        break;
      }
      if (kind() == 1u) {
        // A turbofan's nozzle and plug: heat-blued steel, darker toward the outlet.
        rough = 0.4;
        albedo = mix(vec3f(0.36, 0.33, 0.30), vec3f(0.10, 0.09, 0.10), sat(fine.r * 0.6 + 0.2)) * (0.75 + 0.4 * ns.g);
        albedo = mix(albedo, vec3f(0.30, 0.17, 0.09), smoothstep(0.55, 0.8, ns.b) * 0.5);
        break;
      }
      rough = 0.45;
      let t = sat((L.y + 0.45) / -0.3);
      albedo = mix(vec3f(0.42, 0.33, 0.20), vec3f(0.18, 0.16, 0.20), t) * (0.6 + 0.6 * fine.r);
      albedo = mix(albedo, vec3f(0.35, 0.18, 0.08), smoothstep(0.55, 0.8, ns.b) * 0.6);
      albedo = mix(albedo, vec3f(0.02), smoothstep(-2.2, -2.11, L.z));
    }
    case 12u: { // screw head: a cross recess, paint worn off the rim
      let q = in.uv;
      let cr = min(max(abs(q.x) - 0.15, abs(q.y) - 0.62), max(abs(q.y) - 0.15, abs(q.x) - 0.62));
      let slot = 1.0 - smoothstep(-0.05, 0.05, cr);
      let kind = in.mat.w;
      metal = select(1.0, select(0.0, 0.4, kind > 1.5), kind > 0.5);
      rough = select(0.3, select(0.18, 0.5, kind > 1.5), kind > 0.5);
      if (kind > 0.5 && kind < 1.5) {
        let worn = smoothstep(0.6, 0.85, length(q)) * smoothstep(0.4, 0.7, fine.g) + slot * 0.6;
        albedo = mix(albedo, vec3f(0.5, 0.5, 0.52), sat(worn));
        metal = sat(worn);
      }
      albedo *= 1.0 - slot * 0.9;
      ao *= 1.0 - slot * 0.7;
    }
    case 13u: { // propeller: grey face, black back, painted tips, eroded edge
      let r = in.uv.x;
      let front = in.uv.y > 0.0;
      albedo = select(vec3f(0.018), vec3f(0.30, 0.30, 0.31), front);
      rough = select(0.75, 0.4, front);
      let tip = smoothstep(0.864, 0.866, r);
      let band = smoothstep(0.865, 0.867, r) * smoothstep(0.885, 0.883, r);
      albedo = mix(albedo, vec3f(0.78, 0.78, 0.74), tip);
      albedo = mix(albedo, vec3f(0.5, 0.03, 0.02), band);
      let ero = smoothstep(0.5, 0.95, in.wear) * smoothstep(0.35, 0.9, r) * smoothstep(0.4, 0.7, fine.r);
      albedo = mix(albedo, vec3f(0.6, 0.6, 0.62), ero);
      metal = ero;
    }
    case 14u: { // backlit display under glass
      if (INSTR == 0u) { break; }
      albedo = vec3f(0.01);
      rough = 0.05;
      let did = u32(in.mat.w + 0.5);
      if (did >= 60u) {
        // The G650's PlaneView displays, SMCs, MCDUs and readouts (deck650.wgsl).
        emit = display650(did, in.uv, duv) * G.sun_dir.w * mix(0.07, 0.01, night);
        albedo = vec3f(0.010, 0.011, 0.013);
        rough = 0.1;
        ssr = 0.3;
      } else if (did >= 40u) {
        // The Hornet's DDIs, AMPCD, UFC windows and IFEI (pit18.wgsl).
        emit = display18(did, in.uv, duv) * G.sun_dir.w * mix(0.06, 0.009, night);
        albedo = vec3f(0.010, 0.012, 0.012);
        rough = 0.1;
        ssr = 0.3;
      } else if (did >= 20u) {
        // The 747's CRTs: bright enough by day, turned down at night.
        emit = display747(did, in.uv, duv) * G.sun_dir.w * mix(0.07, 0.01, night);
        albedo = vec3f(0.012, 0.014, 0.016);
        rough = 0.12;
        ssr = 0.3;
      } else {
      emit = display(did, in.uv, max(duv * 0.012, 0.00015)) * G.sun_dir.w * mix(0.09, 0.012, night) * G.cockpit[2].z;
      }
    }
    case 18u: { // a downloaded aircraft's surface: its texture times its material colour
      albedo = tex.rgb * in.col.rgb;
      ao = 1.0;
      let r = in.mat.z;
      let glow = r >= 1.5;
      rough = clamp(select(r, r - 2.0, glow), 0.08, 1.0);
      if (glow) { emit = albedo * G.sun_dir.w * mix(0.02, 0.004, 1.0 - night); }
      ssr = select(0.0, 0.25, rough < 0.3);
    }
    default: {}
  }

  // The 747's skin seen from the flight deck (slivers at the window cut-outs): insulation blanket.
  if (kind() == 1u && back && pt == P_FUSE && L.z < -22.0 && !torn()) {
    albedo = vec3f(0.035, 0.034, 0.032);
    metal = 0.0;
    rough = 0.9;
    ssr = 0.0;
  }
  // Damage. The inside of a torn skin: primer, stringers and ribs, no paint.
  let exterior = pt == P_WING || pt == P_TAIL || pt == P_CTRL || pt == P_FUSE || pt == P_COWL;
  if (torn() && back && exterior) {
    var inner = vec3f(0.30, 0.36, 0.24) * (0.7 + 0.5 * fine.g);
    let rib = line((fract((abs(L.x) - 0.62) / 0.305 + 0.5) - 0.5) * 0.305, 0.012);
    let str_ = line((fract(L.y / 0.09 + 0.5) - 0.5) * 0.09, 0.006) * select(0.0, 1.0, pt == P_FUSE);
    inner *= 1.0 - 0.5 * max(rib, str_);
    albedo = inner;
    metal = 0.0;
    rough = 0.6;
    spec_k = 0.5;
    ssr = 0.0;
    emit = vec3f(0.0);
    // Open to the sky through the break: more light gets in than the bake knew.
    ao = max(ao, 0.55);
  }
  // The torn edge: bare, crumpled metal curling back from the break.
  if (torn()) {
    let rip = smoothstep(-0.05, -0.004, td);
    let shred = nz(L * 40.0).r;
    albedo = mix(albedo, vec3f(0.36, 0.37, 0.39) * (0.5 + 0.8 * shred), rip);
    metal = mix(metal, 0.85, rip);
    rough = mix(rough, 0.5, rip);
    N = normalize(N + (nz(L * 23.0).gba - 0.5) * rip * 1.2);
    ao *= 1.0 - 0.35 * rip;
  }
  // Scrapes and dents from trees, roofs and the ground, streaked back along
  // the airflow; green smears where foliage went over the leading edges.
  if (O.dmg.z > 0.0 && exterior && (id == M_LIVERY || id == M_PAINT || id == M_SATIN)) {
    let scuff = O.dmg.z;
    // Where it gets knocked: leading edges, the belly, and patches at random.
    let where_ = sat(in.wear * 1.5 + sat(-in.Nl.y) * 0.5) * 0.6 + 0.6 * smoothstep(0.55, 0.8, nz(L * 1.7).a);
    let sc = nz(vec3f(L.x * 9.0, L.y * 9.0, L.z * 1.3));
    let scratch = smoothstep(0.84 - 0.2 * scuff * where_, 0.9 - 0.2 * scuff * where_, sc.r) * scuff * where_;
    albedo = mix(albedo, vec3f(0.5, 0.5, 0.52), scratch * 0.8);
    metal = max(metal, scratch * 0.7);
    rough = mix(rough, 0.4, scratch);
    let sap = smoothstep(0.4, 0.95, in.wear) * scuff * smoothstep(0.45, 0.7, sc.b);
    albedo = mix(albedo, vec3f(0.12, 0.16, 0.05), sap * 0.5);
    // Dents.
    N = normalize(N + (nz(L * 4.0).gba - 0.5) * 0.3 * scuff * where_ * smoothstep(0.5, 0.75, nz(L * 2.3).r));
  }

  // Wreckage: soot and burn-through.
  let cn = nz(L * 0.35 + O.glow.w);
  let char = clamp(O.tint.a * 1.6 - cn.g * 0.7, 0.0, 1.0);
  albedo = mix(albedo, vec3f(0.015, 0.013, 0.012), char);
  rough = mix(rough, 0.9, char);
  metal *= 1.0 - char;
  ssr *= 1.0 - char;

  // Light: sun (with the aircraft's own fine shadow), sky, ground bounce, moon.
  let sh = shadow_at(in.P, Ng);
  let Ls = G.sun_dir.xyz;
  let ndl = max(dot(N, Ls), 0.0);
  let ndv = max(dot(N, -V), 1e-3);
  let H = normalize(Ls - V);
  let a = max(rough * rough, 0.002);
  let F0 = mix(vec3f(0.04), albedo, metal);
  let Fs = F0 + (1.0 - F0) * pow(1.0 - max(dot(-V, H), 0.0), 5.0);
  let spec = l.sun * ggx(max(dot(N, H), 0.0), a) * vis(ndv, ndl, a) * Fs * ndl * sh * spec_k;
  // Inside the cabin, light bounces around off every surface: a fill that the
  // occlusion term alone would miss.
  var inside = step(abs(L.x), 0.58) * step(-1.42, L.z) * step(L.z, 1.5) * step(L.y, 0.62) * select(0.0, 1.0, id != M_LIVERY && pt != P_FUSE && pt != P_COWL);
  if (kind() == 1u) { inside = step(abs(L.x), 2.2) * step(-29.0, L.z) * step(L.z, -22.4) * step(1.9, L.y) * select(0.0, 1.0, id != M_LIVERY && pt != P_FUSE); }
  // The Hornet's cockpit tub, under its canopy.
  if (kind() == 2u) { inside = step(abs(L.x), 0.56) * step(-6.2, L.z) * step(L.z, -3.1) * step(0.0, L.y) * select(0.0, 1.0, pt != P_HORNET); }
  if (kind() == 3u) { inside = step(abs(L.x), F4_TUB.x) * step(F4_TUB.z, L.z) * step(L.z, F4_TUB.w) * step(F4_TUB.y, L.y) * select(0.0, 1.0, pt != P_HORNET); }
  if (kind() == 4u) { inside = step(abs(L.x), F22_TUB.x) * step(F22_TUB.z, L.z) * step(L.z, F22_TUB.w) * step(F22_TUB.y, L.y) * select(0.0, 1.0, pt != P_HORNET); }
  if (kind() == 6u) { inside = step(abs(L.x), CONCORDE_TUB.x) * step(CONCORDE_TUB.z, L.z) * step(L.z, CONCORDE_TUB.w) * step(CONCORDE_TUB.y, L.y) * select(0.0, 1.0, pt != P_HORNET); }
  if (kind() == 9u) { inside = step(abs(L.x), G650_TUB.x) * step(G650_TUB.z, L.z) * step(L.z, G650_TUB.w) * step(G650_TUB.y, L.y) * select(0.0, 1.0, pt != P_HORNET); }
  if (kind() == 8u) { inside = step(abs(L.x), 0.56) * step(-0.62, L.z) * step(L.z, 1.66) * step(L.y, 0.62) * select(0.0, 1.0, id != M_LIVERY && pt != P_FUSE && pt != P_COWL); }
  if (kind() == 10u) { inside = step(abs(L.x), 0.58) * step(-1.11, L.z) * step(L.z, 2.69) * step(-0.5, L.y) * step(L.y, 0.78) * select(0.0, 1.0, id != M_LIVERY && pt != P_FUSE && pt != P_COWL && pt != P_WING); }
  if (kind() == 7u) { inside = step(abs(L.x), SR71_TUB.x) * step(SR71_TUB.z, L.z) * step(L.z, SR71_TUB.w) * step(SR71_TUB.y, L.y) * select(0.0, 1.0, pt != P_HORNET); }
  // (a downloaded aircraft has no cabin of ours where its own model is)
  if (id == M_TEX) { inside = 0.0; }
  // (The baked occlusion already counts only the sky seen through the
  // windows; the eye adapts to the dimmer cabin, main.rs / cockpit.rs.)
  let amb_ao = mix(ao * ao * (3.0 - 2.0 * ao), 0.06 + 0.94 * ao, inside);
  let fill = (l.amb * 0.2 + l.sun * max(dot(l.upn, Ls), 0.0) * 0.03) * (0.25 + 0.75 * ao) * inside;
  let bounce = l.sun * max(dot(l.upn, Ls), 0.0) * 0.05 * sat(0.5 - 0.5 * dot(N, l.upn)) + l.sun * max(dot(l.upn, Ls), 0.0) * 0.025;
  let moon = moon_light(l.upn, l.h) * max(dot(N, G.moon.xyz), 0.0);
  let diffuse = albedo * (1.0 - metal) * ((l.sun * ndl * sh + moon) / PI + (l.amb * (0.55 + 0.45 * dot(N, l.upn)) + bounce) * amb_ao + fill + fire_light(in.P, N));
  let R = reflect(V, N);
  let env_sharp = env_reflect(in.P, R, l.upn, false);
  let env_soft = sky_lut(normalize(mix(R, l.upn, 0.5))) * 0.9;
  let Fe = F0 + (max(vec3f(1.0 - rough), F0) - F0) * pow(1.0 - ndv, 5.0);
  let spec_occ = sat(pow(ndv + ao, 2.0) - 1.0 + ao) * ao;
  let envw = Fe * spec_occ * spec_k;
  var c = diffuse + spec + mix(env_sharp, env_soft, sat(rough * 1.6)) * envw;
  // The cabin has its own occlusion baked in; only the outside takes the screen-space kind.
  g_amb = albedo * (1.0 - metal) * (l.amb * (0.55 + 0.45 * dot(N, l.upn)) + bounce) * amb_ao * (1.0 - inside);
  // Instrument flood lighting at night, warm and dim, inside the cabin:
  // enough to make out the panel and glareshield (from outside too, through
  // the windows), still well under the displays.
  c += albedo * vec3f(1.0, 0.62, 0.38) * G.sun_dir.w * 0.0045 * night * inside * ao;
  c += emit;
  // Embers along the charred edges while it burns.
  let ember = smoothstep(0.45, 0.7, cn.b) * smoothstep(0.2, 0.6, char) * O.glow.x;
  c += vec3f(1.0, 0.28, 0.04) * ember * G.sun_dir.w * 1.5;
  // Screen-space reflections (in the composite) only where they would show.
  let rw = dot(envw, vec3f(0.333));
  let reflect_w = select(0.0, rw, ssr > 0.0 && rough < 0.3 && rw > 0.09) * ssr;
  return Shaded(vec4f(c, 1.0), vec4f(N, -reflect_w), select(0.0, 1.0, kill), amb_share(c));
}

// ---------------------------------------------------------------- rain on the glass

// Rain on the windows (rain.rs): G.start.y is the rain falling (0..1), .z how
// wet the glass is (0..1), .w the airspeed over it (kt indicated). A window
// piece carries the phases of its water: glow.z the beads' turnover
// (lifetimes), glow.w how far drops have run down (m), tint.w how far the
// airflow has driven them (m). The water is on the outside: from the cabin
// each drop is a tiny lens that shows the scene behind it upside down and
// shrunk, ringed by a dark band where the light is reflected back inside it.

// How hard the airflow drags a drop against the surface tension pinning it,
// and how fast it then drives the drops (m/s); match rain.rs.
fn rain_mob(kt: f32) -> f32 { return smoothstep(8.0, 40.0, kt); }
fn rain_flow_speed(kt: f32) -> f32 { let s = kt / 60.0; return min(rain_mob(kt) * 0.25 * s * s, 6.0); }
// An integer hash of a cell (exact at any phase, unlike the sine hash).
fn rh(c: vec2f, s: u32) -> f32 {
  var x = bitcast<u32>(i32(c.x)) * 1664525u + bitcast<u32>(i32(c.y)) * 2246822519u + s * 3266489917u;
  x ^= x >> 15u; x *= 2654435761u; x ^= x >> 13u; x *= 1274126177u; x ^= x >> 16u;
  return f32(x >> 8u) * (1.0 / 16777216.0);
}

// Water over a point of the glass: height above it (m), its slope along the
// window's two axes, and how much of the pixel a drop covers.
struct Water { h: f32, g: vec2f, cov: f32, f: f32 };

// A drop sitting on the glass: a spherical cap of footprint radius r meeting
// it at a contact angle with sine sc. q is the offset from its middle, y along
// the way it moves; e stretches it that way (running drops elongate, and a
// fast one smears over a frame), taper narrows it behind into a tail, k
// flattens it (a film-thin streak), fade is the share of the frame it is
// there for (a smeared drop only passes over a point for part of it).
fn rain_cap(w: ptr<function, Water>, q0: vec2f, r: f32, e: f32, taper: f32, sc: f32, k: f32, px: f32, fade: f32) {
  let sx = 1.0 + taper * max(-q0.y, 0.0) / r;
  let q = vec2f(q0.x * sx, q0.y / e);
  let rho2 = dot(q, q);
  let rp = r + px;
  if (rho2 >= rp * rp) { return; }
  let rho = sqrt(rho2);
  let R = r / sc;
  let cc = sqrt(1.0 - sc * sc);
  let s = sqrt(max(R * R - min(rho2, r * r), 1e-12));
  let h = max(s - R * cc, 0.0) * k;
  // Footprint edge, a pixel wide (the cap's own slope is steepest there).
  let cov = sat((r - rho) / max(px * 0.75, 1e-6) + 0.5) * fade;
  if (h > (*w).h || ((*w).h == 0.0 && cov > (*w).cov)) {
    let g = -q / s * k;
    (*w).h = h;
    (*w).g = vec2f(g.x * sx, g.y / e);
  }
  (*w).cov = max((*w).cov, cov);
}

// Drops running along a (across c) at speed set by phase, one at most per
// cell of a column, each with the trail it leaves: a thin rivulet narrowing
// behind it and the beads it strands (fixed to the glass, so they stay put
// as the drop moves on). Columns slip at their own pace, in fits and starts
// (stick) where the drag barely beats the pinning.
fn rain_run(w: ptr<function, Water>, a: f32, c: f32, cw: f32, cl: f32, phase: f32, r0: f32, dens: f32, e: f32, trail: f32, stick: f32, k: f32, px: f32, seed: u32) {
  let col = floor(c / cw);
  let hs = rh(vec2f(col, 0.0), seed);
  let fq = 6.2831853 / (cl * (0.6 + 0.8 * rh(vec2f(col, 1.0), seed)));
  let off = (0.55 + 0.9 * hs) * (phase + stick * 0.85 / fq * sin(fq * phase + 6.2831853 * hs));
  let as_ = a - off;
  let row = floor(as_ / cl);
  let kc = vec2f(col, row);
  if (rh(kc, seed + 1u) > dens) { return; }
  let r = r0 * (0.6 + 0.8 * rh(kc, seed + 2u));
  // The drop near the head of its cell, so its trail stays inside it.
  let ad = (row + 0.72 + 0.2 * rh(kc, seed + 3u)) * cl + off;
  // The path it wanders down, a function of where along the glass it is.
  let amp = max(cw * 0.5 - r * 1.6, 0.0) * 0.6;
  let h1 = rh(kc, seed + 4u) * 6.2831853;
  let wob = sin(a * 31.0 + h1) * 0.6 + sin(a * 83.0 + h1 * 2.3) * 0.4;
  let xc = (col + 0.5) * cw + amp * wob;
  let x = c - xc;
  let t = ad - a;
  if (abs(x) > cw * 0.5 || t < -r * e * 1.5 || t > trail + r) { return; }
  // (Smeared once e is mostly motion blur: fainter, as it passes in part of a frame.)
  let fade = min(1.0, sqrt(2.5 / e));
  rain_cap(w, vec2f(x, -t), r, e, 0.35, 0.82, k, px, fade);
  if (t > r * 0.5 && trail > 0.0) {
    // The rivulet: a thin cylindrical bead of water (meeting the glass at
    // some 35 degrees), narrowing to nothing.
    let f = sat((t - r * 0.5) / trail);
    let wt = r * 0.32 * sqrt(1.0 - f);
    if (abs(x) < wt + px) {
      let R = wt / 0.57;
      let xm = min(abs(x), wt);
      let s = sqrt(max(R * R - xm * xm, 1e-12));
      let h = max(s - R * 0.82, 0.0) * k;
      let cov = sat((wt - abs(x)) / max(px * 0.75, 1e-6) + 0.5) * (1.0 - f * f) * fade;
      if (h > (*w).h || ((*w).h == 0.0 && cov > (*w).cov)) { (*w).h = h; (*w).g = vec2f(-sign(x) * xm / s * k, 0.0); }
      (*w).cov = max((*w).cov, cov);
    }
    // Stranded beads, every few millimetres along the glass.
    let ds = r * 1.3;
    let bi = floor(a / ds);
    let kb = vec2f(col, bi);
    if (rh(kb, seed + 5u) < 0.55) {
      let ab = (bi + 0.5) * ds;
      let tb = ad - ab;
      if (tb > r && tb < trail * (0.6 + 0.4 * rh(kb, seed + 6u))) {
        let rb = r * (0.18 + 0.22 * rh(kb, seed + 7u));
        let xb = (col + 0.5) * cw + amp * (sin(ab * 31.0 + h1) * 0.6 + sin(ab * 83.0 + h1 * 2.3) * 0.4) + (rh(kb, seed + 8u) - 0.5) * r * 0.6;
        rain_cap(w, vec2f(c - xb, a - ab), rb, 1.0, 0.0, 0.85, k, px, 1.0);
      }
    }
  }
}

// Beads that land and sit, one at most per cell, each living part of every
// turnover before it merges away and another lands elsewhere in the cell.
fn rain_beads(w: ptr<function, Water>, p: vec2f, cs: f32, rmin: f32, rmax: f32, dens: f32, phase: f32, px: f32, seed: u32) {
  let c = floor(p / cs);
  let ph = phase * (0.6 + 0.8 * rh(c, seed)) + rh(c, seed + 1u);
  let kc = c + vec2f(floor(ph) * 7.0, 0.0);
  if (rh(kc, seed + 2u) > dens) { return; }
  let fr = fract(ph);
  let r = mix(rmin, rmax, pow(rh(kc, seed + 3u), 2.2)) * smoothstep(0.0, 0.02, fr) * (1.0 - smoothstep(0.96, 1.0, fr));
  if (r <= 0.0) { return; }
  let m = rmax / cs;
  let o = (c + 0.5 + (vec2f(rh(kc, seed + 4u), rh(kc, seed + 5u)) - 0.5) * (1.0 - 2.0 * m)) * cs;
  // Bigger beads sag a little and sit flatter.
  let sc = mix(0.88, 0.72, sat((r - 0.0012) / 0.002));
  rain_cap(w, p - o, r, 1.0, 0.0, sc, 1.0, px, 1.0);
}

// Folds one layer's water (slopes along its own across/along axes, given as
// directions in the window's axes) into the total, the higher surface winning.
fn rain_merge(w: ptr<function, Water>, s: Water, dc: vec2f, da: vec2f) {
  if (s.h > (*w).h || ((*w).h == 0.0 && s.cov > (*w).cov)) {
    (*w).h = s.h;
    (*w).g = s.g.x * dc + s.g.y * da;
  }
  (*w).cov = max((*w).cov, s.cov);
}

// All the water on one window, in its own axes p (m): the airflow runs along
// fdir over it (unit, in those axes; up a windshield, aft along the side),
// gravity toward -y. gt: how steeply the glass leans (1 = upright), edge: 1
// at the frame where the air is slow and beads cling. Returns the film's
// share in .f.
fn rain_water(p: vec2f, fdir: vec2f, gt: f32, edge: f32, px: f32) -> Water {
  var w = Water(0.0, vec2f(0.0), 0.0, 0.0);
  let rain = G.start.y;
  let wet = G.start.z;
  let kt = G.start.w;
  let mob = rain_mob(kt);
  let vs = rain_flow_speed(kt);
  let fast = smoothstep(55.0, 170.0, kt);
  // Patchy, as rain on glass always is.
  let patchy = 0.65 + 0.7 * nz(vec3f(p * 0.9, 0.37)).r;
  // Beads: swept off once the drag beats the pinning, except in the slow air along the frame.
  let keep = max(1.0 - smoothstep(18.0, 110.0, kt), edge * 0.8 * (1.0 - 0.7 * fast));
  let bd = sat(wet * 1.25) * keep * patchy;
  if (bd > 0.0) {
    rain_beads(&w, p, 0.0072, 0.0007, 0.0021, bd * 0.85, O.glow.z, px, 11u);
    rain_beads(&w, p + vec2f(0.0041, 0.0023), 0.0145, 0.0015, 0.0044, bd * 0.6, O.glow.z * 0.7, px, 23u);
  }
  // Big drops running down while the airflow is weak (and the glass leans enough).
  let gd = sat(wet * 1.4 - 0.25) * (1.0 - mob) * smoothstep(0.25, 0.6, gt) * patchy;
  if (gd > 0.0) {
    var s = Water(0.0, vec2f(0.0), 0.0, 0.0);
    rain_run(&s, -p.y, p.x, 0.032, 0.17, O.glow.w, 0.004, gd * 0.55, 1.25, 0.10, 1.0, 1.0, px, 37u);
    rain_merge(&w, s, vec2f(1.0, 0.0), vec2f(0.0, -1.0));
  }
  // Driven by the airflow: up and back over a windshield, aft along the side.
  // (Where the glass curves, so does the flow, and the streams with it.)
  let da = fdir;
  let dc = vec2f(da.y, -da.x);
  let fa = dot(p, da);
  let fc = dot(p, dc);
  // Smeared over a frame (1/60 s) once they move fast.
  let blur = vs * 0.016;
  let fd = sat(max(wet * 1.3, rain * 0.6)) * mob * (1.0 - fast * 0.85) * patchy;
  if (fd > 0.0) {
    let r = 0.0024;
    var s = Water(0.0, vec2f(0.0), 0.0, 0.0);
    rain_run(&s, fa, fc, 0.021, 0.13, O.tint.w, r, fd * 0.6, 1.15 + blur / r, 0.05 + 0.04 * mob, 1.0 - mob * 0.7, 1.0, px, 53u);
    rain_merge(&w, s, dc, da);
  }
  // Fast flight: fresh drops torn into streaks that race over the glass.
  let sd = sat(rain * 0.9 + wet * 0.5) * fast;
  if (sd > 0.0) {
    let r = 0.0011;
    var s = Water(0.0, vec2f(0.0), 0.0, 0.0);
    rain_run(&s, fa, fc, 0.0075, 0.24, O.tint.w * 1.3, r, sd * 0.3, min(1.6 + blur / r, 60.0), 0.16, 0.0, 0.25, px, 71u);
    rain_merge(&w, s, dc, da);
  }
  // The thin film the airflow leaves, rippled into streaks along the flow.
  let film = sat(wet * 1.5) * smoothstep(25.0, 140.0, kt);
  if (film > 0.0) {
    let n = nz(vec3f(fc * 9.0, (fa - O.tint.w * 0.8) * 0.45, 0.61));
    let m = nz(vec3f(fc * 31.0, (fa - O.tint.w) * 1.4, 0.13));
    let gf = ((n.gb - 0.5) * vec2f(0.05, 0.012) + (m.gb - 0.5) * vec2f(0.03, 0.006)) * film;
    w.g += gf.x * dc + gf.y * da;
    w.f = film;
  }
  return w;
}

// ---------------------------------------------------------------- transparent pass

// How far a window bends the view through it (radians along its two in-plane
// axes). Formed acrylic is never optically flat: broad lens-like swells, faint
// ripples drawn into the sheet, and worse where it is clamped at the frame and
// where the windshield is pulled round its corners.
fn glass_bend(g: vec2f, edge: f32, curve: f32) -> vec2f {
  let a = textureSampleLevel(t_wave, s_repeat, g * 0.8 + vec2f(0.31, 0.77), 0.0).rg * 2.0 - 1.0;
  let b = textureSampleLevel(t_wave, s_repeat, vec2f(g.x * 0.3, g.y * 2.4) + vec2f(0.61, 0.23), 0.0).rg * 2.0 - 1.0;
  let c = textureSampleLevel(t_wave, s_repeat, g * 2.7 + vec2f(0.13, 0.52), 0.0).rg * 2.0 - 1.0;
  let s = a * 0.005 + vec2f(b.x * 0.3, b.y) * 0.0026 + c * 0.0006;
  return s * (1.0 + 2.5 * edge + 1.5 * curve);
}
// What the inside of a window mirrors, seen from the cabin: the glareshield
// in the windshield (lit where the sun falls on it), elsewhere the dim cabin.
fn cabin_mirror(p0: vec3f, r: vec3f, l: Light, ls: vec3f) -> vec3f {
  var t = 0.01;
  for (var i = 0; i < 22; i++) {
    let p = p0 + r * t;
    let g = (GS_Z + 0.025 - p.z) / 0.46;
    if (g >= 0.0 && g <= 1.0 && abs(p.x) < 0.56 && p.y < 0.185 + 0.12 * g * g * g) {
      let up = normalize(O.model[1].xyz);
      let sg = shadow_at((O.model * vec4f(p, 1.0)).xyz, up);
      let weave = 0.75 + 0.5 * nz(p * 23.0).g;
      // Black fabric, a little dusty.
      return vec3f(0.055, 0.054, 0.056) * weave / PI * (l.sun * max(ls.y, 0.0) * sg + l.amb * 0.55);
    }
    if (p.y < 0.17 && p.z < GS_Z + 0.03) { break; }
    t += 0.03;
  }
  // Seats, lining, a shirt: lighter and darker patches as the eye moves.
  return l.amb * 0.035 * (0.5 + 1.0 * nz(r * 0.9 + vec3f(0.3, 0.1, 0.6)).r);
}
const GS_Z = -1.30; // cabin::PANEL_Z

// A downloaded aircraft's see-through surfaces (windows, propeller discs):
// its texture's alpha times its material's, lit, with the sky mirrored on top.
fn tex_glass(in: VOut) -> vec4f {
  let t = tex_at(in.uv, in.mat.w, dpdx(in.uv), dpdy(in.uv));
  let V = normalize(in.P);
  var N = normalize(in.N);
  if (dot(N, V) > 0.0) { N = -N; }
  let l = light_at(in.P);
  let sh = shadow_at(in.P, N);
  let Ls = G.sun_dir.xyz;
  let a = sat(t.a * in.col.a);
  let base = t.rgb * in.col.rgb;
  let lit = base * (l.sun * max(dot(N, Ls), 0.0) * sh / PI + l.amb * (0.55 + 0.45 * dot(N, l.upn)));
  let ndv = max(dot(N, -V), 0.0);
  let F = 0.04 + 0.96 * pow(1.0 - ndv, 5.0);
  let R = reflect(V, N);
  // (a window, whose material is see-through, reflects; a texture's cut-outs, like a propeller's blur, don't)
  let k = select(0.0, F * (1.0 - a * 0.8), in.col.a < 0.99);
  let H = normalize(Ls - V);
  let glint = l.sun * ggx(max(dot(N, H), 0.0), 0.004) * vis(ndv, max(dot(N, Ls), 0.0), 0.004) * F * max(dot(N, Ls), 0.0) * sh;
  return vec4f(lit * a + env_reflect(in.P, R, l.upn, false) * k + glint * k, sat(a + k));
}

@fragment fn fs_glass(in: VOut) -> @location(0) vec4f {
  let px = max(length(dpdx(in.P)), length(dpdy(in.P)));
  let id = u32(in.mat.x + 0.5);
  if (id == M_PLUME) { return plume(in); }
  if (id == M_TEX) { return tex_glass(in); }
  let V = normalize(in.P);
  var N = normalize(in.N);
  let from_inside = dot(N, V) > 0.0;
  if (from_inside) { N = -N; }
  let l = light_at(in.P);
  let sh = shadow_at(in.P, N);
  let Ls = G.sun_dir.xyz;
  if (id == M_PROPDISC) {
    // The blur of the spinning blades: their share of each ring, a slow
    // shutter beat, painted tips showing as a ring.
    let r = length(in.uv) * 0.975;
    if (r < 0.2 || r > 0.975) { discard; }
    let c = max(0.155 - 0.11 * pow((r - 0.55) / 0.42, 2.0), 0.05);
    var cover = 2.0 * c / (6.2831853 * r);
    let ang = atan2(in.L.y + 0.03, in.L.x);
    let beat = 0.65 + 0.35 * cos(2.0 * (ang - G.time.x * 1.9));
    let tip = smoothstep(0.86, 0.87, r);
    // One blade snapped: only the inner part of the disc is swept twice.
    if (torn()) { cover *= select(0.5, 1.0, r < 0.45); }
    let lit = l.sun * max(dot(N, Ls), 0.0) * sh / PI + l.amb * 0.8;
    let back = from_inside == false && dot(in.N, -V) > 0.0;
    var col = select(vec3f(0.30), vec3f(0.02), dot(in.N, V) < 0.0);
    col = mix(col, vec3f(0.78, 0.78, 0.74), tip);
    let alpha = sat(cover * beat * O.tint.x * (1.0 - smoothstep(0.955, 0.975, r)) * 1.3);
    let _b = back;
    return vec4f(col * lit * alpha, alpha);
  }
  // The Hornet's HUD combiner: the symbology, focused at infinity. (The
  // combiner's top is at 1.045; the canopy passes over it from 1.13 up.)
  if (kind() == 2u && in.L.y > 0.86 && in.L.y < 1.06 && abs(in.L.x) < 0.1 && in.L.z > -5.3 && in.L.z < -5.15) { return hud18(in, px); }
  let d = window(in.L);
  let cov = 1.0 - smoothstep(-px, px, d);
  if (cov <= 0.0) { discard; }
  let ndv = max(dot(N, -V), 0.0);
  let F = 0.04 + 0.96 * pow(1.0 - ndv, 5.0);
  let R = reflect(V, N);
  // The body frame (rigid): where the eye is, and the window's own axes.
  let ml = transpose(mat3x3f(O.model[0].xyz, O.model[1].xyz, O.model[2].xyz));
  let eye_l = -(ml * O.model[3].xyz);
  var in_cabin = from_inside && abs(eye_l.x) < 0.6 && eye_l.y > -0.5 && eye_l.y < 0.85 && eye_l.z > -1.3 && eye_l.z < 1.6;
  if (kind() == 1u) { in_cabin = from_inside && abs(eye_l.x) < 1.6 && eye_l.y > 2.2 && eye_l.y < 4.8 && eye_l.z > -28.5 && eye_l.z < -22.5; }
  if (kind() == 2u) { in_cabin = from_inside && abs(eye_l.x) < 0.6 && eye_l.y > 0.2 && eye_l.y < 1.5 && eye_l.z > -6.3 && eye_l.z < -2.6; }
  if (kind() == 3u) { in_cabin = from_inside && abs(eye_l.x) < F4_CABIN[0] && eye_l.y > F4_CABIN[1] && eye_l.y < F4_CABIN[2] && eye_l.z > F4_CABIN[3] && eye_l.z < F4_CABIN[4]; }
  if (kind() == 4u) { in_cabin = from_inside && abs(eye_l.x) < F22_CABIN[0] && eye_l.y > F22_CABIN[1] && eye_l.y < F22_CABIN[2] && eye_l.z > F22_CABIN[3] && eye_l.z < F22_CABIN[4]; }
  if (kind() == 6u) { in_cabin = from_inside && abs(eye_l.x) < CONCORDE_CABIN[0] && eye_l.y > CONCORDE_CABIN[1] && eye_l.y < CONCORDE_CABIN[2] && eye_l.z > CONCORDE_CABIN[3] && eye_l.z < CONCORDE_CABIN[4]; }
  if (kind() == 9u) { in_cabin = from_inside && abs(eye_l.x) < G650_CABIN[0] && eye_l.y > G650_CABIN[1] && eye_l.y < G650_CABIN[2] && eye_l.z > G650_CABIN[3] && eye_l.z < G650_CABIN[4]; }
  if (kind() == 8u) { in_cabin = from_inside && abs(eye_l.x) < 0.56 && eye_l.y > -0.52 && eye_l.y < 0.62 && eye_l.z > -0.62 && eye_l.z < 1.66; }
  if (kind() == 10u) { in_cabin = from_inside && abs(eye_l.x) < 0.58 && eye_l.y > -0.46 && eye_l.y < 0.78 && eye_l.z > -1.1 && eye_l.z < 2.7; }
  if (kind() == 7u) { in_cabin = from_inside && abs(eye_l.x) < SR71_CABIN[0] && eye_l.y > SR71_CABIN[1] && eye_l.y < SR71_CABIN[2] && eye_l.z > SR71_CABIN[3] && eye_l.z < SR71_CABIN[4]; }
  let wind = abs(in.Nl.x) < 0.5;
  let tb = select(vec3f(0.0, 0.0, 1.0), vec3f(1.0, 0.0, 0.0), wind);
  let bb = select(vec3f(0.0, 1.0, 0.0), vec3f(0.0, 0.454, 0.891), wind);
  let g2 = vec2f(dot(in.L, tb), dot(in.L, bb));
  // Looking out, the glass mostly mirrors the dim cabin.
  var env = l.amb * 0.035;
  if (!from_inside) { env = env_reflect(in.P, R, l.upn, true); }
  // A sheet has two faces, and from inside both mirror the cabin.
  // low preset skips the cabin reflection, rain spots and scratches.
  let lowq = G.quality.x > 1.5;
  if (in_cabin && !lowq) {
    env = select(cabin_mirror(in.L, ml * R, l, ml * Ls), deck_mirror(in.L, ml * R, l, ml * Ls), kind() == 1u) * 2.0 / (1.0 + F);
    if (fighter()) { env = pit_mirror(in.L, ml * R, l, ml * Ls) * 2.0 / (1.0 + F); }
  }
  let H = normalize(Ls - V);
  let ndl = max(dot(N, Ls), 0.0);
  let glint = l.sun * ggx(max(dot(N, H), 0.0), 0.02) * vis(max(ndv, 1e-3), ndl, 0.02) * ndl * sh * (0.04 + 0.96 * pow(1.0 - max(dot(-V, H), 0.0), 5.0));
  // Dust, smudges, grime along the edges and bugs low on the windshield; they
  // light up when you look toward the sun.
  let L = in.L;
  let n1 = nz(L * 2.3);
  let n2 = nz(L * 17.0);
  var haze = 0.012 + 0.03 * smoothstep(0.45, 0.85, n1.r) + 0.02 * smoothstep(0.6, 0.9, n2.g);
  haze += 0.06 * smoothstep(-0.05, 0.0, d);
  // Dried rain: rings of grime left where drops evaporated, in clusters.
  var spots = 0.0;
  let spot_cs = 0.016;
  let fine = select(1.0 - smoothstep(spot_cs * 0.06, spot_cs * 0.25, px), 0.0, lowq);
  if (fine > 0.0) {
    let cl = smoothstep(0.5, 0.78, nz(L * 1.6 + vec3f(0.4, 0.0, 0.2)).g);
    if (cl > 0.0) {
      let cell = floor(g2 / spot_cs);
      var sp = 0.0;
      for (var j = -1; j <= 1; j++) { for (var i = -1; i <= 1; i++) {
        let c = cell + vec2f(f32(i), f32(j));
        if (hash2(c + 41.3) > 0.3) { continue; }
        let o = (c + vec2f(hash2(c + 7.7), hash2(c + 13.1))) * spot_cs;
        let rad = spot_cs * (0.15 + 0.5 * hash2(c + 23.9) * hash2(c + 29.3));
        let q = (g2 - o) / rad;
        let rr = length(q * vec2f(1.0, 1.0 + 0.3 * hash2(c + 3.3))) + 0.12 * (nz(vec3f(q * 0.8 + c, 0.5) * 0.2).r - 0.5);
        let ring = 1.0 - smoothstep(0.0, max(0.22, px * 1.2 / rad), abs(rr - 0.88));
        let fill = (1.0 - smoothstep(0.75, 0.95, rr)) * 0.3;
        sp = max(sp, max(ring, fill));
      } }
      spots = sp * cl * fine;
      haze += spots * 0.05;
    }
  }
  let bugs = smoothstep(0.86, 0.9, nz(L * 12.0).a) * step(L.z, -1.25) * smoothstep(0.55, 0.2, L.y) * select(1.0, 0.0, kind() == 1u || kind() == 6u || kind() == 9u) * (1.0 - smoothstep(0.002, 0.008, px));
  let toward = pow(max(dot(V, Ls), 0.0), 6.0);
  let lit = l.sun * sh * (0.15 + 2.5 * toward) + l.amb * 0.6;
  var rgb = env * F + glint + haze * lit * 0.35 + vec3f(0.05, 0.04, 0.02) * bugs * (l.amb + l.sun * sh * 0.2);
  // The spots scatter forward: they glow round the sun, and are faint grey otherwise.
  let fwd = max(dot(V, Ls), 0.0);
  rgb += spots * (l.amb * 0.02 + l.sun * sh * (0.003 + 0.06 * pow(fwd, 6.0) + 0.5 * pow(fwd, 60.0)));
  var trans = (1.0 - F) * (1.0 - haze) * (1.0 - bugs * 0.9) * 0.93;
  // The Raptor's canopy carries a thin metal-oxide coating: seen from outside it mirrors the sky in gold and is darker to look through.
  if (kind() == 4u && !in_cabin) {
    rgb += env * (0.10 + 0.25 * F) * vec3f(1.0, 0.76, 0.40);
    trans *= 0.80;
  }
  // Hairline scratches from years of wiping, fixed in the plastic. Each one
  // scatters sunlight only when it lies across the line to the sun, so the
  // lit ones ring the sun and slide round as the head moves.
  let face = dot(V, Ls);
  if (face > 0.2 && !lowq) {
    let dv = V - Ls;
    let d2 = vec2f(dot(dv, normalize((O.model * vec4f(tb, 0.0)).xyz)), dot(dv, normalize((O.model * vec4f(bb, 0.0)).xyz)));
    let dn = d2 / max(length(d2), 1e-5);
    let cs = 0.014;
    let cell = floor(g2 / cs);
    let w = max(px * 0.7, 0.00005);
    var sc = 0.0;
    for (var j = -1; j <= 1; j++) { for (var i = -1; i <= 1; i++) {
      let c = cell + vec2f(f32(i), f32(j));
      if (hash2(c) > 0.85) { continue; }
      // Mostly the circular strokes of a polishing cloth, some random.
      let o = (c + vec2f(hash2(c + 3.1), hash2(c + 5.7))) * cs;
      let swirl = normalize(vec2f(-(o.y - 0.4), o.x + 0.1) + vec2f(1e-4));
      let ra = hash2(c + 17.3) * 3.14159;
      let sd = normalize(mix(vec2f(cos(ra), sin(ra)), swirl, step(0.35, hash2(c + 2.9))));
      let len = cs * (0.8 + 2.2 * hash2(c + 9.2));
      let q = g2 - o;
      let along = dot(q, sd);
      let across = abs(q.x * sd.y - q.y * sd.x);
      let ln = (1.0 - smoothstep(w * 0.5, w * 1.5, across)) * (1.0 - smoothstep(len * 0.3, len * 0.5, abs(along)));
      let m = 1.0 - abs(dot(sd, dn));
      sc = max(sc, ln * pow(m, 14.0) * (0.25 + 0.75 * hash2(c + 1.7)));
    } }
    sc *= sqrt(min(1.0, 0.00015 / w));
    rgb += l.sun * sh * sc * (0.015 + 0.6 * pow(face, 18.0) + 12.0 * pow(face, 120.0)) * 1.6;
    // The ones too fine to see one by one, and the smears a rag leaves: a
    // milky, blotchy glare round the sun.
    let smear = 0.35 + 1.3 * smoothstep(0.3, 0.8, n1.g) + 0.5 * n2.b;
    rgb += l.sun * sh * smear * (0.002 + 0.03 * pow(face, 24.0) + 0.12 * pow(face, 400.0)) * 0.5;
  }
  if (L.z < -0.95 && kind() == 0u) {
    // Engine oil blown back over the windshield: a brown film, thickest low
    // down, drawn up and back by the airflow into streaks.
    let oil = O.dmg2.x;
    if (oil > 0.0) {
      let st = nz(vec3f(L.x * 14.0, L.y * 2.2 - L.z * 1.6, 0.3));
      let low = 1.0 - clamp((L.y - 0.05) / 0.8, 0.0, 1.0);
      let film = clamp((st.r * 0.7 + st.g * 0.45 + low * 0.7 - 1.05 + oil * 1.1) * 2.5, 0.0, 1.0) * smoothstep(0.0, 0.15, oil);
      rgb += vec3f(0.22, 0.13, 0.05) * film * (l.amb * 0.8 + l.sun * sh * 0.25 + l.sun * toward * 0.6);
      trans *= 1.0 - film * 0.92;
    }
    // A crack: radial spokes with rings between, scattering the light.
    let size = O.dmg2.w;
    if (size > 0.0) {
      let q = vec2f(L.z, L.y) - O.dmg2.yz;
      let r = length(q);
      if (r < size * 1.05) {
        let th = atan2(q.y, q.x);
        let k = round(th / 0.8976);
        let ak = k * 0.8976 + 0.25 * sin(k * 4.7) + 0.35 * (nz(vec3f(r * 5.0, k * 0.3, 0.7)).r - 0.5);
        let spoke = (1.0 - smoothstep(0.0, px * 1.5 + 0.0012, abs(sin(th - ak)) * r)) * step(r, size * (0.75 + 0.25 * nz(vec3f(k, 0.2, 0.4)).g));
        let ring = (1.0 - smoothstep(0.0, px * 1.5 + 0.001, abs(fract(r / (size * 0.28) + 0.3 * nz(vec3f(th, 0.5, 0.1)).r) - 0.5) * size * 0.28)) * step(r, size * 0.7) * step(size * 0.08, r);
        let web = max(spoke, ring * 0.7) * (1.0 - smoothstep(size * 0.6, size, r)) + (1.0 - smoothstep(0.0, size * 0.1, r)) * 0.6;
        rgb += web * (l.amb * 0.9 + l.sun * sh * (0.08 + 0.5 * toward));
        trans *= 1.0 - web * 0.7;
      }
    }
  }
  if (in_cabin) {
    // From inside, the glass replaces what is behind it: the view out, bent
    // by the acrylic and tinted by it (darker in a band along the top of the
    // windshield). Whatever lies in front of the glass is not bent.
    var tint = vec3f(0.95, 0.98, 0.965);
    if (wind && L.z < -0.95 && kind() == 0u) { tint *= mix(vec3f(1.0), vec3f(0.62, 0.72, 0.69), smoothstep(0.64, 0.77, L.y)); }
    // The 747's laminated, electrically heated glass: a faint green-blue, deeper toward the frame.
    if (kind() == 1u) { tint = mix(vec3f(0.93, 0.965, 0.955), vec3f(0.80, 0.88, 0.86), 1.0 - smoothstep(0.0, 0.05, -d)); }
    // The Hornet's stretched acrylic canopy: a faint warm tint.
    if (fighter()) { tint = vec3f(0.95, 0.95, 0.91); }
    // (The Raptor's coated one: amber.)
    if (kind() == 4u) { tint = vec3f(0.96, 0.91, 0.78); }
    // (Concorde's is laminated and heated like the 747's.)
    if (kind() == 6u || kind() == 9u) { tint = vec3f(0.93, 0.965, 0.955); }
    let edge = 1.0 - smoothstep(0.0, 0.08, -d);
    let curve = select(0.0, smoothstep(0.12, 0.45, abs(in.Nl.x)), wind);
    // (The 747's laminated glass is far flatter than formed acrylic: a faint waviness only.)
    let bend = select(glass_bend(g2, edge, curve), glass_bend(g2 * 0.6, 0.0, 0.0) * 0.18, kind() == 1u || kind() == 6u || kind() == 9u);
    let dw = (O.model * vec4f(tb * bend.x + bb * bend.y, 0.0)).xyz;
    var seen = textureLoad(t_scene, vec2i(in.clip.xy), 0).rgb;
    let cp = G.view_proj * vec4f(V + dw, 0.0);
    if (cp.w > 1e-4) {
      let uv = vec2f(0.5 + 0.5 * cp.x / cp.w, 0.5 - 0.5 * cp.y / cp.w);
      let q = vec2i(clamp(uv * G.screen.xy, vec2f(0.0), G.screen.xy - 1.0));
      if (textureLoad(t_gdepth, q, 0) <= in.clip.z) { seen = textureSampleLevel(t_scene, s_clamp, uv, 0.0).rgb; }
    }
    // The Hornet's canopy runs on down inside the sills, in front of the
    // consoles: no rain where the tub's own parts are what lies behind it.
    var skirt = false;
    if (kind() == 2u && (G.start.y > 0.0 || G.start.z > 0.002)) {
      let uvp = in.clip.xy * G.screen.zw;
      let pb = G.inv_view_proj * vec4f(uvp.x * 2.0 - 1.0, 1.0 - uvp.y * 2.0, textureLoad(t_gdepth, vec2i(in.clip.xy), 0), 1.0);
      let lb = ml * (pb.xyz / pb.w - O.model[3].xyz);
      skirt = abs(lb.x) < 0.6 && lb.z > -6.2 && lb.z < -3.1 && lb.y > 0.0 && lb.y < in.L.y;
    }
    if ((G.start.y > 0.0 || G.start.z > 0.002) && !skirt) {
      // Rain on the outside (rain_water). Its axes: those of the spots, and
      // on the Hornet's one-piece canopy both sets, blended where they meet.
      let nl = normalize(in.Nl);
      // Copied to a plain vec3 first: naga's Metal output for mat3 * -packed vec3 does not compile.
      let upn: vec3<f32> = l.upn;
      let down = ml * -upn;
      let gt = length(down - nl * dot(down, nl));
      let edge_r = select(1.0 - smoothstep(0.0, 0.05, -d), 0.0, kind() == 2u);
      let side_w = smoothstep(0.42, 0.58, abs(nl.x));
      var wa = Water(0.0, vec2f(0.0), 0.0, 0.0);
      var ws = Water(0.0, vec2f(0.0), 0.0, 0.0);
      let tw = vec3f(1.0, 0.0, 0.0);
      let bw = vec3f(0.0, 0.454, 0.891);
      let ts = vec3f(0.0, 0.0, 1.0);
      let bs = vec3f(0.0, 1.0, 0.0);
      // The airflow over the glass: straight back, along it.
      let f3 = vec3f(0.0, 0.0, 1.0) - nl * nl.z;
      let fw = vec2f(dot(f3, tw), dot(f3, bw));
      let fs = vec2f(dot(f3, ts), dot(f3, bs));
      // Axes about the middle of the cockpit glass, so the turn of the flow
      // over curved glass bends the streams rather than shearing them apart.
      var lr = L - vec3f(0.0, 0.4, -1.3);
      if (kind() == 1u) { lr = L - vec3f(0.0, 4.0, -26.5); }
      if (kind() == 2u) { lr = L - vec3f(0.0, 1.0, -4.6); }
      if (side_w < 1.0) { wa = rain_water(vec2f(dot(lr, tw), dot(lr, bw)), select(vec2f(0.0, 1.0), normalize(fw), length(fw) > 0.05), gt, edge_r, px); }
      if (side_w > 0.0) { ws = rain_water(vec2f(dot(lr, ts), dot(lr, bs)), select(vec2f(1.0, 0.0), normalize(fs), length(fs) > 0.05), gt, edge_r, px); }
      // Slopes into the body frame, then the world.
      var gl = wa.g.x * tw + wa.g.y * bw;
      var wc = wa.cov;
      var wf = wa.f;
      if (side_w > 0.0) {
        let gs = ws.g.x * ts + ws.g.y * bs;
        let pick = ws.h * side_w > wa.h * (1.0 - side_w);
        gl = select(select(gl, gs, pick), gs, side_w >= 1.0);
        wc = mix(wa.cov, ws.cov, side_w);
        wf = mix(wa.f, ws.f, side_w);
      }
      let wv = max(wc, wf);
      if (wv > 0.0) {
        let nout = -N;
        var gw = (O.model * vec4f(gl, 0.0)).xyz;
        gw -= nout * dot(gw, nout);
        let nd = normalize(nout - gw);
        // Through the glass into the water (the flat sheet only bends the
        // ray once, air to water), then out through the drop's curved face.
        let vw = refract(V, N, 1.0 / 1.333);
        let vo = refract(vw, -nd, 1.333);
        let night = night_at(l.upn);
        // Bent back inside: the dim cabin and its lights.
        let inner = env + l.amb * 0.05 + vec3f(1.0, 0.62, 0.38) * G.sun_dir.w * 0.0025 * night * 0.12;
        // Drops are not perfect lenses: each also scatters some of the light
        // round it, most at its rim, so bright lights gather a halo of lit drops.
        var near = vec3f(0.0);
        let a0 = hash2(in.clip.xy) * 6.2831853;
        for (var i = 0; i < 8; i++) {
          let ang = a0 + f32(i) * 2.3999632;
          let rad = select(9.0, 30.0, i >= 4) * (G.screen.x / 1600.0);
          let uv = (in.clip.xy + vec2f(cos(ang), sin(ang)) * rad) * G.screen.zw;
          near += textureSampleLevel(t_scene, s_clamp, uv, 0.0).rgb;
        }
        near *= 0.125;
        var thru = inner + near * 0.3;
        if (dot(vo, vo) > 0.5) {
          let ct = max(dot(vo, nd), 0.0);
          let fr = 0.02 + 0.98 * pow(1.0 - ct, 5.0);
          // The lensed view out: whatever lies that way on screen beyond
          // the glass, else the sky or the ground.
          // (Under a storm that sky is its dark base: the clear sky's blue
          // lit every runner and bead up as a bright cyan hairline.)
          var look = (env_reflect(in.P, vo, l.upn, false) + l.sun * sh * pow(max(dot(vo, Ls), 0.0), 3000.0) * 0.5) * (1.0 - 0.85 * G.stormfx.w);
          let cq = G.view_proj * vec4f(vo, 0.0);
          if (cq.w > 1e-4) {
            let uv = vec2f(0.5 + 0.5 * cq.x / cq.w, 0.5 - 0.5 * cq.y / cq.w);
            if (all(uv > vec2f(0.0)) && all(uv < vec2f(1.0))) {
              let q = vec2i(uv * G.screen.xy);
              if (textureLoad(t_gdepth, q, 0) <= in.clip.z) { look = textureSampleLevel(t_scene, s_clamp, uv, 0.0).rgb; }
            }
          }
          // Light scattered through the drops toward the sun.
          look += l.sun * sh * pow(max(dot(vo, Ls), 0.0), 300.0) * 0.004;
          thru = look * (1.0 - fr) + (inner + near * 0.3) * fr + near * 0.05;
        }
        rgb *= 1.0 - 0.5 * wc;
        seen = mix(seen, thru, wv);
      }
    }
    return vec4f((rgb + seen * trans * tint) * cov, cov);
  }
  let alpha = 1.0 - trans;
  return vec4f(rgb * cov, alpha * cov);
}

// ---------------------------------------------------------------- the 747 (aircraft/b747)
// Body frame: the origin 31.6 m behind the nose (d = L.z + 31.6), 5.0 m above the ground.

fn sd_box2(p: vec2f, b: vec4f) -> f32 {
  let c = vec2f((b.x + b.y) * 0.5, (b.z + b.w) * 0.5);
  let h = vec2f((b.y - b.x) * 0.5, (b.w - b.z) * 0.5);
  let q = abs(p - c) - h;
  return length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0);
}
fn sd_rbox(p: vec2f, h: vec2f, r: f32) -> f32 {
  let q = abs(p) - h + vec2f(r);
  return length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0) - r;
}
// The flight deck windows; matches b747::window.
fn window747(p: vec3f) -> f32 {
  let d = p.z + 31.6;
  if (d < 3.9 || d > 6.7 || p.y < 2.0) { return 1.0; }
  var best = 1.0;
  if (d < 5.45) {
    let ws = array<vec2f, 5>(vec2f(0.13, 3.18), vec2f(1.08, 2.94), vec2f(1.20, 3.05), vec2f(0.84, 3.49), vec2f(0.13, 3.81));
    best = max(sd_poly(vec2f(abs(p.x), p.y), ws, 5) - 0.06, d - 5.26);
  }
  let th = atan2(abs(p.x), p.y - 0.3);
  for (var i = 0; i < 2; i++) {
    let v = WIN747_BAYS[i];
    var q = vec2f(abs(d - (v.x + v.y) * 0.5) - (v.y - v.x) * 0.5, (abs(th - (v.z + v.w) * 0.5) - (v.w - v.z) * 0.5) * 3.4);
    q += vec2f(0.06);
    best = min(best, length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0) - 0.06);
  }
  return best;
}
// The gear wells; matches b747::well.
fn well747(p: vec3f) -> f32 {
  if (p.y > -1.2) { return 1.0; }
  let q = vec2f(p.z, abs(p.x));
  var d = sd_box2(q, vec4f(-27.35, -22.95, -0.78, 0.78));
  d = min(d, sd_box2(q, vec4f(-0.95, 0.45, 1.15, 4.0)));
  d = min(d, sd_box2(q, vec4f(0.15, 3.05, 1.05, 2.8)));
  d = min(d, sd_box2(q, vec4f(3.0, 5.75, 1.6, 2.24)));
  d = min(d, seg_d(q, vec2f(0.85, 5.75), vec2f(-0.30, 2.75)) - 0.36);
  return d + 0.005;
}
// Door outlines on the fuselage sides in (d, y): the five main deck doors
// each side, the upper deck exit, and the cargo doors on the right.
fn door747(L: vec3f) -> f32 {
  let d = L.z + 31.6;
  var best = 1e3;
  for (var i = 0; i < 5; i++) {
    best = min(best, sd_rbox(vec2f(d - DOORS747[i], L.y - 0.92), vec2f(0.595, 0.965), 0.16));
  }
  best = min(best, sd_rbox(vec2f(d - 15.2, L.y - 3.15), vec2f(0.42, 0.78), 0.14));
  if (L.x > 0.0) {
    best = min(best, sd_rbox(vec2f(d - 13.35, L.y + 1.75), vec2f(1.30, 0.86), 0.12));
    best = min(best, sd_rbox(vec2f(d - 45.2, L.y + 1.70), vec2f(1.30, 0.86), 0.12));
    best = min(best, sd_rbox(vec2f(d - 48.75, L.y + 1.45), vec2f(0.56, 0.58), 0.1));
  }
  return best;
}
// The nearest main deck door: (along, up from its centre, 1 inside it / 0).
fn door_local747(L: vec3f) -> vec3f {
  let d = L.z + 31.6;
  var best = vec2f(1e3);
  for (var i = 0; i < 5; i++) {
    let q = vec2f(d - DOORS747[i], L.y - 0.92);
    if (abs(q.x) < abs(best.x)) { best = q; }
  }
  let inside = abs(best.x) < 0.595 && abs(best.y) < 0.965 && abs(L.x) > 2.0;
  return vec3f(best, select(0.0, 1.0, inside));
}

// The finish of the 747's paint, after the livery: each skin panel's paint a
// shade apart, glazed windows, door seams and their markings, the radome's
// lightning strips, bare leading edges, walkways, grime and soot.
struct Fin { c: vec3f, rough: f32, metal: f32 };
fn skin747(f0: Fin, pt: u32, L: vec3f, Nl: vec3f, uv: vec2f, wear: f32, px: f32) -> Fin {
  var f = f0;
  let d = L.z + 31.6;
  let ax = abs(L.x);
  let side = sign(L.x);
  if (pt == P_FUSE) {
    // Panels between the butt joints and the lap joints, each painted a touch apart.
    let s = abs(uv.y);
    let pi = floor((d - 1.6) / 6.096);
    let pj = step(1.7, s) + step(3.6, s) + step(5.6, s) + step(7.7, s);
    let hp = hash2(vec2f(pi * 1.37 + side * 0.71, pj * 2.91));
    f.c *= 0.982 + 0.036 * hp;
    f.c = mix(f.c, f.c * vec3f(1.0, 0.992, 0.972), hash1(pi * 3.1 + pj * 5.7 + side) * 0.7);
    f.rough += 0.05 * (hash1(pi * 7.3 + pj * 1.9 + side * 2.0) - 0.5);
    // The radome: its own paint, greyer and duller, and the lightning diverter strips (dashed).
    if (d < 2.25) {
      let m = 0.30;
      let a = atan2(L.x, L.y - m);
      let k = (fract(a / 6.2831853 * 8.0 + 0.5) - 0.5) / 8.0 * 6.2831853;
      let r = length(vec2f(L.x, L.y - m));
      let dash = step(0.35, fract(d / 0.06)) * smoothstep(0.35, 0.55, d) * smoothstep(2.18, 2.05, d);
      let strip = aline(k * r, 0.012, px) * dash * step(abs(a), 2.6);
      f.c = mix(f.c * vec3f(0.93, 0.925, 0.90), vec3f(0.30, 0.30, 0.31), strip);
      f.rough = mix(f.rough + 0.12, 0.35, strip);
      f.metal = max(f.metal, strip * 0.8);
      f.c = mix(f.c, vec3f(0.62, 0.62, 0.60), aline(d - 2.2, 0.006, px));
    }
    // Glazing: smoky acrylic in a grey frame, smooth as glass.
    var ws = 1.0;
    if (ax > 1.3 && L.y > -0.2) { ws = cabin_win_sd747(L); }
    if (ws < 0.03) {
      let pane = afill(ws + 0.010, px);
      let frame = afill(ws, px) - pane;
      f.c = mix(f.c, vec3f(0.20, 0.205, 0.21), frame);
      f.c = mix(f.c, vec3f(0.012, 0.014, 0.017), pane);
      f.rough = mix(f.rough, 0.025, pane);
      f.metal *= 1.0 - afill(ws, px);
    }
    // Door gaps, and on each main door its handle flap with a red outline
    // and the OPEN arrow; the cargo doors' latch row.
    if (ax > 1.5) {
      let dd = door747(L);
      f.c *= 1.0 - 0.75 * aline(dd, 0.005, px);
      let dl = door_local747(L);
      if (dl.z > 0.0) {
        let q = dl.xy - vec2f(0.0, 0.08);
        let hd = sd_rbox(q, vec2f(0.16, 0.045), 0.02);
        f.c = mix(f.c, vec3f(0.06), afill(hd + 0.004, px) * 0.35);
        let red = aline(sd_rbox(q, vec2f(0.24, 0.10), 0.03), 0.008, px);
        let qa = vec2f(q.x * side, q.y + 0.17);
        let arrow = afill(max(abs(qa.y) - 0.012, abs(qa.x) - 0.14), px) + afill(max(abs(qa.y) - 0.05 + (qa.x - 0.10) * 0.9, -(qa.x - 0.10)), px);
        f.c = mix(f.c, vec3f(0.55, 0.035, 0.025), sat(red + arrow * 0.9));
      }
      if (L.x > 0.0 && L.y < -0.7) {
        for (var i = 0; i < 2; i++) {
          let c = select(45.2, 13.35, i == 0);
          let qx = d - c;
          if (abs(qx) < 1.2) {
            let lp = (fract(qx / 0.4 + 0.5) - 0.5) * 0.4;
            f.c *= 1.0 - 0.5 * afill(sd_rbox(vec2f(lp, L.y + 2.50), vec2f(0.05, 0.025), 0.01), px);
          }
        }
      }
    }
    // The air conditioning packs' ram air inlets in the belly fairing ahead of
    // the wing (a ramp widening aft into a dark mouth), and their exhaust louvres behind.
    if (L.y < -1.5 && d > 21.5 && d < 29.0) {
      let t = (d - 21.8) / 1.7;
      let w = 0.06 + 0.16 * sat(t);
      let q = vec2f(d - 22.65, ax - 1.25);
      let ramp = max(abs(q.x) - 0.85, abs(q.y) - w);
      let inside = afill(ramp, px);
      f.c = mix(f.c, f.c * mix(0.75, 0.05, smoothstep(0.4, 1.0, t)), inside);
      f.c *= 1.0 - 0.5 * aline(ramp, 0.004, px);
      f.rough = mix(f.rough, 0.6, inside * smoothstep(0.6, 1.0, t));
      let lq = vec2f(d - 27.6, ax - 1.15);
      let louv = sd_rbox(lq, vec2f(0.45, 0.22), 0.03);
      let slat = 0.5 + 0.5 * cos((d - 27.6) / 0.06 * 3.14159);
      f.c = mix(f.c, f.c * (0.25 + 0.5 * slat), afill(louv, px) * (1.0 - smoothstep(0.02, 0.05, px)) + afill(louv, px) * smoothstep(0.02, 0.05, px) * 0.5);
      f.c *= 1.0 - 0.5 * aline(louv, 0.004, px);
    }
    // Static ports on the nose: polished discs in a red no-paint ring.
    if (ax > 1.5 && d > 7.6 && d < 8.6 && abs(L.y + 0.35) < 0.3) {
      let q = vec2f(d - 8.1, L.y + 0.35);
      let sp = length(vec2f(abs(q.x) - 0.12, q.y));
      f.c = mix(f.c, vec3f(0.55, 0.035, 0.025), aline(sd_rbox(q, vec2f(0.24, 0.10), 0.04), 0.008, px));
      let disc = afill(sp - 0.035, px);
      f.c = mix(f.c, vec3f(0.75, 0.76, 0.78), disc);
      f.metal = max(f.metal, disc);
      f.rough = mix(f.rough, 0.15, disc);
    }
  }
  if (pt == P_WING || pt == P_CTRL || (pt == P_TAIL && ax > 0.9)) {
    let s = abs(uv.x);
    let av = abs(uv.y);
    let upper = uv.y > 0.0;
    // Skin planks running spanwise, each a slightly different grey.
    let hp = hash2(vec2f(floor(av / 0.62), floor(s / 6.2) + side * 0.5 + f32(pt) * 3.0));
    f.c *= 0.955 + 0.09 * hp;
    f.rough += 0.06 * (hp - 0.5);
    // Grime drawn back along the chord by the airflow; heavier underneath and
    // on the flaps, sooty behind the engines.
    let g = nz(vec3f(L.x * 2.2, L.y * 2.2, L.z * 0.10)).r;
    let gg = nz(vec3f(L.x * 9.0, L.y * 9.0, L.z * 0.4)).g;
    let dirt = smoothstep(0.35, 0.85, g) * (0.6 + 0.5 * gg) * select(0.16, 0.08, upper);
    f.c *= 1.0 - dirt;
    f.rough += dirt * 0.6;
    if (!upper && pt != P_TAIL && L.z < 12.0) {
      let e = eng747(L);
      let soot = exp(-e.x * e.x / 1.4) * smoothstep(5.0, 7.5, e.z) * (0.5 + 0.5 * gg);
      f.c = mix(f.c, vec3f(0.06, 0.055, 0.05), sat(soot * 0.35));
    }
    // Bare, polished leading edges (the wing's and the stabiliser's).
    if (pt != P_CTRL) {
      let bare = smoothstep(0.08, 0.14, wear) * select(1.0, 0.0, pt == P_TAIL && ax > 30.0);
      let polish = 0.85 + 0.15 * nz(vec3f(L.x * 0.8, L.y * 30.0, L.z * 30.0)).r;
      f.c = mix(f.c, vec3f(0.58, 0.59, 0.61) * polish * (1.0 - 0.25 * dirt), bare);
      f.metal = mix(f.metal, 1.0, bare);
      f.rough = mix(f.rough, 0.38 + 0.12 * gg, bare);
    }
    // The walkway over the wing root, outlined in black, and the no-step
    // lines along the flaps' and spoilers' front edges.
    if (upper && pt == P_WING && s < 13.0 && L.z < 12.0) {
      let c = chord747(s);
      let inr = step(3.9, s) * step(s, 11.6);
      let ina = step(0.17 * c, av) * step(av, 0.56 * c);
      let w = (aline(s - 3.9, 0.02, px) + aline(s - 11.6, 0.02, px)) * ina + (aline(av - 0.17 * c, 0.02, px) + aline(av - 0.56 * c, 0.02, px)) * inr;
      f.c = mix(f.c, vec3f(0.02), sat(w) * 0.85);
      // A non-slip finish inside it.
      f.rough = mix(f.rough, 0.7, inr * ina);
      f.c *= 1.0 - 0.06 * inr * ina;
    }
    if (upper && pt == P_WING && s > 3.4 && s < 24.0 && L.z < 12.0) {
      f.c = mix(f.c, vec3f(0.02), aline(av - 0.575 * chord747(s), 0.012, px) * 0.8);
    }
  }
  if (pt == P_NACELLE) {
    let e = eng747(L);
    let r = length(e.xy);
    if (r < 1.34) {
      // Oil and exhaust grime along the bottom of the cowls, soot at the sleeve's end.
      let bot = sat(-e.y / r);
      let st = nz(vec3f(L.x * 6.0, L.y * 6.0, L.z * 0.25)).r;
      f.c *= 1.0 - 0.18 * bot * bot * smoothstep(0.3, 0.8, st) * smoothstep(0.5, 2.5, e.z);
      f.c *= 1.0 - 0.25 * smoothstep(2.55, 2.95, e.z);
      // Each cowl panel a shade apart.
      f.c *= 0.975 + 0.04 * hash1(step(0.95, e.z) + step(1.9, e.z) * 2.0 + side * 5.0 + e.w * 9.0);
    } else {
      // The pylon: a little grimier toward its aft end.
      f.c *= 1.0 - 0.10 * smoothstep(3.0, 7.0, e.z) * smoothstep(0.3, 0.8, nz(L * vec3f(3.0, 3.0, 0.3)).r);
    }
  }
  if (pt == P_SPINNER) {
    // A dark spinner with a white swirl, so ground crew can see the fan turn.
    let e = eng747(L);
    let r = length(e.xy);
    let a = atan2(e.x, e.y) * side;
    let k = wrapa(a - r * 9.0);
    let band = aline(k * max(r, 0.02), 0.03, px) * smoothstep(0.03, 0.06, r) * step(e.z, 1.13);
    f.c = mix(vec3f(0.035, 0.036, 0.04), vec3f(0.80), band);
    f.rough = 0.22;
    f.metal = 0.0;
  }
  return f;
}

// Cabin windows on the skin (there is no cabin behind them): 10 x 15 in,
// every 20 in along the main deck, and along the upper deck; the small
// window in each door. Signed distance (m) to the nearest pane.
const DOORS747 = array<f32, 5>(9.50, 18.80, 30.61, 40.74, 55.14);
fn cabin_win_sd747(L: vec3f) -> f32 {
  let d = L.z + 31.6;
  var sd = 1.0;
  if (abs(L.x) > 2.3 && abs(L.y - 0.69) < 0.3 && d > 2.4 && d < 57.8) {
    var near_door = false;
    let k = (fract((d - 2.4) / 0.508) - 0.5) * 0.508;
    // (Whole windows are left out beside the doors, not cut through.)
    for (var i = 0; i < 5; i++) { if (abs(d - k - DOORS747[i]) < 0.9) { near_door = true; } }
    if (!near_door) {
      sd = sd_rbox(vec2f(k, L.y - 0.69), vec2f(0.127, 0.19), 0.07);
    }
  }
  if (abs(L.x) > 1.3 && abs(L.y - 3.42) < 0.3 && d > 9.6 && d < 22.3) {
    let k = (fract((d - 9.6) / 0.508) - 0.5) * 0.508;
    if (abs(d - k - 15.2) > 0.6) { sd = min(sd, sd_rbox(vec2f(k, L.y - 3.42), vec2f(0.127, 0.18), 0.07)); }
  }
  if (abs(L.x) > 2.3 && abs(L.y - 1.32) < 0.2) {
    for (var i = 0; i < 5; i++) { sd = min(sd, sd_rbox(vec2f(d - DOORS747[i], L.y - 1.32), vec2f(0.11, 0.15), 0.06)); }
  }
  if (abs(L.x) > 1.3 && abs(L.y - 3.42) < 0.3) { sd = min(sd, sd_rbox(vec2f(d - 15.2, L.y - 3.42), vec2f(0.11, 0.15), 0.06)); }
  return sd;
}
fn cabin_windows747(L: vec3f, aa: f32) -> f32 { return smoothstep(aa, -aa, cabin_win_sd747(L)); }

// A line |d| < hw wide, filtered over a footprint px: it fades to its average
// coverage once it is thinner than a pixel instead of vanishing or aliasing.
fn aline(d: f32, hw: f32, px: f32) -> f32 {
  let w = max(hw, px * 0.5);
  return sat((w - abs(d)) / max(px, 1e-5) + 0.5) * (hw / w);
}
// Filled shape (sd < 0) filtered the same way.
fn afill(sd: f32, px: f32) -> f32 { return sat(0.5 - sd / max(px, 1e-5)); }

// The nearest engine: (x outboard of its axis, y above it, z behind its inlet lip, inboard 1/outboard 0).
fn eng747(L: vec3f) -> vec4f {
  let ib = abs(L.x) < 16.3;
  let ex = select(21.0, 11.68, ib);
  return vec4f(L.x - sign(L.x) * ex, L.y - select(-1.58, -2.5, ib), L.z - select(0.81, -8.36, ib), select(0.0, 1.0, ib));
}

const B_BELLY = vec3f(0.46, 0.48, 0.51);
const B_GREY = vec3f(0.34, 0.36, 0.39);
const B_NAVY = vec3f(0.012, 0.03, 0.095);
const B_RED = vec3f(0.42, 0.025, 0.02);

// The G650 in one of the 747's fictional airlines' schemes (g650::LIVERIES):
// painted over the plain white skin (assets/g650_plain.jpg), whose panel
// lines, rivets and door outlines darken the colour as they did the white.
// Its frame (g650.py): station d aft of the radome's tip, height h above the
// ground; the cabin's axis about 2.77 m up, 1.3 m round.
fn livery_g650(L: vec3f, base: vec3f, aa: f32) -> vec3f {
  let d = L.z + 14.35;
  let h = L.y + 2.55;
  let ax = abs(L.x);
  // How white the skin is here (1: the plain paint; darker at its lines):
  // windows, frames and dark parts keep their own colour.
  let shade = luma(base) / (0.92 * 0.85);
  let paintable = smoothstep(0.5, 0.75, shade);
  let body = length(vec2f(L.x, h - 2.77)) < 1.42 && d < 27.5;
  let fin = ax < 0.35 && h > 4.0 && d > 21.0;
  let nacelle = abs(ax - 2.2) < 0.75 && abs(h - 3.75) < 0.75 && d > 18.8 && d < 22.7;
  let style = livery_style();
  var c = WHITE_PAINT;
  var a = 0.0;
  // (The title and registration read nose-first on the left.)
  let side = vec2f((11.2 - d) * sign(L.x), h);
  if (style == 1u) {
    // Orivante: the fin and tail-cone wrap in PT.a behind arcs of PT.b; teal nacelles, titles.
    let q = vec2f(d - 15.0, h - 0.6);
    let r = length(q);
    let arc = sat(smoothstep(aa, -aa, abs(r - 9.4) - 0.24) + smoothstep(aa, -aa, abs(r - 10.05) - 0.08));
    if (fin) { c = mix(PT.a.rgb, PT.b.rgb, arc); a = 1.0; }
    else if (nacelle) { c = select(PT.a.rgb, PT.c.rgb, d < 19.1); a = 1.0; }
    else if (body) {
      let wrap = smoothstep(aa, -aa, 9.15 - r) * step(19.0, d);
      let ink = title_ink(side, vec2f(0.0, 3.62), 0.46, 0.3, aa);
      c = mix(PT.a.rgb, PT.b.rgb, arc * step(19.0, d) * step(r, 10.3));
      a = max(max(wrap, arc * step(19.0, d) * step(r, 10.3)), ink);
      c = select(c, PT.a.rgb, ink > 0.0 && wrap < 0.5);
    }
  } else {
    // Kestralis: a PT.a belly rising to fill the tail behind a PT.b pinstripe, a chevron on the fin.
    if (fin) {
      // (Scaled 0.8 and centred on the fin between its swept leading edge,
      // its trailing edge, the tail cone and the T-tail, g650.py `fin_le`, `fin_te`.)
      let p = (vec2f(d, h) - vec2f(24.65, 5.75)) / 0.8;
      let e = aa / 0.8;
      let s1 = min(seg_d(p, vec2f(0.0), vec2f(2.6, 1.3)), seg_d(p, vec2f(0.0), vec2f(2.6, -1.3)));
      let s2 = min(seg_d(p, vec2f(0.9, 0.0), vec2f(2.9, 0.9)), seg_d(p, vec2f(0.9, 0.0), vec2f(2.9, -0.9)));
      c = mix(PT.a.rgb, PT.b.rgb, smoothstep(e, -e, s1 - 0.22));
      c = mix(c, WHITE_PAINT, smoothstep(e, -e, s2 - 0.09));
      a = 1.0;
    } else if (nacelle) { c = select(PT.c.rgb, PT.b.rgb, d < 19.1); a = 1.0; }
    else if (body) {
      let ye = 2.5 - max(4.0 - d, 0.0) * max(4.0 - d, 0.0) * 0.04 + max(d - 16.5, 0.0) * max(d - 16.5, 0.0) * 0.032;
      let under = smoothstep(aa, -aa, h - ye);
      let pin = smoothstep(aa, -aa, abs(h - ye - 0.09) - 0.03) * (1.0 - under);
      let ink = title_ink(side, vec2f(0.0, 3.62), 0.42, 0.3, aa);
      c = mix(mix(WHITE_PAINT, PT.a.rgb, under), PT.b.rgb, pin);
      c = mix(c, PT.a.rgb, ink);
      a = max(max(under, pin), ink);
    }
  }
  // Colour in place of the white, shaded as the white was.
  return mix(base, c * 0.85 * min(shade, 1.05) * (0.92 / 0.80), a * paintable);
}

// The 747's paint, in the scheme picked (b747::LIVERIES, all fictional
// airlines): style 0 the classic white over a grey belly, a navy cheatline
// with a red pinstripe sweeping up the tail, grey wings, a navy fin with a
// white emblem; 1 and 2 the Orivante and Kestralis designs.
fn livery747(L: vec3f, aa: f32, pt: u32) -> vec3f {
  let style = livery_style();
  if (style == 1u) { return orivante747(L, aa, pt); }
  if (style == 2u) { return kestralis747(L, aa, pt); }
  // (Classic: the colours sent, or the original ones if none were.)
  let sent = livery_ours() && dot(PT.d.rgb, vec3f(1.0)) > 0.0;
  let navy = select(B_NAVY, PT.a.rgb, sent);
  let red = select(B_RED, PT.b.rgb, sent);
  let belly = select(B_BELLY, PT.c.rgb, sent);
  let grey = select(B_GREY, PT.d.rgb, sent);
  let d = L.z + 31.6;
  let ax = abs(L.x);
  let fin = ax < 0.9 && L.y > 3.9 && d > 52.0;
  if (fin) {
    var c = navy;
    // The emblem: a white ring with three swept bands inside, a red ring outside it.
    // (Centred on the fin's area, 2.5 m across its red ring, clear of its
    // root, swept leading edge and trailing edge.)
    let q = (vec2f(d, L.y) - vec2f(64.5, 8.3)) / 1.3;
    let e = aa / 1.3;
    let r = length(q);
    let ring = smoothstep(e, -e, abs(r - 1.68) - 0.09);
    let rr = smoothstep(e, -e, abs(r - 1.92) - 0.035);
    let band = abs(fract((q.x * 0.55 + q.y) / 0.95 + 0.25) - 0.5) - 0.13;
    let inner = smoothstep(e, -e, band) * smoothstep(1.5 + e, 1.5 - e, r);
    c = mix(c, WHITE_PAINT, max(ring, inner));
    c = mix(c, red, rr);
    return c;
  }
  if (ax > 30.5 && (pt == P_TAIL || L.y > 1.6)) { return navy; }
  if (pt == P_WING || pt == P_CTRL || pt == P_TAIL) { return grey; }
  if (pt == P_NACELLE) { return WHITE_PAINT; }
  // Unpatterned painted parts: the flap track fairings under the wings are
  // wing grey, the gear doors belly grey.
  if (pt == 0u) { return select(grey, belly, ax < 6.0); }
  var c = WHITE_PAINT;
  // Grey belly below a line that rises with the tail's upsweep.
  let yb = -1.15 + max(d - 46.0, 0.0) * 0.17 - max(4.0 - d, 0.0) * 0.12;
  c = mix(c, belly, smoothstep(yb + aa, yb - aa, L.y));
  // The cheatline under the windows, sweeping up over the tail cone.
  if (d > 1.4 && d < 68.5 && ax > 0.2) {
    let rise = max(d - 50.0, 0.0) * 0.16;
    let taper = smoothstep(1.4, 4.0, d) * (1.0 - smoothstep(64.0, 68.5, d));
    let y0 = 0.17 + rise;
    let nv = smoothstep(aa, -aa, abs(L.y - y0) - 0.13 * taper);
    let pin = smoothstep(aa, -aa, abs(L.y - y0 - 0.215 * taper) - 0.022 * taper);
    c = mix(c, navy, nv);
    c = mix(c, red, pin);
  }
  c = mix(c, navy, reg747(L, aa));
  // The cabin windows.
  c = mix(c, vec3f(0.018, 0.02, 0.025), cabin_windows747(L, aa));
  return c;
}

// Registration on the rear fuselage, reading nose-first on the left.
fn reg747(L: vec3f, aa: f32) -> f32 {
  let d = L.z + 31.6;
  if (abs(L.x) > 0.5 && d > 54.5 && d < 60.5 && L.y > 1.3 && L.y < 2.1) {
    let reg = pack(10u, 7u, 4u, 7u, 12u, 11u);
    return label_code(vec2f((57.5 - d) * sign(L.x), L.y), vec2f(0.0, 1.7), reg, 6u, 0.5, aa);
  }
  return 0.0;
}
// The airline's name on the forward fuselage, between the main deck's
// windows and the upper deck's, reading nose-first on the left.
fn titles747(L: vec3f, aa: f32, h: f32) -> f32 {
  let d = L.z + 31.6;
  if (abs(L.x) < 1.5 || d < 12.0 || d > 32.0 || abs(L.y - 2.05) > 1.0) { return 0.0; }
  return title_ink(vec2f((21.5 - d) * sign(L.x), L.y), vec2f(0.0, 2.05), h, 0.3, aa);
}
// The parts every style paints alike: windows on top, wings and tailplane
// grey (PT.d).
fn windows747(c: vec3f, L: vec3f, aa: f32) -> vec3f {
  return mix(c, vec3f(0.018, 0.02, 0.025), cabin_windows747(L, aa));
}

// Orivante (fictional): white overall; the fin in PT.a with a sweeping arc of
// PT.b, the fin's colour wrapping down over the tail cone behind a PT.b
// pinstripe; nacelles in PT.a with polished lips (PT.c); big titles.
fn orivante747(L: vec3f, aa: f32, pt: u32) -> vec3f {
  let d = L.z + 31.6;
  let ax = abs(L.x);
  let teal = PT.a.rgb;
  let coral = PT.b.rgb;
  // Arcs across the fin (and the wrap): circles about a point ahead of and below it.
  let q = vec2f(d - 46.0, L.y + 6.0);
  let r = length(q);
  let arc = smoothstep(aa, -aa, abs(r - 19.2) - 0.55) + smoothstep(aa, -aa, abs(r - 20.6) - 0.18);
  if (ax < 0.9 && L.y > 3.9 && d > 52.0) { return mix(teal, coral, sat(arc)); }
  if (ax > 30.5 && (pt == P_TAIL || L.y > 1.6)) { return teal; }
  if (pt == P_WING || pt == P_CTRL || pt == P_TAIL) { return PT.d.rgb; }
  if (pt == P_NACELLE) {
    let e = eng747(L);
    return select(teal, PT.c.rgb, e.z < 0.55);
  }
  if (pt == 0u) { return select(PT.d.rgb, vec3f(0.62, 0.63, 0.64), ax < 6.0); }
  var c = WHITE_PAINT;
  // A pale grey belly, barely parted from the white.
  let yb = -1.6 + max(d - 46.0, 0.0) * 0.17;
  c = mix(c, vec3f(0.62, 0.63, 0.64), smoothstep(yb + aa, yb - aa, L.y));
  // The wrap: the fin's colour over the tail cone, its front edge an arc.
  let wrap = smoothstep(aa, -aa, 18.6 - r) * step(48.0, d);
  c = mix(c, teal, wrap);
  c = mix(c, coral, sat(arc) * step(48.0, d) * step(r, 21.0));
  c = mix(c, teal, titles747(L, aa, 1.25));
  c = mix(c, select(teal, WHITE_PAINT, wrap > 0.5), reg747(L, aa));
  return windows747(c, L, aa);
}

// Kestralis (fictional): a deep PT.a belly that rises in a curve to fill the
// tail, a PT.b pinstripe along its edge, a chevron of PT.b on the fin,
// nacelles in PT.c with PT.b lips, PT.b winglets, titles above the windows.
fn kestralis747(L: vec3f, aa: f32, pt: u32) -> vec3f {
  let d = L.z + 31.6;
  let ax = abs(L.x);
  let blue = PT.a.rgb;
  let orange = PT.b.rgb;
  if (ax < 0.9 && L.y > 3.9 && d > 52.0) {
    // Two chevrons, wings swept back from a point at the front: orange, then white.
    // (Scaled 0.8 and placed so the whole of it sits on the fin, centred
    // between its swept leading edge and its trailing edge.)
    let p = (vec2f(d, L.y) - vec2f(60.5, 8.2)) / 0.8;
    let e = aa / 0.8;
    let s1 = min(seg_d(p, vec2f(0.0, 0.0), vec2f(6.5, 3.6)), seg_d(p, vec2f(0.0, 0.0), vec2f(6.5, -3.6)));
    let s2 = min(seg_d(p, vec2f(2.2, 0.0), vec2f(7.2, 2.6)), seg_d(p, vec2f(2.2, 0.0), vec2f(7.2, -2.6)));
    var c = mix(blue, orange, smoothstep(e, -e, s1 - 0.55));
    return mix(c, WHITE_PAINT, smoothstep(e, -e, s2 - 0.22));
  }
  if (ax > 30.5 && (pt == P_TAIL || L.y > 1.6)) { return orange; }
  if (pt == P_WING || pt == P_CTRL || pt == P_TAIL) { return PT.d.rgb; }
  if (pt == P_NACELLE) {
    let e = eng747(L);
    return select(PT.c.rgb, orange, e.z < 0.55);
  }
  if (pt == 0u) { return select(PT.d.rgb, blue, ax < 6.0); }
  var c = WHITE_PAINT;
  // The belly's edge: under the windows along the cabin, curving down to the
  // nose and up behind the wing to fill the tail cone.
  let ye = 0.12 - max(9.0 - d, 0.0) * max(9.0 - d, 0.0) * 0.035 + max(d - 44.0, 0.0) * max(d - 44.0, 0.0) * 0.028;
  let under = smoothstep(aa, -aa, L.y - ye);
  c = mix(c, blue, under);
  c = mix(c, orange, smoothstep(aa, -aa, abs(L.y - ye - 0.2) - 0.07) * (1.0 - under));
  c = mix(c, blue, titles747(L, aa, 1.15));
  c = mix(c, select(blue, WHITE_PAINT, under > 0.5), reg747(L, aa));
  return windows747(c, L, aa);
}

fn weather747(L: vec3f, N: vec3f, base: vec3f, belly: f32, n: vec4f) -> vec4f {
  var c = base;
  // Streaks of grime back along the belly from the drains, and a darker
  // patch behind the APU exhaust.
  let streak = nz(vec3f(L.x * 1.5, L.y * 1.5, L.z * 0.06)).r;
  c *= 1.0 - 0.08 * belly * smoothstep(0.4, 0.8, streak);
  let d = L.z + 31.6;
  let fine = nz(vec3f(L.x * 8.0, L.y * 8.0, L.z * 0.3)).g;
  // Hydraulic fluid and brake dust swept back from the gear wells, brownish.
  if (L.y < -0.6 && L.z > -1.5 && L.z < 24.0) {
    let k = smoothstep(-1.5, 2.0, L.z) * (1.0 - smoothstep(10.0, 24.0, L.z)) * sat(-L.y - 0.6) * smoothstep(0.35, 0.75, streak) * (0.5 + 0.7 * fine);
    c = mix(c, c * vec3f(0.62, 0.55, 0.48), sat(k * 0.55));
  }
  // Drain masts under the belly: a stain trailing from each.
  for (var i = 0; i < 2; i++) {
    let z0 = select(15.4, -15.6, i == 0);
    let t = L.z - z0;
    if (t > 0.0 && L.y < -2.0) {
      let k = exp(-L.x * L.x / (0.02 + t * 0.012)) * exp(-t / 5.0) * (0.5 + 0.6 * fine);
      c = mix(c, c * vec3f(0.55, 0.50, 0.44), sat(k * 0.7));
    }
  }
  // Rain has run down from the window frames and door seams on the sides.
  if (abs(L.x) > 2.0 && L.y > -1.2 && L.y < 3.8) {
    let run = nz(vec3f(L.z * 3.0, L.y * 0.12, L.x * 0.1)).r;
    let below = select(0.0, 1.0, L.y < 0.5) * smoothstep(-1.2, 0.3, L.y);
    c *= 1.0 - 0.035 * smoothstep(0.55, 0.85, run) * (0.4 + 0.6 * below);
  }
  // The wing-to-body fairing's lower edge and the keel beam: scuffed, darker.
  if (L.y < -2.6 && abs(L.z) < 18.0) { c *= 1.0 - 0.06 * smoothstep(0.5, 0.8, nz(L * 1.3).b); }
  if (d > 68.0) { c = mix(c, vec3f(0.03), smoothstep(68.6, 69.8, d) * 0.7); }
  return vec4f(c, 0.08 * belly * n.b);
}

// Chord of the wing at a span station (plan view).
fn chord747(x: f32) -> f32 {
  let le = select(37.086 + 0.841 * (x - 21.0), 18.06 + 0.906 * x, x < 21.0);
  let te = select(39.151 + 0.58 * (x - 13.4), 34.93 + 0.315 * x, x < 13.4);
  return te - le;
}

// The 747's panel seams and rivet rows (height, seam dirt, rivets).
fn relief747(pt: u32, uv: vec2f, L: vec3f, Nl: vec3f, px: f32) -> vec3f {
  var h = 0.0;
  var dirt = 0.0;
  var rv = 0.0;
  let gw = max(0.0012, px * 0.9);
  let ga = 0.0007 * min(1.0, 0.0018 / gw);
  let rr = 0.004;
  let rf = 1.0 - smoothstep(rr * 0.5, rr * 1.6, px);
  switch pt {
    case 1u: {
      let d = L.z + 31.6;
      let s = abs(uv.y);
      // A rivet row on every frame (20 in apart), a butt joint every twelfth.
      let fr = (fract(d / 0.508 + 0.5) - 0.5) * 0.508;
      rv = max(rv, rivets(s, abs(fr), 0.04, rr) * rf * 0.6);
      let bj = (fract((d - 1.6) / 6.096 + 0.5) - 0.5) * 6.096;
      h -= ga * line(bj, gw);
      dirt += line(bj, gw * 2.5);
      for (var i = 0; i < 4; i++) {
        let ds = s - LAPS747[i];
        h -= ga * line(ds, gw);
        dirt += line(ds, gw * 2.0);
        rv = max(rv, rivets(d, abs(abs(ds) - 0.014), 0.035, rr) * rf);
      }
      if (abs(L.x) > 1.5) {
        let dd = door747(L);
        h -= ga * 2.0 * line(dd, gw * 1.3);
        dirt += line(dd, gw * 3.0);
        // The panes sit a little in from the skin, the edge of the opening bevelled.
        let ws = cabin_win_sd747(L);
        h -= 0.006 * smoothstep(0.004, -0.010, ws) * (1.0 - smoothstep(0.004, 0.012, px));
        dirt += 0.5 * line(ws - 0.002, gw * 1.5);
        // Each door's flush handle flap, and the hinged access panels on the belly.
        let dl = door_local747(L);
        if (dl.z > 0.0) {
          let hd = sd_rbox(dl.xy - vec2f(0.0, 0.08), vec2f(0.16, 0.045), 0.02);
          h -= ga * 1.5 * line(hd, gw);
          dirt += line(hd, gw * 2.0);
        }
      }
      if (L.y < -1.5) {
        let pz = (fract(L.z / 1.524 + 0.5) - 0.5) * 1.524;
        let bp = sd_rbox(vec2f(pz, abs(L.x) - 0.9), vec2f(0.28, 0.20), 0.03);
        let keep = step(0.55, hash1(floor(L.z / 1.524) + sign(L.x) * 0.37));
        h -= ga * line(bp, gw) * keep;
        dirt += line(bp, gw * 2.0) * keep;
        rv = max(rv, rivets(pz + abs(L.x) * 3.0, abs(bp + 0.012), 0.04, rr) * rf * keep);
      }
      h += rv * 0.0008;
    }
    case 2u, 5u: {
      let s = abs(uv.x);
      let av = abs(uv.y);
      let c = select(chord747(s), 6.0, pt == 5u);
      rv = max(rv, rivets(av, (fract(s / 0.6 + 0.5) - 0.5) * 0.6, 0.045, rr) * rf);
      rv = max(rv, rivets(s, av - 0.15 * c, 0.04, rr) * rf);
      rv = max(rv, rivets(s, av - 0.62 * c, 0.04, rr) * rf);
      let le = av - 0.12 * c;
      h -= ga * line(le, gw);
      dirt += line(le, gw * 2.0);
      h += rv * 0.0008;
    }
    case 3u: {
      let dd = (fract(abs(uv.x) / 0.5) - 0.5) * 0.5;
      h -= ga * 0.7 * line(dd, gw);
      dirt += 0.3 * line(dd, gw * 2.0);
    }
    case 7u: { // the flight deck lining: panel joints, a fine grain
      let d = L.z + 31.6;
      let j = (fract((d - 4.6) / 0.62 + 0.5) - 0.5) * 0.62;
      h -= 0.0012 * line(j, max(0.002, px));
      dirt += 0.4 * line(j, max(0.004, px * 2.0));
      h += 0.00012 * nz(L * 50.0).r * (1.0 - smoothstep(0.0005, 0.002, px));
    }
    case 8u: {
      if (abs(Nl.x) < 0.7) {
        let t = fract(L.x / 0.065 + 0.5) * 2.0 - 1.0;
        h += 0.004 * sqrt(max(1.0 - t * t, 0.0));
        dirt += smoothstep(0.85, 1.0, abs(t));
      }
      h += 0.0004 * nz(L * 60.0).g * (1.0 - smoothstep(0.0005, 0.002, px));
    }
    case 9u: { h += 0.0015 * nz(L * 45.0).r * (1.0 - smoothstep(0.001, 0.004, px)); }
    case 10u: { h += 0.00012 * nz(L * 220.0).r * (1.0 - smoothstep(0.0002, 0.0009, px)); }
    case 12u: { h += 0.00025 * sin((L.x + L.y * 1.3 + L.z * 0.7) * 2600.0) * (1.0 - smoothstep(0.0002, 0.0008, px)); }
    case 15u: { h += 0.003 * nz(L * 70.0).r * (1.0 - smoothstep(0.001, 0.004, px)); }
    case 13u: {
      // Inlet cowl / fan cowl / reverser joints round the nacelle, and the
      // fan cowl halves' hinge line on top and latch line underneath.
      let e = eng747(L);
      let r = length(e.xy);
      if (r < 1.34) {
        let a = atan2(e.x, e.y);
        var cj = min(abs(e.z - 0.95), abs(e.z - 1.90));
        h -= ga * 1.4 * line(cj, gw);
        dirt += line(cj, gw * 2.5);
        let ls = min(abs(a), 3.14159 - abs(a)) * r;
        let on = step(0.95, e.z) * step(e.z, 2.95);
        h -= ga * line(ls, gw) * on;
        dirt += line(ls, gw * 2.0) * on;
        rv = max(rv, rivets(e.z, abs(cj - 0.016), 0.03, rr) * rf);
        rv = max(rv, rivets(r * a, abs(e.z - 0.25), 0.035, rr) * rf);
        h += rv * 0.0008;
      } else {
        let sj = uv.y - 0.25;
        h -= ga * line(sj, gw);
        dirt += line(sj, gw * 2.5);
      }
    }
    case 6u: {
      let ax = abs(L.x);
      var c = CHORDS747[0];
      for (var i = 1; i < 5; i++) { if (abs(ax - CHORDS747[i]) < abs(ax - c)) { c = CHORDS747[i]; } }
      if (abs(Nl.x) < 0.6) {
        let o = abs(ax - c);
        let gd = min(abs(o - 0.07), abs(o - 0.16));
        h -= 0.003 * line(gd, max(0.005, px));
        dirt += line(gd, max(0.006, px));
      }
    }
    default: {}
  }
  return vec3f(h, sat(dirt) * min(1.0, 0.003 / gw), rv);
}

// ---------------------------------------------------------------- the Bonanza (kind 10, aircraft/bonanza)
// An aluminium semi-monocoque: lapped skins on frames and stringers, flush
// rivets (seen as faint rings, not domes), the cabin door and the double
// cargo doors on the right, a nose bowl and two engine compartment doors
// hinged on the top centre line. Body frame; uv as bonanza/mod.rs builds it
// (fuselage: station z, arc from the top; wing and tail: span station,
// signed chord distance). Window and well outlines: keep in step with
// bonanza::window and bonanza::well.

const BON_WIN_R = 0.035;
// (No loops in the Bonanza's code: a loop over a constant table in its
// relief made the NVIDIA driver take half an hour to compile this shader.
// Each family of seams is drawn once, at the nearest one, found by `bon_near`.)
// The signed distance from z to `a`, if it is nearer than `cur`.
fn bon_near(z: f32, a: f32, cur: f32) -> f32 { return select(cur, z - a, abs(z - a) < abs(cur)); }

fn bon_rect(p: vec2f, lo: vec2f, hi: vec2f) -> f32 {
  let c = (lo + hi) * 0.5;
  let h = (hi - lo) * 0.5;
  let q = abs(p - c) - h;
  return length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0);
}
fn window_bonanza(p: vec3f) -> f32 {
  let zy = vec2f(p.z, p.y);
  var d = sd_poly(zy, array<vec2f, 5>(vec2f(-1.054, 0.36), vec2f(-0.762, 0.375), vec2f(-0.292, 0.731), vec2f(-0.241, 0.82), vec2f(-1.092, 0.82)), 5) + 0.01;
  if (abs(p.x) > 0.33 && p.y < 0.75) {
    d = min(d, sd_poly(zy, array<vec2f, 5>(vec2f(-0.56, 0.265), vec2f(0.31, 0.265), vec2f(0.31, 0.665), vec2f(-0.214, 0.665), vec2f(-0.616, 0.36)), 5) - BON_WIN_R);
    d = min(d, bon_rect(zy, vec2f(0.446, 0.280), vec2f(0.956, 0.650)) - BON_WIN_R);
    d = min(d, bon_rect(zy, vec2f(1.196, 0.280), vec2f(1.565, 0.574)) - BON_WIN_R);
    d = min(d, bon_rect(zy, vec2f(1.790, 0.296), vec2f(2.162, 0.513)) - BON_WIN_R);
  }
  return d;
}
fn well_bonanza(p: vec3f) -> f32 {
  var d = 1.0;
  if (p.y < -0.40 && p.z < -1.0) {
    d = min(d, bon_rect(vec2f(p.z, p.x), vec2f(-2.20, -0.115), vec2f(-1.25, 0.115)) + 0.005);
  }
  let ax = abs(p.x);
  if (p.y < -0.2 && p.y > -0.7 && ax > 0.6 && ax < 1.8) {
    let q = vec2f(ax, p.z);
    let c = length(q - vec2f(1.037, 0.404)) - 0.25;
    let s = bon_rect(q, vec2f(1.06, 0.33), vec2f(1.67, 0.48));
    d = min(d, min(c, s) + 0.005);
  }
  return d;
}
// The cabin door (right side), and the cargo doors aft of the wing: distance to their outlines.
fn door_bonanza(L: vec3f) -> f32 {
  return sd_poly(vec2f(L.z, L.y), array<vec2f, 5>(vec2f(-0.69, -0.153), vec2f(0.368, -0.153), vec2f(0.368, 0.77), vec2f(-0.28, 0.77), vec2f(-0.76, 0.39)), 5);
}
fn cargo_bonanza(L: vec3f) -> f32 {
  let q = vec2f(L.z, L.y);
  let o = bon_rect(q, vec2f(1.074 + 0.04, -0.313 + 0.04), vec2f(2.299 - 0.04, 0.654 - 0.04)) - 0.04;
  // (the split between the two leaves)
  let split = select(1.0, abs(L.z - 1.687), L.y > -0.313 && L.y < 0.654);
  return min(abs(o), split);
}
// The black non-slip walkway on the right wing root (the doors are on the right).
fn walkway_bonanza(L: vec3f, aa: f32) -> f32 {
  if (L.x < 0.6 || L.x > 1.2) { return 0.0; }
  let le = -0.4953 + 0.05512 * L.x;
  let c = 2.220 - 0.2135 * L.x;
  let xc = (L.z - le) / c;
  let chord_y = -0.407 + 0.1051 * L.x - (L.z - le) * 0.06;
  if (L.y < chord_y + 0.02) { return 0.0; }
  let e = min(min(L.x - 0.66, 1.15 - L.x), min(xc - 0.18, 0.74 - xc) * c);
  return smoothstep(-aa, aa, e);
}
// The registration, N741TZ (made up), in glyph units as `text` lays them
// out (1 wide, 2 tall, 1.45 apart): ink coverage. Drawn segment by segment
// for the one glyph under p, without `text`'s loop over the font table (see
// the note on loops above).
fn bon_reg(p: vec2f, stroke: f32, aa: f32) -> f32 {
  if (p.y < -0.6 || p.y > 2.6 || p.x < -0.6) { return 0.0; }
  let k = i32(floor((p.x + 0.225) / 1.45));
  if (k < 0 || k > 5) { return 0.0; }
  let q = vec2f(p.x - f32(k) * 1.45, p.y);
  var d = 1e3;
  switch k {
    case 0: { // N
      d = min(min(seg_d(q, vec2f(0.0, 0.0), vec2f(0.0, 2.0)), seg_d(q, vec2f(0.0, 2.0), vec2f(1.0, 0.0))), seg_d(q, vec2f(1.0, 0.0), vec2f(1.0, 2.0)));
    }
    case 1: { // 7
      d = min(seg_d(q, vec2f(0.0, 2.0), vec2f(1.0, 2.0)), seg_d(q, vec2f(1.0, 2.0), vec2f(0.35, 0.0)));
    }
    case 2: { // 4
      d = min(min(seg_d(q, vec2f(0.75, 0.0), vec2f(0.75, 2.0)), seg_d(q, vec2f(0.75, 2.0), vec2f(0.0, 0.65))), seg_d(q, vec2f(0.0, 0.65), vec2f(1.0, 0.65)));
    }
    case 3: { // 1
      d = min(seg_d(q, vec2f(0.55, 0.0), vec2f(0.55, 2.0)), seg_d(q, vec2f(0.55, 2.0), vec2f(0.2, 1.65)));
    }
    case 4: { // T
      d = min(seg_d(q, vec2f(0.0, 2.0), vec2f(1.0, 2.0)), seg_d(q, vec2f(0.5, 2.0), vec2f(0.5, 0.0)));
    }
    default: { // Z
      d = min(min(seg_d(q, vec2f(0.0, 2.0), vec2f(1.0, 2.0)), seg_d(q, vec2f(1.0, 2.0), vec2f(0.0, 0.0))), seg_d(q, vec2f(0.0, 0.0), vec2f(1.0, 0.0)));
    }
  }
  return 1.0 - smoothstep(stroke - aa, stroke + aa, d);
}
// The paint: white, and the scheme picked (bonanza::LIVERIES, livery_style()
// 1: PT.a the main stripe, PT.b its pinstripe, PT.c the tips). In the manner
// of Beech's factory schemes of the 1970s-80s (period-inspired, not traced
// from a drawing; see bonanza::LIVERIES): a broad stripe under the window
// belt from the nose bowl, sweeping up the tail cone and on across the fin
// and rudder at about 39°, a pinstripe over it, coloured wing tips,
// stabiliser tips and spinner. The registration (made up) sits aft of the
// last window over the stripe, both sides reading the right way round.
// Clear of the walkway; the glass, rubber, exhausts and the bare propeller
// are other materials and never come here. Style 0 (none picked, or another
// type flown): plain white with a black registration.
fn livery_bonanza(L: vec3f, aa: f32, pt: u32) -> vec3f {
  var c = WHITE_PAINT;
  if (pt == P_WING) {
    let wk = walkway_bonanza(L, aa);
    if (wk > 0.0) { return mix(c, vec3f(0.035, 0.035, 0.037), wk); }
  }
  let ax = abs(L.x);
  let picked = livery_style() > 0u;
  let main_c = PT.a.rgb;
  let pin_c = PT.b.rgb;
  let tip_c = PT.c.rgb;
  if (picked) {
    // One centre line for the whole stripe, side view: level under the
    // windows (their lower corners at y 0.23), a parabola up the tail cone
    // from z 2.975 meeting the fin's root at z 4.0, y 0.48 at a slope of
    // 0.8, and straight on up the fin. Distance measured square to it.
    let t = clamp(L.z - 2.975, 0.0, 1.025);
    let yc = 0.07 + 0.39 * t * t + 0.8 * max(L.z - 4.0, 0.0);
    let slope = select(0.78 * t, 0.8, L.z > 4.0);
    let d = (L.y - yc) / sqrt(1.0 + slope * slope);
    if (ax < 0.62 && L.z > -2.32 && L.y > -0.3) {
      // A point at the nose bowl.
      let taper = smoothstep(-2.32, -1.95, L.z);
      let hw = 0.062 * taper;
      let band = smoothstep(hw + aa, hw - aa, abs(d));
      let pin = smoothstep(0.011 * taper + aa, 0.011 * taper - aa, abs(d - 0.105 * taper));
      c = mix(c, main_c, band);
      c = mix(c, pin_c, pin);
    }
    // The wing's glass-fibre tip caps beyond the outer rib (bonanza::TIP_RIB).
    if (ax > 4.84 && L.y > -0.5) { c = tip_c; }
    // The stabiliser's rounded tips (outboard of STAB_HALF - 0.17).
    if (ax > 1.685 && L.z > 4.0 && abs(L.y - 0.31) < 0.12) { c = tip_c; }
    // The spinner, ahead of the nose bowl's lip.
    if (L.z < -2.325 && length(vec2f(L.x, L.y - 0.05)) < 0.19) { c = tip_c; }
  }
  // The registration, both sides of the fuselage aft of the last window,
  // over the stripe and clear of the cargo door (to z 2.30): nose-to-tail on
  // the left, tail-to-nose on the right; the stripe's colour, or black.
  if (ax > 0.25 && L.z > 2.33 && L.z < 3.02 && L.y > 0.22 && L.y < 0.42) {
    let h = 0.15;
    let s = h * 0.5;
    let w = 8.25 * s;
    let tx = (2.67 - L.z) * sign(L.x) + w * 0.5;
    let ink = bon_reg(vec2f(tx, L.y - (0.32 - s)) / s, 0.14, aa / s + 0.01);
    c = mix(c, select(vec3f(0.012, 0.012, 0.014), main_c, picked), ink);
  }
  return c;
}
// Exhaust soot from the two stacks under the cowl along the belly, oil
// mist from the breather, a tide mark round each fuel filler, scuffs by the step.
fn weather_bonanza(L: vec3f, N: vec3f, base: vec3f, belly: f32) -> vec4f {
  var c = base;
  let ax = abs(L.x);
  let d = L.z + 1.12;
  if (d > -0.05 && L.y < -0.25 && L.z < 3.0 && ax < 0.62) {
    let cx = 0.23 - min(max(d, 0.0), 1.5) * 0.08;
    let spread = 0.04 + max(d, 0.0) * 0.06;
    let streak = nz(vec3f(L.x * 9.0, L.y * 9.0, L.z * 0.4)).r;
    let soot = exp(-pow((ax - cx) / spread, 2.0)) * exp(-max(d, 0.0) / 1.5) * smoothstep(-0.05, 0.03, d) * sat(belly * 1.6) * (0.5 + 0.7 * streak);
    c = mix(c, vec3f(0.035, 0.03, 0.026), sat(soot * 1.3));
  }
  if (L.y < -0.5 && L.z > -1.2 && L.z < 2.0) {
    let oil = exp(-L.x * L.x / 0.012) * exp(-(L.z + 1.1) / 1.2) * (0.4 + 0.6 * nz(vec3f(L.x * 6.0, 0.0, L.z * 0.6)).b);
    c = mix(c, c * vec3f(0.6, 0.5, 0.34), sat(oil * 0.6));
  }
  let fl = length(vec2f(ax - 1.95, L.z + 0.136));
  if (fl < 0.3 && L.y > -0.3) {
    let f = exp(-fl / 0.07) * (0.4 + 0.6 * nz(vec3f(L.x * 14.0, 0.0, L.z * 14.0)).r) * smoothstep(0.04, 0.05, fl);
    c = mix(c, c * vec3f(0.74, 0.84, 1.0), sat(f * 1.5));
  }
  if (L.x > 0.5 && L.x < 1.25 && L.y > -0.5 && L.y < -0.2 && L.z > -0.4 && L.z < 1.9) {
    let scuff = smoothstep(0.55, 0.9, nz(L * vec3f(5.0, 5.0, 2.0)).g) * sat(N.y * 2.0);
    c = mix(c, c * vec3f(0.80, 0.78, 0.74), scuff * 0.5);
  }
  return vec4f(c, 0.08 * belly);
}
// A flush rivet row along `along`, `across` from it: 0..1 coverage of the head.
fn bon_rivets(along: f32, across: f32, pitch: f32, r: f32) -> f32 { return rivets(along, across, pitch, r); }
fn relief_bonanza(pt0: u32, uv: vec2f, L: vec3f, Nl: vec3f, px: f32) -> vec3f {
  var h = 0.0;
  var dirt = 0.0;
  var rv = 0.0;
  let gw = max(0.0007, px * 0.9);
  let ga = 0.00045 * min(1.0, 0.0012 / gw);
  let rr = 0.0021;
  let rf = 1.0 - smoothstep(rr * 0.5, rr * 1.6, px);
  // The inboard lower wing skin carries the fuselage pattern (so the wheel
  // well can be cut out of it): it gets the wing's seams.
  let pt = select(pt0, 2u, pt0 == 1u && abs(L.x) > 0.5 && Nl.y < -0.7 && L.y < -0.1);
  switch pt {
    case 1u: { // fuselage
      let z = uv.x;
      let s = abs(uv.y);
      // (Each family of seams is drawn once, at the nearest of them: far
      // cheaper for the shader compiler than a seam drawn per entry.)
      var dl = 1.0;
      dl = bon_near(z, -1.118, dl); dl = bon_near(z, 0.368, dl); dl = bon_near(z, 1.074, dl); dl = bon_near(z, 2.299, dl); dl = bon_near(z, 2.794, dl);
      dl = bon_near(z, 3.30, dl); dl = bon_near(z, 3.85, dl); dl = bon_near(z, 4.40, dl); dl = bon_near(z, 4.95, dl); dl = bon_near(z, 5.30, dl);
      var df = 1.0;
      df = bon_near(z, -0.86, df); df = bon_near(z, -0.46, df); df = bon_near(z, 0.05, df); df = bon_near(z, 0.72, df);
      df = bon_near(z, 1.45, df); df = bon_near(z, 1.95, df); df = bon_near(z, 2.55, df); df = bon_near(z, 3.57, df);
      h -= ga * line(dl, gw);
      dirt += line(dl, gw * 2.5);
      rv = max(rv, bon_rivets(s, abs(abs(dl) - 0.011), 0.024, rr) * rf);
      rv = max(rv, bon_rivets(s, abs(df), 0.030, rr) * rf);
      // Longitudinal laps: under the window belt and low on the sides of
      // the cabin; on the tail cone, two laps and stringer rows between.
      let cab = z > -1.12 && z < 2.80 && abs(L.x) > 0.3;
      let dy = select(L.y + 0.30, L.y - 0.19, abs(L.y - 0.19) < abs(L.y + 0.30));
      let ds = select(s - 0.95, s - 0.40, abs(s - 0.40) < abs(s - 0.95));
      let dlong = select(select(1.0, ds, z > 2.79), dy, cab);
      h -= ga * line(dlong, gw);
      dirt += line(dlong, gw * 2.0);
      rv = max(rv, bon_rivets(z, abs(dlong - 0.010), 0.026, rr) * rf);
      let k = (fract(s / 0.16) - 0.5) * 0.16;
      rv = max(rv, bon_rivets(z, k, 0.040, rr * 0.9) * rf * 0.8 * step(2.79, z));
      // The doors on the right (a wider, darker gap); an access panel low on
      // the left of the tail cone (the ELT and battery).
      var dd = 1.0;
      if (L.x > 0.2) { dd = min(abs(door_bonanza(L)), cargo_bonanza(L)); }
      if (L.x < -0.2 && z > 2.3 && z < 2.7) { dd = abs(bon_rect(vec2f(z, L.y), vec2f(2.40, 0.05), vec2f(2.62, 0.22)) - 0.02); }
      h -= ga * 1.8 * line(dd, gw * 1.3);
      dirt += line(dd, gw * 3.0);
    }
    case 2u: { // wing
      let s = abs(uv.x);
      let av = abs(uv.y);
      let c = 2.220 - 0.2135 * min(s, 4.84);
      let xc = av / c;
      let hinge = select(1.0, select(0.79, 0.80, s < 2.86), s > 0.66 && s < 4.81);
      if (s > 0.70 && s < 4.84 && xc < hinge - 0.01) {
        let d = (fract((s - 0.70) / 0.28 + 0.5) - 0.5) * 0.28;
        rv = max(rv, bon_rivets(av, d, 0.026, rr) * rf);
      }
      rv = max(rv, bon_rivets(s, abs(abs(xc - 0.23) * c - 0.011), 0.025, rr) * rf);
      rv = max(rv, bon_rivets(s, abs(xc - 0.62) * c, 0.028, rr) * rf);
      // The leading-edge skin's lap, both surfaces.
      let le = av - 0.12 * c;
      h -= ga * line(le, gw);
      dirt += line(le, gw * 2.0);
      rv = max(rv, bon_rivets(s, abs(le + 0.010), 0.024, rr) * rf);
      // Spanwise splices: at the flap's end, at the tip rib.
      let ds = bon_near(s, 4.84, s - 2.86);
      h -= ga * line(ds, gw);
      dirt += line(ds, gw * 2.0);
      rv = max(rv, bon_rivets(av, abs(abs(ds) - 0.011), 0.024, rr) * rf);
      if (uv.y < 0.0) {
        // The fuel cell access plates under the leading edge.
        let q = vec2f(bon_near(s, 1.95, bon_near(s, 1.45, s - 0.95)), av - 0.13 * c);
        let o = bon_rect(q, vec2f(-0.09, -0.05), vec2f(0.09, 0.05)) - 0.02;
        h -= ga * 1.4 * line(o, gw * 1.2);
        dirt += line(o, gw * 2.5);
        rv = max(rv, bon_rivets(atan2(q.y, q.x) * 0.1, abs(o + 0.012), 0.009, rr * 1.4) * rf);
      } else if (L.x > 0.62 && L.x < 1.18 && xc > 0.17 && xc < 0.75) {
        // The walkway's grit.
        h += 0.0005 * nz(L * 180.0).r * (1.0 - smoothstep(0.0003, 0.0012, px));
      }
    }
    case 5u: { // stabiliser and fin
      let fin = abs(Nl.x) > 0.7;
      let s = select(abs(uv.x), uv.x, fin);
      let av = abs(uv.y);
      var c = 0.0;
      var hinge = 0.0;
      if (fin) {
        let y = 0.42 + s;
        let le = 4.293 + 0.7225 * (y - 0.804);
        c = 5.461 + 0.18 * (y - 0.29) - le;
        hinge = 4.978 + 0.36 * (y - 0.41) - le;
        let d = (fract((s - 0.10) / 0.25 + 0.5) - 0.5) * 0.25;
        if (av < hinge - 0.02 && s > 0.05) { rv = max(rv, bon_rivets(av, d, 0.024, rr) * rf); }
      } else {
        let a = min(s / 1.685, 1.0);
        let le = 4.293 + 0.177 * a;
        c = 5.461 - 0.152 * a - le;
        hinge = 4.928 - le;
        // Rib stations (shop manual, 7.28 .. 66.55 in out).
        var d = bon_near(s, 0.185, s - 0.387);
        d = bon_near(s, 0.648, d); d = bon_near(s, 0.908, d); d = bon_near(s, 1.168, d); d = bon_near(s, 1.429, d); d = bon_near(s, 1.690, d);
        if (av < hinge - 0.02) { rv = max(rv, bon_rivets(av, abs(d), 0.024, rr) * rf); }
      }
      rv = max(rv, bon_rivets(s, abs(av - 0.22 * c), 0.024, rr) * rf);
      rv = max(rv, bon_rivets(s, abs(av - (hinge - 0.02)), 0.026, rr) * rf);
    }
    case 3u: { // control surfaces: rib rows and the spar along the nose
      let s = abs(uv.x);
      let av = abs(uv.y);
      let d = (fract(s / 0.16) - 0.5) * 0.16;
      rv = max(rv, bon_rivets(av, abs(d), 0.028, rr) * rf * 0.9);
      dirt += 0.15 * line(abs(d), gw * 2.0);
    }
    case 4u: { // cowling
      let fwd = L.z > -2.29;
      if (fwd && L.z < -1.10) {
        // The nose bowl's joint, the firewall edge.
        let dz = bon_near(L.z, -1.125, L.z + 2.032);
        h -= ga * 1.4 * line(dz, gw);
        dirt += line(dz, gw * 2.5);
        if (L.z > -2.032) {
          // The engine compartment doors: their bottom edges, the piano hinge on top.
          let dy = L.y + 0.15;
          h -= ga * 1.5 * line(dy, gw);
          dirt += line(dy, gw * 2.5);
          if (L.y > 0.2) {
            let hx = abs(L.x) - 0.006;
            h -= ga * 1.2 * line(hx, gw);
            dirt += line(hx, gw * 2.0);
            let kn = step(0.5, fract(L.z / 0.025)) * line(L.x, 0.004);
            h += 0.0002 * kn;
            dirt += 0.3 * kn;
            // The oil door, left of the hinge.
            let o = bon_rect(vec2f(L.z, L.x), vec2f(-1.66, -0.24), vec2f(-1.44, -0.10)) - 0.02;
            h -= ga * 1.4 * line(o, gw);
            dirt += line(o, gw * 2.0);
          }
          // The louvre panels' outlines.
          let o = bon_rect(vec2f(L.z, L.y), vec2f(-1.97, -0.32), vec2f(-1.54, -0.10)) - 0.01;
          h -= ga * 1.3 * line(o, gw);
          dirt += line(o, gw * 2.0);
        }
        // Camloc-row and frame rivets along the door edges.
        rv = max(rv, bon_rivets(L.z, abs(L.y + 0.15 + 0.012), 0.03, rr) * rf * step(-2.03, L.z));
      }
    }
    case 6u: { // tyre tread: circumferential grooves
      var c = vec3f(sign(L.x) * 1.457, -0.908, 0.404);
      var r = 0.222;
      var g = vec2f(0.022, 0.050);
      if (L.z < -1.0) { c = vec3f(0.0, -1.07, -2.05); r = 0.18; g = vec2f(0.016, 0.038); }
      let ax = abs(L.x - c.x);
      let rad = length(L.yz - c.yz);
      if (rad > r * 0.9) {
        let gd = min(abs(ax - g.x), abs(ax - g.y));
        h -= 0.002 * line(gd, max(0.003, px));
        dirt += line(gd, max(0.004, px));
      }
    }
    default: {}
  }
  // Flush rivets: barely proud of the skin; the ring of grime round each shows them.
  h += rv * 0.00006;
  return vec3f(h, sat(dirt) * min(1.0, 0.0022 / gw), rv);
}
