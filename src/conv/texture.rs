extern crate reqwest;

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