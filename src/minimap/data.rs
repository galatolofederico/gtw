//! Small CPU rasterizer: bake the plan once per tile, never per frame.
use crate::{geo::TileId, mvt, sources::Sources};
use image::{Rgba, RgbaImage};

pub struct Place {
    pub name: String,
    pub fr: String,
    pub en: String,
    pub xy: [f64; 2],
    pub priority: u8,
    pub min_zoom: u8,
}

impl Place {
    /// Resolve at draw time so changing the game language also updates cached tiles.
    pub fn label(&self, lang: crate::i18n::Lang) -> &str {
        // The tiles carry English and French names only; the other languages
        // look the English up in their language file (`text.place.*`), and
        // all fall back to the tile's own name when the field is blank.
        let translated: &str = match lang.locale() {
            "fr" => &self.fr,
            "en" => &self.en,
            _ => crate::i18n::tr_en_in(lang, &self.en).filter(|_| !self.en.is_empty()).unwrap_or(&self.en),
        };
        if translated.is_empty() { &self.name } else { translated }
    }

    fn from_feature(f: &mvt::Feature, id: TileId, extent: f32) -> Option<Self> {
        if f.kind != 1 || !extent.is_finite() || extent <= 0.0 { return None; }
        let get = |key: &str| f.props.get(key).and_then(mvt::Val::str).unwrap_or("").trim();
        let (priority, min_zoom) = match get("class") {
            "city" => (0, 3), "town" => (1, 9), "village" => (2, 11), "hamlet" => (3, 13),
            _ => return None,
        };
        let p = f.rings.first()?.first()?;
        let name = [get("name"), get("name:en"), get("name:fr")].into_iter().find(|s| !s.is_empty())?;
        let n = (1u32 << id.z) as f64;
        Some(Self { name: name.chars().take(80).collect(), fr: get("name:fr").chars().take(80).collect(), en: get("name:en").chars().take(80).collect(),
            xy: [(id.x as f64 + p[0] as f64/extent as f64)/n, (id.y as f64 + p[1] as f64/extent as f64)/n], priority, min_zoom })
    }
}

pub struct Data {
    pub image: RgbaImage,
    pub places: Vec<Place>,
}

fn polygon(img: &mut RgbaImage, rings: &[Vec<[f32; 2]>], k: f32, color: Rgba<u8>) {
    // Even/odd fill preserves holes and disjoint polygons.
    let mut xs = Vec::new();
    for y in 0..256 {
        xs.clear();
        let scan = (y as f32 + 0.5) / k;
        for ring in rings {
            if ring.len() < 3 { continue; }
            for i in 0..ring.len() {
                let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                if (a[1] > scan) != (b[1] > scan) {
                    xs.push((a[0] + (scan - a[1]) * (b[0] - a[0]) / (b[1] - a[1])) * k);
                }
            }
        }
        xs.sort_by(f32::total_cmp);
        for pair in xs.chunks_exact(2) {
            for x in ((pair[0] - 0.5).ceil() as i32).max(0)..((pair[1] - 0.5).ceil() as i32).min(256) {
                img.put_pixel(x as u32, y, color);
            }
        }
    }
}

fn line(img: &mut RgbaImage, a: [f32; 2], b: [f32; 2], k: f32, width: f32, color: Rgba<u8>) {
    let (ax, ay, bx, by) = (a[0]*k, a[1]*k, b[0]*k, b[1]*k);
    let (dx, dy) = (bx-ax, by-ay);
    let len = dx*dx+dy*dy;
    let r = width * 0.5;
    for y in ((ay.min(by)-r).floor() as i32).max(0)..=((ay.max(by)+r).ceil() as i32).min(255) {
        for x in ((ax.min(bx)-r).floor() as i32).max(0)..=((ax.max(bx)+r).ceil() as i32).min(255) {
            let (px, py) = (x as f32+0.5-ax, y as f32+0.5-ay);
            let t = if len > 0.0 { ((px*dx+py*dy)/len).clamp(0.0, 1.0) } else { 0.0 };
            if (px-t*dx).powi(2)+(py-t*dy).powi(2) <= r*r { img.put_pixel(x as u32, y as u32, color); }
        }
    }
}

pub fn load(src: &Sources, id: TileId) -> Option<Data> {
    let mut layers = src.map_layers(id).ok()?;
    let mut image = RgbaImage::from_pixel(256, 256, Rgba([236, 233, 221, 255]));
    for name in ["landcover", "landuse", "water"] {
        if let Some(layer) = layers.remove(name) {
            if layer.extent <= 0.0 { continue; }
            for f in layer.features.iter().filter(|f| f.kind == 3) {
                let class = f.props.get("class").and_then(mvt::Val::str).unwrap_or("");
                let color = match (name, class) {
                    ("water", _) => [153, 202, 222, 255],
                    (_, "wood" | "forest") => [177, 205, 161, 255],
                    (_, "grass" | "park" | "farmland") => [211, 224, 185, 255],
                    ("landuse", "residential" | "commercial" | "industrial") => [218, 213, 206, 255],
                    _ => continue,
                };
                polygon(&mut image, &f.rings, 256.0/layer.extent, Rgba(color));
            }
        }
    }
    if let Some(layer) = layers.remove("transportation") {
        if layer.extent > 0.0 {
            for f in layer.features.iter().filter(|f| f.kind == 2) {
                let class = f.props.get("class").and_then(mvt::Val::str).unwrap_or("");
                let (width, color) = match class {
                    "motorway" | "trunk" => (2.5, [232, 160, 96, 255]),
                    "primary" | "secondary" => (1.8, [244, 198, 112, 255]),
                    "tertiary" | "minor" if id.z >= 11 => (1.3, [255, 255, 250, 255]),
                    _ => continue,
                };
                for ring in &f.rings { for pair in ring.windows(2) { line(&mut image, pair[0], pair[1], 256.0/layer.extent, width, Rgba(color)); } }
            }
        }
    }
    let mut places = Vec::new();
    if let Some(layer) = layers.remove("place") {
        places.extend(layer.features.iter().filter_map(|f| Place::from_feature(f, id, layer.extent)));
    }

    Some(Data { image, places })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn place_names_follow_language_without_reloading() {
        use crate::i18n::Lang;
        let mut feature = mvt::Feature { kind: 1, rings: vec![vec![[2048.0, 2048.0]]], props: std::collections::HashMap::new() };
        for (key, value) in [("class", "city"), ("name", "Brussel"), ("name:fr", "Bruxelles"), ("name:en", "Brussels")] {
            feature.props.insert(key.into(), mvt::Val::Str(value.into()));
        }
        let id = TileId::new(3, 4, 2);
        let place = Place::from_feature(&feature, id, 4096.0).unwrap();
        assert_eq!(place.label(Lang::parse("fr").unwrap()), "Bruxelles");
        assert_eq!(place.label(Lang::EN), "Brussels");
        assert_eq!(place.label(Lang::parse("fr").unwrap()), "Bruxelles");
        assert_eq!(place.xy, [4.5 / 8.0, 2.5 / 8.0]);
        feature.props.remove("name:en");
        feature.props.insert("name:fr".into(), mvt::Val::Str("  ".into()));
        let place = Place::from_feature(&feature, id, 4096.0).unwrap();
        assert_eq!(place.label(Lang::EN), "Brussel");
        assert_eq!(place.label(Lang::parse("fr").unwrap()), "Brussel");
        feature.props.remove("name");
        assert!(Place::from_feature(&feature, id, 4096.0).is_none());
        feature.props.insert("name:en".into(), mvt::Val::Str("Brussels".into()));
        assert_eq!(Place::from_feature(&feature, id, 4096.0).unwrap().label(Lang::parse("fr").unwrap()), "Brussels");
    }
    #[test]
    fn polygon_preserves_holes() {
        let mut image = RgbaImage::new(256, 256);
        let outer = vec![[0.0,0.0],[20.0,0.0],[20.0,20.0],[0.0,20.0]];
        let hole = vec![[5.0,5.0],[15.0,5.0],[15.0,15.0],[5.0,15.0]];
        polygon(&mut image, &[outer,hole], 1.0, Rgba([255;4]));
        assert_eq!(image.get_pixel(2,2).0, [255;4]);
        assert_eq!(image.get_pixel(10,10).0, [0;4]);
    }
}
