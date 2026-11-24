// Shared by the fire simulation (fluid_sim.wgsl) and its renderer
// (fluid_draw.wgsl). The grid size FNX, FNY, FNZ is prepended by fluid.rs.
//
// Fields per cell: velocity (m/s, domain axes x east, y north, z up) and the
// scalars soot, temperature, fuel, dust. Temperature is the excess over the
// ambient air in thousands of kelvin; soot, fuel and dust are in arbitrary
// units whose extinction per metre is set by `ext`.
struct FU {
  a: vec4f,      // dt, cell size h (m), time, number of sources
  wind: vec4f,   // wind at 10 m (domain axes, m/s); w vorticity confinement
  k1: vec4f,     // buoyancy, cooling, burn rate, expansion per unit of fuel burnt
  k2: vec4f,     // soot per unit of fuel burnt, heat per unit of fuel burnt, ignition temperature, soot fade
  k3: vec4f,     // dust fade, dust weight, tree drag, wind pull
  sun: vec4f,    // direction to the sun (domain axes), w SOR weight
  ext: vec4f,    // extinction (1/m) per unit of soot, dust, fuel; w flame brightness
  org: vec4f,    // domain corner relative to the camera; w = 1 while it is live
  ax_e: vec4f,   // domain axes in the planet frame; ax_e.w = height of the ground at the site (domain z)
  ax_n: vec4f,   // ax_n.w = detail noise strength
  ax_u: vec4f,   // ax_u.w = sky light scale
  k4: vec4f,     // disturbance (m/s^2), its scale (1/m), flame speed (m/s), fuel flicker
  coarse: vec4f, // corner of the scene's coarse smoke volume (domain metres), w its cell size
};

const FN = vec3i(i32(FNX), i32(FNY), i32(FNZ));
// The lighting grid is half the simulation's resolution.
const LN = vec3i(i32(FNX / 2u), i32(FNY / 2u), i32(FNZ / 2u));

fn fl_sigma(s: vec4f, ext: vec4f) -> f32 {
  return max(s.x, 0.0) * ext.x + max(s.w, 0.0) * ext.y + max(s.z, 0.0) * ext.z;
}
fn fl_kelvin(T: f32) -> f32 { return 290.0 + 1000.0 * max(T, 0.0); }
// Planck's law at a wavelength (um), up to a constant.
fn fl_planck(l: f32, tk: f32) -> f32 {
  return 1.0 / (l * l * l * l * l * (exp(14388.0 / (l * tk)) - 1.0));
}
// Colour of a blackbody as a daylight-balanced camera sees it, scaled to unit luminance.
fn fl_bb_color(tk: f32) -> vec3f {
  let c = vec3f(fl_planck(0.61, tk), fl_planck(0.55, tk), fl_planck(0.465, tk));
  let wb = vec3f(fl_planck(0.61, 6500.0), fl_planck(0.55, 6500.0), fl_planck(0.465, 6500.0));
  let r = c / wb;
  return r / max(dot(r, vec3f(0.2126, 0.7152, 0.0722)), 1e-20);
}
// Visible radiance of hot gas relative to 1500 K. True Planck radiance at
// 550 nm rises about 80 times from 1500 to 2000 K; a camera's (and the eye's)
// response is far gentler than that, so the curve is the square root of
// Planck's: dull red at 1000 K still shows, yellow at 2000 K does not burn
// the whole fireball out to white.
fn fl_radiance(tk: f32) -> f32 {
  return exp(0.5 * 26160.0 * (1.0 / 1500.0 - 1.0 / max(tk, 300.0)));
}
// How much of the extinction emits: soot at its temperature, plus the
// reaction zone where fuel is burning.
fn fl_emitter(s: vec4f, ext: vec4f, ign: f32) -> f32 {
  let burning = smoothstep(ign, ign + 0.5, s.y);
  return max(s.x, 0.0) * ext.x * 0.8 + max(s.z, 0.0) * ext.z * burning;
}
