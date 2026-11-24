//! Planet geometry. The world is a sphere of mean Earth radius; positions are f64
//! in a planet-fixed frame (x through lat 0 / lon 0, z through the north pole).
use glam::DVec3;
use std::f64::consts::PI;

pub const R: f64 = 6_371_008.8;

pub fn lla_to_pos(lat: f64, lon: f64, h: f64) -> DVec3 {
    let (sl, cl) = lat.sin_cos();
    let (so, co) = lon.sin_cos();
    DVec3::new(cl * co, cl * so, sl) * (R + h)
}

/// Returns (lat, lon, height above the sphere), radians / metres.
pub fn pos_to_lla(p: DVec3) -> (f64, f64, f64) {
    let r = p.length();
    ((p.z / r).clamp(-1.0, 1.0).asin(), p.y.atan2(p.x), r - R)
}

/// Local east / north / up unit vectors at a position.
pub fn enu(p: DVec3) -> (DVec3, DVec3, DVec3) {
    let up = p.normalize();
    let mut east = DVec3::Z.cross(up);
    if east.length_squared() < 1e-12 {
        east = DVec3::Y;
    }
    let east = east.normalize();
    (east, up.cross(east), up)
}

pub fn merc_y_to_lat(y: f64) -> f64 {
    (PI * (1.0 - 2.0 * y)).sinh().atan()
}
pub fn lat_to_merc_y(lat: f64) -> f64 {
    let lat = lat.clamp(-1.4844, 1.4844);
    (1.0 - lat.tan().asinh() / PI) * 0.5
}
pub fn lon_to_merc_x(lon: f64) -> f64 {
    (lon + PI) / (2.0 * PI)
}

#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug)]
pub struct TileId {
    pub z: u8,
    pub x: u32,
    pub y: u32,
}

impl TileId {
    pub fn new(z: u8, x: u32, y: u32) -> Self {
        Self { z, x, y }
    }
    pub fn children(self) -> [TileId; 4] {
        let (z, x, y) = (self.z + 1, self.x * 2, self.y * 2);
        [TileId::new(z, x, y), TileId::new(z, x + 1, y), TileId::new(z, x, y + 1), TileId::new(z, x + 1, y + 1)]
    }
    /// Ancestor at zoom `z` (or self if already at or above it) plus this tile's
    /// sub-rectangle inside it as (offset_x, offset_y, scale) in 0..1 units.
    pub fn ancestor(self, z: u8) -> (TileId, f64, f64, f64) {
        if self.z <= z {
            return (self, 0.0, 0.0, 1.0);
        }
        let d = self.z - z;
        let n = (1u32 << d) as f64;
        let a = TileId::new(z, self.x >> d, self.y >> d);
        (a, (self.x - (a.x << d)) as f64 / n, (self.y - (a.y << d)) as f64 / n, 1.0 / n)
    }
    pub fn at(lat: f64, lon: f64, z: u8) -> TileId {
        Self::at_merc(lon_to_merc_x(lon).rem_euclid(1.0), lat_to_merc_y(lat), z)
    }
    /// tile at zoom z for a point already in mercator units, so callers convert once.
    #[inline]
    pub fn at_merc(mx: f64, my: f64, z: u8) -> TileId {
        let n = (1u64 << z) as f64;
        let x = (mx * n).floor().min(n - 1.0);
        let y = (my * n).floor().clamp(0.0, n - 1.0);
        TileId::new(z, x as u32, y as u32)
    }
    /// Fractional position of a lat/lon inside this tile (0..1, y down).
    pub fn frac(self, lat: f64, lon: f64) -> (f64, f64) {
        let n = (1u64 << self.z) as f64;
        (lon_to_merc_x(lon).rem_euclid(1.0) * n - self.x as f64, lat_to_merc_y(lat) * n - self.y as f64)
    }
    /// lat/lon of a fractional position inside the tile. Rows touching the
    /// mercator limit are pulled to the pole so the globe is closed.
    pub fn lat_lon(self, u: f64, v: f64) -> (f64, f64) {
        let n = (1u64 << self.z) as f64;
        let my = (self.y as f64 + v) / n;
        let lat = if my <= 1e-9 {
            PI / 2.0
        } else if my >= 1.0 - 1e-9 {
            -PI / 2.0
        } else {
            merc_y_to_lat(my)
        };
        (lat, (self.x as f64 + u) / n * 2.0 * PI - PI)
    }
    /// Approximate east-west width in metres at the tile's centre latitude.
    pub fn width_m(self) -> f64 {
        let (lat, _) = self.lat_lon(0.5, 0.5);
        2.0 * PI * R * lat.cos().max(0.02) / (1u64 << self.z) as f64
    }
}
