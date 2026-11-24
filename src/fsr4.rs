//! AMD FSR 4.1 (machine-learning upscaling) from AMD's own runtime.
//!
//! FSR 4 is not open source: AMD ships it only as signed Windows DLLs in the
//! FSR SDK, under AMD's licence (docs/upscaling.md). So the game never ships
//! it. On request (graphics settings) it downloads the two Vulkan DLLs and the
//! licence from AMD's FSR SDK v2.4.0 release on GitHub, pinned by size and
//! SHA-256, into the game's data folder. The same files hold AMD's own
//! FSR 3.1.6, which AMD's runtime uses on GPUs without FSR 4.
//!
//! Requirements (AMD, SDK 2.4.0 release notes and docs): Windows, the Vulkan
//! renderer, and a Radeon RX 6000 series or newer, a Ryzen AI Max 300/400
//! ("Strix Halo", "Gorgon Halo"), a Ryzen AI 9 300/400 ("Strix Point",
//! "Gorgon Point") or a Ryzen Z2 handheld; AMD Software: Adrenalin Edition
//! 26.10.1 or later for Vulkan.

use parking_lot::Mutex;
use std::path::PathBuf;

/// The SDK release the files come from.
pub const SDK: &str = "2.4.0";
/// The FSR 4 version that release carries.
pub const VERSION: &str = "4.1.2";
const BASE: &str = "https://raw.githubusercontent.com/GPUOpen-LibrariesAndSDKs/FidelityFX-SDK/v2.4.0/Kits/FidelityFX/";

/// A file of the runtime: where it is in the SDK, its size and SHA-256
/// (checked against GitHub's blob ids for the v2.4.0 tag).
pub struct File {
    pub path: &'static str,
    pub name: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

pub const FILES: [File; 3] = [
    File { path: "signedbin/amd_fidelityfx_loader_vk.dll", name: "amd_fidelityfx_loader_vk.dll", size: 39_184, sha256: "38a56a01fb3565c0757d9f5c1e4e69de4b65e5f2e5769b323b200dc1f142bab9" },
    File { path: "signedbin/amd_fidelityfx_upscaler_vk.dll", name: "amd_fidelityfx_upscaler_vk.dll", size: 36_135_696, sha256: "4a7e895af6e0b615d40a84d59eb8c2b1054ad91cdd0c739567940270d51bdab7" },
    File { path: "docs/license.md", name: "license.md", size: 65_770, sha256: "32357259b965d6a8001e9e508ba60eda6034027c5923a7d226ffbaff813c2bfb" },
];

/// Bytes to download in all.
pub fn download_size() -> u64 {
    FILES.iter().map(|f| f.size).sum()
}

/// Where the runtime goes: `<data>/fablegame/amd-fsr-sdk-2.4.0/` (the data
/// folder keeps its historical name, see `paths::data`).
pub fn dir() -> Option<PathBuf> {
    crate::paths::data().map(|d| d.join(format!("amd-fsr-sdk-{SDK}")))
}

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    NotInstalled,
    /// bytes so far, of
    Downloading(u64, u64),
    Installed,
    Failed(String),
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

/// The runtime's state (checked on disk the first time).
pub fn state() -> State {
    let mut s = STATE.lock();
    if s.is_none() {
        *s = Some(if on_disk() { State::Installed } else { State::NotInstalled });
    }
    s.clone().unwrap()
}

fn set(st: State) {
    *STATE.lock() = Some(st);
}

/// Whether every file is there at its size (the hashes were checked when they
/// were downloaded, and are again before the DLLs are loaded).
fn on_disk() -> bool {
    let Some(d) = dir() else { return false };
    FILES.iter().all(|f| std::fs::metadata(d.join(f.name)).is_ok_and(|m| m.len() == f.size))
}

/// Starts downloading the runtime on a thread (graphics settings' button).
pub fn install() {
    if matches!(state(), State::Downloading(..)) {
        return;
    }
    set(State::Downloading(0, download_size()));
    std::thread::spawn(|| {
        let r = download(&|done, total| set(State::Downloading(done, total)));
        set(match r {
            Ok(()) => State::Installed,
            Err(e) => {
                log::warn!("FSR 4 download failed: {e}");
                State::Failed(e)
            }
        });
    });
}

/// Downloads and checks every file, then moves them into place; nothing is
/// left half-installed on failure.
pub fn download(progress: &dyn Fn(u64, u64)) -> Result<(), String> {
    use std::io::{Read, Write};
    let d = dir().ok_or("no data folder")?;
    let tmp = d.with_extension("part");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(std::time::Duration::from_secs(20)))
        .timeout_recv_body(Some(std::time::Duration::from_secs(120)))
        .user_agent(crate::fetch::USER_AGENT)
        .build()
        .into();
    let total = download_size();
    let mut done = 0u64;
    for f in &FILES {
        let url = format!("{BASE}{}", f.path);
        let mut r = agent.get(&url).call().map_err(|e| format!("{}: {e}", f.name))?;
        let mut rd = r.body_mut().with_config().limit(f.size + 1).reader();
        let mut out = std::fs::File::create(tmp.join(f.name)).map_err(|e| e.to_string())?;
        let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
        let mut buf = vec![0u8; 1 << 20];
        let mut n_file = 0u64;
        loop {
            let n = rd.read(&mut buf).map_err(|e| format!("{}: {e}", f.name))?;
            if n == 0 {
                break;
            }
            ctx.update(&buf[..n]);
            out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            n_file += n as u64;
            done += n as u64;
            progress(done, total);
        }
        if n_file != f.size {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(format!("{}: {n_file} bytes, expected {}", f.name, f.size));
        }
        let got = hex(ctx.finish().as_ref());
        if got != f.sha256 {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(format!("{}: checksum mismatch", f.name));
        }
    }
    std::fs::write(tmp.join("NOTICE.txt"), notice()).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&d);
    std::fs::rename(&tmp, &d).map_err(|e| e.to_string())?;
    Ok(())
}

/// Deletes the downloaded runtime.
pub fn remove() {
    if let Some(d) = dir() {
        let _ = std::fs::remove_dir_all(d);
    }
    set(State::NotInstalled);
}

/// Checks the installed files' hashes (before loading code from them).
pub fn verify() -> Result<PathBuf, String> {
    let d = dir().ok_or("no data folder")?;
    for f in &FILES {
        let data = std::fs::read(d.join(f.name)).map_err(|e| format!("{}: {e}", f.name))?;
        if hex(ring::digest::digest(&ring::digest::SHA256, &data).as_ref()) != f.sha256 {
            return Err(format!("{}: checksum mismatch; download it again", f.name));
        }
    }
    Ok(d)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn notice() -> String {
    format!(
        "AMD FSR runtime (FSR Upscaling {VERSION}, FSR 3.1.6), from AMD's FSR SDK v{SDK}:\n\
         https://github.com/GPUOpen-LibrariesAndSDKs/FidelityFX-SDK/releases/tag/v{SDK}\n\n\
         Downloaded by GTW at the player's request; not part of GTW\n\
         and not covered by its licence. Copyright (C) Advanced Micro Devices, Inc.\n\
         The terms are in license.md, next to this file. Remove it from the\n\
         graphics settings, or by deleting this folder.\n"
    )
}

/// Why FSR 4.1 cannot run on this machine, if it cannot: AMD's runtime is
/// Windows and Vulkan only, for the GPUs AMD lists; `failed` is the runtime's
/// own complaint when it was tried.
pub fn blocked(gpu: &wgpu::AdapterInfo, failed: Option<&str>) -> Option<String> {
    if let Some(e) = blocked_os() {
        return Some(e.into());
    }
    if gpu.backend != wgpu::Backend::Vulkan {
        return Some(format!("the game is drawing with {:?}, and FSR 4.1 is set up for Vulkan here", gpu.backend));
    }
    if gpu_support(gpu.vendor, &gpu.name).is_none() {
        return Some(format!("AMD does not list the {} for FSR 4", gpu.name));
    }
    failed.map(|e| format!("AMD's runtime would not start ({e})"))
}

/// AMD's runtime is Windows only (FABLE_FSR4_DEVICE=1 lets the device setup
/// be tried elsewhere, for testing; the DLLs still will not load).
pub fn blocked_os() -> Option<&'static str> {
    if cfg!(windows) { None } else { Some("AMD ships FSR 4 for Windows only") }
}

/// Whether AMD lists a GPU for FSR 4 (by its PCI vendor and name; AMD's
/// runtime has the final say when it starts). The model kind is AMD's.
pub fn gpu_support(vendor: u32, name: &str) -> Option<&'static str> {
    if vendor != 0x1002 {
        return None;
    }
    let n = name.to_ascii_uppercase();
    let series = |prefix: &str| {
        n.split(|c: char| !c.is_ascii_alphanumeric()).any(|w| w.len() == 4 && w.starts_with(prefix) && w.chars().all(|c| c.is_ascii_digit()))
    };
    let rx = n.contains("RX ");
    if rx && series("9") {
        Some("RDNA 4, FP8 model")
    } else if rx && series("7") {
        Some("RDNA 3, INT8 model")
    } else if rx && series("6") {
        Some("RDNA 2, INT8 model")
    } else if ["8060S", "8050S", "8040S"].iter().any(|m| n.contains(m)) {
        Some("Ryzen AI Max, INT8 model")
    } else if ["890M", "880M", "860M", "Z2"].iter().any(|m| n.contains(m)) {
        Some("Ryzen AI / Z2, lightweight INT8 model")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn which_gpus_amd_lists() {
        assert!(gpu_support(0x1002, "AMD Radeon RX 9070 XT").is_some());
        assert!(gpu_support(0x1002, "AMD Radeon RX 7800 XT").is_some());
        assert!(gpu_support(0x1002, "AMD Radeon RX 6600").is_some());
        assert!(gpu_support(0x1002, "AMD Radeon(TM) 890M Graphics").is_some());
        assert!(gpu_support(0x1002, "AMD Radeon 8060S Graphics").is_some());
        assert!(gpu_support(0x1002, "AMD Radeon RX 5700 XT").is_none(), "RDNA 1 is not listed");
        assert!(gpu_support(0x1002, "AMD Radeon 780M Graphics").is_none(), "Phoenix is not listed");
        assert!(gpu_support(0x10DE, "NVIDIA GeForce RTX 3060").is_none());
    }

    #[test]
    fn files_are_pinned() {
        assert_eq!(download_size(), 39_184 + 36_135_696 + 65_770);
        for f in &FILES {
            assert_eq!(f.sha256.len(), 64);
            assert!(f.path.ends_with(f.name));
        }
    }
}

// ---- Running AMD's runtime (Vulkan) ----
//
// AMD's DLLs record into a Vulkan command buffer of ours and read our images,
// so this reaches under wgpu: the device is opened with the extensions AMD's
// own samples enable for FSR 4 (cooperative matrices, 8-bit floats, integer
// dot products, 8/16-bit storage, subgroup size control), and each frame the
// runtime records into a command buffer of its own (wgpu does not allow raw
// and wgpu commands in one encoder), between the scene and the tonemap. It is
// compiled on Linux too, so it is type-checked there, but only runs on Windows.

#[cfg(any(windows, target_os = "linux"))]
pub mod rt {
    use ash::vk;
    use std::ffi::{c_char, c_void, CStr};
    use wgpu::hal::api::Vulkan;

    // VK_EXT_shader_float8 is newer than ash 0.38 (vulkan_core.h).
    const EXT_SHADER_FLOAT8: &CStr = c"VK_EXT_shader_float8";
    const STYPE_FLOAT8: vk::StructureType = vk::StructureType::from_raw(1_000_567_000);
    #[repr(C)]
    struct Float8Features {
        s_type: vk::StructureType,
        p_next: *mut c_void,
        shader_float8: vk::Bool32,
        shader_float8_cooperative_matrix: vk::Bool32,
    }

    /// Whether to open the device for FSR 4: on Windows with the runtime
    /// downloaded, or anywhere with FABLE_FSR4_DEVICE=1 (to test this path).
    pub fn wanted(adapter: &wgpu::AdapterInfo) -> bool {
        let forced = std::env::var("FABLE_FSR4_DEVICE").is_ok_and(|v| v == "1");
        adapter.backend == wgpu::Backend::Vulkan && (forced || (cfg!(windows) && super::gpu_support(adapter.vendor, &adapter.name).is_some() && matches!(super::state(), super::State::Installed)))
    }

    /// The wgpu features FSR 4 wants that wgpu can turn on itself (16-bit
    /// floats and integers in shaders), where the adapter has them.
    pub fn features(adapter: &wgpu::Adapter) -> wgpu::Features {
        adapter.features() & (wgpu::Features::SHADER_F16 | wgpu::Features::SHADER_I16)
    }

    /// Opens the device through wgpu-hal with FSR 4's extensions and features
    /// added to wgpu's own (only those the GPU has).
    pub fn open_device(adapter: &wgpu::Adapter, desc: &wgpu::DeviceDescriptor) -> Result<(wgpu::Device, wgpu::Queue), String> {
        unsafe {
            let open = {
                let hal = adapter.as_hal::<Vulkan>().ok_or("not a Vulkan adapter")?;
                let instance = hal.shared_instance().raw_instance().clone();
                let pd = hal.raw_physical_device();
                let have: Vec<String> = instance
                    .enumerate_device_extension_properties(pd)
                    .map_err(|e| e.to_string())?
                    .iter()
                    .filter_map(|e| e.extension_name_as_c_str().ok().map(|s| s.to_string_lossy().into_owned()))
                    .collect();
                let has = |n: &CStr| have.iter().any(|h| h.as_bytes() == n.to_bytes());
                // What the GPU supports of each feature block.
                let mut coop = vk::PhysicalDeviceCooperativeMatrixFeaturesKHR::default();
                let mut f16i8 = vk::PhysicalDeviceShaderFloat16Int8Features::default();
                let mut s16 = vk::PhysicalDevice16BitStorageFeatures::default();
                let mut s8 = vk::PhysicalDevice8BitStorageFeatures::default();
                let mut sub = vk::PhysicalDeviceSubgroupSizeControlFeatures::default();
                let mut dot = vk::PhysicalDeviceShaderIntegerDotProductFeatures::default();
                let mut f8 = Float8Features { s_type: STYPE_FLOAT8, p_next: std::ptr::null_mut(), shader_float8: 0, shader_float8_cooperative_matrix: 0 };
                {
                    let mut q = vk::PhysicalDeviceFeatures2::default().push_next(&mut f16i8).push_next(&mut s16).push_next(&mut s8).push_next(&mut sub).push_next(&mut dot);
                    if has(vk::KHR_COOPERATIVE_MATRIX_NAME) {
                        q = q.push_next(&mut coop);
                    }
                    if has(EXT_SHADER_FLOAT8) {
                        f8.p_next = q.p_next;
                        q.p_next = &mut f8 as *mut Float8Features as *mut c_void;
                    }
                    instance.get_physical_device_features2(pd, &mut q);
                }
                let want_coop = coop.cooperative_matrix == vk::TRUE;
                let want_f8 = f8.shader_float8 == vk::TRUE && f8.shader_float8_cooperative_matrix == vk::TRUE;
                let mut ext: Vec<&'static CStr> = vec![];
                for (n, on) in [
                    (vk::KHR_COOPERATIVE_MATRIX_NAME, want_coop),
                    (EXT_SHADER_FLOAT8, want_f8),
                    (vk::KHR_SHADER_FLOAT16_INT8_NAME, true),
                    (vk::KHR_16BIT_STORAGE_NAME, true),
                    (vk::KHR_8BIT_STORAGE_NAME, true),
                    (vk::EXT_SUBGROUP_SIZE_CONTROL_NAME, true),
                    (vk::KHR_SHADER_INTEGER_DOT_PRODUCT_NAME, true),
                ] {
                    if on && has(n) {
                        ext.push(n);
                    }
                }
                log::info!("FSR 4 device: adding {:?}", ext);
                let cb = Box::new(move |args: wgpu::hal::vulkan::CreateDeviceCallbackArgs<'_, '_, '_>| {
                    for e in &ext {
                        if !args.extensions.contains(e) {
                            args.extensions.push(e);
                        }
                    }
                    let ci = args.create_info;
                    // Turn the features on in wgpu's own blocks where it chained
                    // them, and chain ours for the rest (Vulkan forbids twins).
                    let mut seen = std::collections::HashSet::new();
                    let mut p = ci.p_next as *mut vk::BaseOutStructure;
                    while !p.is_null() {
                        let st = (*p).s_type;
                        seen.insert(st);
                        match st {
                            vk::StructureType::PHYSICAL_DEVICE_COOPERATIVE_MATRIX_FEATURES_KHR => (*(p as *mut vk::PhysicalDeviceCooperativeMatrixFeaturesKHR)).cooperative_matrix |= coop.cooperative_matrix,
                            vk::StructureType::PHYSICAL_DEVICE_SHADER_FLOAT16_INT8_FEATURES => {
                                let s = &mut *(p as *mut vk::PhysicalDeviceShaderFloat16Int8Features);
                                s.shader_float16 |= f16i8.shader_float16;
                                s.shader_int8 |= f16i8.shader_int8;
                            }
                            vk::StructureType::PHYSICAL_DEVICE_16BIT_STORAGE_FEATURES => {
                                let s = &mut *(p as *mut vk::PhysicalDevice16BitStorageFeatures);
                                s.storage_buffer16_bit_access |= s16.storage_buffer16_bit_access;
                                s.uniform_and_storage_buffer16_bit_access |= s16.uniform_and_storage_buffer16_bit_access;
                            }
                            vk::StructureType::PHYSICAL_DEVICE_8BIT_STORAGE_FEATURES => {
                                let s = &mut *(p as *mut vk::PhysicalDevice8BitStorageFeatures);
                                s.storage_buffer8_bit_access |= s8.storage_buffer8_bit_access;
                                s.uniform_and_storage_buffer8_bit_access |= s8.uniform_and_storage_buffer8_bit_access;
                            }
                            vk::StructureType::PHYSICAL_DEVICE_SUBGROUP_SIZE_CONTROL_FEATURES => {
                                let s = &mut *(p as *mut vk::PhysicalDeviceSubgroupSizeControlFeatures);
                                s.subgroup_size_control |= sub.subgroup_size_control;
                                s.compute_full_subgroups |= sub.compute_full_subgroups;
                            }
                            vk::StructureType::PHYSICAL_DEVICE_SHADER_INTEGER_DOT_PRODUCT_FEATURES => (*(p as *mut vk::PhysicalDeviceShaderIntegerDotProductFeatures)).shader_integer_dot_product |= dot.shader_integer_dot_product,
                            _ => {}
                        }
                        p = (*p).p_next;
                    }
                    // Our blocks live as long as the device (a few bytes, once).
                    let mut chain = |st: vk::StructureType, block: *mut vk::BaseOutStructure| {
                        if !seen.contains(&st) {
                            (*block).p_next = ci.p_next as *mut vk::BaseOutStructure;
                            ci.p_next = block as *const c_void;
                        }
                    };
                    if want_coop {
                        let b = Box::into_raw(Box::new(vk::PhysicalDeviceCooperativeMatrixFeaturesKHR { cooperative_matrix: vk::TRUE, ..Default::default() }));
                        chain(vk::StructureType::PHYSICAL_DEVICE_COOPERATIVE_MATRIX_FEATURES_KHR, b as *mut vk::BaseOutStructure);
                    }
                    let b = Box::into_raw(Box::new(vk::PhysicalDeviceShaderFloat16Int8Features { shader_float16: f16i8.shader_float16, shader_int8: f16i8.shader_int8, ..Default::default() }));
                    chain(vk::StructureType::PHYSICAL_DEVICE_SHADER_FLOAT16_INT8_FEATURES, b as *mut vk::BaseOutStructure);
                    let b = Box::into_raw(Box::new(vk::PhysicalDevice16BitStorageFeatures { storage_buffer16_bit_access: s16.storage_buffer16_bit_access, uniform_and_storage_buffer16_bit_access: s16.uniform_and_storage_buffer16_bit_access, ..Default::default() }));
                    chain(vk::StructureType::PHYSICAL_DEVICE_16BIT_STORAGE_FEATURES, b as *mut vk::BaseOutStructure);
                    let b = Box::into_raw(Box::new(vk::PhysicalDevice8BitStorageFeatures { storage_buffer8_bit_access: s8.storage_buffer8_bit_access, uniform_and_storage_buffer8_bit_access: s8.uniform_and_storage_buffer8_bit_access, ..Default::default() }));
                    chain(vk::StructureType::PHYSICAL_DEVICE_8BIT_STORAGE_FEATURES, b as *mut vk::BaseOutStructure);
                    let b = Box::into_raw(Box::new(vk::PhysicalDeviceSubgroupSizeControlFeatures { subgroup_size_control: sub.subgroup_size_control, compute_full_subgroups: sub.compute_full_subgroups, ..Default::default() }));
                    chain(vk::StructureType::PHYSICAL_DEVICE_SUBGROUP_SIZE_CONTROL_FEATURES, b as *mut vk::BaseOutStructure);
                    let b = Box::into_raw(Box::new(vk::PhysicalDeviceShaderIntegerDotProductFeatures { shader_integer_dot_product: dot.shader_integer_dot_product, ..Default::default() }));
                    chain(vk::StructureType::PHYSICAL_DEVICE_SHADER_INTEGER_DOT_PRODUCT_FEATURES, b as *mut vk::BaseOutStructure);
                    if want_f8 {
                        let b = Box::into_raw(Box::new(Float8Features { s_type: STYPE_FLOAT8, p_next: std::ptr::null_mut(), shader_float8: vk::TRUE, shader_float8_cooperative_matrix: vk::TRUE }));
                        chain(STYPE_FLOAT8, b as *mut vk::BaseOutStructure);
                    }
                });
                hal.open_with_callback(desc.required_features, &desc.required_limits, &desc.memory_hints, Some(cb)).map_err(|e| e.to_string())?
            };
            adapter.create_device_from_hal(open, desc).map_err(|e| e.to_string())
        }
    }

    // ---- AMD's API (ffx_api.h, ffx_upscale.h, ffx_api_vk.h of SDK 2.4.0) ----

    #[repr(C)]
    struct Header {
        ty: u64,
        next: *mut Header,
    }
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Dim {
        width: u32,
        height: u32,
    }
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Float2 {
        x: f32,
        y: f32,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct ResourceDesc {
        ty: u32,
        format: u32,
        width: u32,
        height: u32,
        depth: u32,
        mip_count: u32,
        flags: u32,
        usage: u32,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Resource {
        resource: *mut c_void,
        desc: ResourceDesc,
        state: u32,
    }
    impl Resource {
        fn null() -> Self {
            Resource { resource: std::ptr::null_mut(), desc: ResourceDesc { ty: 0, format: 0, width: 0, height: 0, depth: 0, mip_count: 0, flags: 0, usage: 0 }, state: 0 }
        }
    }
    #[repr(C)]
    struct VkDeviceContext {
        device: vk::Device,
        physical: vk::PhysicalDevice,
        get_device_proc_addr: vk::PFN_vkGetDeviceProcAddr,
    }
    #[repr(C)]
    struct CreateBackendVk {
        header: Header,
        ctx: VkDeviceContext,
    }
    #[repr(C)]
    struct CreateUpscale {
        header: Header,
        flags: u32,
        max_render: Dim,
        max_upscale: Dim,
        message: Option<unsafe extern "C" fn(u32, *const u16)>,
    }
    #[repr(C)]
    struct CreateUpscaleVersion {
        header: Header,
        version: u32,
    }
    #[repr(C)]
    struct QueryProviderVersion {
        header: Header,
        version_id: u64,
        version_name: *const c_char,
    }
    #[repr(C)]
    struct DispatchUpscale {
        header: Header,
        command_list: *mut c_void,
        color: Resource,
        depth: Resource,
        motion: Resource,
        exposure: Resource,
        reactive: Resource,
        transparency: Resource,
        output: Resource,
        jitter: Float2,
        mv_scale: Float2,
        render_size: Dim,
        upscale_size: Dim,
        sharpen: bool,
        sharpness: f32,
        frame_time_ms: f32,
        pre_exposure: f32,
        reset: bool,
        near: f32,
        far: f32,
        fov_y: f32,
        view_to_meters: f32,
        flags: u32,
    }

    const TYPE_CREATE_UPSCALE: u64 = 0x0001_0000;
    const TYPE_DISPATCH_UPSCALE: u64 = 0x0001_0001;
    const TYPE_CREATE_UPSCALE_VERSION: u64 = 0x0001_000b;
    const TYPE_BACKEND_VK: u64 = 0x0100_0003;
    const TYPE_GET_PROVIDER_VERSION: u64 = 6;
    const UPSCALER_VERSION: u32 = (4 << 22) | (1 << 12) | 2;
    const ENABLE_HDR: u32 = 1 << 0;
    const ENABLE_DEPTH_INVERTED: u32 = 1 << 3;
    const ENABLE_DEPTH_INFINITE: u32 = 1 << 4;
    const TEX2D: u32 = 2;
    const FMT_RGBA16F: u32 = 4;
    const FMT_RG32F: u32 = 6;
    const FMT_RG16F: u32 = 18;
    const FMT_R32F: u32 = 28;
    const USAGE_UAV: u32 = 1 << 1;
    const USAGE_DEPTH: u32 = 1 << 2;
    const STATE_UAV: u32 = 1 << 1;
    const STATE_COMPUTE_READ: u32 = 1 << 2;

    type Ctx = *mut c_void;
    type FnCreate = unsafe extern "C" fn(*mut Ctx, *mut Header, *const c_void) -> u32;
    type FnDestroy = unsafe extern "C" fn(*mut Ctx, *const c_void) -> u32;
    type FnQuery = unsafe extern "C" fn(*mut Ctx, *mut Header) -> u32;
    type FnDispatch = unsafe extern "C" fn(*mut Ctx, *const Header) -> u32;

    /// AMD's loader, the upscaling context made with it, and the provider AMD
    /// picked (FSR 4.1.2 where it runs, AMD's FSR 3.1.6 otherwise).
    pub struct Runtime {
        _lib: libloading::Library,
        destroy: FnDestroy,
        dispatch: FnDispatch,
        ctx: Ctx,
        pub provider: String,
        pub render: (u32, u32),
        pub display: (u32, u32),
    }

    // The context is only touched from the render thread.
    unsafe impl Send for Runtime {}

    impl Runtime {
        /// Loads the verified runtime and makes an upscaling context for
        /// these sizes on wgpu's Vulkan device.
        pub fn new(device: &wgpu::Device, render: (u32, u32), display: (u32, u32)) -> Result<Self, String> {
            let dir = super::verify()?;
            unsafe {
                let lib = libloading::Library::new(dir.join("amd_fidelityfx_loader_vk.dll")).map_err(|e| e.to_string())?;
                let create: FnCreate = *lib.get::<FnCreate>(b"ffxCreateContext\0").map_err(|e| e.to_string())?;
                let destroy: FnDestroy = *lib.get::<FnDestroy>(b"ffxDestroyContext\0").map_err(|e| e.to_string())?;
                let query: FnQuery = *lib.get::<FnQuery>(b"ffxQuery\0").map_err(|e| e.to_string())?;
                let dispatch: FnDispatch = *lib.get::<FnDispatch>(b"ffxDispatch\0").map_err(|e| e.to_string())?;
                let hal = device.as_hal::<Vulkan>().ok_or("not a Vulkan device")?;
                let gdpa = hal.shared_instance().raw_instance().fp_v1_0().get_device_proc_addr;
                let mut backend = CreateBackendVk {
                    header: Header { ty: TYPE_BACKEND_VK, next: std::ptr::null_mut() },
                    ctx: VkDeviceContext { device: hal.raw_device().handle(), physical: hal.raw_physical_device(), get_device_proc_addr: gdpa },
                };
                let mut version = CreateUpscaleVersion { header: Header { ty: TYPE_CREATE_UPSCALE_VERSION, next: &mut backend.header }, version: UPSCALER_VERSION };
                let mut desc = CreateUpscale {
                    header: Header { ty: TYPE_CREATE_UPSCALE, next: &mut version.header },
                    flags: ENABLE_HDR | ENABLE_DEPTH_INVERTED | ENABLE_DEPTH_INFINITE,
                    max_render: Dim { width: render.0, height: render.1 },
                    max_upscale: Dim { width: display.0, height: display.1 },
                    message: Some(message),
                };
                let mut ctx: Ctx = std::ptr::null_mut();
                let r = create(&mut ctx, &mut desc.header, std::ptr::null());
                if r != 0 || ctx.is_null() {
                    return Err(format!("ffxCreateContext returned {r}"));
                }
                let mut v = QueryProviderVersion { header: Header { ty: TYPE_GET_PROVIDER_VERSION, next: std::ptr::null_mut() }, version_id: 0, version_name: std::ptr::null() };
                let provider = if query(&mut ctx, &mut v.header) == 0 && !v.version_name.is_null() { CStr::from_ptr(v.version_name).to_string_lossy().into_owned() } else { "unknown".into() };
                log::info!("AMD FSR runtime: upscaler provider {provider}");
                drop(hal);
                Ok(Runtime { _lib: lib, destroy, dispatch, ctx, provider, render, display })
            }
        }

        /// Records the upscale into a command buffer of its own and submits it.
        /// The images must be where wgpu last left them: colour, motion and
        /// exposure sampled, depth sampled (wgpu's depth read-only layout,
        /// moved to AMD's shader-read layout and back here), output as storage.
        pub fn run(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, i: &Images, f: &Frame) -> Result<(), String> {
            let img = |t: &wgpu::Texture| unsafe { t.as_hal::<Vulkan>().map(|h| h.raw_handle()) };
            let (color, depth, motion, exposure, output) = (img(i.color).ok_or("colour")?, img(i.depth).ok_or("depth")?, img(i.motion).ok_or("motion")?, img(i.exposure).ok_or("exposure")?, img(i.output).ok_or("output")?);
            let res = |image: vk::Image, format: u32, (w, h): (u32, u32), usage: u32, state: u32| Resource {
                resource: vk::Handle::as_raw(image) as *mut c_void,
                desc: ResourceDesc { ty: TEX2D, format, width: w, height: h, depth: 1, mip_count: 1, flags: 0, usage },
                state,
            };
            let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("AMD FSR runtime") });
            let mut err = None;
            unsafe {
                let raw_dev = device.as_hal::<Vulkan>().ok_or("not a Vulkan device")?.raw_device().clone();
                enc.as_hal_mut::<Vulkan, _, _>(|e| {
                    let Some(e) = e else {
                        err = Some("no command buffer".to_string());
                        return;
                    };
                    let cmd = e.raw_handle();
                    let depth_barrier = |old: vk::ImageLayout, new: vk::ImageLayout| {
                        let b = vk::ImageMemoryBarrier::default()
                            .src_access_mask(vk::AccessFlags::SHADER_READ)
                            .dst_access_mask(vk::AccessFlags::SHADER_READ)
                            .old_layout(old)
                            .new_layout(new)
                            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                            .image(depth)
                            .subresource_range(vk::ImageSubresourceRange { aspect_mask: vk::ImageAspectFlags::DEPTH, base_mip_level: 0, level_count: 1, base_array_layer: 0, layer_count: 1 });
                        raw_dev.cmd_pipeline_barrier(cmd, vk::PipelineStageFlags::ALL_COMMANDS, vk::PipelineStageFlags::ALL_COMMANDS, vk::DependencyFlags::empty(), &[], &[], &[b]);
                    };
                    depth_barrier(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
                    let d = DispatchUpscale {
                        header: Header { ty: TYPE_DISPATCH_UPSCALE, next: std::ptr::null_mut() },
                        command_list: vk::Handle::as_raw(cmd) as *mut c_void,
                        color: res(color, FMT_RGBA16F, self.render, 0, STATE_COMPUTE_READ),
                        depth: res(depth, FMT_R32F, self.render, USAGE_DEPTH, STATE_COMPUTE_READ),
                        motion: res(motion, FMT_RG16F, self.render, 0, STATE_COMPUTE_READ),
                        exposure: res(exposure, FMT_RG32F, (1, 1), 0, STATE_COMPUTE_READ),
                        reactive: Resource::null(),
                        transparency: Resource::null(),
                        output: res(output, FMT_RGBA16F, self.display, USAGE_UAV, STATE_UAV),
                        jitter: Float2 { x: f.jitter[0], y: f.jitter[1] },
                        // our motion vectors are uv offsets; AMD's are scaled to pixels by this
                        mv_scale: Float2 { x: self.render.0 as f32, y: self.render.1 as f32 },
                        render_size: Dim { width: self.render.0, height: self.render.1 },
                        upscale_size: Dim { width: self.display.0, height: self.display.1 },
                        sharpen: false,
                        sharpness: 0.0,
                        frame_time_ms: f.dt * 1000.0,
                        pre_exposure: 1.0,
                        reset: f.reset,
                        near: f.near,
                        far: 1.0e7,
                        fov_y: f.fov_y,
                        view_to_meters: 1.0,
                        flags: 0,
                    };
                    let r = (self.dispatch)(&mut self.ctx, &d.header);
                    depth_barrier(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL, vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL);
                    if r != 0 {
                        err = Some(format!("ffxDispatch returned {r}"));
                    }
                });
            }
            queue.submit([enc.finish()]);
            err.map_or(Ok(()), Err)
        }
    }

    impl Drop for Runtime {
        fn drop(&mut self) {
            unsafe {
                (self.destroy)(&mut self.ctx, std::ptr::null());
            }
        }
    }

    unsafe extern "C" fn message(ty: u32, msg: *const u16) {
        if msg.is_null() {
            return;
        }
        let mut n = 0;
        while unsafe { *msg.add(n) } != 0 && n < 4096 {
            n += 1;
        }
        let s = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(msg, n) });
        if ty == 0 { log::error!("AMD FSR: {s}") } else { log::warn!("AMD FSR: {s}") }
    }

    /// The textures the runtime reads and writes.
    pub struct Images<'a> {
        pub color: &'a wgpu::Texture,
        pub depth: &'a wgpu::Texture,
        pub motion: &'a wgpu::Texture,
        pub exposure: &'a wgpu::Texture,
        pub output: &'a wgpu::Texture,
    }

    pub struct Frame {
        pub jitter: [f32; 2],
        pub near: f32,
        pub fov_y: f32,
        pub dt: f32,
        pub reset: bool,
    }
}

/// Where AMD's runtime cannot exist (macOS): the same face, never ready.
#[cfg(not(any(windows, target_os = "linux")))]
pub mod rt {
    pub fn wanted(_: &wgpu::AdapterInfo) -> bool {
        false
    }
    pub fn features(_: &wgpu::Adapter) -> wgpu::Features {
        wgpu::Features::empty()
    }
    pub fn open_device(_: &wgpu::Adapter, _: &wgpu::DeviceDescriptor) -> Result<(wgpu::Device, wgpu::Queue), String> {
        Err("FSR 4 needs Windows".into())
    }
    pub struct Runtime {
        pub provider: String,
        pub render: (u32, u32),
        pub display: (u32, u32),
    }
    impl Runtime {
        pub fn new(_: &wgpu::Device, _: (u32, u32), _: (u32, u32)) -> Result<Self, String> {
            Err("FSR 4 needs Windows".into())
        }
        pub fn run(&mut self, _: &wgpu::Device, _: &wgpu::Queue, _: &Images, _: &Frame) -> Result<(), String> {
            Err("FSR 4 needs Windows".into())
        }
    }
    pub struct Images<'a> {
        pub color: &'a wgpu::Texture,
        pub depth: &'a wgpu::Texture,
        pub motion: &'a wgpu::Texture,
        pub exposure: &'a wgpu::Texture,
        pub output: &'a wgpu::Texture,
    }
    pub struct Frame {
        pub jitter: [f32; 2],
        pub near: f32,
        pub fov_y: f32,
        pub dt: f32,
        pub reset: bool,
    }
}
