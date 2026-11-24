// Atmosphere lookup tables: transmittance, multiple scattering, sky view.
@fragment fn fs_trans(in: FsIn) -> @location(0) vec4f {
  let rg = G.planet.w;
  let uv = (in.uv - vec2f(0.5 / 256.0, 0.5 / 64.0)) / vec2f(255.0 / 256.0, 63.0 / 64.0);
  let hh = sqrt(ATM_H * (2.0 * rg + ATM_H));
  let rho = hh * clamp(uv.y, 0.0, 1.0);
  let r = sqrt(rho * rho + rg * rg);
  let h = rho * rho / (r + rg);
  let dmin = ATM_H - h;
  let dmax = rho + hh;
  let d = dmin + clamp(uv.x, 0.0, 1.0) * (dmax - dmin);
  var mu = 1.0;
  if (d > 0.0) { mu = clamp((hh * hh - rho * rho - d * d) / (2.0 * r * d), -1.0, 1.0); }
  let o = vec3f(0.0, r, 0.0);
  let dir = vec3f(sqrt(max(1.0 - mu * mu, 0.0)), mu, 0.0);
  var od = vec3f(0.0);
  let n = 48;
  for (var i = 0; i < n; i++) {
    let p = o + dir * (d * (f32(i) + 0.5) / f32(n));
    od += atm_ext(atm_density(length(p) - rg)) * (d / f32(n));
  }
  return vec4f(exp(-od), 1.0);
}

// Hillaire 2020 multiple-scattering approximation.
@fragment fn fs_ms(in: FsIn) -> @location(0) vec4f {
  let rg = G.planet.w;
  let uv = (in.uv - 0.5 / 32.0) / (31.0 / 32.0);
  let mu_s = clamp(uv.x, 0.0, 1.0) * 2.0 - 1.0;
  let h = clamp(uv.y, 0.0, 1.0) * ATM_H;
  let o = vec3f(0.0, rg + max(h, 1.0), 0.0);
  let sun = vec3f(sqrt(max(1.0 - mu_s * mu_s, 0.0)), mu_s, 0.0);
  var l2 = vec3f(0.0);
  var fms = vec3f(0.0);
  let nd = 64;
  for (var k = 0; k < nd; k++) {
    // Fibonacci sphere
    let z = 1.0 - (2.0 * f32(k) + 1.0) / f32(nd);
    let rr = sqrt(max(1.0 - z * z, 0.0));
    let ph = f32(k) * 2.399963;
    let dir = vec3f(rr * cos(ph), z, rr * sin(ph));
    let top = ray_sphere(o, dir, rg + ATM_H);
    var tmax = top.y;
    let gnd = ray_sphere(o, dir, rg);
    let hit = gnd.x > 0.0;
    if (hit) { tmax = gnd.x; }
    var T = vec3f(1.0);
    var L = vec3f(0.0);
    var F = vec3f(0.0);
    let n = 20;
    let dt = tmax / f32(n);
    for (var i = 0; i < n; i++) {
      let p = o + dir * (dt * (f32(i) + 0.5));
      let r = length(p);
      let hp = r - rg;
      let d = atm_density(hp);
      let ext = atm_ext(d);
      let sc = RAYLEIGH * d.x + vec3f(MIE_S * d.y);
      let st = exp(-ext * dt);
      let w = T * (vec3f(1.0) - st) / max(ext, vec3f(1e-12));
      L += w * sc * sun_trans(hp, dot(p / r, sun)) / (4.0 * PI);
      F += w * sc;
      T *= st;
    }
    if (hit) {
      let p = o + dir * tmax;
      let nrm = normalize(p);
      L += T * sun_trans(0.0, dot(nrm, sun)) * max(dot(nrm, sun), 0.0) * 0.3 / PI;
    }
    l2 += L / f32(nd);
    fms += F / f32(nd);
  }
  return vec4f(l2 / (vec3f(1.0) - fms), 1.0);
}

@fragment fn fs_sky(in: FsIn) -> @location(0) vec4f {
  let v = 1.0 - in.uv.y;
  let s = v * 2.0 - 1.0;
  let el = sign(s) * s * s * PI * 0.5;
  let az = (in.uv.x - 0.5) * 2.0 * PI;
  let rd = normalize(cos(el) * (cos(az) * G.east.xyz + sin(az) * G.north.xyz) + sin(el) * G.up.xyz);
  let sc = sky_march(rd, 32, 0.5);
  var c = sc.L;
  // Looking down from altitude: a dim ocean-ish ground so reflections are not black.
  let gnd = ray_sphere(-G.planet.xyz, rd, G.planet.w);
  if (gnd.x > 0.0) {
    let p = -G.planet.xyz + rd * gnd.x;
    let mu = dot(normalize(p), G.sun_dir.xyz);
    c += sc.T * vec3f(0.03, 0.06, 0.09) * sun_trans(0.0, mu) * max(mu, 0.0) * G.sun_dir.w / PI;
  } else {
    // Airglow and scattered starlight, matching sky.wgsl so night reflections
    // see the deep blue night sky rather than black.
    let night = 1.0 - smoothstep(-0.2, 0.05, dot(G.up.xyz, G.sun_dir.xyz));
    c += vec3f(0.0004, 0.0008, 0.0019) * night * (1.0 - 0.4 * max(sin(el), 0.0));
  }
  return vec4f(c, 1.0);
}
