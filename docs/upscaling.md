# Upscaling

Settings → Graphics → **Upscaling** chooses how a frame drawn below the
window's resolution is brought up to it. The render resolution is the
Display card's slider, or one of AMD's quality modes on the Upscaling card.

| Upscaler | What it does | Ships with the game | Where it runs |
|---|---|---|---|
| **Bilinear** | stretches the frame after the game's temporal AA (what the game always did) | yes | everywhere |
| **FSR 1** | AMD FidelityFX Super Resolution 1.0: edge-adaptive upsampling (EASU) after the game's temporal AA, then RCAS sharpening | yes (MIT) | everywhere |
| **FSR 3.1** | AMD FSR 3.1.6's temporal upscaler, ported to WGSL; replaces the game's temporal AA | yes (MIT) | everywhere |
| **FSR 4.1** | AMD FSR Upscaling 4.1.2, AMD's machine-learning upscaler, from AMD's own runtime | **no**: downloaded on request | Windows, Vulkan, the GPUs AMD lists |

Upscalers only apply below 100%. FSR 3.1/4.1 also anti-alias at 100% (AMD's
"Native AA") and never supersample, so the render resolution is held to 100%
while one of them is chosen. Above 100% the frame is supersampled and box
filtered down, as before.

The card marks a **suggested** upscaler (★) for the GPU and window, and can
apply it: FSR 4.1 on GPUs AMD lists where it can run, FSR 3.1 on other
discrete GPUs (NVIDIA, Intel Arc, older AMD), FSR 1 on integrated GPUs and
the plain stretch on software renderers. The quality mode follows AMD's
guidance: Performance (50%) at 4K, Balanced (59%) at 1440p, Quality (67%) at
1080p (`graphics::recommend`).

Command line: `--upscaler bilinear|fsr1|fsr3|fsr4` with `--render-scale`, and
`--fsr4-install` to fetch FSR 4.1's runtime as the settings' button does.

## FSR 1 (`post.wgsl` `fs_easu`, `fs_final_fsr1`)

AMD's float EASU and RCAS from `ffx_fsr1.h` (FSR SDK v2.4.0). The twelve EASU
taps are loaded one by one rather than gathered, so they can be squashed into
0..1 first: AMD's FSR is made for display-range colour, so the frame is put at
its display exposure and through FSR's reversible `c / (1 + max(c))`, upscaled
and sharpened (0.25 stops), then unsquashed for the game's tonemap.

## FSR 3.1 (`fsr3.rs`, `shaders/fsr3.wgsl`)

A line-for-line port of AMD's 32-bit shader paths in
`Kits/FidelityFX/upscalers/fsr3/include/gpu/fsr3upscaler/` (FSR SDK v2.4.0,
FSR 3.1.6), and of what `ffx_fsr3upscaler.cpp` does around them: the
constants, jitter (Halton 2,3 over `8 × ratio²` phases), resets and the pass
order (prepare inputs, luma pyramid, shading change pyramid, shading change,
prepare reactivity, luma instability, accumulate; RCAS at sharpness 0.8 in
the tonemap). Configured as AMD's HDR, inverted and infinite depth,
low-resolution motion vector permutation, with the reference Lanczos for the
history and the approximate one for new samples. The plumbing differs where
WGSL does; `fsr3.wgsl`'s header lists every difference (no SPD: plain 2x2
means for the mips the upscaler reads; atomics and the lock mask in storage
buffers; wider formats where WebGPU cannot write AMD's).

Inputs the game provides (`post.wgsl` `fs_motion`, `render.rs`):

- motion vectors from depth, the camera and the aircraft's own motion, as the
  temporal AA reprojects (the aircraft flies with the camera);
- the reactive mask from what rain streaks and lightning mark in the colour's
  alpha (`precip.wgsl`);
- the exposure the tonemap uses (`G.time.z × AGX_EXPOSURE`), so AMD's
  auto-exposure is not needed.

Measured on an RTX 3060 at 1920×1080 (Linux, `FABLE_GPU_TIMES=1`, KSFO
cockpit): the anti-aliasing/upscaling stage costs 0.3 ms with the game's TAA
at 100%, 1.8 ms with FSR 3.1 at 50% and 3.2 ms with FSR 3.1 at 100% (Native
AA); accumulate is most of it (1.7 ms, limited by its texture reads). FSR 1 at
50% costs 0.8 ms more than the plain stretch. The whole frame went from 48.8
fps native to 83.7 fps with FSR 3.1 at 50% and 97.4 fps with FSR 1 at 50%.

Known limitations:

- Cars, live traffic, other aircraft and birds move through the world without
  motion vectors of their own (the game's temporal AA has the same gap), so
  they can smear behind themselves under FSR 3.1, more so at low resolutions.
- There is no transparency and composition mask (AMD's default, zero).
- Not compared pixel for pixel with AMD's own build: AMD ships its FSR 3.1
  for Windows only, inside the same DLL as FSR 4.

## FSR 4.1 (`fsr4.rs`)

AMD ships FSR 4 only as signed Windows DLLs (`amd_fidelityfx_loader_vk.dll`,
`amd_fidelityfx_upscaler_vk.dll`) in its FSR SDK, so the game does not ship
it. The **Download from AMD** button fetches those two files and AMD's
`license.md` (34.6 MB in all) from the FSR SDK v2.4.0 tag on GitHub, checks
each against a pinned size and SHA-256 (taken from AMD's files, whose GitHub
blob ids for the tag were checked), and moves them into
`<data dir>/fablegame/amd-fsr-sdk-2.4.0/` (the data folder keeps its historical
name so existing downloads are not orphaned) with a NOTICE; **Remove** deletes
the folder. The hashes are checked again before the DLLs are loaded.

AMD's terms (`license.md`): binary redistribution with the notice; no reverse
engineering, decompilation or disassembly. The file also lists the signed
DLLs among the files under MIT, which reads as a contradiction; downloading
them from AMD at the player's request sidesteps the question.

Requirements (AMD's SDK 2.4.0 release notes and docs): Windows; Vulkan
(`ffx_api_vk`); AMD Software: Adrenalin Edition 26.10.1 or later; a Radeon
RX 9000 (FP8 model), RX 7000 or RX 6000 (INT8), a Ryzen AI Max 300/400
(INT8), or a Ryzen AI 9 300/400 or Ryzen Z2 (lightweight INT8). AMD's timings
in performance mode: 1.3 ms at 4K on an RX 9070 XT, 2.0 ms at 1440p on an RX
7800 XT, 2.2 ms at 1080p on an RX 6600.

How it runs:

- With the runtime downloaded, on Windows, with Vulkan and a GPU AMD lists,
  the device is opened through wgpu-hal with the extensions AMD's own samples
  enable for FSR 4 where the GPU has them (`VK_KHR_cooperative_matrix`,
  `VK_EXT_shader_float8`, float16/int8, 8/16-bit storage, subgroup size
  control, integer dot product). Downloading in a session needs a restart.
- Each frame the game writes the exposure, runs a one-thread dispatch that
  uses every image the way AMD's runtime is told they are (so wgpu leaves them
  in those layouts), submits, and has the runtime record `ffxDispatch` into a
  command buffer of its own (wgpu does not allow raw and wgpu commands in one
  encoder), with the depth moved between wgpu's depth read-only layout and
  AMD's shader-read layout around it. The tonemap sharpens the result with
  RCAS like FSR 3.1's.
- AMD's runtime picks the provider (shown on the card): FSR 4.1.2 where it
  runs, AMD's FSR 3.1.6 otherwise. If the runtime cannot load or fails, the
  game's FSR 3.1 port takes over and the card says why.

Status: the download, verification, settings and the device setup
(`FABLE_FSR4_DEVICE=1` forces it on any Vulkan GPU) were run on Linux with an
NVIDIA GPU, where the extensions above (but not float8) were added and the
game ran normally. **The DLL calls themselves have not been run**: that needs
Windows and a supported AMD GPU. They are type-checked against AMD's headers
(SDK 2.4.0) on Linux, and fall back to FSR 3.1 on any error.

## AMD's licence for the ported shaders

`shaders/fsr3.wgsl` and the FSR 1 and RCAS functions in `shaders/post.wgsl`
are derived from AMD's FSR SDK v2.4.0, under this licence:

> Copyright (C) Advanced Micro Devices, Inc. All rights reserved.
>
> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to
> deal in the Software without restriction, including without limitation the
> rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
> sell copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in
> all copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
> FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
> DEALINGS IN THE SOFTWARE.

Sources: AMD FSR SDK v2.4.0 release
(https://github.com/GPUOpen-LibrariesAndSDKs/FidelityFX-SDK/releases/tag/v2.4.0),
`Kits/FidelityFX/docs/license.md`, `docs/techniques/super-resolution-ml.md`,
`super-resolution-upscaler.md`, `getting-started/ffx-api.md`.
