use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::io::Write;
use std::ops::{Add, Sub, Mul, Div, AddAssign, SubAssign};

/// Struct to represent Roblox Models
#[derive(Debug)]
pub struct Model<'a> {
    pub name: &'a str,
    pub referent: &'a str,
    pub models: Vec<Model<'a>>,
    pub parts: Vec<Part<'a>>,
}

/// Struct to iterate (recursively) through all the parts (and child-models) of a Model
pub struct ModelIter<'a> {
    model: &'a Model<'a>,
    index: usize,
    part_iter: std::slice::Iter<'a, Part<'a>>,
    model_iter: Option<Box<ModelIter<'a>>>,
}

impl<'a> Iterator for ModelIter<'a> {
    type Item = (&'a Part<'a>, &'a Model<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        let part_next = self.part_iter.next();
        if part_next.is_some() {
            return part_next.map(|part| (part, self.model));
        }

        loop {
            let model_iter = if let Some(model_iter) = &mut self.model_iter {
                model_iter
            } else {
                if let Some(model) = self.model.models.get(self.index) {
                    self.index += 1;
                    self.model_iter = Some(Box::new(model.into_iter()));
                    self.model_iter.as_mut().unwrap()
                } else {
                    return None;
                }
            };
            let model_next = model_iter.next();
            if model_next.is_some() {
                return model_next;
            } else {
                self.model_iter.take();
            }
        }
    }
}

impl<'a> IntoIterator for &'a Model<'a> {
    type Item = (&'a Part<'a>, &'a Model<'a>);
    type IntoIter = ModelIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        ModelIter {
            model: self,
            index: 0,
            part_iter: self.parts.iter(),
            model_iter: None,
        }
    }
}

/// Struct to represent Roblox parts
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Part<'a> {
    pub part_type: PartType,
    pub shape: PartShape,
    pub is_detail: bool,
    pub referent: &'a str,
    pub size: Vector3,
    pub cframe: CFrame,
    pub color: Color3,
    pub transparency: f64,
    pub reflectance: f64,
    pub material: Material,
    pub decals: [Option<Material>; 6],   // 0 = Front =-Z, 1 = Back = +Z, 2 = Top = +Y, 3 Bottom = -Y, 4 Right = +X, 5 = Left = -X
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct PartVisualHash {
    pub is_detail: bool,
    pub color: Color3,
    pub transparency: u64,
    pub reflectance: u64,
    pub material: MaterialHash,
    pub decals: [Option<MaterialHash>; 6],
}

impl<'a> Part<'a> {
    /// Returns the bounding-box vertices for this part
    pub fn vertices(self) -> [Vector3; 8] {
        [
            Vector3 { x: (self.size.x / 2.0), y: (-self.size.y / 2.0), z: (-self.size.z / 2.0) } * self.cframe,   // 0
            Vector3 { x: (self.size.x / 2.0), y: (-self.size.y / 2.0), z: (self.size.z / 2.0) } * self.cframe,    // 1
            Vector3 { x: (-self.size.x / 2.0), y: (-self.size.y / 2.0), z: (self.size.z / 2.0) } * self.cframe,   // 2
            Vector3 { x: (-self.size.x / 2.0), y: (-self.size.y / 2.0), z: (-self.size.z / 2.0) } * self.cframe,  // 3
            Vector3 { x: (self.size.x / 2.0), y: (self.size.y / 2.0), z: (-self.size.z / 2.0) } * self.cframe,    // 4
            Vector3 { x: (self.size.x / 2.0), y: (self.size.y / 2.0), z: (self.size.z / 2.0) } * self.cframe,     // 5
            Vector3 { x: (-self.size.x / 2.0), y: (self.size.y / 2.0), z: (self.size.z / 2.0) } * self.cframe,    // 6
            Vector3 { x: (-self.size.x / 2.0), y: (self.size.y / 2.0), z: (-self.size.z / 2.0) } * self.cframe,   // 7
        ]
    }

    pub fn sides(self) -> [[Vector3; 4]; 6] {
        let vertices = self.vertices();
        [
            [vertices[5], vertices[7], vertices[4], vertices[6]],   // +Y
            [vertices[0], vertices[2], vertices[1], vertices[3]],   // -Y
            [vertices[2], vertices[7], vertices[6], vertices[3]],   // -X
            [vertices[5], vertices[0], vertices[1], vertices[4]],   // +X
            [vertices[3], vertices[4], vertices[7], vertices[0]],   // -Z
            [vertices[6], vertices[1], vertices[2], vertices[5]]    // +Z
        ]
    }


    fn visual_hash(&self) -> Option<PartVisualHash> {
        if self.part_type == PartType::Part && self.shape == PartShape::Block {
            let decal_hashes: Option<[Option<MaterialHash>; 6]> = try {
                [
                    if let Some(decal) = self.decals[0] { Some(decal.material_hash()?) } else { None },
                    if let Some(decal) = self.decals[1] { Some(decal.material_hash()?) } else { None },
                    if let Some(decal) = self.decals[2] { Some(decal.material_hash()?) } else { None },
                    if let Some(decal) = self.decals[3] { Some(decal.material_hash()?) } else { None },
                    if let Some(decal) = self.decals[4] { Some(decal.material_hash()?) } else { None },
                    if let Some(decal) = self.decals[5] { Some(decal.material_hash()?) } else { None },
                ]
            };
            if let (Some(material), Some(decals)) = (self.material.material_hash(), decal_hashes) {
                Some(PartVisualHash {
                    is_detail: self.is_detail,
                    color: self.color,
                    transparency: self.transparency.to_bits(),
                    reflectance: self.reflectance.to_bits(),
                    material,
                    decals,
                })
            } else {
                None
            }
        } else {
            None
        }
    }

    // TODO: Performance can be increased through splitting the parts list into "chunks" of 3D space as to further cull comparisons
    pub fn join_adjacent(parts: Vec<Part<'a>>, print_progress: bool) -> Vec<Part<'a>> {
        let mut map = HashMap::new();
        let mut unique_parts = Vec::new();
        for part in parts.into_iter() {
            if let Some(hash) = part.visual_hash() {
                map.entry(hash)
                    .or_insert_with(Vec::new)
                    .push(part)
            } else {
                unique_parts.push(part);
            }
        }

        let map_len = map.len();
        for (index, parts) in map.values_mut().enumerate() {
            if print_progress {
                print!("\n\t{}/{}\t[                                                  ]", index + 1, map_len);
                std::io::stdout().flush().unwrap_or_default();
            }
            let mut reset_point = 0;
            'join_loop: loop {
                if print_progress {
                    print!("\r\t{}/{}\t[", index + 1, map_len);
                    let progress = (reset_point * 50) / parts.len();
                    for _ in 0..progress {
                        print!("-");
                    }
                    for _ in progress..50 {
                        print!(" ");
                    }
                    print!("]");
                    std::io::stdout().flush().unwrap_or_default();
                }

                for i in 0..parts.len() {
                    for j in (i + 1)..parts.len() {
                        if i != j {
                            // This is safe as i and j are not equal and thus cannot point to the same element
                            let (part_1, part_2) = {
                                if i > j {
                                    let (front, back) = parts.split_at_mut(i);
                                    (&mut back[0], &mut front[j])
                                } else {
                                    let (front, back) = parts.split_at_mut(j);
                                    (&mut front[i], &mut back[0])
                                }
                            };

                            for side_1 in part_1.sides() {
                                for mut side_2 in part_2.sides() {
                                    side_2.rotate_right(2);
                                    if side_1 == side_2 {
                                        let side_1_direction = (Vector3::centroid(side_1) / part_1.cframe).closest_axis();
                                        let side_2_direction = (Vector3::centroid(side_2) / part_2.cframe).closest_axis();

                                        let change_magnitude = (side_1_direction * part_1.size).magnitude();    // Magnitude implicitly performs `abs()`
                                        let size_change = side_2_direction.abs() * change_magnitude;

                                        part_2.size += size_change;

                                        let position_vector = Vector3::centroid(side_2) - part_2.cframe.position;
                                        part_2.cframe.position += (position_vector / position_vector.magnitude()) * (change_magnitude / 2.0);

                                        let last_index = parts.len() - 1;
                                        if i != last_index {
                                            parts.swap(i, last_index);
                                        }
                                        parts.truncate(last_index);

                                        reset_point = i;
                                        continue 'join_loop;
                                    }
                                }
                            }
                        }
                    }
                }

                if print_progress {
                    print!("\r\t{}/{}\t[--------------------------------------------------]", index + 1, map_len);
                    std::io::stdout().flush().unwrap_or_default();
                }
                break;
            }
        }

        if print_progress {
            println!();
            std::io::stdout().flush().unwrap_or_default();
        }

        map.into_values()
            .flat_map(|values| values.into_iter())
            .chain(unique_parts.into_iter())
            .collect()
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum PartType {
    Part,
    SpawnLocation,
    Truss,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum PartShape {
    Sphere,
    Block,
    Cylinder,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Material {
    Plastic,
    Wood,
    Slate,
    Concrete,
    CorrodedMetal,
    DiamondPlate,
    Foil,
    Grass,
    Ice,
    Marble,
    Granite,
    Brick,
    Pebble,
    Sand,
    Fabric,
    SmoothPlastic,
    Metal,
    WoodPlanks,
    Cobblestone,
    ForceField,
    Decal { id: u64, size_x: f64, size_y: f64 },
    Texture { id: u64, size_x: f64, size_y: f64, studs_per_u: f64, studs_per_v: f64, offset_u: f64, offset_v: f64 },
    Custom {
        texture: &'static str,
        fill: bool,
        generate: bool, // TODO: Fix this 'main' API leak
        size_x: f64,
        size_y: f64,
    },
}


#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum MaterialHash {
    Regular(u32),
    Custom {
        texture: &'static str,
        size_x: u64,
        size_y: u64,
    },
}

impl Material {
    pub fn from_id(id: u32) -> Option<Material> {
        use crate::rbx::Material::*;
        match id {
            256 => Some(Plastic),
            512 => Some(Wood),
            800 => Some(Slate),
            816 => Some(Concrete),
            1040 => Some(CorrodedMetal),
            1056 => Some(DiamondPlate),
            1072 => Some(Foil),
            1280 => Some(Grass),
            1536 => Some(Ice),
            784 => Some(Marble),
            832 => Some(Granite),
            848 => Some(Brick),
            864 => Some(Pebble),
            1296 => Some(Sand),
            1312 => Some(Fabric),
            272 => Some(SmoothPlastic),
            1088 => Some(Metal),
            528 => Some(WoodPlanks),
            880 => Some(Cobblestone),
            1584 => Some(ForceField),
            _ => None
        }
    }

    pub fn dimension_x(self) -> f64 {
        match self {
            Material::Plastic => 512.0,
            Material::Wood => 1024.0,
            Material::Slate => 1024.0,
            Material::Concrete => 1024.0,
            Material::CorrodedMetal => 1024.0,
            Material::DiamondPlate => 512.0,
            Material::Foil => 512.0,
            Material::Grass => 1024.0,
            Material::Ice => 1024.0,
            Material::Marble => 1024.0,
            Material::Granite => 1024.0,
            Material::Brick => 1024.0,
            Material::Pebble => 512.0,
            Material::Sand => 1024.0,
            Material::Fabric => 512.0,
            Material::SmoothPlastic => 512.0,
            Material::Metal => 512.0,
            Material::WoodPlanks => 1024.0,
            Material::Cobblestone => 1024.0,
            Material::ForceField => 1024.0,
            Material::Decal { size_x, .. } => size_x,
            Material::Texture { size_x, .. } => size_x,
            Material::Custom { size_x, .. } => size_x,
        }
    }

    pub fn dimension_y(self) -> f64 {
        match self {
            Material::Plastic => 512.0,
            Material::Wood => 1024.0,
            Material::Slate => 1024.0,
            Material::Concrete => 1024.0,
            Material::CorrodedMetal => 1024.0,
            Material::DiamondPlate => 512.0,
            Material::Foil => 512.0,
            Material::Grass => 1024.0,
            Material::Ice => 1024.0,
            Material::Marble => 1024.0,
            Material::Granite => 1024.0,
            Material::Brick => 1024.0,
            Material::Pebble => 512.0,
            Material::Sand => 1024.0,
            Material::Fabric => 512.0,
            Material::SmoothPlastic => 512.0,
            Material::Metal => 512.0,
            Material::WoodPlanks => 1024.0,
            Material::Cobblestone => 1024.0,
            Material::ForceField => 1024.0,
            Material::Decal { size_y, .. } => size_y,
            Material::Texture { size_y, .. } => size_y,
            Material::Custom { size_y, .. } => size_y
        }
    }

    pub fn material_hash(self) -> Option<MaterialHash> {
        match self {
            Material::Plastic => Some(MaterialHash::Regular(256)),
            Material::Wood => Some(MaterialHash::Regular(512)),
            Material::Slate => Some(MaterialHash::Regular(800)),
            Material::Concrete => Some(MaterialHash::Regular(816)),
            Material::CorrodedMetal => Some(MaterialHash::Regular(1040)),
            Material::DiamondPlate => Some(MaterialHash::Regular(1056)),
            Material::Foil => Some(MaterialHash::Regular(1072)),
            Material::Grass => Some(MaterialHash::Regular(1280)),
            Material::Ice => Some(MaterialHash::Regular(1536)),
            Material::Marble => Some(MaterialHash::Regular(784)),
            Material::Granite => Some(MaterialHash::Regular(832)),
            Material::Brick => Some(MaterialHash::Regular(848)),
            Material::Pebble => Some(MaterialHash::Regular(864)),
            Material::Sand => Some(MaterialHash::Regular(1296)),
            Material::Fabric => Some(MaterialHash::Regular(1312)),
            Material::SmoothPlastic => Some(MaterialHash::Regular(272)),
            Material::Metal => Some(MaterialHash::Regular(1088)),
            Material::WoodPlanks => Some(MaterialHash::Regular(528)),
            Material::Cobblestone => Some(MaterialHash::Regular(880)),
            Material::ForceField => Some(MaterialHash::Regular(1584)),
            Material::Decal { .. } => None,
            Material::Texture { .. } => None,
            Material::Custom { texture, fill, size_x, size_y, .. } => {
                if !fill {
                    Some(MaterialHash::Custom {
                        texture,
                        size_x: size_x.to_bits(),
                        size_y: size_y.to_bits(),
                    })
                } else {
                    None
                }
            }
        }
    }
}

impl Display for Material {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Material::Plastic => write!(f, "plastic"),
            Material::Wood => write!(f, "wood"),
            Material::Slate => write!(f, "slate"),
            Material::Concrete => write!(f, "concrete"),
            Material::CorrodedMetal => write!(f, "rust"),
            Material::DiamondPlate => write!(f, "diamondplate"),
            Material::Foil => write!(f, "aluminium"),
            Material::Grass => write!(f, "grass"),
            Material::Ice => write!(f, "ice"),
            Material::Marble => write!(f, "marble"),
            Material::Granite => write!(f, "granite"),
            Material::Brick => write!(f, "brick"),
            Material::Pebble => write!(f, "pebble"),
            Material::Sand => write!(f, "sand"),
            Material::Fabric => write!(f, "fabric"),
            Material::SmoothPlastic => write!(f, "smoothplastic"),
            Material::Metal => write!(f, "metal"),
            Material::WoodPlanks => write!(f, "woodplanks"),
            Material::Cobblestone => write!(f, "cobblestone"),
            Material::ForceField => write!(f, "forcefield"),
            Material::Custom { texture, .. } => write!(f, "{}", texture),
            Material::Decal { id, .. } => write!(f, "decal_{}", id),
            Material::Texture { id, .. } => write!(f, "texture_{}", id)
        }
    }
}


#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct Color3 {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Color3 {
    pub fn white() -> Color3 {
        Color3::from(u32::MAX)
    }
}

impl From<u32> for Color3 {
    fn from(int: u32) -> Self {
        let bytes = int.to_be_bytes();
        Color3 {
            red: bytes[1],
            green: bytes[2],
            blue: bytes[3],
        }
    }
}

/// 3D vector type with behavior matching Roblox
#[derive(Debug, Copy, Clone)]
pub struct Vector3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vector3 {
    #[inline(always)]
    pub fn from_array(array: [f64; 3]) -> Vector3 {
        Vector3 {
            x: array[0],
            y: array[1],
            z: array[2],
        }
    }

    pub fn array(&self) -> [f64; 3] {
        [self.x, self.y, self.z]
    }

    pub fn abs(self) -> Vector3 {
        Vector3 {
            x: self.x.abs(),
            y: self.y.abs(),
            z: self.z.abs(),
        }
    }

    pub fn magnitude(self) -> f64 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    pub fn closest_axis(self) -> Vector3 {
        if self.x.abs() >= self.y.abs() && self.x.abs() >= self.z.abs() {
            if self.x.is_sign_positive() {
                Vector3 { x: 1.0, y: 0.0, z: 0.0 }
            } else {
                Vector3 { x: -1.0, y: 0.0, z: 0.0 }
            }
        } else if self.y.abs() >= self.x.abs() && self.y.abs() >= self.z.abs() {
            if self.y.is_sign_positive() {
                Vector3 { x: 0.0, y: 1.0, z: 0.0 }
            } else {
                Vector3 { x: 0.0, y: -1.0, z: 0.0 }
            }
        } else {
            debug_assert!(self.z.abs() >= self.x.abs() && self.z.abs() >= self.y.abs());
            if self.z.is_sign_positive() {
                Vector3 { x: 0.0, y: 0.0, z: 1.0 }
            } else {
                Vector3 { x: 0.0, y: 0.0, z: -1.0 }
            }
        }
    }

    /// Returns the centroid of the given points
    pub fn centroid<const N: usize>(points: [Vector3; N]) -> Vector3 {
        let mut sum = Vector3 { x: 0.0, y: 0.0, z: 0.0 };
        for vertex in points {
            sum = sum + vertex;
        }
        sum / (N as f64)
    }
}

impl Add for Vector3 {
    type Output = Vector3;

    fn add(self, rhs: Self) -> Self::Output {
        Vector3 {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            z: self.z + rhs.z,
        }
    }
}

impl AddAssign for Vector3 {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs
    }
}

impl Sub for Vector3 {
    type Output = Vector3;

    fn sub(self, rhs: Self) -> Self::Output {
        Vector3 {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
            z: self.z - rhs.z,
        }
    }
}

impl SubAssign for Vector3 {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs
    }
}

impl Mul<f64> for Vector3 {
    type Output = Vector3;

    fn mul(self, rhs: f64) -> Self::Output {
        Vector3 {
            x: self.x * rhs,
            y: self.y * rhs,
            z: self.z * rhs,
        }
    }
}

impl Mul<Vector3> for f64 {
    type Output = Vector3;

    fn mul(self, rhs: Vector3) -> Self::Output {
        rhs * self
    }
}

impl Div<f64> for Vector3 {
    type Output = Vector3;

    fn div(self, rhs: f64) -> Self::Output {
        Vector3 {
            x: self.x / rhs,
            y: self.y / rhs,
            z: self.z / rhs,
        }
    }
}

impl Mul<Vector3> for Vector3 {
    type Output = Vector3;

    fn mul(self, rhs: Vector3) -> Self::Output {
        Vector3 {
            x: self.x * rhs.x,
            y: self.y * rhs.y,
            z: self.z * rhs.z,
        }
    }
}

impl Div<Vector3> for Vector3 {
    type Output = Vector3;

    fn div(self, rhs: Vector3) -> Self::Output {
        Vector3 {
            x: self.x / rhs.x,
            y: self.y / rhs.y,
            z: self.z / rhs.z,
        }
    }
}

/// Translates this Vector3 by the given CFrame
impl Mul<CFrame> for Vector3 {
    // Technically should be cf*v3 instead of the other way around to be mathematically correct
    type Output = Vector3;
    fn mul(self, mut cframe: CFrame) -> Self::Output {
        cframe = cframe.transpose();
        Vector3 {
            x: cframe.rot_matrix[0][0] * self.x + cframe.rot_matrix[0][1] * self.y + cframe.rot_matrix[0][2] * self.z,
            y: cframe.rot_matrix[1][0] * self.x + cframe.rot_matrix[1][1] * self.y + cframe.rot_matrix[1][2] * self.z,
            z: cframe.rot_matrix[2][0] * self.x + cframe.rot_matrix[2][1] * self.y + cframe.rot_matrix[2][2] * self.z,
        } + cframe.position
    }
}

/// Reverses a translation by a given CFrame
impl Div<CFrame> for Vector3 {
    type Output = Vector3;

    fn div(mut self, cframe: CFrame) -> Self::Output {
        self -= cframe.position;
        Vector3 {
            x: cframe.rot_matrix[0][0] * self.x + cframe.rot_matrix[0][1] * self.y + cframe.rot_matrix[0][2] * self.z,
            y: cframe.rot_matrix[1][0] * self.x + cframe.rot_matrix[1][1] * self.y + cframe.rot_matrix[1][2] * self.z,
            z: cframe.rot_matrix[2][0] * self.x + cframe.rot_matrix[2][1] * self.y + cframe.rot_matrix[2][2] * self.z,
        }
    }
}

impl PartialEq for Vector3 {
    fn eq(&self, other: &Self) -> bool {
        let eq = self.x.eq(&other.x)
            && self.z.eq(&other.y)
            && self.z.eq(&other.z);
        if !eq {
            const MARGIN: f64 = 1.0 / 10_000.0;   // Floating point equality isn't exact.

            (self.x - other.x).abs() <= MARGIN &&
                (self.y - other.y).abs() <= MARGIN &&
                (self.z - other.z).abs() <= MARGIN
        } else {
            true
        }
    }
}

/// Struct representing Roblox CFrames; Holds the position and rotation of a part
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct CFrame {
    pub position: Vector3,
    pub rot_matrix: [[f64; 3]; 3],
}

impl CFrame {
    pub fn right_vector(self) -> Vector3 {
        Vector3 {
            x: self.rot_matrix[0][0],
            y: self.rot_matrix[1][0],
            z: self.rot_matrix[2][0],
        }
    }
    pub fn up_vector(self) -> Vector3 {
        Vector3 {
            x: self.rot_matrix[0][1],
            y: self.rot_matrix[1][1],
            z: self.rot_matrix[2][1],
        }
    }
    pub fn back_vector(self) -> Vector3 {
        Vector3 {
            x: self.rot_matrix[0][2],
            y: self.rot_matrix[1][2],
            z: self.rot_matrix[2][2],
        }
    }

    pub fn transpose(self) -> CFrame {
        let m = self.rot_matrix;
        CFrame {
            position: self.position,
            rot_matrix: [
                [m[0][0], m[1][0], m[2][0]],
                [m[0][1], m[1][1], m[2][1]],
                [m[0][2], m[1][2], m[2][2]]
            ],
        }
    }

    pub fn rotate_x(self, radians: f64) -> CFrame {
        let a = [
            [1.0, 0.0, 0.0],
            [0.0, radians.cos(), -radians.sin()],
            [0.0, radians.sin(), radians.cos()],
        ];
        let b = self.rot_matrix;
        CFrame {
            position: self.position,
            rot_matrix: [
                [a[0][0] * b[0][0] + a[0][1] * b[1][0] + a[0][2] * b[2][0], a[0][0] * b[0][1] + a[0][1] * b[1][1] + a[0][2] * b[2][1], a[0][0] * b[0][2] + a[0][1] * b[1][2] + a[0][2] * b[2][2]],
                [a[1][0] * b[0][0] + a[1][1] * b[1][0] + a[1][2] * b[2][0], a[1][0] * b[0][1] + a[1][1] * b[1][1] + a[1][2] * b[2][1], a[1][0] * b[0][2] + a[1][1] * b[1][2] + a[1][2] * b[2][2]],
                [a[2][0] * b[0][0] + a[2][1] * b[1][0] + a[2][2] * b[2][0], a[2][0] * b[0][1] + a[2][1] * b[1][1] + a[2][2] * b[2][1], a[2][0] * b[0][2] + a[2][1] * b[1][2] + a[2][2] * b[2][2]],
            ],
        }
    }

    pub fn rotate_y(self, radians: f64) -> CFrame {
        let a = [
            [radians.cos(), 0.0, radians.sin()],
            [0.0, 1.0, 0.0],
            [-radians.sin(), 0.0, radians.cos()],
        ];
        let b = self.rot_matrix;
        CFrame {
            position: self.position,
            rot_matrix: [
                [a[0][0] * b[0][0] + a[0][1] * b[1][0] + a[0][2] * b[2][0], a[0][0] * b[0][1] + a[0][1] * b[1][1] + a[0][2] * b[2][1], a[0][0] * b[0][2] + a[0][1] * b[1][2] + a[0][2] * b[2][2]],
                [a[1][0] * b[0][0] + a[1][1] * b[1][0] + a[1][2] * b[2][0], a[1][0] * b[0][1] + a[1][1] * b[1][1] + a[1][2] * b[2][1], a[1][0] * b[0][2] + a[1][1] * b[1][2] + a[1][2] * b[2][2]],
                [a[2][0] * b[0][0] + a[2][1] * b[1][0] + a[2][2] * b[2][0], a[2][0] * b[0][1] + a[2][1] * b[1][1] + a[2][2] * b[2][1], a[2][0] * b[0][2] + a[2][1] * b[1][2] + a[2][2] * b[2][2]],
            ],
        }
    }

    pub fn rotate_z(self, radians: f64) -> CFrame {
        let a = [
            [radians.cos(), -radians.sin(), 0.0],
            [radians.sin(), radians.cos(), 0.0],
            [0.0, 0.0, 1.0],
        ];
        let b = self.rot_matrix;
        CFrame {
            position: self.position,
            rot_matrix: [
                [a[0][0] * b[0][0] + a[0][1] * b[1][0] + a[0][2] * b[2][0], a[0][0] * b[0][1] + a[0][1] * b[1][1] + a[0][2] * b[2][1], a[0][0] * b[0][2] + a[0][1] * b[1][2] + a[0][2] * b[2][2]],
                [a[1][0] * b[0][0] + a[1][1] * b[1][0] + a[1][2] * b[2][0], a[1][0] * b[0][1] + a[1][1] * b[1][1] + a[1][2] * b[2][1], a[1][0] * b[0][2] + a[1][1] * b[1][2] + a[1][2] * b[2][2]],
                [a[2][0] * b[0][0] + a[2][1] * b[1][0] + a[2][2] * b[2][0], a[2][0] * b[0][1] + a[2][1] * b[1][1] + a[2][2] * b[2][1], a[2][0] * b[0][2] + a[2][1] * b[1][2] + a[2][2] * b[2][2]],
            ],
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct BoundingBox {
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
    pub z_min: f64,
    pub z_max: f64,
}

impl BoundingBox {
    pub fn zeros() -> BoundingBox {
        BoundingBox {
            x_min: 0.0,
            x_max: 0.0,
            y_min: 0.0,
            y_max: 0.0,
            z_min: 0.0,
            z_max: 0.0,
        }
    }

    pub fn include(mut self, part: Part) -> BoundingBox {
        for point in part.vertices() {
            if point.x < self.x_min {
                self.x_min = point.x;
            }
            if point.x > self.x_max {
                self.x_max = point.x
            }
            if point.y < self.y_min {
                self.y_min = point.y;
            }
            if point.y > self.y_max {
                self.y_max = point.y
            }
            if point.z < self.z_min {
                self.z_min = point.z;
            }
            if point.z > self.z_max {
                self.z_max = point.z
            }
        }
        self
    }
}