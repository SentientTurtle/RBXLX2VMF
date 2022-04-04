#![allow(non_camel_case_types)]

use std::io::Write;

pub trait VMFTexture: PartialEq {
    fn name(&self) -> String;
    fn scale_x(&self, side: Side) -> f64;
    fn scale_z(&self, side: Side) -> f64;
    fn offset_x(&self, side: Side) -> f64;
    fn offset_y(&self, side: Side) -> f64;
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
}

impl<T: VMFTexture> IntoIterator for TextureMap<T> {
    type Item = T;
    type IntoIter = <Vec<T> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
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
    pub sides: Vec<Side>
}


/// Struct to represent source engine brush displacement
#[derive(Debug, Copy, Clone)]
pub struct Displacement {
    pub offsets: [[f64; 15]; 5],
    pub offset_normals: [[f64; 15]; 5],
    pub start_position: [f64; 3],
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
            TextureFace::X_POS => "0 1 0",
            TextureFace::X_NEG => "0 -1 0",
            TextureFace::Z_POS => "1 0 0",
            TextureFace::Z_NEG => "-1 0 0",
            TextureFace::Y_POS => "0 1 0",
            TextureFace::Y_NEG => "0 -1 0",
        }
    }

    pub fn v_axis(self) -> &'static str {
        match self {
            TextureFace::X_POS => "0 0 -1",
            TextureFace::X_NEG => "0 0 -1",
            TextureFace::Z_POS => "0 0 -1",
            TextureFace::Z_NEG => "0 0 -1",
            TextureFace::Y_POS => "1 0 0",
            TextureFace::Y_NEG => "1 0 0",
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Side {
    pub id: u32,
    pub texture: TextureID,
    pub texture_face: TextureFace,
    pub plane: [[f64; 3]; 3],
    pub displacement: Option<Displacement>
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
                        \t\t\t\"uaxis\" \"[{} {}] {}\"\n\
                        \t\t\t\"vaxis\" \"[{} {}] {}\"\n\
                        \t\t\t\"rotation\" \"0\"\n\
                        \t\t\t\"lightmapscale\" \"16\"\n\
                        \t\t\t\"smoothing_groups\" \"0\"\n",
                    side.id,
                    side.plane[0][0], side.plane[0][1], side.plane[0][2], side.plane[1][0], side.plane[1][1], side.plane[1][2], side.plane[2][0], side.plane[2][1], side.plane[2][2],
                    texture.name(),
                    side.texture_face.u_axis(), texture.offset_x(side), texture.scale_x(side),
                    side.texture_face.v_axis(), texture.offset_y(side), texture.scale_z(side)
                )?;
                if let Some(displacement) = side.displacement {
                    write!(
                        self.0,
                        r#"
                        dispinfo
                        {{
                            "power" "2"
                            "startposition" "[{} {} {}]"
                            "flags" "0"
                            "elevation" "0"
                            "subdiv" "1"
                            normals
                            {{
                                "row0" "0 0 0 0 0 0 0 0 0 0 0 0 0 0 0"
                                "row1" "0 0 0 0 0 0 0 0 0 0 0 0 0 0 0"
                                "row2" "0 0 0 0 0 0 0 0 0 0 0 0 0 0 0"
                                "row3" "0 0 0 0 0 0 0 0 0 0 0 0 0 0 0"
                                "row4" "0 0 0 0 0 0 0 0 0 0 0 0 0 0 0"
                            }}
                            distances
                            {{
                                "row0" "1e-05 1e-05 1e-05 1e-05 1e-05"
                                "row1" "1e-05 1e-05 1e-05 1e-05 1e-05"
                                "row2" "1e-05 1e-05 1e-05 1e-05 1e-05"
                                "row3" "1e-05 1e-05 1e-05 1e-05 1e-05"
                                "row4" "1e-05 1e-05 1e-05 1e-05 1e-05"
                            }}
                            offsets
                            {{
                                "row0" "{} {} {} {} {} {} {} {} {} {} {} {} {} {} {}"
                                "row1" "{} {} {} {} {} {} {} {} {} {} {} {} {} {} {}"
                                "row2" "{} {} {} {} {} {} {} {} {} {} {} {} {} {} {}"
                                "row3" "{} {} {} {} {} {} {} {} {} {} {} {} {} {} {}"
                                "row4" "{} {} {} {} {} {} {} {} {} {} {} {} {} {} {}"
                            }}
                            offset_normals
                            {{
                                "row0" "{} {} {} {} {} {} {} {} {} {} {} {} {} {} {}"
                                "row1" "{} {} {} {} {} {} {} {} {} {} {} {} {} {} {}"
                                "row2" "{} {} {} {} {} {} {} {} {} {} {} {} {} {} {}"
                                "row3" "{} {} {} {} {} {} {} {} {} {} {} {} {} {} {}"
                                "row4" "{} {} {} {} {} {} {} {} {} {} {} {} {} {} {}"
                            }}
                            alphas
                            {{
                                "row0" "0 0 0 0 0"
                                "row1" "0 0 0 0 0"
                                "row2" "0 0 0 0 0"
                                "row3" "0 0 0 0 0"
                                "row4" "0 0 0 0 0"
                            }}
                            triangle_tags
                            {{
                                "row0" "0 0 0 0 0 0 0 0"
                                "row1" "0 0 0 0 0 0 0 0"
                                "row2" "0 0 0 0 0 0 0 0"
                                "row3" "0 0 0 0 0 0 0 0"
                            }}
                            allowed_verts
                            {{
                                "10" "-1 -1 -1 -1 -1 -1 -1 -1 -1 -1"
                            }}
                        }}
                        "#,
                        displacement.start_position[0],
                        displacement.start_position[1],
                        displacement.start_position[2],
                        displacement.offsets[0][0], displacement.offsets[0][1], displacement.offsets[0][2], displacement.offsets[0][3], displacement.offsets[0][4], displacement.offsets[0][5], displacement.offsets[0][6], displacement.offsets[0][7], displacement.offsets[0][8], displacement.offsets[0][9], displacement.offsets[0][10], displacement.offsets[0][11], displacement.offsets[0][12], displacement.offsets[0][13], displacement.offsets[0][14],
                        displacement.offsets[1][0], displacement.offsets[1][1], displacement.offsets[1][2], displacement.offsets[1][3], displacement.offsets[1][4], displacement.offsets[1][5], displacement.offsets[1][6], displacement.offsets[1][7], displacement.offsets[1][8], displacement.offsets[1][9], displacement.offsets[1][10], displacement.offsets[1][11], displacement.offsets[1][12], displacement.offsets[1][13], displacement.offsets[1][14],
                        displacement.offsets[2][0], displacement.offsets[2][1], displacement.offsets[2][2], displacement.offsets[2][3], displacement.offsets[2][4], displacement.offsets[2][5], displacement.offsets[2][6], displacement.offsets[2][7], displacement.offsets[2][8], displacement.offsets[2][9], displacement.offsets[2][10], displacement.offsets[2][11], displacement.offsets[2][12], displacement.offsets[2][13], displacement.offsets[2][14],
                        displacement.offsets[3][0], displacement.offsets[3][1], displacement.offsets[3][2], displacement.offsets[3][3], displacement.offsets[3][4], displacement.offsets[3][5], displacement.offsets[3][6], displacement.offsets[3][7], displacement.offsets[3][8], displacement.offsets[3][9], displacement.offsets[3][10], displacement.offsets[3][11], displacement.offsets[3][12], displacement.offsets[3][13], displacement.offsets[3][14],
                        displacement.offsets[4][0], displacement.offsets[4][1], displacement.offsets[4][2], displacement.offsets[4][3], displacement.offsets[4][4], displacement.offsets[4][5], displacement.offsets[4][6], displacement.offsets[4][7], displacement.offsets[4][8], displacement.offsets[4][9], displacement.offsets[4][10], displacement.offsets[4][11], displacement.offsets[4][12], displacement.offsets[4][13], displacement.offsets[4][14],
                        displacement.offset_normals[0][0], displacement.offset_normals[0][1], displacement.offset_normals[0][2], displacement.offset_normals[0][3], displacement.offset_normals[0][4], displacement.offset_normals[0][5], displacement.offset_normals[0][6], displacement.offset_normals[0][7], displacement.offset_normals[0][8], displacement.offset_normals[0][9], displacement.offset_normals[0][10], displacement.offset_normals[0][11], displacement.offset_normals[0][12], displacement.offset_normals[0][13], displacement.offset_normals[0][14],
                        displacement.offset_normals[1][0], displacement.offset_normals[1][1], displacement.offset_normals[1][2], displacement.offset_normals[1][3], displacement.offset_normals[1][4], displacement.offset_normals[1][5], displacement.offset_normals[1][6], displacement.offset_normals[1][7], displacement.offset_normals[1][8], displacement.offset_normals[1][9], displacement.offset_normals[1][10], displacement.offset_normals[1][11], displacement.offset_normals[1][12], displacement.offset_normals[1][13], displacement.offset_normals[1][14],
                        displacement.offset_normals[2][0], displacement.offset_normals[2][1], displacement.offset_normals[2][2], displacement.offset_normals[2][3], displacement.offset_normals[2][4], displacement.offset_normals[2][5], displacement.offset_normals[2][6], displacement.offset_normals[2][7], displacement.offset_normals[2][8], displacement.offset_normals[2][9], displacement.offset_normals[2][10], displacement.offset_normals[2][11], displacement.offset_normals[2][12], displacement.offset_normals[2][13], displacement.offset_normals[2][14],
                        displacement.offset_normals[3][0], displacement.offset_normals[3][1], displacement.offset_normals[3][2], displacement.offset_normals[3][3], displacement.offset_normals[3][4], displacement.offset_normals[3][5], displacement.offset_normals[3][6], displacement.offset_normals[3][7], displacement.offset_normals[3][8], displacement.offset_normals[3][9], displacement.offset_normals[3][10], displacement.offset_normals[3][11], displacement.offset_normals[3][12], displacement.offset_normals[3][13], displacement.offset_normals[3][14],
                        displacement.offset_normals[4][0], displacement.offset_normals[4][1], displacement.offset_normals[4][2], displacement.offset_normals[4][3], displacement.offset_normals[4][4], displacement.offset_normals[4][5], displacement.offset_normals[4][6], displacement.offset_normals[4][7], displacement.offset_normals[4][8], displacement.offset_normals[4][9], displacement.offset_normals[4][10], displacement.offset_normals[4][11], displacement.offset_normals[4][12], displacement.offset_normals[4][13], displacement.offset_normals[4][14],
                    )?;
                }
                write!(self.0, "\t\t}}\n")?;
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
                        \t\t\t\"uaxis\" \"[{} {}] {}\"\n\
                        \t\t\t\"vaxis\" \"[{} {}] {}\"\n\
                        \t\t\t\"rotation\" \"0\"\n\
                        \t\t\t\"lightmapscale\" \"16\"\n\
                        \t\t\t\"smoothing_groups\" \"0\"\n\
                    \t\t}}\n",
                    side.id,
                    side.plane[0][0], side.plane[0][1], side.plane[0][2], side.plane[1][0], side.plane[1][1], side.plane[1][2], side.plane[2][0], side.plane[2][1], side.plane[2][2],
                    texture.name(),
                    side.texture_face.u_axis(), texture.offset_x(side), texture.scale_x(side),
                    side.texture_face.v_axis(), texture.offset_y(side), texture.scale_z(side)
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