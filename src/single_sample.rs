//! The g-buffer without MSAA (graphics settings, 1 sample): the shaders that
//! read its multisampled textures (composite.wgsl, resolve.wgsl) are given
//! plain ones instead. `textureLoad`'s last argument then picks a mip level
//! rather than a sample, and with one sample the loops only ask for 0.

/// `src` as it should be compiled for the g-buffer's sample count.
pub fn for_samples(src: &str, samples: u32) -> String {
    if samples > 1 {
        return src.to_string();
    }
    src.replace("texture_depth_multisampled_2d", "texture_depth_2d")
        .replace("texture_multisampled_2d<f32>", "texture_2d<f32>")
        .replace("textureNumSamples(t_aux_ms)", "1u")
        .replace("textureNumSamples(t_d)", "1u")
}
