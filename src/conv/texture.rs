extern crate reqwest;

use std::collections::HashMap;
use std::io::Read;
use flate2::read::GzDecoder;
use image::{DynamicImage, EncodableLayout, GenericImageView, ImageBuffer, ImageFormat, Rgba};
use image::imageops::FilterType;
use reqwest::Client;
use crate::rbx::{Color3, Vector3, Material};
use crate::vmf::{Side, TextureFace, VMFTexture};

#[derive(PartialEq, Copy, Clone)]
pub enum TextureScale {
    FILL,
    FIXED { scale_x: f64, scale_z: f64 },
}

#[derive(PartialEq, Copy, Clone)]
pub struct RobloxTexture {
    pub material: Material,
    pub color: Color3,
    pub transparency: u8,
    pub reflectance: u8,
    pub scale: TextureScale,
    pub no_offset: bool,
    pub dimension_x: u64,
    pub dimension_y: u64,
}

impl RobloxTexture {
    pub fn must_generate(&self) -> bool {
        match self.material {
            Material::Custom { generate, .. } => generate,
            _ => true
        }
    }
}

impl VMFTexture for RobloxTexture {
    fn name(&self) -> String {
        if let Material::Custom { texture, generate: false , ..} = self.material {
            format!("{}", texture)
        } else {
            format!("rbx/{}_{:x}-{:x}-{:x}-{:x}-{:x}", self.material, self.color.red, self.color.blue, self.color.green, self.transparency, self.reflectance)
        }
    }

    fn scale_x(&self, side: Side) -> f64 {
        match self.scale {
            TextureScale::FILL => (Vector3::from_array(side.plane[2]) - Vector3::from_array(side.plane[1])).magnitude() / (self.dimension_x as f64),
            TextureScale::FIXED { scale_x, .. } => scale_x
        }
    }

    fn scale_z(&self, side: Side) -> f64 {
        match self.scale {
            TextureScale::FILL => (Vector3::from_array(side.plane[2]) - Vector3::from_array(side.plane[0])).magnitude() / (self.dimension_y as f64),
            TextureScale::FIXED { scale_z, .. } => scale_z
        }
    }

    fn offset_x(&self, side: Side) -> f64 {
        if self.no_offset {
            0.0
        } else {
            let position = match side.texture_face {
                TextureFace::X_POS => -side.plane[2][1],
                TextureFace::X_NEG => side.plane[2][1],
                TextureFace::Z_POS => -side.plane[2][0],
                TextureFace::Z_NEG => side.plane[2][0],
                TextureFace::Y_POS => -side.plane[2][1],
                TextureFace::Y_NEG => side.plane[2][1]
            };
            (position / self.scale_x(side)) % (self.dimension_x as f64)
        }
    }

    fn offset_y(&self, side: Side) -> f64 {
        if self.no_offset {
            0.0
        } else {
            let position = match side.texture_face {
                TextureFace::X_POS => side.plane[2][2],
                TextureFace::X_NEG => side.plane[2][2],
                TextureFace::Z_POS => side.plane[2][2],
                TextureFace::Z_NEG => -side.plane[2][2],
                TextureFace::Y_POS => -side.plane[2][0],
                TextureFace::Y_NEG => -side.plane[2][0]
            };
            (position / self.scale_z(side)) % (self.dimension_y as f64)
        }
    }
}

pub async fn fetch_texture(http_client: &Client, id: u64, background: RobloxTexture, width: u32, height: u32) -> Result<DynamicImage, String> {
    let response = http_client.get(format!("https://assetdelivery.roblox.com/v1/assetId/{}", id))
        .send()
        .await
        .map_err(|err| format!("{}", err))?
        .json::<HashMap<String, serde_json::Value>>()
        .await
        .map_err(|err| format!("{}", err))?;
    let location = response.get("location")
        .and_then(|value| value.as_str())
        .ok_or("No location specified!".to_string())?;

    let bytes = http_client.get(location)
        .send()
        .await
        .map_err(|err| format!("{}", err))?
        .bytes()
        .await
        .map_err(|err| format!("{}", err))?;

    let mut buffer = Vec::with_capacity(bytes.len());   // reqwest supports automatic deflating, but that does not function reliably with the roblox api
    let bytes = match GzDecoder::new(bytes.as_bytes()).read_to_end(&mut buffer) {
        Ok(_) => &buffer[..],
        Err(_) => bytes.as_bytes(),
    };

    let image = image::load_from_memory_with_format(bytes.as_bytes(), ImageFormat::Png)
        .or_else(|_| image::load_from_memory_with_format(bytes.as_bytes(), ImageFormat::Jpeg))
        .map_err(|err| { format!("{}", err) })?;

// Resize image; Source engine only supports power-of-two sized images. Image is resized into a square, texture UVs scale it into the proper ratio in-engine
    let image = image.resize_exact(width, height, FilterType::Lanczos3);

    let mut buf = ImageBuffer::from_pixel(image.width(), image.height(), Rgba([background.color.red, background.color.green, background.color.blue, background.transparency]));

    image::imageops::overlay(&mut buf, &image, 0, 0);

    Ok(DynamicImage::ImageRgba8(buf))
}