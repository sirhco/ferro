//! Image transforms: resize, format, quality. Keyed cache lives upstream.

use image::{imageops::FilterType, DynamicImage, ImageFormat};
use serde::{Deserialize, Serialize};

use crate::error::{MediaError, MediaResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transform {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fit: Fit,
    pub format: OutFormat,
    pub quality: Option<u8>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    Cover,
    Contain,
    Fill,
    Inside,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutFormat {
    Webp,
    Jpeg,
    Png,
    Original,
}

impl OutFormat {
    fn to_image(self, orig: ImageFormat) -> ImageFormat {
        match self {
            Self::Webp => ImageFormat::WebP,
            Self::Jpeg => ImageFormat::Jpeg,
            Self::Png => ImageFormat::Png,
            Self::Original => orig,
        }
    }
}

pub fn apply(input: &[u8], t: &Transform) -> MediaResult<(Vec<u8>, ImageFormat)> {
    let format = image::guess_format(input).map_err(|e| MediaError::Backend(e.to_string()))?;
    let img = image::load_from_memory(input).map_err(|e| MediaError::Backend(e.to_string()))?;
    let resized = resize(&img, t);
    let out_fmt = t.format.to_image(format);
    let mut buf = Vec::new();
    resized
        .write_to(&mut std::io::Cursor::new(&mut buf), out_fmt)
        .map_err(|e| MediaError::Backend(e.to_string()))?;
    Ok((buf, out_fmt))
}

fn resize(img: &DynamicImage, t: &Transform) -> DynamicImage {
    match (t.width, t.height, t.fit) {
        (None, None, _) => img.clone(),
        (Some(w), Some(h), Fit::Fill) => img.resize_exact(w, h, FilterType::Lanczos3),
        (Some(w), Some(h), Fit::Cover) => img.resize_to_fill(w, h, FilterType::Lanczos3),
        (Some(w), Some(h), Fit::Contain | Fit::Inside) => img.resize(w, h, FilterType::Lanczos3),
        (Some(w), None, _) => img.resize(w, u32::MAX, FilterType::Lanczos3),
        (None, Some(h), _) => img.resize(u32::MAX, h, FilterType::Lanczos3),
    }
}
