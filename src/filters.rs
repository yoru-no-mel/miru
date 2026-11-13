use serde::{Deserialize, Serialize};

/// Image scaling filter, selectable from the right-click menu (and the
/// U / I / S keys). These control how the texture is sampled when the
/// image is scaled on screen:
/// - `NoFilter`: nearest-neighbor sampling, unsmoothed pixels.
/// - `Soften`: linear (bilinear) sampling, smooth magnification.
/// - `SoftenSharpen`: linear sampling plus mipmapped minification, so a
///   downscaled image stays crisp instead of averaging away detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FilterMode {
    #[default]
    NoFilter,
    Soften,
    SoftenSharpen,
}

impl FilterMode {
    /// Texture sampling options that realize the filter at draw time.
    pub fn texture_options(self) -> egui::TextureOptions {
        match self {
            Self::NoFilter => egui::TextureOptions::NEAREST,
            Self::Soften => egui::TextureOptions::LINEAR,
            Self::SoftenSharpen => {
                egui::TextureOptions::LINEAR.with_mipmap_mode(Some(egui::TextureFilter::Linear))
            }
        }
    }
}
