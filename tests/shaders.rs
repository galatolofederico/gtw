//! Parses, validates, and translates every shader to MSL without needing a GPU
//! (and compiles the translated MSL with Apple's Metal compiler on macOS).

#[path = "../src/single_sample.rs"]
mod single_sample;

fn msl_check(name: &str, m: &naga::Module, info: &naga::valid::ModuleInfo) -> bool {
    use naga::back::msl;
    let mut resources = msl::BindingMap::default();
    let (mut buf, mut tex, mut samp) = (0u8, 0u8, 0u8);
    for (_, var) in m.global_variables.iter() {
        let Some(binding) = var.binding else { continue };
        let target = match m.types[var.ty].inner {
            naga::TypeInner::Image { .. } => {
                let t = msl::BindTarget { texture: Some(tex), ..Default::default() };
                tex = tex.wrapping_add(1);
                t
            }
            naga::TypeInner::Sampler { .. } => {
                let t = msl::BindTarget { sampler: Some(msl::BindSamplerTarget::Resource(samp)), ..Default::default() };
                samp = samp.wrapping_add(1);
                t
            }
            _ => {
                let mutable = matches!(var.space, naga::AddressSpace::Storage { access } if access.contains(naga::StorageAccess::STORE));
                let t = msl::BindTarget { buffer: Some(buf), mutable, ..Default::default() };
                buf = buf.wrapping_add(1);
                t
            }
        };
        resources.insert(binding, target);
    }
    let mut per_entry_point_map = msl::EntryPointResourceMap::default();
    for ep in &m.entry_points {
        per_entry_point_map.insert(
            ep.name.clone(),
            msl::EntryPointResources { resources: resources.clone(), immediates_buffer: None, sizes_buffer: Some(30) },
        );
    }
    let options = msl::Options {
        lang_version: (2, 4),
        per_entry_point_map,
        fake_missing_bindings: false,
        ..Default::default()
    };
    let mut ok = true;
    for ep in &m.entry_points {
        let (m_ep, info_ep) = match naga::back::pipeline_constants::process_overrides(
            m,
            info,
            Some((ep.stage, ep.name.as_str())),
            &Default::default(),
        ) {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("--- {name} ({}) override resolution failed: {e}", ep.name);
                ok = false;
                continue;
            }
        };
        let pipe = msl::PipelineOptions {
            entry_point: Some((ep.stage, ep.name.clone())),
            vertex_pulling_transform: false,
            ..Default::default()
        };
        match msl::write_string(&m_ep, &info_ep, &options, &pipe) {
            Err(e) => {
                eprintln!("--- {name} ({}) MSL translate failed: {e}", ep.name);
                ok = false;
            }
            Ok((msl_src, _)) => {
                #[cfg(target_os = "macos")]
                if !compile_metal(name, &ep.name, &msl_src) {
                    ok = false;
                }
                #[cfg(not(target_os = "macos"))]
                let _ = msl_src;
            }
        }
    }
    ok
}

#[cfg(target_os = "macos")]
fn compile_metal(name: &str, ep: &str, msl: &str) -> bool {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let Ok(mut child) = Command::new("xcrun")
        .args(["-sdk", "macosx", "metal", "-x", "metal", "-std=macos-metal2.4", "-c", "-", "-o", "/dev/null"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    else {
        eprintln!("--- {name} ({ep}): could not invoke xcrun metal");
        return false;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(msl.as_bytes());
    }
    match child.wait_with_output() {
        Ok(out) if out.status.success() => true,
        Ok(out) => {
            eprintln!("--- {name} ({ep}): Metal refuses the translated shader:\n{}", String::from_utf8_lossy(&out.stderr));
            false
        }
        Err(e) => {
            eprintln!("--- {name} ({ep}): waiting for metal: {e}");
            false
        }
    }
}

fn check(name: &str, src: &str) -> bool {
    match naga::front::wgsl::parse_str(src) {
        Err(e) => {
            eprintln!("--- {name}\n{}", e.emit_to_string(src));
            false
        }
        Ok(m) => match naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all()).validate(&m) {
            Err(e) => {
                eprintln!("--- {name}\n{}", e.emit_to_string(src));
                false
            }
            Ok(info) => msl_check(name, &m, &info),
        },
    }
}

#[test]
fn shaders_validate() {
    let common = include_str!("../src/shaders/common.wgsl");
    // Modules that draw the sky get sky.wgsl too, as in render.rs.
    let sky = include_str!("../src/shaders/sky.wgsl");
    let mut ok = check("noise3d", include_str!("../src/shaders/noise3d.wgsl"));
    ok &= check("resolve", include_str!("../src/shaders/resolve.wgsl"));
    ok &= check("csmix", include_str!("../src/shaders/csmix.wgsl"));
    ok &= check("detail", include_str!("../src/shaders/detail.wgsl"));
    ok &= check("ocean", include_str!("../src/shaders/ocean.wgsl"));
    ok &= check("shore", include_str!("../src/shaders/shore.wgsl"));
    ok &= check("hiz", include_str!("../src/shaders/hiz.wgsl"));
    // AMD FSR 3.1 upscaler port: standalone compute passes (fsr3.rs).
    ok &= check("fsr3", include_str!("../src/shaders/fsr3.wgsl"));
    for (name, src) in [
        ("lut", include_str!("../src/shaders/lut.wgsl")),
        ("atmlut", include_str!("../src/shaders/atmlut.wgsl")),
        ("terrain", include_str!("../src/shaders/terrain.wgsl")),
        ("photo", include_str!("../src/shaders/photo.wgsl")),
        ("post", include_str!("../src/shaders/post.wgsl")),
        ("particles_sim", include_str!("../src/shaders/particles_sim.wgsl")),
        ("particles_draw", include_str!("../src/shaders/particles_draw.wgsl")),
        ("roads", include_str!("../src/shaders/roads.wgsl")),
        ("trees", include_str!("../src/shaders/trees.wgsl")),
        ("vol", include_str!("../src/shaders/vol.wgsl")),
        ("ao", include_str!("../src/shaders/ao.wgsl")),
        ("taa", include_str!("../src/shaders/taa.wgsl")),
        ("airfield", include_str!("../src/shaders/airfield.wgsl")),
        ("lights", include_str!("../src/shaders/lights.wgsl")),
        ("precip", include_str!("../src/shaders/precip.wgsl")),
        ("contrails", include_str!("../src/shaders/contrails.wgsl")),
        ("scenery", include_str!("../src/shaders/scenery.wgsl")),
    ] {
        ok &= check(name, &format!("{common}\n{src}"));
    }
    let fluid_prefix = format!(
        "const FNX: u32 = 64u;\nconst FNY: u32 = 64u;\nconst FNZ: u32 = 64u;\nconst VOLNX: u32 = 128u;\nconst VOLNY: u32 = 96u;\nconst VOLNZ: u32 = 128u;\n{}\n",
        include_str!("../src/shaders/fluid_common.wgsl")
    );
    ok &= check("fluid_sim", &format!("{fluid_prefix}\n{}", include_str!("../src/shaders/fluid_sim.wgsl")));
    ok &= check("fluid_draw", &format!("{common}\n{fluid_prefix}\n{}", include_str!("../src/shaders/fluid_draw.wgsl")));
    // As in render.rs, buildings and instanced solids share the facade shading.
    let facade = include_str!("../src/shaders/facade.wgsl");
    for (name, src) in [("buildings", include_str!("../src/shaders/buildings.wgsl")), ("inst", include_str!("../src/shaders/inst.wgsl"))] {
        ok &= check(name, &format!("{common}\n{facade}\n{src}"));
    }
    for (name, src) in [("composite", include_str!("../src/shaders/composite.wgsl")), ("clouds", include_str!("../src/shaders/clouds.wgsl"))] {
        ok &= check(name, &format!("{common}\n{sky}\n{src}"));
    }
    // Without MSAA (graphics settings) the composite and the depth resolve read plain textures.
    let comp1 = single_sample::for_samples(&format!("{common}\n{sky}\n{}", include_str!("../src/shaders/composite.wgsl")), 1);
    let res1 = single_sample::for_samples(include_str!("../src/shaders/resolve.wgsl"), 1);
    for v in [&comp1, &res1] {
        assert!(!v.contains("_multisampled_2d") && !v.contains("textureNumSamples"), "single-sample variant still multisampled");
    }
    ok &= check("composite (no MSAA)", &comp1);
    ok &= check("resolve (no MSAA)", &res1);
    // plane.wgsl uses the 747 deck's generated string IDs: checked (including MSL/Metal) in b747::deck's tests.
    assert!(ok, "shader errors above");
}
