//! Reuse vision features only for pixel-identical adjacent video inputs.
use super::preprocess::{self, Patches};
use anyhow::Result;
use image::RgbImage;

#[derive(Default)]
pub(super) struct FrameCache {
    previous: Option<(RgbImage, Vec<f32>, usize)>,
}

impl FrameCache {
    pub fn encode(
        &mut self,
        pixels: RgbImage,
        encode: impl FnOnce(Patches) -> Result<Vec<f32>>,
    ) -> Result<(&[f32], usize)> {
        if !self
            .previous
            .as_ref()
            .is_some_and(|(last, _, _)| *last == pixels)
        {
            let patches = preprocess::patchify_resized(&pixels, 140)?;
            let count = patches.soft_tokens;
            let features = encode(patches)?;
            self.previous = Some((pixels, features, count));
        }
        let (_, features, count) = self.previous.as_ref().expect("encoded video input");
        Ok((features, *count))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/video_cache.rs"]
mod tests;
