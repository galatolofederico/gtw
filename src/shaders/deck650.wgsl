// The G650's flight deck (aircraft/g650/deck.rs): what its screens show, in
// the manner of Honeywell's Primus Epic "PlaneView II" as Gulfstream fits it.
// Appended to plane.wgsl after deck747.wgsl and pit18.wgsl, whose text and
// drawing helpers (str_at, num_at, rtext, aa_fill, aa_line, sd_box,
// seg_line, put) it uses; the strings are b747::deck::STRINGS.
//
// What each screen shows (the default PlaneView formats as the photographs
// show them; the crew can pick others in the real aircraft):
// - DU1 / DU4, the PFDs: synthetic vision behind the attitude (sky, a flat
//   terrain grid at the radio height under the aircraft; no terrain model),
//   pitch ladder, bank scale with slip, the aircraft symbol, single-cue flight
//   director, flight path marker; the speed tape (trend vector, Vmo/Mmo
//   barber pole, low-speed band, selected speed bug), Mach and normalised
//   angle of attack under it; the altitude tape (preselect bug and readout,
//   trend, the ground below), the baro setting under it, the vertical speed
//   scale; the mode annunciators across the top (autothrottle, lateral and
//   vertical, armed modes in white) with AP1 and YD; the HSI along the bottom
//   (heading bug, course pointer and deviation for NAV 1, ADF pointer), nav
//   source, ground speed, TAS and TAT; radio altitude, glideslope and
//   localiser scales on an ILS.
// - DU2, the left MFD: the moving map (heading up, range ring, the tuned NAV
//   station with its course, the VORs near by, the heading bug) over a
//   vertical situation display, and the engine instruments on the inboard
//   third: N1, ITT, N2 dials, fuel flow, oil, hydraulic pressure, fuel.
// - DU3, the right MFD: CAS messages (from the simulated state), the trim,
//   flap, speed brake and gear summary under them; the Flight Controls
//   synoptic (spoilers, flaps, the surfaces' hydraulics, trims).
// - SMC 1 the standby attitude, airspeed and altitude; SMC 2 its PFD menu.
// - MCDUs: PROGRESS (live), a flight plan (made-up text) and RADIO (live).
// - The Guidance Panel's windows and mode lights, the cabin altitude, the
//   battery volts, the chronometers and the brake pressure bars.
//
// Sources (layout from photographs; symbology conventions from training
// notes; details not visible anywhere are marked as assumptions):
// - Wikimedia Commons "Cockpit G650 N520GA.jpg" (DU2 with the Flight Controls
//   synoptic and the engine strip on its inboard third: N1, ITT and N2 dials
//   in pairs, then FF, oil pressure and temperature, vibration, hydraulic
//   pressure, fuel tank temperature and fuel quantity; DU3 with the CAS list
//   top left, the trim and flap summary under it) and "Gulfstream G650ER,
//   EBACE 2018 (BL7C0735).jpg" (PFDs: full-screen synthetic vision, three
//   mode boxes at the top, translucent tapes, preselect over the altitude
//   tape, baro under it, the hatched ground band, HSI at the bottom centre;
//   DU2 map over a VSD with the engine strip; DU3 CAS and a chart).
// - S. Foltz, G450/G550 Expanded Notes (code7700.com, rev 9.0, 2022):
//   airspeed and altitude trend vectors are 6 s predictions; the normalised
//   AOA (0..1, 1 = stall) is shown under the speed tape; CAS colours red
//   warning, amber caution, cyan advisory, white status; synoptic colours
//   grey not in use, green in use, amber failed/disagree; the Aircraft
//   Configuration warning (flaps, trim, speed brake, park brake, reversers).
// - J. Albright, G450/G550 to G650 differences (2015): enhanced synthetic
//   vision PFD standard on the G650; AP1/AP2 annunciated on the PFD; the SMC
//   carries the standby instruments.
// - Honeywell colour convention (assumed, not seen in a G650 manual): active
//   modes green, armed white, selected targets cyan, NAV source green, FMS
//   magenta; trend vectors drawn green.
//
// Simplified or decorative: no FMS (no route, VNAV, LNAV, V-speeds or
// minimums); the synthetic terrain is flat; the map has no terrain, weather
// or traffic; the FD cue is computed here from the autopilot's mode and
// targets, not from the autopilot's own commands; the vertical speed, ITT
// and oil values are the sim's estimates; vibration, fuel temperature and
// the trims other than pitch are not simulated (shown centred); both engines
// read the same (the sim has one engine state); the MCDU flight plan page is
// made-up text; text is upper case only (the stroke font has no lower case);
// attitude and heading come straight from the aircraft's once the bus is
// powered (main.rs `instruments`: the IRS, without its alignment time or
// failure flags).
//
// Interface (unchanged from B1):
// - `display650(id, uv, px)` from plane.wgsl (`case 14u`) for every
//   `mat::DISPLAY` quad whose id is 60 or more; it returns the emitted colour.
// - id: g650/deck.rs `screen::*`: 60-63 the DUs (PFD, MFD, MFD, PFD), 64/65
//   the SMCs, 66/67/68 the MCDUs, 69-72 the GP's speed, heading, V/S and
//   altitude windows, 73 cabin altitude, 74/75 battery volts, 76/77 the
//   chronometers, 78 brake pressure, 80 + k the GP mode lights (k: AP, YD,
//   HDG, NAV, APPR, BC, ALT, VS, FLCH, VNAV, A/T, PFD CMD).
// - uv: -1..1 across and up each screen as the pilot sees it; the DUs are
//   4:3, so `uv * vec2f(4/3, 1)` is square.
// - Data: O.params (main.rs `instruments`: IAS kt, altitude ft, V/S fpm,
//   heading; pitch, bank, turn, slip; N1, ITT, fuel flow kg/h an engine, fuel
//   kg; Mach, GS, radio altitude ft, TAT; throttle, local time s, gear, flap
//   deg; lights); G.cockpit rows 0-3 (baro, NAV/ADF, powered) and 8-11 (the
//   autopilot, trim, speed brake, spoilers, reversers, flags, brakes, AOA,
//   battery, cabin altitude, speed trend: common.wgsl); G.nav.

const PV_W = vec3f(0.92, 0.93, 0.95);
const PV_G = vec3f(0.22, 1.0, 0.32);
const PV_C = vec3f(0.22, 0.88, 1.0);
const PV_M = vec3f(1.0, 0.32, 1.0);
const PV_A = vec3f(1.0, 0.68, 0.06);
const PV_R = vec3f(1.0, 0.10, 0.06);
const PV_GREY = vec3f(0.40, 0.40, 0.42);
// Window backgrounds: the synoptics' warm dark grey, the engine strip's.
const PV_WIN = vec3f(0.050, 0.046, 0.046);
const PV_ENG = vec3f(0.030, 0.030, 0.032);
// The G650's speed limits (g650/mod.rs Spec: vne 340 kt, mmo 0.925).
// The sense of the attitude's roll on screen (rot2 of a screen point by
// BANK650 * bank gives horizon-frame coordinates).
const BANK650 = 1.0;
const VMO650 = 340.0;
const MMO650 = 0.925;

fn ap650() -> u32 { return u32(G.cockpit[8].x + 0.5); }
fn fl650() -> u32 { return u32(G.cockpit[10].y + 0.5); }
// Width of string s at glyph height h.
fn sw650(s: u32, h: f32) -> f32 { return (f32(str_len(s)) * 1.45 - 0.45) * h * 0.5; }
fn tri650(p: vec2f, a: vec2f, b: vec2f, c: vec2f) -> f32 { return sd_poly(p, array<vec2f, 5>(a, b, c, a, a), 3); }
// Where the aircraft is, metres east and north on the sphere (from the camera, in the cockpit).
fn own_en650() -> vec2f {
  let P = -G.planet.xyz;
  let r = length(P);
  let lat = asin(clamp(P.z / r, -1.0, 1.0));
  return vec2f(atan2(P.y, P.x) * r * cos(lat), lat * r);
}
// The vertical speed scale: 0.5, 1, 2, 4, 6 thousand fpm.
fn vsi_y650(v: f32) -> f32 {
  let a = abs(v);
  var y = a * 0.16;
  if (a > 1.0) { y = 0.16 + (min(a, 2.0) - 1.0) * 0.11; }
  if (a > 2.0) { y = 0.27 + (min(a, 6.0) - 2.0) * 0.0275; }
  return sign(v) * y;
}

// ---------------------------------------------------------------- PFD

// Synthetic vision behind the attitude: sky, and a flat terrain at the radio
// height with a 1 km grid and haze toward the horizon. k: units a degree.
fn svs650(p: vec2f, ac: vec2f, k: f32, aa: f32) -> vec3f {
  let P0 = O.params[0];
  let P1 = O.params[1];
  let pitch = P1.x;
  let q = rot2(p - ac, rad(P1.y) * BANK650);
  let el = q.y / k + pitch;
  if (el >= 0.0) {
    return mix(vec3f(0.30, 0.50, 0.95), vec3f(0.03, 0.13, 0.60), sqrt(sat(el / 30.0)));
  }
  let tn = tan(rad(min(-el, 89.0)));
  let h = max(O.params[3].z, 4.0) * 0.3048;
  let d = h / max(tn, 1e-4);
  let side = d * tan(rad(clamp(q.x / k, -80.0, 80.0)));
  let ps = rad(P0.w);
  let g = own_en650() + vec2f(sin(ps), cos(ps)) * d + vec2f(cos(ps), -sin(ps)) * side;
  let n1 = nz(vec3f(g / 9000.0, 0.31)).g;
  let n2 = nz(vec3f(g / 1700.0, 0.77)).r;
  var c = mix(vec3f(0.15, 0.19, 0.09), vec3f(0.30, 0.24, 0.13), sat(n1 * 1.8 - 0.4));
  c *= 0.85 + 0.3 * n2;
  // The grid, faded out where it gets finer than the pixels.
  let pxm = d * rad(aa / k) / max(tn, 0.01);
  let gf = abs(fract(g / 1000.0 + 0.5) - 0.5) * 1000.0;
  let w = max(pxm, 1.5);
  let gl = (1.0 - smoothstep(0.5 * w, 1.5 * w, min(gf.x, gf.y))) * (1.0 - smoothstep(60.0, 250.0, pxm));
  c = mix(c, vec3f(0.45, 0.42, 0.30), gl * 0.55);
  return mix(c, vec3f(0.34, 0.38, 0.44), exp(-tn * 22.0));
}

// The aircraft symbol / single-cue command bars: a delta, the point at the origin.
fn delta650(q: vec2f) -> f32 {
  let s = vec2f(abs(q.x), q.y);
  return sd_poly(s, array<vec2f, 5>(vec2f(0.0, 0.0), vec2f(0.21, -0.075), vec2f(0.11, -0.075), vec2f(0.0, -0.035), vec2f(0.0, 0.0)), 4);
}

// One FMA box: the active mode (green), an armed one (white) to its right.
fn fma650(c0: vec3f, p: vec2f, x: f32, act: u32, arm: u32, aa: f32) -> vec3f {
  var c = c0;
  let y = 0.945;
  let b = sd_box(p, vec2f(x, y), vec2f(0.19, 0.042));
  if (b < 0.0) { c = mix(c, vec3f(0.01, 0.03, 0.12), 0.65); }
  c = put(c, PV_GREY, aa_line(b, 0.003, aa));
  let ax = select(x, x - 0.06, arm > 0u);
  if (act > 0u) { c = put(c, PV_G, str_at(p, vec2f(ax, y), act, 0.052, aa, 0.0)); }
  if (arm > 0u) { c = put(c, PV_W, str_at(p, vec2f(x + 0.115, y), arm, 0.042, aa, 0.0)); }
  return c;
}

fn pfd650(p: vec2f, aa: f32) -> vec3f {
  let P0 = O.params[0];
  let P1 = O.params[1];
  let P3 = O.params[3];
  let C8 = G.cockpit[8];
  let C9 = G.cockpit[9];
  let C10 = G.cockpit[10];
  let C11 = G.cockpit[11];
  let bits = ap650();
  let fl = fl650();
  let ap_on = (bits & 1u) != 0u;
  let ias = P0.x;
  let alt = P0.y;
  let vs = P0.z;
  let hdg = P0.w;
  let pitch = P1.x;
  let bank = P1.y;
  let dark = vec3f(0.015, 0.015, 0.02);
  let ac = vec2f(0.0, 0.40);
  let k = 0.026;
  var c = svs650(p, ac, k, aa);
  let q = rot2(p - ac, rad(bank) * BANK650);
  // The zero-pitch line across the screen.
  c = put(c, PV_W, aa_line(q.y + pitch * k, 0.0045, aa));
  // Pitch ladder: every 5 degrees, numbered every 10, dashed below the horizon.
  let rel = q.y / k + pitch;
  let st = round(rel / 5.0) * 5.0;
  if (abs(st) > 0.1 && abs(st) <= 85.0 && abs(p.x) < 0.42 && p.y > -0.05 && p.y < 0.86) {
    let ten = abs(st % 10.0) < 0.1;
    let hw = select(0.055, 0.12, ten);
    let dash = select(1.0, step(0.35, fract(abs(q.x) * 16.0)), st < 0.0);
    c = put(c, PV_W, aa_line((rel - st) * k, 0.0035, aa) * step(abs(q.x), hw) * dash);
    if (ten) {
      let y = (st - pitch) * k;
      c = put(c, PV_W, max(num_at(q, vec2f(-hw - 0.05, y), abs(st), 0u, 1u, 0.04, aa, 0.0), num_at(q, vec2f(hw + 0.05, y), abs(st), 0u, 1u, 0.04, aa, 0.0)));
    }
  }
  // Bank scale: ticks at 10, 20, 30, 45 and 60 degrees, the zero index, the pointer and slip.
  let br = 0.42;
  let rp = p - ac;
  let rl = length(rp);
  if (rp.y > 0.0 && rl > br - 0.005 && rl < br + 0.07) {
    let ang = abs(degrees(atan2(rp.x, rp.y)));
    let marks = array<f32, 5>(10.0, 20.0, 30.0, 45.0, 60.0);
    for (var i = 0; i < 5; i++) {
      let len = select(0.028, 0.05, marks[i] == 30.0 || marks[i] == 60.0);
      let d = abs(ang - marks[i]) * 0.017453 * rl;
      c = put(c, PV_W, (1.0 - smoothstep(0.0035, 0.0035 + aa, d)) * step(br, rl) * step(rl, br + len));
    }
  }
  c = put(c, PV_W, aa_line(tri650(rp, vec2f(-0.024, br + 0.048), vec2f(0.024, br + 0.048), vec2f(0.0, br + 0.006)), 0.0035, aa));
  let rq = rot2(rp, rad(bank) * BANK650);
  let bank_col = select(PV_W, PV_A, abs(bank) > 35.0);
  c = put(c, bank_col, aa_fill(tri650(rq, vec2f(-0.024, br - 0.042), vec2f(0.024, br - 0.042), vec2f(0.0, br - 0.004)), aa));
  let slip = clamp(P1.w, -1.0, 1.0) * 0.04;
  c = put(c, bank_col, aa_fill(sd_box(rq, vec2f(slip, br - 0.058), vec2f(0.026, 0.007)), aa));
  // Flight path marker (conformal in pitch; no drift).
  let gs_kt = P3.y;
  if (gs_kt > 30.0) {
    let gamma = degrees(atan2(vs / 196.85, gs_kt * 0.5144));
    let f = q - vec2f(0.0, (gamma - pitch) * k);
    var fp = aa_line(length(f) - 0.02, 0.004, aa);
    fp = max(fp, seg_line(f, vec2f(0.02, 0.0), vec2f(0.06, 0.0), 0.004, aa));
    fp = max(fp, seg_line(f, vec2f(-0.02, 0.0), vec2f(-0.06, 0.0), 0.004, aa));
    fp = max(fp, seg_line(f, vec2f(0.0, 0.02), vec2f(0.0, 0.045), 0.004, aa));
    c = put(c, PV_G, fp);
  }
  // Single-cue flight director (magenta), with the autopilot: the bank and
  // pitch its mode asks for, estimated here from the targets.
  if (ap_on) {
    var bcmd = bank;
    let lat = bits & 28u;
    let navc = (bits & 256u) != 0u;
    if (lat == 4u || ((lat == 8u || lat == 16u) && !navc)) {
      bcmd = clamp(wrapa(rad(C8.y - hdg)) * 57.29578 * 1.2, -25.0, 25.0);
    } else if (lat != 0u) {
      bcmd = clamp(G.nav.x * 25.0, -25.0, 25.0);
    }
    var pcmd = pitch;
    let vert = bits & 224u;
    if (vert == 32u) { pcmd = pitch - clamp(vs / 1000.0 * 1.5, -5.0, 5.0); }
    if (vert == 64u) { pcmd = pitch + clamp((C9.x - vs) / 1000.0 * 2.0, -5.0, 5.0); }
    if (vert == 128u) { pcmd = pitch + clamp(G.nav.y * 3.0, -3.0, 3.0); }
    // Its centre sits (pcmd - pitch) above the boresight square to the
    // horizon; it leans by the bank still wanted (right wing down for more right bank).
    let v = p - ac - rot2(vec2f(0.0, (pcmd - pitch) * k), -rad(bank) * BANK650);
    let fq = rot2(v, rad(bcmd - bank) * BANK650) / 0.92;
    let fd = delta650(fq + vec2f(0.0, 0.012));
    c = put(c, vec3f(0.0), aa_fill(fd - 0.004, aa));
    c = put(c, PV_M, aa_fill(fd, aa));
  }
  // The aircraft symbol, white outlined in black.
  let sym = delta650(p - ac);
  c = put(c, vec3f(0.0), aa_fill(sym - 0.005, aa));
  c = put(c, PV_W * 0.9, aa_fill(sym, aa));

  // Glideslope and localiser scales on an ILS.
  if ((u32(G.nav.w + 0.5) & 8u) != 0u) {
    for (var i = -2; i <= 2; i++) {
      if (i == 0) { continue; }
      c = put(c, PV_W, aa_line(length(p - vec2f(0.64, ac.y + f32(i) * 0.075)) - 0.012, 0.003, aa));
      c = put(c, PV_W, aa_line(length(p - vec2f(f32(i) * 0.09, ac.y - 0.37)) - 0.012, 0.003, aa));
    }
    let gd = vec2f(0.64, ac.y + clamp(G.nav.y, -1.0, 1.0) * 0.15);
    let ld = vec2f(clamp(G.nav.x, -1.0, 1.0) * 0.18, ac.y - 0.37);
    c = put(c, PV_G, aa_fill(abs(p.x - gd.x) * 1.4 + abs(p.y - gd.y) - 0.026, aa));
    c = put(c, PV_G, aa_fill(abs(p.x - ld.x) + abs(p.y - ld.y) * 1.4 - 0.026, aa));
  }
  // Radio altitude.
  if (P3.z < 2500.0) {
    let rb = sd_box(p, vec2f(0.0, ac.y - 0.21), vec2f(0.085, 0.034));
    if (rb < 0.0) { c = dark; }
    c = put(c, PV_W, aa_line(rb, 0.003, aa));
    c = put(c, PV_G, num_at(p, vec2f(0.0, ac.y - 0.21), round(max(P3.z, 0.0) / select(10.0, 5.0, P3.z < 200.0)) * select(10.0, 5.0, P3.z < 200.0), 0u, 1u, 0.05, aa, 0.0));
  }

  // ---- speed tape
  let sx = -1.06;
  let shw = 0.125;
  let sc = 0.0062;
  let th = 0.42;
  let sr = sx + shw;
  if (abs(p.x - sx) < shw && abs(p.y - ac.y) < th) {
    c = mix(c, dark, 0.55);
    let sv = ias + (p.y - ac.y) / sc;
    let t10 = round(sv / 10.0) * 10.0;
    if (t10 >= 30.0) { c = put(c, PV_W, aa_line(p.y - ((t10 - ias) * sc + ac.y), 0.0035, aa) * step(sr - 0.032, p.x)); }
    let t20 = round(sv / 20.0) * 20.0;
    if (t20 >= 40.0) { c = put(c, PV_W, num_at(p, vec2f(sr - 0.045, (t20 - ias) * sc + ac.y), t20, 0u, 1u, 0.046, aa, 1.0)); }
    // Vmo/Mmo: the red and black barber pole.
    let vmo = min(VMO650, ias * MMO650 / max(P3.x, 0.05));
    let yv = (vmo - ias) * sc + ac.y;
    if (p.y > yv && p.x > sr - 0.024) { c = select(vec3f(0.0), PV_R, fract((p.y - yv) / 0.04) < 0.5); }
    // Low speed: red and black below the stall (normalised AOA 1), amber up to 1.13 times it.
    if ((fl & 16u) == 0u && C10.w > 0.05) {
      let vst = ias * sqrt(clamp(C10.w, 0.05, 4.0));
      let ys = (vst - ias) * sc + ac.y;
      let ya = (vst * 1.13 - ias) * sc + ac.y;
      if (p.x > sr - 0.024) {
        if (p.y < ys) { c = select(vec3f(0.0), PV_R, fract((p.y - ys) / 0.04) < 0.5); }
        else if (p.y < ya && p.x > sr - 0.012) { c = PV_A; }
      }
    }
    // The selected speed (cyan), parked at the end when off the scale.
    if (C8.w > 1.0) {
      let yb = clamp((C8.w - ias) * sc + ac.y, ac.y - th, ac.y + th);
      let bug = max(sd_box(p, vec2f(sr - 0.004, yb), vec2f(0.016, 0.026)), -sd_box(p, vec2f(sr - 0.004, yb), vec2f(0.03, 0.007)));
      c = put(c, PV_C, aa_fill(bug, aa));
    }
  }
  // Speed trend (green), from the pointer.
  let tr = clamp(C11.w, -th / sc, th / sc);
  if (abs(tr) > 2.0) {
    let ty = ac.y + tr * sc;
    c = put(c, PV_G, seg_line(p, vec2f(sr - 0.006, ac.y), vec2f(sr - 0.006, ty), 0.004, aa));
    c = put(c, PV_G, aa_fill(tri650(p, vec2f(sr - 0.024, ty - sign(tr) * 0.025), vec2f(sr + 0.012, ty - sign(tr) * 0.025), vec2f(sr - 0.006, ty)), aa));
  }
  // The readout.
  let sb = min(sd_box(p, vec2f(sx - 0.01, ac.y), vec2f(0.11, 0.045)), tri650(p, vec2f(sx + 0.095, ac.y + 0.03), vec2f(sx + 0.095, ac.y - 0.03), vec2f(sr + 0.01, ac.y)));
  if (sb < 0.0) { c = vec3f(0.0); }
  c = put(c, PV_W, aa_line(sb, 0.003, aa));
  c = put(c, PV_W, num_at(p, vec2f(sx + 0.075, ac.y), floor(max(ias, 0.0)), 0u, 1u, 0.068, aa, 1.0));
  if (C8.w > 1.0) { c = put(c, PV_C, num_at(p, vec2f(sx, ac.y + th + 0.05), round(C8.w), 0u, 1u, 0.055, aa, 0.0)); }
  if (P3.x >= 0.40) {
    c = put(c, PV_W, str_at(p, vec2f(-1.30, ac.y - th - 0.055), S_M, 0.046, aa, -1.0));
    c = put(c, PV_W, num_at(p, vec2f(sx + 0.11, ac.y - th - 0.055), P3.x, 3u, 1u, 0.05, aa, 1.0));
  }
  c = put(c, PV_W, str_at(p, vec2f(-1.30, ac.y - th - 0.13), S_AOA, 0.04, aa, -1.0));
  c = put(c, PV_G, num_at(p, vec2f(sx + 0.11, ac.y - th - 0.13), clamp(C10.w, 0.0, 1.5), 2u, 1u, 0.046, aa, 1.0));

  // ---- altitude tape
  let ax = 0.93;
  let ahw = 0.13;
  let asc = 0.00085;
  let al = ax - ahw;
  if (abs(p.x - ax) < ahw && abs(p.y - ac.y) < th) {
    c = mix(c, dark, 0.55);
    let av = alt + (p.y - ac.y) / asc;
    let t1 = round(av / 100.0) * 100.0;
    c = put(c, PV_W, aa_line(p.y - ((t1 - alt) * asc + ac.y), 0.0035, aa) * step(p.x, al + 0.03));
    let t2 = round(av / 200.0) * 200.0;
    c = put(c, PV_W, num_at(p, vec2f(al + 0.042, (t2 - alt) * asc + ac.y), t2, 0u, 1u, 0.044, aa, -1.0));
    // The ground, hatched amber.
    let yg = ac.y - max(P3.z, 0.0) * asc;
    if (p.y < yg) { c = mix(c, PV_A * 0.75, step(0.5, fract((p.x + p.y) * 20.0))); }
    // The preselected altitude's bug (cyan).
    if (C8.z > 0.5) {
      let yb = clamp((C8.z - alt) * asc + ac.y, ac.y - th, ac.y + th);
      let bug = max(sd_box(p, vec2f(al + 0.004, yb), vec2f(0.016, 0.03)), -sd_box(p, vec2f(al + 0.004, yb), vec2f(0.03, 0.008)));
      c = put(c, PV_C, aa_fill(bug, aa));
    }
  }
  // Altitude trend (6 s).
  let at = clamp(vs * 0.1 * asc, -th, th);
  if (abs(vs) > 150.0) {
    c = put(c, PV_G, seg_line(p, vec2f(al + 0.006, ac.y), vec2f(al + 0.006, ac.y + at), 0.004, aa));
  }
  let ab = min(sd_box(p, vec2f(ax + 0.012, ac.y), vec2f(0.12, 0.045)), tri650(p, vec2f(al + 0.03, ac.y + 0.03), vec2f(al + 0.03, ac.y - 0.03), vec2f(al - 0.035, ac.y)));
  if (ab < 0.0) { c = vec3f(0.0); }
  c = put(c, PV_W, aa_line(ab, 0.003, aa));
  c = put(c, PV_W, num_at(p, vec2f(ax + 0.112, ac.y), round(alt / 20.0) * 20.0, 0u, 1u, 0.064, aa, 1.0));
  if (C8.z > 0.5) {
    let pb = sd_box(p, vec2f(ax, ac.y + th + 0.05), vec2f(0.12, 0.035));
    if (pb < 0.0) { c = vec3f(0.0); }
    c = put(c, PV_GREY, aa_line(pb, 0.003, aa));
    c = put(c, PV_C, num_at(p, vec2f(ax, ac.y + th + 0.05), round(C8.z / 100.0) * 100.0, 0u, 1u, 0.052, aa, 0.0));
  }
  c = put(c, PV_C, num_at(p, vec2f(ax + 0.04, ac.y - th - 0.055), G.cockpit[0].x / 100.0, 2u, 1u, 0.05, aa, 1.0));
  c = put(c, PV_C, str_at(p, vec2f(ax + 0.06, ac.y - th - 0.055), S_IN, 0.04, aa, -1.0));

  // ---- vertical speed
  let vx0 = 1.095;
  let vx1 = 1.30;
  if (p.x > vx0 && p.x < vx1 && abs(p.y - ac.y) < 0.40) {
    c = mix(c, dark, 0.45);
    c = put(c, PV_W, aa_line(p.y - ac.y, 0.004, aa) * step(p.x, vx0 + 0.045));
    let ms = array<f32, 4>(0.5, 1.0, 2.0, 4.0);
    for (var i = 0; i < 4; i++) {
      let y = vsi_y650(ms[i]);
      let tick = aa_line(abs(p.y - ac.y) - y, 0.0035, aa) * step(p.x, vx0 + select(0.03, 0.02, i == 0));
      c = put(c, PV_W, tick);
      if (i > 0) { c = put(c, PV_W, max(num_at(p, vec2f(vx0 + 0.06, ac.y + y), ms[i], 0u, 1u, 0.04, aa, 0.0), num_at(p, vec2f(vx0 + 0.06, ac.y - y), ms[i], 0u, 1u, 0.04, aa, 0.0))); }
    }
    if ((bits & 1u) != 0u && (bits & 224u) == 64u) {
      let yb = ac.y + vsi_y650(C9.x / 1000.0);
      c = put(c, PV_C, aa_fill(sd_box(p, vec2f(vx0 + 0.012, yb), vec2f(0.012, 0.02)), aa));
    }
  }
  if (p.x < vx1) {
    c = put(c, PV_W, seg_line(p, vec2f(vx1 + 0.15, ac.y), vec2f(vx0 + 0.004, ac.y + vsi_y650(vs / 1000.0)), 0.005, aa));
  }
  if (abs(vs) >= 500.0) {
    c = put(c, PV_W, num_at(p, vec2f(1.20, ac.y + select(-0.45, 0.45, vs > 0.0)), round(abs(vs) / 50.0) * 50.0, 0u, 1u, 0.045, aa, 0.0));
  }

  // ---- mode annunciators: autothrottle, lateral, vertical; AP1 and YD
  if (ap_on || (bits & 2u) != 0u) {
    let at_mode = select(0u, S_SPD, (bits & 2u) != 0u);
    var la = 0u;
    var lr = 0u;
    var va = 0u;
    var vr = 0u;
    if (ap_on) {
      let ils = (u32(G.nav.w + 0.5) & 8u) != 0u;
      let lat = bits & 28u;
      let navc = (bits & 256u) != 0u;
      la = S_ROLL;
      if (lat == 4u) { la = S_HDG; }
      if (lat == 8u) { if (navc) { la = select(S_VOR, S_LOC, ils); } else { la = S_HDG; lr = select(S_VOR, S_LOC, ils); } }
      if (lat == 16u) { if (navc) { la = S_LOC; } else { la = S_HDG; lr = S_LOC; } }
      let vert = bits & 224u;
      va = S_PTCH;
      if (vert == 32u) { va = S_ALT; }
      if (vert == 64u) { va = select(S_VS, S_ASEL, (bits & 1024u) != 0u); if ((bits & 1024u) == 0u) { vr = S_ASEL; } }
      if (vert == 128u) { va = S_GS; }
      if (lat == 16u && (bits & 512u) == 0u && vert != 128u) { vr = S_GS; }
    }
    c = fma650(c, p, -0.40, at_mode, 0u, aa);
    c = fma650(c, p, 0.0, la, lr, aa);
    c = fma650(c, p, 0.40, va, vr, aa);
  } else {
    c = fma650(c, p, -0.40, 0u, 0u, aa);
    c = fma650(c, p, 0.0, 0u, 0u, aa);
    c = fma650(c, p, 0.40, 0u, 0u, aa);
  }
  if (ap_on) { c = put(c, PV_G, str_at(p, vec2f(0.70, 0.945), S_AP1, 0.052, aa, 0.0)); }
  if ((fl & 9u) == 9u) { c = put(c, PV_G, str_at(p, vec2f(0.70, 0.865), S_YD, 0.046, aa, 0.0)); }

  // ---- HSI
  let hc = vec2f(0.0, -0.98);
  let hr = 0.50;
  let hp = p - hc;
  let hl = length(hp);
  if (hl < hr + 0.004) {
    c = mix(c, vec3f(0.008), 0.85);
    let a = atan2(hp.x, hp.y);
    let deg = hdg + degrees(a);
    let t5 = round(deg / 5.0) * 5.0;
    let d = abs(wrapa(a - rad(t5 - hdg))) * hl;
    let long_ = abs(t5 % 10.0) < 0.1;
    c = put(c, PV_W, (1.0 - smoothstep(0.003, 0.003 + aa, d)) * step(hr - select(0.024, 0.044, long_), hl));
    let t30 = round(deg / 30.0) * 30.0;
    let la = rad(t30 - hdg);
    let ld = rot2(hp - vec2f(sin(la), cos(la)) * (hr - 0.085), la);
    let k30 = (u32(round(t30 + 720.0)) / 30u) % 12u;
    var s = 0u;
    switch k30 { case 0u: { s = S_N; } case 3u: { s = S_E; } case 6u: { s = S_S; } case 9u: { s = S_W; } default: {} }
    if (s > 0u) { c = put(c, PV_W, str_at(ld, vec2f(0.0), s, 0.058, aa, 0.0)); }
    else { c = put(c, PV_W, num_at(ld, vec2f(0.0), f32(k30 * 3u), 0u, 1u, 0.05, aa, 0.0)); }
    // NAV 1's course pointer (green) and deviation; the TO/FROM arrow.
    let nf = u32(G.nav.w + 0.5);
    let cq = rot2(hp, rad(G.nav.z - hdg));
    var cp = seg_line(cq, vec2f(0.0, 0.17), vec2f(0.0, 0.38), 0.006, aa);
    cp = max(cp, aa_fill(tri650(cq, vec2f(-0.03, 0.36), vec2f(0.03, 0.36), vec2f(0.0, 0.43)), aa));
    cp = max(cp, seg_line(cq, vec2f(0.0, -0.38), vec2f(0.0, -0.17), 0.006, aa));
    if ((nf & 1u) != 0u) {
      let dx = clamp(G.nav.x, -1.0, 1.0) * 0.16;
      cp = max(cp, seg_line(cq, vec2f(dx, -0.15), vec2f(dx, 0.15), 0.006, aa));
      if ((nf & 6u) != 0u) {
        let ty = select(-0.1, 0.1, (nf & 2u) != 0u);
        let tdir = select(-1.0, 1.0, (nf & 2u) != 0u);
        cp = max(cp, aa_fill(tri650(cq, vec2f(-0.08, ty), vec2f(-0.04, ty), vec2f(-0.06, ty + tdir * 0.035)), aa));
      }
    }
    for (var i = -2; i <= 2; i++) {
      if (i == 0) { continue; }
      c = put(c, PV_W, aa_line(length(cq - vec2f(f32(i) * 0.08, 0.0)) - 0.012, 0.003, aa));
    }
    c = put(c, PV_G, cp);
    // ADF bearing pointer (cyan).
    if (G.cockpit[1].w > 0.5) {
      let bq = rot2(hp, rad(G.cockpit[1].z));
      var bp = seg_line(bq, vec2f(0.0, -hr + 0.05), vec2f(0.0, hr - 0.11), 0.003, aa);
      bp = max(bp, seg_line(bq, vec2f(0.0, hr - 0.11), vec2f(-0.025, hr - 0.15), 0.003, aa));
      bp = max(bp, seg_line(bq, vec2f(0.0, hr - 0.11), vec2f(0.025, hr - 0.15), 0.003, aa));
      c = put(c, PV_C, bp);
    }
    // The heading bug (cyan).
    let hq = rot2(hp, rad(C8.y - hdg));
    let bug = max(sd_box(hq, vec2f(0.0, hr - 0.012), vec2f(0.032, 0.014)), -sd_box(hq, vec2f(0.0, hr - 0.004), vec2f(0.009, 0.03)));
    c = put(c, PV_C, aa_fill(bug, aa));
    // The aircraft.
    var am = seg_line(hp, vec2f(0.0, -0.06), vec2f(0.0, 0.06), 0.006, aa);
    am = max(am, seg_line(hp, vec2f(-0.055, 0.01), vec2f(0.055, 0.01), 0.006, aa));
    am = max(am, seg_line(hp, vec2f(-0.022, -0.05), vec2f(0.022, -0.05), 0.006, aa));
    c = put(c, PV_W, am);
  }
  // Lubber line and the heading readout over it.
  c = put(c, PV_W, aa_fill(tri650(p - hc, vec2f(-0.022, hr + 0.03), vec2f(0.022, hr + 0.03), vec2f(0.0, hr - 0.005)), aa));
  let hb = sd_box(p, vec2f(0.0, hc.y + hr + 0.07), vec2f(0.075, 0.036));
  if (hb < 0.0) { c = vec3f(0.0); }
  c = put(c, PV_W, aa_line(hb, 0.003, aa));
  c = put(c, PV_W, num_at(p, vec2f(0.0, hc.y + hr + 0.07), (round(hdg) + 359.0) % 360.0 + 1.0, 0u, 3u, 0.054, aa, 0.0));
  // Nav source and course (left), ground speed, TAS and TAT (right).
  let ils = (u32(G.nav.w + 0.5) & 8u) != 0u;
  let xl = -1.28;
  c = put(c, PV_G, str_at(p, vec2f(xl, -0.50), select(S_VOR1, S_LOC1, ils), 0.05, aa, -1.0));
  c = put(c, PV_G, str_at(p, vec2f(xl, -0.59), S_CRS, 0.042, aa, -1.0));
  c = put(c, PV_G, num_at(p, vec2f(xl + 0.13, -0.59), (round(G.nav.z) + 359.0) % 360.0 + 1.0, 0u, 3u, 0.046, aa, -1.0));
  if (G.cockpit[1].x > 1.0) { c = put(c, PV_W, num_at(p, vec2f(xl, -0.68), G.cockpit[1].x, 2u, 1u, 0.046, aa, -1.0)); }
  if (G.cockpit[1].w > 0.5) { c = put(c, PV_C, str_at(p, vec2f(xl, -0.80), S_ADF, 0.042, aa, -1.0)); }
  let xr = 0.74;
  c = put(c, PV_W, str_at(p, vec2f(xr, -0.50), S_GS, 0.042, aa, -1.0));
  c = put(c, PV_W, num_at(p, vec2f(xr + 0.30, -0.50), round(P3.y), 0u, 1u, 0.048, aa, 1.0));
  c = put(c, PV_W, str_at(p, vec2f(xr, -0.59), S_TAS, 0.042, aa, -1.0));
  c = put(c, PV_W, num_at(p, vec2f(xr + 0.30, -0.59), round(ias * (1.0 + alt * 0.0000168)), 0u, 1u, 0.048, aa, 1.0));
  c = put(c, PV_W, str_at(p, vec2f(xr, -0.68), S_TAT, 0.042, aa, -1.0));
  c = put(c, PV_W, num_at(p, vec2f(xr + 0.30, -0.68), round(P3.w), 0u, 1u, 0.048, aa, 1.0));
  return c;
}

// ---------------------------------------------------------------- MFD

// An engine dial: white arc, ticks, the red limit, the needle, the readout.
fn dial650(c0: vec3f, p: vec2f, o: vec2f, r: f32, v: f32, red: f32, val: f32, dec: u32, aa: f32) -> vec3f {
  var c = c0;
  let a0 = -100.0;
  let a1 = 110.0;
  let rp = p - o;
  let rl = length(rp);
  let ang = degrees(atan2(rp.x, rp.y));
  let av = a0 + (a1 - a0) * clamp(v, 0.0, 1.08);
  let inside = ang >= a0 && ang <= a1;
  if (inside && rl < r && ang < av) { c = put(c, vec3f(0.22, 0.22, 0.24), 0.6); }
  if (inside) {
    c = put(c, PV_W, aa_line(rl - r, 0.004, aa));
    let tv = round((ang - a0) / (a1 - a0) * 10.0) / 10.0;
    let ta = a0 + (a1 - a0) * tv;
    c = put(c, PV_W, aa_line((ang - ta) * 0.017453 * rl, 0.003, aa) * step(r - 0.022, rl) * step(rl, r));
  }
  let ra = rad(a0 + (a1 - a0) * red);
  let rd = vec2f(sin(ra), cos(ra));
  c = put(c, PV_R, seg_line(rp, rd * (r - 0.03), rd * (r + 0.02), 0.006, aa));
  let na = rad(av);
  c = put(c, PV_W, seg_line(rp, vec2f(0.0), vec2f(sin(na), cos(na)) * (r - 0.012), 0.006, aa));
  let bc = o + vec2f(r * 0.30, -r * 0.62);
  let b = sd_box(p, bc, vec2f(0.085, 0.032));
  if (b < 0.0) { c = vec3f(0.0); }
  let over = v > red;
  c = put(c, select(PV_GREY, PV_R, over), aa_line(b, 0.003, aa));
  c = put(c, select(PV_W, PV_R, over), num_at(p, bc + vec2f(0.075, 0.0), val, dec, 1u, 0.046, aa, 1.0));
  return c;
}

// The engine instruments on the inboard third of DU2: x 0.444..1.333.
fn eng650(p: vec2f, aa: f32) -> vec3f {
  let P2 = O.params[2];
  let fl = fl650();
  var c = PV_ENG;
  let run = (fl & 1u) != 0u;
  let n1 = P2.x;
  let n2 = select(0.0, 0.62 + 0.38 * sat((n1 - 0.2) / 0.8), run || n1 > 0.05) * sat(n1 / 0.2);
  let itt = P2.y;
  let xm = 0.889;
  let xs = array<f32, 2>(0.655, 1.125);
  let r = 0.145;
  let jit = array<f32, 2>(0.0, 0.003);
  for (var e = 0; e < 2; e++) {
    let o1 = vec2f(xs[e], 0.76);
    if (length(p - o1) < r + 0.1) { c = dial650(c, p, o1, r, (n1 + jit[e]) / 1.10, 1.02 / 1.10, (n1 + jit[e]) * 100.0, 1u, aa); }
    let o2 = vec2f(xs[e], 0.42);
    if (length(p - o2) < r + 0.1) { c = dial650(c, p, o2, r, itt / 1000.0, 0.90, round(itt), 0u, aa); }
    let o3 = vec2f(xs[e], 0.08);
    if (length(p - o3) < r + 0.1) { c = dial650(c, p, o3, r, n2 / 1.10, 1.04 / 1.10, n2 * 100.0, 1u, aa); }
  }
  c = put(c, PV_W, str_at(p, vec2f(xm, 0.80), S_N1, 0.042, aa, 0.0));
  c = put(c, PV_W, str_at(p, vec2f(xm, 0.46), S_ITT, 0.042, aa, 0.0));
  c = put(c, PV_W, str_at(p, vec2f(xm, 0.12), S_N2, 0.042, aa, 0.0));
  // The digital rows: label in the middle, left and right engine either side.
  let ff = P2.z * 2.2046;
  let oilp = select(0.0, 28.0 + 30.0 * sat(n1), n2 > 0.3);
  let oilt = select(25.0, 60.0 + 40.0 * sat(n1), n2 > 0.3);
  let hyd = select(0.0, 3000.0, n2 > 0.4);
  let labels = array<u32, 4>(S_FF, S_OIL_PRESS, S_OIL_TEMP, S_HYD_PRESS);
  let vals = array<f32, 4>(round(ff / 10.0) * 10.0, round(oilp), round(oilt), hyd);
  for (var i = 0; i < 4; i++) {
    let y = -0.14 - f32(i) * 0.10;
    c = put(c, PV_W, str_at(p, vec2f(xm, y), labels[i], 0.038, aa, 0.0));
    let lo = (i == 1 && run && oilp < 25.0) || (i == 3 && run && hyd < 1500.0);
    let col = select(PV_G, PV_A, lo);
    c = put(c, col, num_at(p, vec2f(0.70, y), vals[i], 0u, 1u, 0.044, aa, 1.0));
    c = put(c, col, num_at(p, vec2f(1.29, y), vals[i] * select(1.0, 1.003, i == 0), 0u, 1u, 0.044, aa, 1.0));
  }
  c = put(c, PV_GREY, aa_line(p.y + 0.55, 0.002, aa) * step(0.48, p.x) * step(p.x, 1.30));
  // Fuel, lb: the left and right tanks and the total.
  let lb = P2.w * 2.2046;
  let low = lb < 2000.0;
  c = put(c, PV_W, str_at(p, vec2f(xm, -0.63), S_FUEL_QTY, 0.04, aa, 0.0));
  for (var e = 0; e < 2; e++) {
    let bx = vec2f(select(0.64, 1.14, e == 1), -0.74);
    c = put(c, PV_GREY, aa_line(sd_box(p, bx, vec2f(0.12, 0.04)), 0.003, aa));
    c = put(c, select(PV_G, PV_A, low), num_at(p, bx + vec2f(0.105, 0.0), round(lb * 0.5 / 10.0) * 10.0, 0u, 1u, 0.048, aa, 1.0));
  }
  c = put(c, PV_W, str_at(p, vec2f(0.64, -0.87), S_TOTAL, 0.038, aa, 0.0));
  c = put(c, select(PV_G, PV_A, low), num_at(p, vec2f(1.18, -0.87), round(lb / 10.0) * 10.0, 0u, 1u, 0.052, aa, 1.0));
  c = put(c, PV_W, str_at(p, vec2f(1.21, -0.87), S_LB, 0.036, aa, -1.0));
  return c;
}

// The map (heading up) over the vertical situation display: DU2's left two
// thirds, x -1.333..0.444.
fn map650(p: vec2f, aa: f32) -> vec3f {
  let P0 = O.params[0];
  let P3 = O.params[3];
  let C8 = G.cockpit[8];
  let hdg = P0.w;
  var c = vec3f(0.004, 0.004, 0.006);
  let mx = -0.444;
  let ac = vec2f(mx, -0.40);
  let R = 1.10;
  let q = p - ac;
  let rl = length(q);
  let a = atan2(q.x, q.y);
  let ps = rad(hdg);
  let fwd = vec2f(sin(ps), cos(ps));
  let rgt = vec2f(cos(ps), -sin(ps));
  let nm = R / 20.0;  // 20 nm to the arc
  if (p.y > -0.60 && p.y < 0.88) {
    // The compass arc, its ticks and numbers.
    c = put(c, PV_W, aa_line(rl - R, 0.004, aa) * step(abs(a), 1.25));
    let deg = hdg + degrees(a);
    let t5 = round(deg / 5.0) * 5.0;
    let d = abs(wrapa(a - rad(t5 - hdg))) * rl;
    let long_ = abs(t5 % 10.0) < 0.1;
    c = put(c, PV_W, (1.0 - smoothstep(0.003, 0.003 + aa, d)) * step(R - select(0.025, 0.045, long_), rl) * step(rl, R) * step(abs(a), 1.25));
    let t30 = round(deg / 30.0) * 30.0;
    let la = rad(t30 - hdg);
    if (abs(la) < 1.15) {
      c = put(c, PV_W, num_at(rot2(q - vec2f(sin(la), cos(la)) * (R - 0.09), la), vec2f(0.0), f32((u32(round(t30 + 720.0)) % 360u) / 10u), 0u, 1u, 0.05, aa, 0.0));
    }
    // The 10 nm ring, dashed, and its label.
    c = put(c, PV_GREY, aa_line(rl - R * 0.5, 0.003, aa) * step(0.5, fract(a * 12.0)) * step(abs(a), 1.4));
    c = put(c, PV_GREY, num_at(p, ac + vec2f(-R * 0.5 * 0.74, R * 0.5 * 0.67), 10.0, 0u, 1u, 0.04, aa, 0.0));
    // The track line ahead, dashed.
    c = put(c, PV_W, aa_line(q.x, 0.003, aa) * step(0.06, q.y) * step(q.y, R) * step(0.5, fract(q.y * 7.0)));
    // The heading bug on the arc (cyan), and its line in HDG.
    let hb = rad(C8.y - hdg);
    let hq = rot2(q, hb);
    if (abs(wrapa(hb)) < 1.25) {
      let bug = max(sd_box(hq, vec2f(0.0, R + 0.012), vec2f(0.03, 0.014)), -sd_box(hq, vec2f(0.0, R + 0.02), vec2f(0.009, 0.03)));
      c = put(c, PV_C, aa_fill(bug, aa));
    }
    if ((ap650() & 29u) == 5u) { c = put(c, PV_C, aa_line(hq.x, 0.003, aa) * step(0.06, hq.y) * step(hq.y, R) * step(0.5, fract(hq.y * 9.0))); }
    // The VORs near by (cyan) and the tuned NAV 1 station (green) with its course.
    for (var i = 4; i < 8; i++) {
      let s = G.cockpit[i];
      if (s.z < 0.5) { continue; }
      let v = s.xy * 10.0;
      let sp = ac + vec2f(dot(v, rgt), dot(v, fwd)) * nm;
      let hx = abs(p - sp);
      c = put(c, PV_C, aa_line(max(hx.x * 0.866 + hx.y * 0.5, hx.y) - 0.022, 0.003, aa));
    }
    let st = G.cockpit[3];
    if (st.z > 0.5) {
      let v = st.xy * 10.0;
      let sp = ac + vec2f(dot(v, rgt), dot(v, fwd)) * nm;
      let hx = abs(p - sp);
      c = put(c, PV_G, aa_line(max(hx.x * 0.866 + hx.y * 0.5, hx.y) - 0.026, 0.004, aa));
      c = put(c, PV_G, aa_fill(length(p - sp) - 0.006, aa));
      let ca = rad(G.nav.z - hdg);
      let cd = vec2f(sin(ca), cos(ca));
      let along = dot(p - sp, cd);
      let across = (p - sp).x * cd.y - (p - sp).y * cd.x;
      c = put(c, PV_G, aa_line(across, 0.003, aa) * step(0.04, abs(along)) * step(abs(along), 1.6));
      c = put(c, PV_G, num_at(p, sp + vec2f(0.05, -0.05), G.cockpit[1].x, 2u, 1u, 0.04, aa, -1.0));
    }
    // The aircraft.
    var am = seg_line(q, vec2f(0.0, -0.05), vec2f(0.0, 0.05), 0.006, aa);
    am = max(am, seg_line(q, vec2f(-0.045, 0.01), vec2f(0.045, 0.01), 0.006, aa));
    am = max(am, seg_line(q, vec2f(-0.02, -0.04), vec2f(0.02, -0.04), 0.006, aa));
    c = put(c, PV_W, am);
  }
  // The header: the window's name, the heading readout, range, GS and TAS.
  let hdb = sd_box(p, vec2f(mx, 0.83), vec2f(0.075, 0.036));
  if (hdb < 0.0) { c = vec3f(0.0); }
  c = put(c, PV_W, aa_line(hdb, 0.003, aa));
  c = put(c, PV_W, num_at(p, vec2f(mx, 0.83), (round(hdg) + 359.0) % 360.0 + 1.0, 0u, 3u, 0.052, aa, 0.0));
  c = put(c, PV_W, str_at(p, vec2f(-1.30, 0.94), S_MAP, 0.042, aa, -1.0));
  c = put(c, PV_GREY, aa_line(p.y - 0.895, 0.002, aa));
  c = put(c, PV_W, str_at(p, vec2f(-1.30, 0.83), S_GS, 0.038, aa, -1.0));
  c = put(c, PV_W, num_at(p, vec2f(-1.05, 0.83), round(P3.y), 0u, 1u, 0.044, aa, 1.0));
  c = put(c, PV_W, str_at(p, vec2f(-1.30, 0.75), S_TAS, 0.038, aa, -1.0));
  c = put(c, PV_W, num_at(p, vec2f(-1.05, 0.75), round(P0.x * (1.0 + P0.y * 0.0000168)), 0u, 1u, 0.044, aa, 1.0));
  c = put(c, PV_W, str_at(p, vec2f(0.40, 0.94), S_RNG, 0.038, aa, 1.0));
  c = put(c, PV_W, num_at(p, vec2f(0.40, 0.86), 20.0, 0u, 1u, 0.044, aa, 1.0));
  // ---- the vertical situation display: 20 nm ahead, the ground at the radio height.
  c = put(c, PV_GREY, aa_line(p.y + 0.62, 0.002, aa));
  if (p.y < -0.625) {
    let x0 = -1.18;
    let yac = -0.80;
    let ksc = 0.000075;
    let ftu = 1.60 / 121520.0;
    let yg = yac - max(P3.z, 0.0) * ksc;
    if (p.y < yg && p.x > x0) { c = mix(c, vec3f(0.17, 0.10, 0.04), 0.9); }
    c = put(c, vec3f(0.55, 0.35, 0.15), aa_line(p.y - yg, 0.003, aa) * step(x0, p.x));
    if (C8.z > 0.5) {
      let ys = yac + (C8.z - P0.y) * ksc;
      c = put(c, PV_C, aa_line(p.y - ys, 0.003, aa) * step(x0, p.x) * step(0.5, fract(p.x * 10.0)) * step(abs(ys + 0.81), 0.18));
    }
    let slope = P0.z / max(P3.y * 101.27, 50.0) * ksc / ftu;
    c = put(c, PV_W, aa_line((p.y - yac) - (p.x - x0) * slope, 0.003, aa) * step(x0, p.x) * step(0.5, fract(p.x * 8.0)));
    c = put(c, PV_W, aa_fill(tri650(p, vec2f(x0 - 0.05, yac + 0.025), vec2f(x0 - 0.05, yac - 0.025), vec2f(x0, yac)), aa));
    // The altitude scale.
    let base = round(P0.y / 1000.0) * 1000.0;
    for (var i = -1; i <= 1; i++) {
      let av = base + f32(i) * 1000.0;
      let y = yac + (av - P0.y) * ksc;
      if (abs(y + 0.81) < 0.17 && av >= 0.0) {
        c = put(c, PV_GREY, aa_line(p.y - y, 0.002, aa) * step(-1.24, p.x) * step(p.x, -1.20));
        c = put(c, PV_GREY, num_at(p, vec2f(-1.25, y), av / 100.0, 0u, 1u, 0.034, aa, 1.0));
      }
    }
  }
  return c;
}

// The CAS window (DU3's left third, x -1.333..-0.444, top) and the summary under it.
fn cas_line650(c0: vec3f, p: vec2f, n: f32, col: vec3f, pre: u32, s: u32, aa: f32) -> vec3f {
  let y = 0.915 - n * 0.072;
  if (abs(p.y - y) > 0.04) { return c0; }
  var x = -1.30;
  var a = 0.0;
  if (pre > 0u) { a = str_at(p, vec2f(x, y), pre, 0.046, aa, -1.0); x += sw650(pre, 0.046) + 0.035; }
  a = max(a, str_at(p, vec2f(x, y), s, 0.046, aa, -1.0));
  return put(c0, col, a);
}

// The CAS conditions, shared by the list and the master lights.
// Warnings: the aircraft configuration (take-off power on the ground with
// the flaps, speed brake, park brake or reversers wrong), gear not down.
fn config650() -> bool {
  let P4 = O.params[4];
  let fl = fl650();
  let flaps_to = abs(P4.w - 10.0) < 1.0 || abs(P4.w - 20.0) < 1.0;
  return (fl & 16u) != 0u && P4.x > 0.7 && (!flaps_to || G.cockpit[9].z > 0.05 || (fl & 32u) != 0u || G.cockpit[10].x > 0.05);
}
fn gear_warn650() -> bool {
  let P4 = O.params[4];
  return (fl650() & 16u) == 0u && P4.z < 0.99 && O.params[3].z < 700.0 && (P4.w > 30.0 || P4.x < 0.15);
}
// Cautions: the speed brake out in the air with the flaps or gear down;
// fuel low; the engines' oil pressure and generators while they are not running.
fn sbrk_caution650() -> bool {
  let P4 = O.params[4];
  return (fl650() & 16u) == 0u && G.cockpit[9].z > 0.05 && (P4.w > 15.0 || P4.z > 0.5);
}
fn fuel_low650() -> bool { return O.params[2].w * 2.2046 < 2000.0; }
// The master WARNING (x) and CAUTION (y) switchlights on the glareshield
// (deck747.wgsl `legend` styles 12 and 13): lit, with the bus powered, while
// the CAS list holds a warning or a caution. (Simplified: steady, not
// flashing, and there is no acknowledging them: they go out with the message.)
fn master650() -> vec2f {
  if (G.cockpit[2].z < 0.5) { return vec2f(0.0); }
  let warn = config650() || gear_warn650();
  let caution = (fl650() & 1u) == 0u || fuel_low650() || sbrk_caution650();
  return vec2f(select(0.0, 1.0, warn), select(0.0, 1.0, caution));
}

fn cas650(p: vec2f, aa: f32) -> vec3f {
  let P4 = O.params[4];
  let C9 = G.cockpit[9];
  let C10 = G.cockpit[10];
  let fl = fl650();
  let run = (fl & 1u) != 0u;
  let gnd = (fl & 16u) != 0u;
  let park = (fl & 32u) != 0u;
  var c = PV_WIN;
  let top = sd_box(p, vec2f(-0.889, 0.565), vec2f(0.43, 0.40));
  if (top < 0.0) {
    c = vec3f(0.0);
    var n = 0.0;
    // Warnings (red).
    if (config650()) { c = cas_line650(c, p, n, PV_R, 0u, S_AIRCRAFT_CONFIG, aa); n += 1.0; }
    if (gear_warn650()) { c = cas_line650(c, p, n, PV_R, 0u, S_GEAR_NOT_DOWN, aa); n += 1.0; }
    // Cautions (amber).
    if (!run) { c = cas_line650(c, p, n, PV_A, S_L_R, S_OIL_PRESS_LOW, aa); n += 1.0; }
    if (!run) { c = cas_line650(c, p, n, PV_A, S_L_R, S_GEN_OFF, aa); n += 1.0; }
    if (fuel_low650()) { c = cas_line650(c, p, n, PV_A, S_L_R, S_FUEL_LEVEL_LOW, aa); n += 1.0; }
    if (sbrk_caution650()) { c = cas_line650(c, p, n, PV_A, 0u, S_SPEEDBRAKE_EXT, aa); n += 1.0; }
    // Advisories (cyan) and status (white).
    if (!run && G.cockpit[11].y < -1.0) { c = cas_line650(c, p, n, PV_C, 0u, S_MAIN_BATT_DISCH, aa); n += 1.0; }
    if (park) { c = cas_line650(c, p, n, PV_C, 0u, S_PARKING_BRAKE, aa); n += 1.0; }
    if (C10.x > 0.05) { c = cas_line650(c, p, n, PV_C, S_L_R, S_REV_DEPLOYED, aa); n += 1.0; }
    if (C9.z > 0.05 && !(!gnd && (P4.w > 15.0 || P4.z > 0.5))) { c = cas_line650(c, p, n, PV_C, 0u, S_SPEEDBRAKE_EXT, aa); n += 1.0; }
    if ((fl & 2u) != 0u) { c = cas_line650(c, p, n, PV_W, S_L_R, S_ENGINE_START, aa); n += 1.0; }
  }
  c = put(c, PV_GREY, aa_line(top, 0.003, aa));
  // The summary: pitch trim, flaps, speed brake, gear.
  // Pitch trim: a vertical scale, nose up at the top.
  let tx = -1.20;
  c = put(c, PV_W, str_at(p, vec2f(tx + 0.02, 0.07), S_PITCH_TRIM, 0.036, aa, 0.0));
  c = put(c, PV_W, aa_line(p.x - tx, 0.003, aa) * step(abs(p.y + 0.30), 0.30));
  for (var i = -2; i <= 2; i++) { c = put(c, PV_W, aa_line(p.y + 0.30 - f32(i) * 0.15, 0.003, aa) * step(abs(p.x - tx + 0.015), 0.015)); }
  let ty = -0.30 + clamp(C9.y / 8.0, -1.0, 1.0) * 0.30;
  c = put(c, PV_G, aa_fill(tri650(p, vec2f(tx + 0.07, ty + 0.025), vec2f(tx + 0.07, ty - 0.025), vec2f(tx + 0.02, ty)), aa));
  c = put(c, PV_W, str_at(p, vec2f(tx - 0.05, 0.0), S_NU, 0.032, aa, 1.0));
  c = put(c, PV_W, str_at(p, vec2f(tx - 0.05, -0.60), S_ND, 0.032, aa, 1.0));
  c = put(c, PV_G, num_at(p, vec2f(tx + 0.10, ty), C9.y, 1u, 1u, 0.036, aa, -1.0));
  // Flaps: a scale 0, 10, 20, 39 and the position.
  let fx = -0.86;
  c = put(c, PV_W, str_at(p, vec2f(fx + 0.04, 0.07), S_FLAPS, 0.036, aa, 0.0));
  let notches = array<f32, 4>(0.0, 10.0, 20.0, 39.0);
  for (var i = 0; i < 4; i++) {
    let y = -0.02 - notches[i] / 39.0 * 0.56;
    c = put(c, PV_W, aa_line(p.y - y, 0.003, aa) * step(abs(p.x - fx), 0.02));
    c = put(c, PV_W, num_at(p, vec2f(fx + 0.045, y), notches[i], 0u, 1u, 0.032, aa, -1.0));
  }
  c = put(c, PV_W, aa_line(p.x - fx, 0.003, aa) * step(-0.58, p.y) * step(p.y, -0.02));
  let fy = -0.02 - P4.w / 39.0 * 0.56;
  c = put(c, PV_G, aa_fill(sd_box(p, vec2f(fx - 0.035, fy), vec2f(0.025, 0.012)), aa));
  // Speed brake: a bar; gear: three boxes.
  let gx = -0.60;
  c = put(c, PV_W, str_at(p, vec2f(gx, 0.07), S_SPD_BRK, 0.036, aa, 0.0));
  c = put(c, PV_GREY, aa_line(sd_box(p, vec2f(gx, -0.18), vec2f(0.03, 0.18)), 0.003, aa));
  let sb = sat(C9.w);
  c = put(c, select(PV_G, PV_A, sb > 0.05 && !gnd), aa_fill(sd_box(p, vec2f(gx, 0.0 - sb * 0.18), vec2f(0.02, sb * 0.18)), aa) * step(0.01, sb));
  let g = P4.z;
  c = put(c, PV_W, str_at(p, vec2f(gx, -0.44), S_GEAR, 0.036, aa, 0.0));
  let gpos = array<vec2f, 3>(vec2f(gx, -0.55), vec2f(gx - 0.075, -0.66), vec2f(gx + 0.075, -0.66));
  for (var i = 0; i < 3; i++) {
    let b = sd_box(p, gpos[i], vec2f(0.035, 0.03));
    if (g > 0.999) {
      c = put(c, PV_G, aa_line(b, 0.003, aa));
      c = put(c, PV_G, str_at(p, gpos[i], S_DN, 0.03, aa, 0.0));
    } else if (g > 0.001) {
      c = put(c, PV_A, aa_line(b, 0.003, aa));
      c = put(c, PV_A, aa_line(fract((p.x + p.y) * 30.0) - 0.5, 0.1, aa * 30.0) * step(b, 0.0));
    } else {
      c = put(c, PV_W, aa_line(b, 0.003, aa));
      c = put(c, PV_W, str_at(p, gpos[i], S_UP, 0.03, aa, 0.0));
    }
  }
  return c;
}

// The Flight Controls synoptic, seen from behind: DU3's right two thirds, x -0.444..1.333.
fn fctl650(p: vec2f, aa: f32) -> vec3f {
  let P4 = O.params[4];
  let C9 = G.cockpit[9];
  let fl = fl650();
  let hyd = (fl & 1u) != 0u;
  let pw = (fl & 8u) != 0u;
  var c = PV_WIN;
  let cx = 0.444;
  let col = select(PV_A, PV_G, hyd);
  c = put(c, PV_W, str_at(p, vec2f(cx, 0.91), S_FLIGHT_CONTROLS, 0.05, aa, 0.0));
  c = put(c, PV_W, aa_line(p.y - 0.865, 0.003, aa) * step(abs(p.x - cx), sw650(S_FLIGHT_CONTROLS, 0.05) * 0.5));
  // The flight control computers.
  for (var i = 0; i < 2; i++) {
    let bx = vec2f(cx + select(-0.30, 0.30, i == 1), 0.74);
    c = put(c, select(PV_GREY, PV_G, pw), aa_line(sd_box(p, bx, vec2f(0.11, 0.04)), 0.003, aa));
    c = put(c, select(PV_GREY, PV_G, pw), str_at(p, bx, select(S_FCC_1, S_FCC_2, i == 1), 0.04, aa, 0.0));
  }
  // The airframe from behind: fuselage, engines, wings, fin and T-tail.
  let yb = -0.30;
  let fz = vec2f(cx, yb);
  var ink = aa_line(length(p - fz) - 0.13, 0.0035, aa);
  ink = max(ink, aa_line(length(p - vec2f(cx - 0.21, yb + 0.07)) - 0.06, 0.0035, aa));
  ink = max(ink, aa_line(length(p - vec2f(cx + 0.21, yb + 0.07)) - 0.06, 0.0035, aa));
  ink = max(ink, seg_line(p, vec2f(cx - 0.12, yb - 0.06), vec2f(cx - 0.82, yb - 0.01), 0.0035, aa));
  ink = max(ink, seg_line(p, vec2f(cx + 0.12, yb - 0.06), vec2f(cx + 0.82, yb - 0.01), 0.0035, aa));
  ink = max(ink, seg_line(p, vec2f(cx, yb + 0.13), vec2f(cx, yb + 0.55), 0.0035, aa));
  ink = max(ink, seg_line(p, vec2f(cx - 0.30, yb + 0.55), vec2f(cx + 0.30, yb + 0.55), 0.0035, aa));
  c = put(c, PV_W, ink);
  // Spoilers: four panels a wing, up with the speed brake and roll spoilers.
  let sp = sat(C9.w);
  for (var s = 0; s < 2; s++) {
    let sg = select(-1.0, 1.0, s == 1);
    for (var i = 0; i < 4; i++) {
      let f = 0.22 + f32(i) * 0.09;
      let x = cx + sg * (0.12 + f * 0.70);
      let yw = yb - 0.06 + f * 0.05 * 1.0 + 0.012;
      let h = 0.02 + 0.11 * sp;
      c = put(c, col, aa_fill(sd_box(p, vec2f(x, yw + h * 0.5), vec2f(0.016, h * 0.5)), aa));
    }
    // Flaps: two segments a wing, down with the flaps.
    let ff = sat(P4.w / 39.0);
    for (var i = 0; i < 2; i++) {
      let f = 0.06 + f32(i) * 0.10;
      let x = cx + sg * (0.12 + f * 0.70 + 0.03);
      let yw = yb - 0.06 + f * 0.05 - 0.012;
      let h = 0.02 + 0.10 * ff;
      c = put(c, col, aa_fill(sd_box(p, vec2f(x, yw - h * 0.5), vec2f(0.022, h * 0.5)), aa));
    }
    // Ailerons (neutral: their deflection is not passed to the displays).
    let xa = cx + sg * (0.12 + 0.86 * 0.70);
    c = put(c, col, aa_fill(sd_box(p, vec2f(xa, yb - 0.02), vec2f(0.05, 0.01)), aa));
    // Elevators on the T-tail.
    c = put(c, col, aa_fill(sd_box(p, vec2f(cx + sg * 0.17, yb + 0.535), vec2f(0.09, 0.01)), aa));
  }
  // Rudder.
  c = put(c, col, aa_fill(sd_box(p, vec2f(cx + 0.012, yb + 0.36), vec2f(0.008, 0.12)), aa));
  // Pitch trim (from the sim), roll and rudder trim (not simulated: centred).
  let px0 = cx - 0.72;
  c = put(c, PV_W, str_at(p, vec2f(px0, 0.60), S_PITCH_TRIM, 0.036, aa, 0.0));
  c = put(c, PV_W, aa_line(p.x - px0, 0.003, aa) * step(abs(p.y - 0.34), 0.20));
  c = put(c, PV_G, aa_fill(sd_box(p, vec2f(px0, 0.34 + 0.20 * 0.25), vec2f(0.008, 0.20 * 0.25)), aa) * 0.6);
  let pty = 0.34 + clamp(C9.y / 8.0, -1.0, 1.0) * 0.20;
  c = put(c, PV_G, aa_fill(tri650(p, vec2f(px0 + 0.06, pty + 0.022), vec2f(px0 + 0.06, pty - 0.022), vec2f(px0 + 0.012, pty)), aa));
  c = put(c, PV_W, str_at(p, vec2f(px0 - 0.03, 0.52), S_NU, 0.03, aa, 1.0));
  c = put(c, PV_W, str_at(p, vec2f(px0 - 0.03, 0.16), S_ND, 0.03, aa, 1.0));
  let rx = cx + 0.70;
  c = put(c, PV_W, str_at(p, vec2f(rx, 0.60), S_ROLL_TRIM, 0.036, aa, 0.0));
  let rr = p - vec2f(rx, 0.32);
  c = put(c, PV_W, aa_line(length(rr) - 0.15, 0.003, aa) * step(0.08, rr.y));
  c = put(c, PV_G, aa_fill(tri650(rr, vec2f(-0.02, 0.17), vec2f(0.02, 0.17), vec2f(0.0, 0.145)), aa));
  c = put(c, PV_W, str_at(p, vec2f(rx, 0.30), S_RUDDER_TRIM, 0.036, aa, 0.0));
  c = put(c, PV_W, aa_line(p.y - 0.22, 0.003, aa) * step(abs(p.x - rx), 0.14));
  c = put(c, PV_G, aa_fill(tri650(p - vec2f(rx, 0.22), vec2f(-0.02, 0.04), vec2f(0.02, 0.04), vec2f(0.0, 0.008)), aa));
  // Flaps and speed brake readouts.
  c = put(c, PV_W, str_at(p, vec2f(cx - 0.40, -0.70), S_FLAPS, 0.04, aa, 0.0));
  c = put(c, PV_G, num_at(p, vec2f(cx - 0.40, -0.80), round(P4.w), 0u, 1u, 0.05, aa, 0.0));
  c = put(c, PV_W, str_at(p, vec2f(cx + 0.40, -0.70), S_SPEED_BRAKE, 0.04, aa, 0.0));
  c = put(c, PV_G, num_at(p, vec2f(cx + 0.40, -0.80), round(sat(C9.z) * 100.0), 0u, 1u, 0.05, aa, 0.0));
  return c;
}

// ---------------------------------------------------------------- SMC, MCDU

// SMC 1: the standby attitude, airspeed, altitude and heading (x -1.333..1.333).
fn stby650(p: vec2f, aa: f32) -> vec3f {
  let P0 = O.params[0];
  let P1 = O.params[1];
  let k = 0.035;
  let ac = vec2f(0.0, 0.05);
  let q = rot2(p - ac, rad(P1.y) * BANK650);
  let hy = -P1.x * k;
  var c = select(vec3f(0.32, 0.17, 0.05), vec3f(0.10, 0.35, 0.90), q.y > hy);
  c = put(c, PV_W, aa_line(q.y - hy, 0.006, aa));
  let rel = q.y / k + P1.x;
  let st = round(rel / 10.0) * 10.0;
  if (abs(st) > 0.1 && abs(q.x) < 0.36 && abs(p.y - ac.y) < 0.6) {
    c = put(c, PV_W, aa_line((rel - st) * k, 0.006, aa) * step(abs(q.x), 0.18));
    c = put(c, PV_W, num_at(q, vec2f(0.27, (st - P1.x) * k), abs(st), 0u, 1u, 0.08, aa, 0.0));
  }
  // Bank pointer and the fixed aircraft symbol (yellow, as standbys draw it).
  let rq = rot2(p - ac, rad(P1.y) * BANK650);
  c = put(c, PV_W, aa_fill(tri650(rq, vec2f(-0.04, 0.58), vec2f(0.04, 0.58), vec2f(0.0, 0.65)), aa));
  c = put(c, PV_W, aa_line(tri650(p - ac, vec2f(-0.04, 0.73), vec2f(0.04, 0.73), vec2f(0.0, 0.665)), 0.006, aa));
  let wing = min(sd_box(abs(p - ac), vec2f(0.30, 0.0), vec2f(0.12, 0.016)), sd_box(p, ac, vec2f(0.02, 0.02)));
  c = put(c, vec3f(0.0), aa_fill(wing - 0.008, aa));
  c = put(c, vec3f(1.0, 0.85, 0.1), aa_fill(wing, aa));
  // Airspeed and altitude in boxes either side, the heading along the bottom.
  let sb = sd_box(p, vec2f(-1.0, ac.y), vec2f(0.28, 0.12));
  if (sb < 0.0) { c = vec3f(0.0); }
  c = put(c, PV_W, aa_line(sb, 0.006, aa));
  c = put(c, PV_W, num_at(p, vec2f(-0.75, ac.y), floor(max(P0.x, 0.0)), 0u, 1u, 0.15, aa, 1.0));
  let ab = sd_box(p, vec2f(0.98, ac.y), vec2f(0.33, 0.12));
  if (ab < 0.0) { c = vec3f(0.0); }
  c = put(c, PV_W, aa_line(ab, 0.006, aa));
  c = put(c, PV_W, num_at(p, vec2f(1.28, ac.y), round(P0.y / 20.0) * 20.0, 0u, 1u, 0.14, aa, 1.0));
  c = put(c, PV_C, num_at(p, vec2f(1.18, -0.82), G.cockpit[0].x / 100.0, 2u, 1u, 0.12, aa, 1.0));
  c = put(c, PV_C, str_at(p, vec2f(1.22, -0.82), S_IN, 0.09, aa, -1.0));
  let hb = sd_box(p, vec2f(0.0, -0.82), vec2f(0.20, 0.10));
  if (hb < 0.0) { c = vec3f(0.0); }
  c = put(c, PV_W, aa_line(hb, 0.006, aa));
  c = put(c, PV_W, num_at(p, vec2f(0.0, -0.82), (round(P0.w) + 359.0) % 360.0 + 1.0, 0u, 3u, 0.13, aa, 0.0));
  c = put(c, PV_W, str_at(p, vec2f(-1.25, 0.86), S_STBY, 0.10, aa, -1.0));
  return c;
}

// SMC 2: the PFD settings page (baro live, the rest as the real page lists them).
fn smcmenu650(p: vec2f, aa: f32) -> vec3f {
  var c = vec3f(0.0);
  c = put(c, PV_W, str_at(p, vec2f(0.0, 0.84), S_PFD, 0.14, aa, 0.0));
  c = put(c, PV_GREY, aa_line(p.y - 0.72, 0.006, aa) * step(abs(p.x), 1.25));
  let ys = array<f32, 4>(0.50, 0.15, -0.20, -0.55);
  let lft = array<u32, 4>(S_BARO, S_MINS, S_RA, S_FPV);
  let rgt = array<u32, 4>(S_SVS, S_HSI, S_MAP, S_RNG);
  for (var i = 0; i < 4; i++) {
    c = put(c, PV_W, str_at(p, vec2f(-1.25, ys[i]), lft[i], 0.12, aa, -1.0));
    c = put(c, PV_W, str_at(p, vec2f(1.25, ys[i]), rgt[i], 0.12, aa, 1.0));
  }
  c = put(c, PV_C, num_at(p, vec2f(-0.15, 0.50), G.cockpit[0].x / 100.0, 2u, 1u, 0.13, aa, 0.0));
  c = put(c, PV_C, num_at(p, vec2f(-0.25, 0.15), 200.0, 0u, 1u, 0.13, aa, 0.0));
  c = put(c, PV_G, str_at(p, vec2f(0.55, 0.50), S_ON, 0.12, aa, 0.0));
  c = put(c, PV_G, num_at(p, vec2f(0.55, -0.55), 20.0, 0u, 1u, 0.12, aa, 0.0));
  return c;
}

// An MCDU (q: x -1.16..1.16, y -1..1). page 0 PROGRESS, 1 the flight plan, 2 RADIO.
fn mcdu650(q: vec2f, aa: f32, page: u32) -> vec3f {
  let P0 = O.params[0];
  let P2 = O.params[2];
  let P3 = O.params[3];
  var c = vec3f(0.0);
  let x0 = -1.08;
  let x1 = 1.08;
  let h = 0.125;
  let hs = 0.09;
  let row = array<f32, 5>(0.60, 0.30, 0.0, -0.30, -0.60);
  if (page == 1u) {
    c = put(c, PV_W, str_at(q, vec2f(0.0, 0.86), S_ACTIVE_FLT_PLAN, h, aa, 0.0));
    for (var i = 0; i < 5; i++) {
      let s = f32(i) * 7.3 + 3.0;
      c = put(c, PV_C, rtext(q, vec2f(x0, row[i] + 0.13), 6u, s, hs, aa, -1.0));
      c = put(c, select(PV_W, PV_M, i == 0), rtext(q, vec2f(x0, row[i]), 5u, s + 1.0, h, aa, -1.0));
      c = put(c, PV_G, rtext(q, vec2f(x1, row[i]), 7u, s + 2.0, h, aa, 1.0));
    }
    c = put(c, PV_W, str_at(q, vec2f(x0, -0.88), S_PREV, hs, aa, -1.0));
    c = put(c, PV_W, str_at(q, vec2f(x1, -0.88), S_NEXT, hs, aa, 1.0));
    return c;
  }
  if (page == 2u) {
    c = put(c, PV_W, str_at(q, vec2f(0.0, 0.86), S_RADIO, h, aa, 0.0));
    c = put(c, PV_C, str_at(q, vec2f(x0, row[0] + 0.13), S_COMM_1, hs, aa, -1.0));
    c = put(c, PV_G, num_at(q, vec2f(x0, row[0]), 118.30, 2u, 1u, h, aa, -1.0));
    c = put(c, PV_C, str_at(q, vec2f(x0, row[1] + 0.13), S_NAV_1, hs, aa, -1.0));
    if (G.cockpit[1].x > 1.0) { c = put(c, PV_G, num_at(q, vec2f(x0, row[1]), G.cockpit[1].x, 2u, 1u, h, aa, -1.0)); }
    c = put(c, PV_C, str_at(q, vec2f(x0, row[2] + 0.13), S_ADF, hs, aa, -1.0));
    if (G.cockpit[1].y > 1.0) { c = put(c, PV_G, num_at(q, vec2f(x0, row[2]), G.cockpit[1].y, 0u, 1u, h, aa, -1.0)); }
    c = put(c, PV_C, str_at(q, vec2f(x1, row[0] + 0.13), S_XPDR, hs, aa, 1.0));
    c = put(c, PV_G, num_at(q, vec2f(x1, row[0]), 2000.0, 0u, 4u, h, aa, 1.0));
    c = put(c, PV_C, str_at(q, vec2f(x1, row[1] + 0.13), S_CRS, hs, aa, 1.0));
    c = put(c, PV_G, num_at(q, vec2f(x1, row[1]), (round(G.nav.z) + 359.0) % 360.0 + 1.0, 0u, 3u, h, aa, 1.0));
    return c;
  }
  // PROGRESS: live values.
  c = put(c, PV_W, str_at(q, vec2f(0.0, 0.86), S_PROGRESS, h, aa, 0.0));
  c = put(c, PV_C, str_at(q, vec2f(x0, row[0] + 0.13), S_NAV_1, hs, aa, -1.0));
  if (G.cockpit[1].x > 1.0) { c = put(c, PV_G, num_at(q, vec2f(x0, row[0]), G.cockpit[1].x, 2u, 1u, h, aa, -1.0)); }
  c = put(c, PV_C, str_at(q, vec2f(x0, row[1] + 0.13), S_GS, hs, aa, -1.0));
  c = put(c, PV_G, num_at(q, vec2f(x0, row[1]), round(P3.y), 0u, 1u, h, aa, -1.0));
  c = put(c, PV_C, str_at(q, vec2f(x1, row[1] + 0.13), S_TAS, hs, aa, 1.0));
  c = put(c, PV_G, num_at(q, vec2f(x1, row[1]), round(P0.x * (1.0 + P0.y * 0.0000168)), 0u, 1u, h, aa, 1.0));
  c = put(c, PV_C, str_at(q, vec2f(x0, row[2] + 0.13), S_FUEL_QTY, hs, aa, -1.0));
  c = put(c, PV_G, num_at(q, vec2f(x0, row[2]), round(P2.w * 2.2046 / 10.0) * 10.0, 0u, 1u, h, aa, -1.0));
  c = put(c, PV_C, str_at(q, vec2f(x1, row[2] + 0.13), S_FF, hs, aa, 1.0));
  c = put(c, PV_G, num_at(q, vec2f(x1, row[2]), round(P2.z * 2.0 * 2.2046 / 10.0) * 10.0, 0u, 1u, h, aa, 1.0));
  c = put(c, PV_C, str_at(q, vec2f(x0, row[3] + 0.13), S_TAT, hs, aa, -1.0));
  c = put(c, PV_G, num_at(q, vec2f(x0, row[3]), round(P3.w), 0u, 1u, h, aa, -1.0));
  c = put(c, PV_C, str_at(q, vec2f(x1, row[3] + 0.13), S_ALT, hs, aa, 1.0));
  c = put(c, PV_G, num_at(q, vec2f(x1, row[3]), round(P0.y / 10.0) * 10.0, 0u, 1u, h, aa, 1.0));
  c = put(c, PV_C, str_at(q, vec2f(x0, row[4] + 0.13), S_CABIN_ALT, hs, aa, -1.0));
  c = put(c, PV_G, num_at(q, vec2f(x0, row[4]), round(G.cockpit[11].z / 10.0) * 10.0, 0u, 1u, h, aa, -1.0));
  c = put(c, PV_W, str_at(q, vec2f(x1, -0.88), S_NEXT_PAGE, hs, aa, 1.0));
  return c;
}

// ---------------------------------------------------------------- dispatch

fn display650(id: u32, uv: vec2f, px: f32) -> vec3f {
  let pu = max(px, 0.003);
  let P0 = O.params[0];
  let P4 = O.params[4];
  let C8 = G.cockpit[8];
  let bits = ap650();
  let fl = fl650();
  // (Dark when the bus is.)
  let on = G.cockpit[2].z;
  if (on < 0.5) { return vec3f(0.0); }
  let q = uv * vec2f(4.0 / 3.0, 1.0);
  let aa = max(px, 0.0022);
  var c = vec3f(0.0);
  switch id {
    case 60u, 63u: { c = pfd650(q, aa); }
    case 61u: {
      if (q.x < 0.444) { c = map650(q, aa); } else { c = eng650(q, aa); }
      c = put(c, PV_GREY, aa_line(q.x - 0.444, 0.004, aa));
    }
    case 62u: {
      if (q.x < -0.444) { c = cas650(q, aa); } else { c = fctl650(q, aa); }
      c = put(c, PV_GREY, aa_line(q.x + 0.444, 0.004, aa));
    }
    case 64u: { c = stby650(q, max(px * 1.333, 0.006)); }
    case 65u: { c = smcmenu650(q, max(px * 1.333, 0.006)); }
    case 66u, 67u, 68u: {
      let page = select(select(1u, 0u, id == 66u), 2u, id == 68u);
      c = mcdu650(uv * vec2f(1.16, 1.0), max(px * 1.16, 0.006), page);
    }
    // The Guidance Panel's windows (amber 7-segment style): the autopilot's targets.
    case 69u: {
      let w = uv * vec2f(2.8, 1.0);
      if (C8.w < 1.0) { c = C_AMBER * str_at(w, vec2f(0.0), S____, 1.25, pu * 2.8, 0.0) * 0.8; }
      else { c = C_AMBER * num_at(w, vec2f(0.0), round(C8.w), 0u, 3u, 1.25, pu * 2.8, 0.0) * 0.8; }
    }
    case 70u: { c = C_AMBER * num_at(uv * vec2f(2.8, 1.0), vec2f(0.0), (round(C8.y) + 359.0) % 360.0 + 1.0, 0u, 3u, 1.25, pu * 2.8, 0.0) * 0.8; }
    case 71u: { c = C_AMBER * num_at(uv * vec2f(2.8, 1.0), vec2f(0.0), round(G.cockpit[9].x / 100.0) * 100.0, 0u, 1u, 1.25, pu * 2.8, 0.0) * 0.8; }
    case 72u: {
      let w = uv * vec2f(3.5, 1.0);
      let a = round(C8.z / 100.0) * 100.0;
      if (C8.z < 0.5) {
        c = C_AMBER * str_at(w, vec2f(0.0), S____, 1.25, pu * 3.5, 0.0) * 0.8;
      } else if (a >= 18000.0) {
        c = C_AMBER * max(str_at(w, vec2f(-0.3, 0.0), S_FL, 1.25, pu * 3.5, 1.0), num_at(w, vec2f(-0.2, 0.0), a / 100.0, 0u, 3u, 1.25, pu * 3.5, -1.0)) * 0.8;
      } else {
        c = C_AMBER * num_at(w, vec2f(0.0), a, 0u, 1u, 1.25, pu * 3.5, 0.0) * 0.8;
      }
    }
    case 73u: { c = C_AMBER * num_at(uv * vec2f(2.35, 1.0), vec2f(0.0), round(G.cockpit[11].z / 10.0) * 10.0, 0u, 1u, 1.2, pu * 2.35, 0.0) * 0.7; }
    case 74u, 75u: { c = C_AMBER * num_at(uv * vec2f(2.7, 1.0), vec2f(0.0), G.cockpit[11].x, 1u, 1u, 1.2, pu * 2.7, 0.0) * 0.7; }
    case 76u, 77u: { // chronometers: hours and minutes, local
      let t = P4.y;
      let hm = floor(t / 3600.0) * 100.0 + floor((t % 3600.0) / 60.0);
      c = vec3f(1.0, 0.45, 0.10) * num_at(uv * vec2f(2.4, 1.0), vec2f(0.0), hm, 0u, 4u, 1.2, pu * 2.4, 0.0) * 0.7;
    }
    case 78u: { // brake pressure, inboard and outboard: the park brake or the toe brakes
      let b = max(select(0.0, 0.92, (fl & 32u) != 0u), G.cockpit[10].z);
      let top = -0.85 + 1.7 * sat(b);
      let x = abs(uv.x) - 0.45;
      let frame = aa_line(sd_box(vec2f(x, uv.y), vec2f(0.0), vec2f(0.24, 0.88)), 0.03, pu * 2.0);
      let bar = aa_fill(sd_box(vec2f(x, uv.y), vec2f(0.0, (-0.85 + top) * 0.5), vec2f(0.17, max((top + 0.85) * 0.5, 0.0))), pu);
      c = C_GREEN * bar * step(0.02, b) * 0.6 + vec3f(0.25) * frame * 0.3;
    }
    default: {
      // The mode lights (80 + k): AP, YD, HDG, NAV, APPR, BC, ALT, VS, FLCH, VNAV, A/T, PFD CMD.
      if (id >= 80u && id < 92u) {
        let k = id - 80u;
        let ap = (bits & 1u) != 0u;
        var lit = false;
        switch k {
          case 0u: { lit = ap; }
          case 1u: { lit = (fl & 9u) == 9u; }
          case 2u: { lit = ap && (bits & 28u) == 4u; }
          case 3u: { lit = ap && (bits & 28u) == 8u; }
          case 4u: { lit = ap && (bits & 28u) == 16u; }
          case 6u: { lit = ap && (bits & 224u) == 32u; }
          case 7u: { lit = ap && (bits & 224u) == 64u; }
          case 10u: { lit = (bits & 2u) != 0u; }
          case 11u: { lit = ap; }
          default: {}
        }
        c = select(vec3f(0.0), vec3f(0.30, 1.0, 0.40) * 0.9, lit);
      }
    }
  }
  return c;
}
