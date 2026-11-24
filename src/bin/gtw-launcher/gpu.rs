//! The graphics card the launcher's window was given (wgpu picks the same
//! high-performance adapter the game does), and the preset suggested for it.
//!
//! The suggestion starts from a rough speed score per GPU model, 100 being an
//! RTX 3060, which flies Medium at about 65 fps at 1080p and High at about
//! 43 (graphics.rs: High is about 23 ms of GPU work a frame on it). The score
//! is divided by the screen's pixels relative to 1080p. It is only a first
//! guess: the graphics test measures the real thing.
use crate::graphics_core::Preset;

#[derive(Clone, Debug)]
pub struct Gpu {
    pub name: String,
    pub vendor: u32,
    pub kind: wgpu::DeviceType,
    pub backend: String,
}

impl Gpu {
    pub fn from_info(i: &wgpu::AdapterInfo) -> Self {
        Gpu { name: i.name.clone(), vendor: i.vendor, kind: i.device_type, backend: format!("{:?}", i.backend) }
    }

    pub fn kind_label(&self) -> &'static str {
        match self.kind {
            wgpu::DeviceType::DiscreteGpu => "Dedicated GPU",
            wgpu::DeviceType::IntegratedGpu => "Integrated GPU",
            wgpu::DeviceType::VirtualGpu => "Virtual GPU",
            wgpu::DeviceType::Cpu => "Software renderer",
            wgpu::DeviceType::Other => "GPU",
        }
    }

    pub fn software(&self) -> bool {
        let n = self.name.to_ascii_lowercase();
        self.kind == wgpu::DeviceType::Cpu || ["llvmpipe", "swiftshader", "lavapipe", "basic render", "warp"].iter().any(|s| n.contains(s))
    }
}

/// The first number of at least `digits` digits after `key` in `name`.
fn model_after(name: &str, key: &str, digits: usize) -> Option<u32> {
    let i = name.find(key)? + key.len();
    let rest = &name[i..];
    let start = rest.find(|c: char| c.is_ascii_digit())?;
    // (only a few characters on: "rtx 3060", "rx 7900 xt", "arc a770")
    if start > 3 {
        return None;
    }
    let num: String = rest[start..].chars().take_while(|c| c.is_ascii_digit()).collect();
    (num.len() >= digits).then(|| num.parse().ok()).flatten()
}

/// Rough speed relative to an RTX 3060 (100); None for a card not listed.
pub fn score(gpu: &Gpu) -> Option<f64> {
    let n = gpu.name.to_ascii_lowercase();
    let table = |m: u32, t: &[(u32, f64)]| t.iter().rev().find(|(k, _)| m >= *k).map(|(_, s)| *s);
    if let Some(m) = model_after(&n, "rtx", 4) {
        let s = match m / 1000 {
            2 => table(m, &[(2050, 55.0), (2060, 75.0), (2070, 90.0), (2080, 110.0)]),
            3 => table(m, &[(3050, 60.0), (3060, 100.0), (3070, 145.0), (3080, 185.0), (3090, 205.0)]),
            4 => table(m, &[(4050, 85.0), (4060, 115.0), (4070, 170.0), (4080, 245.0), (4090, 330.0)]),
            5 => table(m, &[(5050, 100.0), (5060, 140.0), (5070, 200.0), (5080, 280.0), (5090, 400.0)]),
            _ => None,
        };
        let ti = if n.contains(" ti") || n.contains("super") { 1.12 } else { 1.0 };
        return s.map(|s| s * ti);
    }
    if let Some(m) = model_after(&n, "gtx", 3) {
        return table(m, &[(750, 15.0), (950, 20.0), (1050, 30.0), (1060, 48.0), (1070, 62.0), (1080, 78.0), (1630, 32.0), (1650, 40.0), (1660, 55.0)]);
    }
    if let Some(m) = model_after(&n, "rx", 3) {
        let s = table(m, &[(460, 18.0), (560, 28.0), (580, 45.0), (5500, 55.0), (5600, 70.0), (5700, 85.0), (6400, 45.0), (6500, 55.0), (6600, 95.0), (6700, 125.0), (6800, 170.0), (6900, 195.0), (7600, 110.0), (7700, 150.0), (7800, 180.0), (7900, 250.0), (9060, 140.0), (9070, 240.0)]);
        // (the 400 to 500 series sit below the 5000 numbers but are older)
        let s = if (400..1000).contains(&m) { table(m, &[(460, 18.0), (560, 28.0), (580, 45.0)]) } else { s };
        let xt = if n.contains(" xt") { 1.1 } else { 1.0 };
        return s.map(|s| s * xt);
    }
    if let Some(m) = model_after(&n, "arc", 3).or_else(|| model_after(&n, "arc(tm)", 3)) {
        return table(m, &[(310, 30.0), (380, 40.0), (580, 80.0), (750, 95.0), (770, 105.0)]).or(Some(60.0));
    }
    if n.contains("apple m") {
        let base = if n.contains("ultra") { 160.0 } else if n.contains("max") { 110.0 } else if n.contains("pro") { 65.0 } else { 35.0 };
        let generation = model_after(&n, "apple m", 1).unwrap_or(1).clamp(1, 6) as f64;
        return Some(base * (1.0 + 0.15 * (generation - 1.0)));
    }
    None
}

/// The preset suggested for a GPU and a screen (pixels), and why.
pub fn recommend(gpu: &Gpu, screen: (u32, u32)) -> (Preset, String) {
    if gpu.software() {
        return (Preset::VeryLow, "a software renderer (no GPU driver in use): Very Low is all it can manage".into());
    }
    let pixels = (screen.0.max(640) as f64 * screen.1.max(480) as f64) / (1920.0 * 1080.0);
    let s = match score(gpu) {
        Some(s) => s,
        None if gpu.kind == wgpu::DeviceType::IntegratedGpu => {
            // the game's own first-start choice (graphics.rs integrated_preset)
            let p = if gpu.vendor == 0x8086 { Preset::VeryLow } else { Preset::Low };
            return (p, format!("integrated graphics share the computer's memory: {} keeps it flying", p.name()));
        }
        None => 90.0,
    };
    let eff = s / pixels.max(0.25);
    let p = if eff >= 230.0 {
        Preset::Ultra
    } else if eff >= 135.0 {
        Preset::High
    } else if eff >= 80.0 {
        Preset::Medium
    } else if eff >= 42.0 {
        Preset::Low
    } else {
        Preset::VeryLow
    };
    let why = match score(gpu) {
        Some(_) => format!("{} at {}x{}: about {:.0}% of an RTX 3060 per pixel at 1080p", short(&gpu.name), screen.0, screen.1, eff),
        None => format!("{} is not in the launcher's list: run the graphics test to be sure", short(&gpu.name)),
    };
    (p, why)
}

/// The GPU's name without the vendor's boilerplate.
pub fn short(name: &str) -> String {
    let mut s = name.replace("(TM)", "").replace("(R)", "").replace("(tm)", "");
    for cut in [" (", "/PCIe", "/SSE"] {
        if let Some(i) = s.find(cut) {
            s.truncate(i);
        }
    }
    s.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(name: &str, kind: wgpu::DeviceType) -> Gpu {
        Gpu { name: name.into(), vendor: 0x10de, kind, backend: "Vulkan".into() }
    }

    #[test]
    fn known_cards() {
        let d = wgpu::DeviceType::DiscreteGpu;
        assert_eq!(score(&g("NVIDIA GeForce RTX 3060", d)), Some(100.0));
        assert_eq!(recommend(&g("NVIDIA GeForce RTX 3060", d), (1920, 1080)).0, Preset::Medium);
        assert_eq!(recommend(&g("NVIDIA GeForce RTX 4090", d), (2560, 1440)).0, Preset::High);
        assert_eq!(recommend(&g("NVIDIA GeForce RTX 4090", d), (1920, 1080)).0, Preset::Ultra);
        assert_eq!(recommend(&g("NVIDIA GeForce GTX 1060 6GB", d), (1920, 1080)).0, Preset::Low);
        assert_eq!(recommend(&g("AMD Radeon RX 7900 XTX", d), (1920, 1080)).0, Preset::Ultra);
        assert_eq!(recommend(&g("AMD Radeon RX 580 Series", d), (1920, 1080)).0, Preset::Low);
        assert_eq!(recommend(&g("llvmpipe (LLVM 17.0.6, 256 bits)", wgpu::DeviceType::Cpu), (1920, 1080)).0, Preset::VeryLow);
        let mut intel = g("Intel(R) UHD Graphics 620", wgpu::DeviceType::IntegratedGpu);
        intel.vendor = 0x8086;
        assert_eq!(recommend(&intel, (1920, 1080)).0, Preset::VeryLow);
    }
}
