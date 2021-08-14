#![allow(non_camel_case_types)]

use std::io::Write;
use std::fmt::Display;
use std::slice::Iter;

pub trait VMFTexture: Display + PartialEq {
    fn scale_x(&self) -> f64;
    fn scale_z(&self) -> f64;
}

pub struct TextureMap<T: VMFTexture> {
    inner: Vec<T>,
}

impl<T: VMFTexture> TextureMap<T> {
    pub fn new() -> TextureMap<T> {
        TextureMap {
            inner: Vec::new()
        }
    }

    pub fn iter(&self) -> Iter<'_, T> {
        self.inner.iter()
    }
}

impl<T: VMFTexture> TextureMap<T> {
    pub fn store(&mut self, texture: T) -> TextureID {
        if let Some(index) = self.inner.iter().position(|t| t == &texture) {
            TextureID {
                inner: index
            }
        } else {
            self.inner.push(texture);
            TextureID {
                inner: self.inner.len() - 1
            }
        }
    }

    pub fn get_texture(&self, id: TextureID) -> Option<&T> {
        self.inner.get(id.inner)
    }
}

#[derive(Debug, Copy, Clone)]
pub struct TextureID {
    inner: usize,
}

/// Struct to represent source engine solids/brushes
#[derive(Debug, Clone)]
pub struct Solid {
    pub id: u32,
    pub sides: Vec<Side>,
}

/// Direction from which to apply texture
#[derive(Debug, Copy, Clone)]
pub enum TextureFace {
    X_POS,
    X_NEG,
    Z_POS,
    Z_NEG,
    Y_POS,
    Y_NEG,
}

impl TextureFace {
    pub fn u_axis(self) -> &'static str {
        match self {
            TextureFace::X_POS => "[0 1 0 0]",
            TextureFace::X_NEG => "[0 -1 0 0]",
            TextureFace::Z_POS => "[1 0 0 0]",
            TextureFace::Z_NEG => "[-1 0 0 0]",
            TextureFace::Y_POS => "[0 1 0 0]",
            TextureFace::Y_NEG => "[0 -1 0 0]",
        }
    }

    pub fn v_axis(self) -> &'static str {
        match self {
            TextureFace::X_POS => "[0 0 -1 0]",
            TextureFace::X_NEG => "[0 0 -1 0]",
            TextureFace::Z_POS => "[0 0 -1 0]",
            TextureFace::Z_NEG => "[0 0 -1 0]",
            TextureFace::Y_POS => "[1 0 0 0]",
            TextureFace::Y_NEG => "[1 0 0 0]",
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Side {
    pub id: u32,
    pub texture: TextureID,
    pub texture_face: TextureFace,
    pub plane: [[f64; 3]; 3],
}


pub struct VMFBuilder<T: Write>(pub T);

impl<T: Write> VMFBuilder<T> {
    pub fn flush(mut self) -> std::io::Result<Self> {
        self.0.flush()?;
        Ok(self)
    }

    pub fn version_info(mut self, editor_version: u32, editor_build: u32, map_version: u32, prefab: bool) -> std::io::Result<Self> {
        write!(
            self.0,
            "versioninfo\n\
            {{\n\
                \t\"editorversion\" \"{}\"\n\
                \t\"editorbuild\" \"{}\"\n\
                \t\"mapversion\" \"{}\"\n\
                \t\"formatversion\" \"100\"\n\
                \t\"prefab\" \"{}\"\n\
            }}\n",
            editor_version,
            editor_build,
            map_version,
            if prefab { 1 } else { 0 }
        )?;
        Ok(self)
    }

    pub fn visgroups(mut self) -> std::io::Result<Self> {
        write!(self.0, "visgroups{{}}\n")?;
        Ok(self)
    }

    pub fn viewsettings(mut self) -> std::io::Result<Self> {
        write!(self.0, "viewsettings{{}}\n")?;
        Ok(self)
    }

    pub fn world<'a, I: IntoIterator<Item=Solid>, Texture: VMFTexture>(mut self, map_version: u32, skyname: &str, solids: I, texture_map: &TextureMap<Texture>) -> std::io::Result<Self> {
        write!(
            self.0,
            "world\n\
            {{\n\
                \t\"id\" \"1\"\n\
                \t\"mapversion\" \"{}\"\n\
                \t\"classname\" \"worldspawn\"\n\
                \t\"skyname\" \"{}\"\n",
            map_version,
            skyname
        )?;

        for solid in solids.into_iter() {
            let solid: Solid = solid;   // Type hint for IDE
            write!(
                self.0,
                "\tsolid\n\
                \t{{\n\
                    \t\t\"id\" \"{}\"\n",
                solid.id,
            )?;
            for side in solid.sides {
                let texture = texture_map.get_texture(side.texture).unwrap();
                write!(
                    self.0,
                    "\t\tside\n\
                    \t\t{{\n\
                        \t\t\t\"id\" \"{}\"\n\
                        \t\t\t\"plane\" \"({} {} {}) ({} {} {}) ({} {} {})\"\n\
                        \t\t\t\"material\" \"{}\"\n\
                        \t\t\t\"uaxis\" \"{} {}\"\n\
                        \t\t\t\"vaxis\" \"{} {}\"\n\
                        \t\t\t\"rotation\" \"0\"\n\
                        \t\t\t\"lightmapscale\" \"16\"\n\
                        \t\t\t\"smoothing_groups\" \"0\"\n\
                    \t\t}}\n",
                    side.id,
                    side.plane[0][0], side.plane[0][1], side.plane[0][2], side.plane[1][0], side.plane[1][1], side.plane[1][2], side.plane[2][0], side.plane[2][1], side.plane[2][2],
                    texture,
                    side.texture_face.u_axis(), texture.scale_x(),
                    side.texture_face.v_axis(), texture.scale_z()
                )?;
            }

            write!(
                self.0,
                "\t}}\n"
            )?;
        }

        write!(self.0, "}}\n")?;
        Ok(self)
    }

    pub fn detail<'a, I: IntoIterator<Item=(u32, Solid)>, Texture: VMFTexture>(mut self, details: I, texture_map: &TextureMap<Texture>) -> std::io::Result<Self> {  // TODO: Upgrade to support other entities
        for (entity_id, detail_brush) in details {
            write!(
                self.0,
                "entity\n\
            {{\n\
                \t\"id\" \"{}\"\n\
                \t\"classname\" \"func_detail\"\n",
                entity_id
            )?;
            write!(
                self.0,
                "\tsolid\n\
                \t{{\n\
                    \t\t\"id\" \"{}\"\n",
                detail_brush.id,
            )?;
            for side in detail_brush.sides {
                let texture = texture_map.get_texture(side.texture).unwrap();
                write!(
                    self.0,
                    "\t\tside\n\
                    \t\t{{\n\
                        \t\t\t\"id\" \"{}\"\n\
                        \t\t\t\"plane\" \"({} {} {}) ({} {} {}) ({} {} {})\"\n\
                        \t\t\t\"material\" \"{}\"\n\
                        \t\t\t\"uaxis\" \"{} {}\"\n\
                        \t\t\t\"vaxis\" \"{} {}\"\n\
                        \t\t\t\"rotation\" \"0\"\n\
                        \t\t\t\"lightmapscale\" \"16\"\n\
                        \t\t\t\"smoothing_groups\" \"0\"\n\
                    \t\t}}\n",
                    side.id,
                    side.plane[0][0], side.plane[0][1], side.plane[0][2], side.plane[1][0], side.plane[1][1], side.plane[1][2], side.plane[2][0], side.plane[2][1], side.plane[2][2],
                    texture,
                    side.texture_face.u_axis(), texture.scale_x(),
                    side.texture_face.v_axis(), texture.scale_z()
                )?;
            }

            write!(
                self.0,
                   "\t}}\n\
                    }}\n"
            )?;
        }
        Ok(self)
    }
}