//! ESA WorldCover 10 m land cover from the Microsoft Planetary Computer data
//! API, decoded from the 256x256 `LA` PNG tiles it publishes.
//!
//! The data is organised as 3-degree items (`ESA_WorldCover_10m_2021_v200_N33E009`
//! and so on) but served through a WebMercatorQuad tile endpoint. A z14 tile
//! that crosses a 3-degree boundary therefore touches up to four items; each
//! item's tile marks where it has data in its alpha channel, and `compose`
//! combines them into one class grid.

use crate::geo::TileId;
use anyhow::{anyhow, Result};

/// Side of a landcover tile in pixels.
pub const SIZE: usize = 256;

/// Item collection name and version.
pub const VERSION: &str = "ESA_WorldCover_10m_2021_v200";

const ENDPOINT: &str = "https://planetarycomputer.microsoft.com/api/data/v1/collections/esa-worldcover/items";

/// A decoded tile: WorldCover class and coverage, one byte each per pixel.
#[derive(Clone)]
pub struct Cover {
    pub class: Vec<u8>,
    pub alpha: Vec<u8>,
}

#[allow(dead_code)]
impl Cover {
    pub fn class_at(&self, x: usize, y: usize) -> u8 {
        self.class[y.min(SIZE - 1) * SIZE + x.min(SIZE - 1)]
    }
}

/// Decode a 256x256 PNG in the API's `LA` layout into class and coverage.
/// Empty, truncated, or wrongly sized images are errors; a valid image whose
/// pixels are all zero decodes to zeros (the caller then falls back to OSM).
pub fn decode(bytes: &[u8]) -> Result<Cover> {
    if bytes.is_empty() {
        return Err(anyhow!("empty landcover image"));
    }
    let img = image::load_from_memory(bytes).map_err(|e| anyhow!("landcover PNG: {e}"))?;
    let img = img.to_luma_alpha8();
    if img.width() as usize != SIZE || img.height() as usize != SIZE {
        return Err(anyhow!("landcover tile is {}x{}, expected {SIZE}x{SIZE}", img.width(), img.height()));
    }
    let mut class = Vec::with_capacity(SIZE * SIZE);
    let mut alpha = Vec::with_capacity(SIZE * SIZE);
    for p in img.pixels() {
        class.push(p.0[0]);
        alpha.push(p.0[1]);
    }
    Ok(Cover { class, alpha })
}

/// The 3-degree WorldCover item containing a latitude/longitude in degrees.
/// Items are named for their south-west corner: Paris is `N48E000`, Buenos
/// Aires `S36W060` (latitude two digits, longitude three). Longitude 180 wraps
/// to the `W180` item.
pub fn item_id(lat: f64, lon: f64) -> String {
    let lat = if lat.is_nan() { 0.0 } else { lat.clamp(-90.0, 90.0) };
    let lon = if lon.is_nan() { 0.0 } else { (lon + 180.0).rem_euclid(360.0) - 180.0 };
    let sw_lat = ((lat / 3.0).floor() as i32) * 3;
    let sw_lon = ((lon / 3.0).floor() as i32) * 3;
    format!(
        "{VERSION}_{}{:02}{}{:03}",
        if sw_lat < 0 { 'S' } else { 'N' },
        sw_lat.abs(),
        if sw_lon < 0 { 'W' } else { 'E' },
        sw_lon.abs()
    )
}

/// The tile endpoint for one item and WebMercatorQuad tile.
pub fn tile_url(item: &str, id: TileId) -> String {
    format!("{ENDPOINT}/{item}/tiles/WebMercatorQuad/{}/{}/{}@1x?assets=map&format=png", id.z, id.x, id.y)
}

/// The up to four items a z14 tile touches: the corners and the centre, with
/// duplicates removed and the centre's item first.
pub fn items_for_tile(id: TileId) -> Vec<String> {
    let (clat, clon) = id.lat_lon(0.5, 0.5);
    let mut out = vec![item_id(clat.to_degrees(), clon.to_degrees())];
    for (u, v) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
        let (lat, lon) = id.lat_lon(u, v);
        let item = item_id(lat.to_degrees(), lon.to_degrees());
        if !out.contains(&item) {
            out.push(item);
        }
    }
    out
}

/// Compose the items covering a tile into one class grid. `load` fetches and
/// decodes one item; items that fail (a missing ocean item, a network error)
/// are skipped, so a partial boundary still loads. All failing is an error.
pub fn compose(id: TileId, mut load: impl FnMut(&str) -> Result<Cover>) -> Result<Vec<u8>> {
    let mut out = vec![0u8; SIZE * SIZE];
    let mut any = false;
    let mut last = None;
    for item in items_for_tile(id) {
        match load(&item) {
            Ok(c) => {
                any = true;
                for ((d, cl), al) in out.iter_mut().zip(c.class.iter()).zip(c.alpha.iter()) {
                    if *al > 0 {
                        *d = *cl;
                    }
                }
            }
            Err(e) => last = Some(e),
        }
    }
    if any {
        Ok(out)
    } else {
        Err(last.unwrap_or_else(|| anyhow!("no landcover tile")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &[u8] = include_bytes!("../tests/fixtures/landcover_z14.png");

    #[test]
    fn item_ids_for_quadrants_and_boundaries() {
        assert_eq!(item_id(1.0, 1.0), "ESA_WorldCover_10m_2021_v200_N00E000");
        assert_eq!(item_id(-1.0, -1.0), "ESA_WorldCover_10m_2021_v200_S03W003");
        assert_eq!(item_id(-34.6037, -58.3816), "ESA_WorldCover_10m_2021_v200_S36W060");
        assert_eq!(item_id(48.8566, 2.3522), "ESA_WorldCover_10m_2021_v200_N48E000");
        assert_eq!(item_id(37.8085, -122.4650), "ESA_WorldCover_10m_2021_v200_N36W123");
        // Exact 3-degree boundaries belong to the tile starting there.
        assert_eq!(item_id(3.0, 3.0), "ESA_WorldCover_10m_2021_v200_N03E003");
        assert_eq!(item_id(-3.0, -3.0), "ESA_WorldCover_10m_2021_v200_S03W003");
        assert_eq!(item_id(2.999, 2.999), "ESA_WorldCover_10m_2021_v200_N00E000");
        assert_eq!(item_id(90.0, 0.0), "ESA_WorldCover_10m_2021_v200_N90E000");
    }

    #[test]
    fn item_ids_wrap_at_the_antimeridian() {
        assert_eq!(item_id(0.0, 180.0), "ESA_WorldCover_10m_2021_v200_N00W180");
        assert_eq!(item_id(0.0, -180.0), "ESA_WorldCover_10m_2021_v200_N00W180");
        assert_eq!(item_id(0.0, 179.999), "ESA_WorldCover_10m_2021_v200_N00E177");
        assert_eq!(item_id(0.0, -179.999), "ESA_WorldCover_10m_2021_v200_N00W180");
    }

    #[test]
    fn a_tile_can_touch_two_items() {
        // Just south of 3 N: a z14 tile (~2.4 km) straddles the boundary.
        let id = TileId::at((3.0f64 - 0.001).to_radians(), 0.0, 14);
        let items = items_for_tile(id);
        assert!(items.len() >= 2, "{items:?}");
        assert!(items.contains(&"ESA_WorldCover_10m_2021_v200_N00E000".to_string()), "{items:?}");
        assert!(items.contains(&"ESA_WorldCover_10m_2021_v200_N03E000".to_string()), "{items:?}");
        // A tile away from any boundary touches one item only.
        let id = TileId::at(2.0f64.to_radians(), 1.0f64.to_radians(), 14);
        assert_eq!(items_for_tile(id).len(), 1);
    }

    #[test]
    fn decodes_the_fixture_quadrants() {
        let c = decode(FIXTURE).unwrap();
        assert_eq!(c.class.len(), SIZE * SIZE);
        assert_eq!(c.alpha.len(), SIZE * SIZE);
        assert_eq!(c.class_at(0, 0), 10);
        assert_eq!(c.class_at(SIZE - 1, 0), 20);
        assert_eq!(c.class_at(0, SIZE - 1), 40);
        assert_eq!(c.class_at(SIZE - 1, SIZE - 1), 80);
        assert_eq!(c.class_at(SIZE / 2, SIZE / 2), 80);
        assert!(c.alpha.iter().all(|a| *a == 255));
    }

    #[test]
    fn empty_truncated_and_bad_inputs_fail() {
        assert!(decode(&[]).is_err());
        assert!(decode(&FIXTURE[..FIXTURE.len() / 2]).is_err());
        assert!(decode(&[0u8; 64]).is_err());
    }

    #[test]
    fn an_all_zero_image_decodes_without_classes() {
        let img = image::GrayAlphaImage::from_pixel(SIZE as u32, SIZE as u32, image::LumaA([0, 0]));
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let c = decode(&png).unwrap();
        assert!(c.class.iter().all(|v| *v == 0));
        assert!(c.alpha.iter().all(|v| *v == 0));
    }

    #[test]
    fn compose_merges_overlapping_items() {
        let id = TileId::at((3.0f64 - 0.001).to_radians(), 0.0, 14);
        let items = items_for_tile(id);
        assert!(items.len() >= 2);
        let south = items[0].clone();
        let cover = |item: &str| {
            let mut c = Cover { class: vec![0; SIZE * SIZE], alpha: vec![0; SIZE * SIZE] };
            let (first, class) = if item == south { (true, 40u8) } else { (false, 10u8) };
            for y in 0..SIZE {
                for x in 0..SIZE {
                    let left = x < SIZE / 2;
                    if left == first {
                        c.class[y * SIZE + x] = class;
                        c.alpha[y * SIZE + x] = 255;
                    }
                }
            }
            Ok(c)
        };
        let out = compose(id, cover).unwrap();
        assert_eq!(out[0], 40);
        assert_eq!(out[SIZE - 1], 10);
    }

    #[test]
    fn compose_fails_when_every_item_fails() {
        let id = TileId::at(2.0f64.to_radians(), 1.0f64.to_radians(), 14);
        assert!(compose(id, |_| Err(anyhow!("offline"))).is_err());
    }
}
