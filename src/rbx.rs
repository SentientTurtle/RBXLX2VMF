use std::ops::{Add, Sub, Mul, Div};

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
                    self.model_iter.insert(Box::new(model.into_iter()));
                    self.model_iter.as_mut().unwrap()
                } else {
                    return None
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
#[derive(Debug, Copy, Clone)]
pub struct Part<'a> {
    pub part_type: PartType,
    pub is_detail: bool,
    pub referent: &'a str,
    pub size: Vector3,
    pub cframe: CFrame,
    pub color: Color3,
    pub material: Material,
    pub decals: Option<[Option<Material>; 6]>   // 0 = Front =-Z, 1 = Back = +Z, 2 = Top = +Y, 3 Bottom = -Y, 4 Right = +X, 5 = Left = -X
}
// const DECAL_FRONT: usize = 5;
// const DECAL_BACK: usize = 2;
// const DECAL_TOP: usize = 1;
// const DECAL_BOTTOM: usize = 4;
// const DECAL_RIGHT: usize = 0;
// const DECAL_LEFT: usize = 3;

impl<'a> Part<'a> {
    /// Returns the bounding-box vertices for this part
    pub fn boundaries(self) -> [Vector3; 8] {
        [
            Vector3 { x: (self.size.x/2.0), y: (-self.size.y/2.0), z: (-self.size.z/2.0) } * self.cframe,   // 0
            Vector3 { x: (self.size.x/2.0), y: (-self.size.y/2.0), z: (self.size.z/2.0) } * self.cframe,    // 1
            Vector3 { x: (-self.size.x/2.0), y: (-self.size.y/2.0), z: (self.size.z/2.0) } * self.cframe,   // 2
            Vector3 { x: (-self.size.x/2.0), y: (-self.size.y/2.0), z: (-self.size.z/2.0) } * self.cframe,  // 3
            Vector3 { x: (self.size.x/2.0), y: (self.size.y/2.0), z: (-self.size.z/2.0) } * self.cframe,    // 4
            Vector3 { x: (self.size.x/2.0), y: (self.size.y/2.0), z: (self.size.z/2.0) } * self.cframe,     // 5
            Vector3 { x: (-self.size.x/2.0), y: (self.size.y/2.0), z: (self.size.z/2.0) } * self.cframe,    // 6
            Vector3 { x: (-self.size.x/2.0), y: (self.size.y/2.0), z: (-self.size.z/2.0) } * self.cframe,   // 7
        ]
    }

    /// Returns the centroid point of this part
    pub fn centroid(self) -> Vector3 {
        Vector3::centroid(self.boundaries())
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum PartType {
    Part,
    SpawnLocation,
    Truss
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
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
    Custom {
        texture: &'static str,
        generate: bool  // TODO: Fix this 'main' API leak
    }
}

impl Material {
    pub fn texture(&self) -> &'static str {
        match self {
            Material::Plastic => "plastic",
            Material::Wood => "wood",
            Material::Slate => "slate",
            Material::Concrete => "concrete",
            Material::CorrodedMetal => "corrodedmetal",
            Material::DiamondPlate => "diamondplate",
            Material::Foil => "foil",
            Material::Grass => "grass",
            Material::Ice => "ice",
            Material::Marble => "marble",
            Material::Granite => "granite",
            Material::Brick => "brick",
            Material::Pebble => "pebble",
            Material::Sand => "sand",
            Material::Fabric => "fabric",
            Material::SmoothPlastic => "smoothplastic",
            Material::Metal => "metal",
            Material::WoodPlanks => "woodplanks",
            Material::Cobblestone => "cobblestone",
            Material::ForceField => "forcefield",
            Material::Custom { texture, .. } => texture
        }
    }
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
            _ => None
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Color3 {
    pub red: u8,
    pub green: u8,
    pub blue: u8
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
            blue: bytes[3]
        }
    }
}

/// 3D vector type with behavior matching Roblox
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Vector3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vector3 {
    pub fn array(&self) -> [f64; 3] {
        [self.x, self.y, self.z]
    }

    pub fn magnitude(self) -> f64 {
        (self.x*self.x+self.y*self.y+self.z*self.z).sqrt()
    }

    /// Returns the centroid of the given points
    pub fn centroid<const N: usize>(point: [Vector3; N]) -> Vector3 {
        let mut sum = Vector3 { x: 0.0, y: 0.0, z: 0.0 };
        for vertex in point {
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
            z: self.z + rhs.z
        }
    }
}

impl Sub for Vector3 {
    type Output = Vector3;

    fn sub(self, rhs: Self) -> Self::Output {
        Vector3 {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
            z: self.z - rhs.z
        }
    }
}

impl Mul<f64> for Vector3 {
    type Output = Vector3;

    fn mul(self, rhs: f64) -> Self::Output {
        Vector3 {
            x: self.x*rhs,
            y: self.y*rhs,
            z: self.z*rhs
        }
    }
}

impl Div<f64> for Vector3 {
    type Output = Vector3;

    fn div(self, rhs: f64) -> Self::Output {
        Vector3 {
            x: self.x/rhs,
            y: self.y/rhs,
            z: self.z/rhs
        }
    }
}

/// Translates this Vector3 by the given CFrame
impl Mul<CFrame> for Vector3 {  // Technically should be cf*v3 instead of the other way around to be mathematically correct
type Output = Vector3;
    fn mul(self, mut cframe: CFrame) -> Self::Output {
        cframe = cframe.transpose();
        Vector3 {
            x: cframe.rot_matrix[0][0]*self.x + cframe.rot_matrix[0][1]*self.y + cframe.rot_matrix[0][2]*self.z,
            y: cframe.rot_matrix[1][0]*self.x + cframe.rot_matrix[1][1]*self.y + cframe.rot_matrix[1][2]*self.z,
            z: cframe.rot_matrix[2][0]*self.x + cframe.rot_matrix[2][1]*self.y + cframe.rot_matrix[2][2]*self.z
        } + cframe.position
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
            z: self.rot_matrix[2][0]
        }
    }
    pub fn up_vector(self) -> Vector3 {
        Vector3 {
            x: self.rot_matrix[0][1],
            y: self.rot_matrix[1][1],
            z: self.rot_matrix[2][1]
        }
    }
    pub fn back_vector(self) -> Vector3 {
        Vector3 {
            x: self.rot_matrix[0][2],
            y: self.rot_matrix[1][2],
            z: self.rot_matrix[2][2]
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
            ]
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
            ]
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
            ]
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
            ]
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct BoundingBox {
    pub part_count: u32,
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
    pub z_min: f64,
    pub z_max: f64
}

impl BoundingBox {
    pub fn zeros() -> BoundingBox {
        BoundingBox {
            part_count: 0,
            x_min: 0.0,
            x_max: 0.0,
            y_min: 0.0,
            y_max: 0.0,
            z_min: 0.0,
            z_max: 0.0
        }
    }

    pub fn include(mut self, part: Part) -> BoundingBox {
        self.part_count+=1;
        for point in part.boundaries() {
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