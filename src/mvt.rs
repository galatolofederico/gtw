//! Minimal Mapbox Vector Tile reader: just enough protobuf to pull polygon
//! layers (water, buildings) and their properties.
use std::collections::HashMap;
use std::io::Read;

struct Pb<'a> {
    b: &'a [u8],
    p: usize,
}
impl<'a> Pb<'a> {
    fn new(b: &'a [u8]) -> Self {
        Self { b, p: 0 }
    }
    fn done(&self) -> bool {
        self.p >= self.b.len()
    }
    fn varint(&mut self) -> u64 {
        let mut v = 0u64;
        let mut s = 0;
        while self.p < self.b.len() {
            let c = self.b[self.p];
            self.p += 1;
            v |= ((c & 0x7f) as u64) << s;
            if c & 0x80 == 0 || s >= 63 {
                break;
            }
            s += 7;
        }
        v
    }
    fn bytes(&mut self) -> &'a [u8] {
        let n = self.varint() as usize;
        let e = (self.p + n).min(self.b.len());
        let s = &self.b[self.p..e];
        self.p = e;
        s
    }
    fn skip(&mut self, wire: u64) {
        match wire {
            0 => {
                self.varint();
            }
            1 => self.p += 8,
            2 => {
                self.bytes();
            }
            5 => self.p += 4,
            _ => self.p = self.b.len(),
        }
    }
    fn fixed(&mut self, n: usize) -> &'a [u8] {
        let e = (self.p + n).min(self.b.len());
        let s = &self.b[self.p..e];
        self.p = e;
        s
    }
}

#[derive(Clone, Debug)]
pub enum Val {
    Str(String),
    Num(f64),
    /// a bool, not kept.
    Bool,
}
impl Val {
    pub fn num(&self) -> Option<f64> {
        match self {
            Val::Num(n) => Some(*n),
            Val::Str(s) => s.parse().ok(),
            _ => None,
        }
    }
    pub fn str(&self) -> Option<&str> {
        match self {
            Val::Str(s) => Some(s),
            _ => None,
        }
    }
}

pub struct Feature {
    /// 1 point, 2 line, 3 polygon
    pub kind: u8,
    /// Rings / lines in tile units (0..extent, y down).
    pub rings: Vec<Vec<[f32; 2]>>,
    pub props: HashMap<String, Val>,
}

pub struct Layer {
    pub extent: f32,
    pub features: Vec<Feature>,
}

pub fn signed_area(r: &[[f32; 2]]) -> f32 {
    let mut a = 0.0;
    for i in 0..r.len() {
        let (p, q) = (r[i], r[(i + 1) % r.len()]);
        a += p[0] * q[1] - q[0] * p[1];
    }
    a * 0.5
}

/// Decode the requested layers of a (possibly gzipped) vector tile.
pub fn decode(data: &[u8], want: &[&str]) -> HashMap<String, Layer> {
    let mut buf = Vec::new();
    let data = if data.len() > 2 && data[0] == 0x1f && data[1] == 0x8b {
        if flate2::read::GzDecoder::new(data).read_to_end(&mut buf).is_err() {
            return HashMap::new();
        }
        &buf[..]
    } else {
        data
    };
    let mut out = HashMap::new();
    let mut pb = Pb::new(data);
    while !pb.done() {
        let key = pb.varint();
        if key >> 3 == 3 && key & 7 == 2 {
            let lb = pb.bytes();
            if let Some((name, layer)) = decode_layer(lb, want) {
                out.insert(name, layer);
            }
        } else {
            pb.skip(key & 7);
        }
    }
    out
}

fn decode_layer(b: &[u8], want: &[&str]) -> Option<(String, Layer)> {
    // First pass: name only, so unwanted layers cost nothing.
    let mut pb = Pb::new(b);
    let mut name = "";
    while !pb.done() {
        let key = pb.varint();
        if key >> 3 == 1 && key & 7 == 2 {
            name = std::str::from_utf8(pb.bytes()).unwrap_or("");
            break;
        }
        pb.skip(key & 7);
    }
    if !want.contains(&name) {
        return None;
    }
    let mut pb = Pb::new(b);
    let (mut keys, mut vals, mut feats, mut extent) = (Vec::new(), Vec::new(), Vec::new(), 4096.0);
    while !pb.done() {
        let key = pb.varint();
        match (key >> 3, key & 7) {
            (2, 2) => feats.push(pb.bytes()),
            (3, 2) => keys.push(String::from_utf8_lossy(pb.bytes()).into_owned()),
            (4, 2) => vals.push(decode_value(pb.bytes())),
            (5, 0) => extent = pb.varint() as f32,
            (_, w) => pb.skip(w),
        }
    }
    let features = feats.into_iter().map(|f| decode_feature(f, &keys, &vals)).collect();
    Some((name.to_string(), Layer { extent, features }))
}

fn decode_value(b: &[u8]) -> Val {
    let mut pb = Pb::new(b);
    let mut v = Val::Bool;
    while !pb.done() {
        let key = pb.varint();
        v = match (key >> 3, key & 7) {
            (1, 2) => Val::Str(String::from_utf8_lossy(pb.bytes()).into_owned()),
            (2, 5) => Val::Num(f32::from_le_bytes(pb.fixed(4).try_into().unwrap_or([0; 4])) as f64),
            (3, 1) => Val::Num(f64::from_le_bytes(pb.fixed(8).try_into().unwrap_or([0; 8]))),
            (4, 0) | (5, 0) => Val::Num(pb.varint() as i64 as f64),
            (6, 0) => {
                let n = pb.varint();
                Val::Num(((n >> 1) as i64 ^ -((n & 1) as i64)) as f64)
            }
            (7, 0) => {
                pb.varint();
                Val::Bool
            }
            (_, w) => {
                pb.skip(w);
                v
            }
        };
    }
    v
}

fn decode_feature(b: &[u8], keys: &[String], vals: &[Val]) -> Feature {
    let mut pb = Pb::new(b);
    let (mut kind, mut tags, mut geom) = (0u8, Vec::new(), Vec::new());
    while !pb.done() {
        let key = pb.varint();
        match (key >> 3, key & 7) {
            (2, 2) => {
                let mut p = Pb::new(pb.bytes());
                while !p.done() {
                    tags.push(p.varint() as usize);
                }
            }
            (3, 0) => kind = pb.varint() as u8,
            (4, 2) => {
                let mut p = Pb::new(pb.bytes());
                while !p.done() {
                    geom.push(p.varint() as u32);
                }
            }
            (_, w) => pb.skip(w),
        }
    }
    let mut props = HashMap::new();
    for kv in tags.chunks_exact(2) {
        if let (Some(k), Some(v)) = (keys.get(kv[0]), vals.get(kv[1])) {
            props.insert(k.clone(), v.clone());
        }
    }
    let zz = |n: u32| ((n >> 1) as i32) ^ -((n & 1) as i32);
    let (mut rings, mut cur): (Vec<Vec<[f32; 2]>>, Vec<[f32; 2]>) = (Vec::new(), Vec::new());
    let (mut x, mut y, mut i) = (0i32, 0i32, 0usize);
    while i < geom.len() {
        let (cmd, count) = (geom[i] & 7, (geom[i] >> 3) as usize);
        i += 1;
        match cmd {
            1 | 2 => {
                for _ in 0..count {
                    if i + 1 >= geom.len() {
                        break;
                    }
                    if cmd == 1 && !cur.is_empty() {
                        rings.push(std::mem::take(&mut cur));
                    }
                    x += zz(geom[i]);
                    y += zz(geom[i + 1]);
                    i += 2;
                    cur.push([x as f32, y as f32]);
                }
            }
            7 => {
                if !cur.is_empty() {
                    rings.push(std::mem::take(&mut cur));
                }
            }
            _ => break,
        }
    }
    if !cur.is_empty() {
        rings.push(cur);
    }
    Feature { kind, rings, props }
}

/// Even-odd scanline fill of a set of rings into an 8-bit raster. `rings` are
/// already in pixel coordinates. Pixels inside get `max(existing, value)`.
pub fn fill_rings(buf: &mut [u8], w: usize, h: usize, rings: &[Vec<[f32; 2]>], value: u8) {
    let mut rows: Vec<Vec<f32>> = vec![Vec::new(); h];
    for r in rings {
        let n = r.len();
        if n < 3 {
            continue;
        }
        for i in 0..n {
            let (a, b) = (r[i], r[(i + 1) % n]);
            if a[1] == b[1] {
                continue;
            }
            let (a, b) = if a[1] < b[1] { (a, b) } else { (b, a) };
            // Scanlines sample at pixel centres y + 0.5.
            let y0 = (a[1] - 0.5).ceil().max(0.0) as i64;
            let y1 = ((b[1] - 0.5).ceil() as i64).min(h as i64);
            let slope = (b[0] - a[0]) / (b[1] - a[1]);
            for y in y0..y1 {
                rows[y as usize].push(a[0] + (y as f32 + 0.5 - a[1]) * slope);
            }
        }
    }
    for (y, xs) in rows.iter_mut().enumerate() {
        if xs.len() < 2 {
            continue;
        }
        xs.sort_by(|a, b| a.total_cmp(b));
        for pair in xs.chunks_exact(2) {
            let x0 = (pair[0] - 0.5).ceil().max(0.0) as i64;
            let x1 = ((pair[1] - 0.5).ceil() as i64).min(w as i64);
            for x in x0..x1 {
                let c = &mut buf[y * w + x as usize];
                *c = (*c).max(value);
            }
        }
    }
}
