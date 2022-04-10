use crate::{BoundingBox, CFrame, Color3, Displacement, Material, Part, PartShape, PartType, RobloxTexture, Side, Solid, TextureFace, TextureMap, TextureScale, Vector3};

pub mod parse;
pub mod texture;

/// Converts roblox coordinates to source engine coordinates
pub fn to_source_coordinates(vector: Vector3) -> [f64; 3] {
    [
        vector.x,
        -vector.z, // Negation corrects for mirroring in hammer/VMF
        vector.y
    ]
}

/// Decomposes a Roblox part into it's polyhedron faces, and returns them as source engine Sides
pub fn decompose_part(part: Part, id: &mut u32, map_scale: f64, texture_map: &mut TextureMap<RobloxTexture>) -> Vec<Side> {
    let vertices = part.vertices();

    const DECAL_FRONT: usize = 5;
    const DECAL_BACK: usize = 2;
    const DECAL_TOP: usize = 1;
    const DECAL_BOTTOM: usize = 4;
    const DECAL_RIGHT: usize = 0;
    const DECAL_LEFT: usize = 3;

    // First three boundaries of a plane form the defining points, in the order required by source engine
    let planes = [
        ([vertices[5], vertices[7], vertices[4], vertices[6]], DECAL_TOP),      // +Y
        ([vertices[0], vertices[2], vertices[1], vertices[3]], DECAL_BOTTOM),   // -Y
        ([vertices[2], vertices[7], vertices[6], vertices[3]], DECAL_RIGHT),    // -X
        ([vertices[5], vertices[0], vertices[1], vertices[4]], DECAL_LEFT),     // +X
        ([vertices[3], vertices[4], vertices[7], vertices[0]], DECAL_FRONT),    // -Z
        ([vertices[6], vertices[1], vertices[2], vertices[5]], DECAL_BACK)      // +Z
    ];

    let part_centroid = part.cframe.position;

    let sides = planes.into_iter().map(|(plane, decal_side)| {
        // Calculate normal vectors of the plane
        let vector_a = plane[0] - plane[1];
        let vector_b = plane[2] - plane[1];

        let plane_centroid = Vector3::centroid(plane);
        let centroid_vector = part_centroid - plane_centroid;

        let normal_a = Vector3 {
            x: vector_a.y * vector_b.z - vector_a.z * vector_b.y,
            y: vector_a.z * vector_b.x - vector_a.x * vector_b.z,
            z: vector_a.x * vector_b.y - vector_a.y * vector_b.x,
        };
        let normal_b = Vector3 {
            x: vector_b.y * vector_a.z - vector_b.z * vector_a.y,
            y: vector_b.z * vector_a.x - vector_b.x * vector_a.z,
            z: vector_b.x * vector_a.y - vector_b.y * vector_a.x,
        };

        // Determine which of the normal vectors points 'outward' from the shape
        let dot_a = centroid_vector.x * normal_a.x + centroid_vector.y * normal_a.y + centroid_vector.z * normal_a.z;
        let dot_b = centroid_vector.x * normal_b.x + centroid_vector.y * normal_b.y + centroid_vector.z * normal_b.z;

        let out_vector = if dot_a > dot_b {
            normal_b
        } else {
            normal_a
        };

        // Determine which cardinal direction the plane normal vector points; This will be the direction from which the texture is rendered in source engine.
        let texture_face = if out_vector.x.abs() >= out_vector.y.abs() && out_vector.x.abs() >= out_vector.z.abs() {
            if out_vector.x.is_sign_positive() {
                TextureFace::X_POS
            } else {
                TextureFace::X_NEG
            }
        } else if out_vector.y.abs() >= out_vector.x.abs() && out_vector.y.abs() >= out_vector.z.abs() {
            if out_vector.y.is_sign_positive() {
                TextureFace::Y_POS
            } else {
                TextureFace::Y_NEG
            }
        } else {
            debug_assert!(out_vector.z.abs() >= out_vector.x.abs() && out_vector.z.abs() >= out_vector.y.abs());
            if out_vector.z.is_sign_positive() {
                TextureFace::Z_POS
            } else {
                TextureFace::Z_NEG
            }
        };

        let texture =
            if let Some(side_decal) = part.decals[decal_side] {
                RobloxTexture {
                    material: side_decal,
                    color: part.color,
                    transparency: (255.0 * (1.0 - part.transparency)) as u8,
                    reflectance: (255.0 * part.reflectance) as u8,
                    scale: match side_decal {
                        Material::Decal { .. } | Material::Custom { fill: true, .. } => TextureScale::FILL,
                        Material::Texture { size_x, size_y, studs_per_u, studs_per_v, .. } => {
                            TextureScale::FIXED {
                                scale_x: map_scale * studs_per_u / size_x,
                                scale_z: map_scale * studs_per_v / size_y,
                            }
                        }
                        _ => TextureScale::FIXED { scale_x: map_scale / 32.0, scale_z: map_scale / 32.0 },
                    },
                    dimension_x: side_decal.dimension_x(),
                    dimension_y: side_decal.dimension_y(),
                }
            } else {
                RobloxTexture {
                    material: part.material,
                    color: part.color,
                    transparency: (255.0 * (1.0 - part.transparency)) as u8,
                    reflectance: (255.0 * part.reflectance) as u8,
                    scale: TextureScale::FIXED { scale_x: map_scale / 32.0, scale_z: map_scale / 32.0 },
                    dimension_x: part.material.dimension_x(),
                    dimension_y: part.material.dimension_y(),
                }
            };

        let displacement = match part.shape {
            PartShape::Sphere => {
                let (mut offsets, offset_normals) = match texture_face {
                    TextureFace::X_POS => {
                        let offsets = [
                            [-237.861, 237.861, 237.861, -201.755, 92.3116, 201.755, -183.246, 0.0, 183.246, -201.755, -92.3116, 201.755, -237.861, -237.861, 237.861],
                            [-201.755, 201.755, 92.3116, -125.21, 84.672, 84.6719, -100.481, 0.0, 77.4723, -125.21, -84.6719, 84.6719, -201.755, -201.755, 92.3116],
                            [-183.246, 183.246, 0.0, -100.481, 77.4723, 0.0, -69.1808, 0.0, 0.0, -100.481, -77.4723, 0.0, -183.246, -183.246, 0.0],
                            [-201.755, 201.755, -92.3116, -125.21, 84.672, -84.672, -100.481, 0.0, -77.4723, -125.21, -84.672, -84.672, -201.755, -201.755, -92.3116],
                            [-237.861, 237.861, -237.861, -201.755, 92.3116, -201.755, -183.246, 0.0, -183.246, -201.755, -92.3116, -201.755, -237.861, -237.861, -237.861]
                        ];
                        let offset_normals = [
                            [0.563819, -0.563819, -0.563819, 0.653738, -0.305397, -0.658052, 0.690534, 0.0, -0.690534, 0.655635, 0.303805, -0.656901, 0.563819, 0.563819, -0.563819],
                            [0.656901, -0.655635, -0.303805, 0.890386, -0.296795, -0.269814, 0.917459, -0.00062678, -0.334578, 0.883334, 0.294445, -0.294445, 0.649801, 0.662669, -0.303812],
                            [0.690534, -0.690534, 0.0, 0.918221, -0.332479, -0.00122529, 0.976562, 0.0, 0.0, 0.917459, 0.334578, 0.00062678, 0.690534, 0.690534, 0.0],
                            [0.658053, -0.653738, 0.305397, 0.901673, -0.265198, 0.265198, 0.918221, 0.00122529, 0.332479, 0.890386, 0.269814, 0.296795, 0.65954, 0.652252, 0.305366],
                            [0.563819, -0.563819, 0.563819, 0.662669, -0.303812, 0.649801, 0.690534, 0.0, 0.690534, 0.652252, 0.305367, 0.65954, 0.563819, 0.563819, 0.563819]
                        ];
                        (offsets, offset_normals)
                    }
                    TextureFace::X_NEG => {
                        let offsets = [
                            [237.861, 237.861, 237.861, 201.755, 201.755, 92.3116, 183.246, 183.246, 0.0, 201.755, 201.755, -92.3116, 237.861, 237.861, -237.861],
                            [201.755, 92.3116, 201.755, 125.211, 84.672, 84.6719, 100.481, 77.4723, 0.0, 125.211, 84.672, -84.672, 201.755, 92.3116, -201.755],
                            [183.246, 0.0, 183.246, 100.481, 0.0, 77.4723, 69.181, 0.0, 0.0, 100.481, 0.0, -77.4723, 183.246, 0.0, -183.246],
                            [201.755, -92.3116, 201.755, 125.211, -84.672, 84.6719, 100.481, -77.4723, 0.0, 125.211, -84.672, -84.672, 201.755, -92.3116, -201.755],
                            [237.861, -237.861, 237.861, 201.755, -201.755, 92.3116, 183.246, -183.246, 0.0, 201.755, -201.755, -92.3116, 237.861, -237.861, -237.861]
                        ];
                        let offset_normals = [
                            [-0.563819, -0.563819, -0.563819, -0.65954, -0.652252, -0.305366, -0.690534, -0.690534, 0.0, -0.649801, -0.662669, 0.303812, -0.563819, -0.563819, 0.563819],
                            [-0.652252, -0.305367, -0.65954, -0.890386, -0.269814, -0.296795, -0.917459, -0.334578, -0.00062678, -0.883334, -0.294445, 0.294445, -0.655635, -0.303805, 0.656901],
                            [-0.690534, 0.0, -0.690534, -0.918221, -0.0012253, -0.332479, -0.976562, 0.0, 0.0, -0.917459, 0.000626779, 0.334578, -0.690534, 0.0, 0.690534],
                            [-0.662669, 0.303812, -0.649801, -0.901673, 0.265198, -0.265198, -0.918221, 0.332479, 0.00122528, -0.890386, 0.296795, 0.269814, -0.653738, 0.305397, 0.658053],
                            [-0.563819, 0.563819, -0.563819, -0.658053, 0.653738, -0.305397, -0.690534, 0.690534, 0.0, -0.656901, 0.655635, 0.303805, -0.563819, 0.563819, 0.563819]
                        ];
                        (offsets, offset_normals)
                    }
                    TextureFace::Z_POS => {
                        let offsets = [
                            [237.861, 237.861, 237.861, 92.3116, 201.755, 201.755, 0.0, 183.246, 183.246, -92.3116, 201.755, 201.755, -237.861, 237.861, 237.861],
                            [201.755, 201.755, 92.3116, 84.6719, 125.21, 84.6719, 0.0, 100.481, 77.4723, -84.672, 125.21, 84.6719, -201.755, 201.755, 92.3116],
                            [183.246, 183.246, 0.0, 77.4723, 100.481, 0.0, 0.0, 69.1808, 0.0, -77.4723, 100.481, 0.0, -183.246, 183.246, 0.0],
                            [201.755, 201.755, -92.3116, 84.672, 125.21, -84.672, 0.0, 100.481, -77.4723, -84.672, 125.21, -84.672, -201.755, 201.755, -92.3116],
                            [237.861, 237.861, -237.861, 92.3116, 201.755, -201.755, 0.0, 183.246, -183.246, -92.3116, 201.755, -201.755, -237.861, 237.861, -237.861]
                        ];
                        let offset_normals = [
                            [-0.563819, -0.563819, -0.563819, -0.305367, -0.65954, -0.652252, 0.0, -0.690534, -0.690534, 0.303813, -0.649801, -0.662669, 0.563819, -0.563819, -0.563819],
                            [-0.65954, -0.652252, -0.305366, -0.296795, -0.890386, -0.269814, -0.00062678, -0.917459, -0.334578, 0.294445, -0.883334, -0.294445, 0.656901, -0.655635, -0.303805],
                            [-0.690534, -0.690534, 0.0, -0.332479, -0.918221, -0.0012253, 0.0, -0.976562, 0.0, 0.334578, -0.917459, 0.00062678, 0.690534, -0.690534, 0.0],
                            [-0.649801, -0.662669, 0.303812, -0.265198, -0.901673, 0.265198, 0.00122531, -0.918221, 0.332479, 0.269814, -0.890386, 0.296795, 0.658053, -0.653738, 0.305397],
                            [-0.563819, -0.563819, 0.563819, -0.305397, -0.658052, 0.653738, 0.0, -0.690534, 0.690534, 0.303805, -0.656901, 0.655635, 0.563819, -0.563819, 0.563819]
                        ];
                        (offsets, offset_normals)
                    }
                    TextureFace::Z_NEG => {
                        let offsets = [
                            [237.861, -237.861, 237.861, 201.755, -201.755, 92.3116, 183.246, -183.246, 0.0, 201.755, -201.755, -92.3116, 237.861, -237.861, -237.861],
                            [92.3116, -201.755, 201.755, 84.672, -125.211, 84.6719, 77.4723, -100.481, 0.0, 84.6719, -125.211, -84.672, 92.3116, -201.755, -201.755],
                            [0.0, -183.246, 183.246, 0.0, -100.481, 77.4723, 0.0, -69.181, 0.0, 0.0, -100.481, -77.4723, 0.0, -183.246, -183.246],
                            [-92.3116, -201.755, 201.755, -84.672, -125.211, 84.6719, -77.4723, -100.481, 0.0, -84.672, -125.211, -84.672, -92.3116, -201.755, -201.755],
                            [-237.861, -237.861, 237.861, -201.755, -201.755, 92.3116, -183.246, -183.246, 0.0, -201.755, -201.755, -92.3116, -237.861, -237.861, -237.861]
                        ];
                        let offset_normals = [
                            [-0.563819, 0.563819, -0.563819, -0.658053, 0.653738, -0.305397, -0.690534, 0.690534, 0.0, -0.656901, 0.655635, 0.303805, -0.563819, 0.563819, 0.563819],
                            [-0.303805, 0.656901, -0.655635, -0.269814, 0.890386, -0.296795, -0.334578, 0.917459, -0.00062678, -0.294445, 0.883334, 0.294445, -0.303812, 0.649801, 0.662669],
                            [0.0, 0.690534, -0.690534, -0.00122528, 0.918221, -0.332479, 0.0, 0.976562, 0.0, 0.00062678, 0.917459, 0.334578, 0.0, 0.690534, 0.690534],
                            [0.305397, 0.658053, -0.653738, 0.265198, 0.901673, -0.265198, 0.332479, 0.918221, 0.00122528, 0.296795, 0.890386, 0.269814, 0.305366, 0.65954, 0.652252],
                            [0.563819, 0.563819, -0.563819, 0.649801, 0.662669, -0.303812, 0.690534, 0.690534, 0.0, 0.65954, 0.652252, 0.305366, 0.563819, 0.563819, 0.563819]
                        ];
                        (offsets, offset_normals)
                    }
                    TextureFace::Y_NEG => {
                        let offsets = [
                            [237.861, 237.861, 237.861, 201.755, 92.3116, 201.755, 183.246, 0.0, 183.246, 201.755, -92.3116, 201.755, 237.861, -237.861, 237.861],
                            [92.3116, 201.755, 201.755, 84.672, 84.672, 125.21, 77.4723, 0.0, 100.481, 84.6719, -84.6719, 125.21, 92.3116, -201.755, 201.755],
                            [0.0, 183.246, 183.246, 0.0, 77.4723, 100.481, 0.0, 0.0, 69.1808, 0.0, -77.4723, 100.481, 0.0, -183.246, 183.246],
                            [-92.3116, 201.755, 201.755, -84.672, 84.672, 125.21, -77.4723, 0.0, 100.481, -84.672, -84.672, 125.21, -92.3116, -201.755, 201.755],
                            [-237.861, 237.861, 237.861, -201.755, 92.3116, 201.755, -183.246, 0.0, 183.246, -201.755, -92.3116, 201.755, -237.861, -237.861, 237.861],
                        ];
                        let offset_normals = [
                            [-0.563819, -0.563819, -0.563819, -0.652252, -0.305367, -0.65954, -0.690534, 0.0, -0.690534, -0.662669, 0.303812, -0.649801, -0.563819, 0.563819, -0.563819],
                            [-0.305367, -0.65954, -0.652252, -0.269814, -0.296795, -0.890386, -0.334578, -0.00062678, -0.917459, -0.294445, 0.294445, -0.883334, -0.303805, 0.656901, -0.655635],
                            [0.0, -0.690534, -0.690534, -0.00122527, -0.332479, -0.918221, 0.0, 0.0, -0.976562, 0.00062678, 0.334578, -0.917459, 0.0, 0.690534, -0.690534],
                            [0.303813, -0.649801, -0.662669, 0.265198, -0.265198, -0.901673, 0.332479, 0.00122528, -0.918221, 0.296795, 0.269814, -0.890386, 0.305397, 0.658053, -0.653738],
                            [0.563819, -0.563819, -0.563819, 0.653738, -0.305397, -0.658052, 0.690534, 0.0, -0.690534, 0.655635, 0.303805, -0.656901, 0.563819, 0.563819, -0.563819],
                        ];
                        (offsets, offset_normals)
                    }
                    TextureFace::Y_POS => {
                        let offsets = [
                            [237.861, 237.861, -237.861, 92.3116, 201.755, -201.755, 0.0, 183.246, -183.246, -92.3116, 201.755, -201.755, -237.861, 237.861, -237.861],
                            [201.755, 92.3116, -201.755, 84.6719, 84.672, -125.211, 0.0, 77.4723, -100.481, -84.672, 84.672, -125.21, -201.755, 92.3116, -201.755],
                            [183.246, 0.0, -183.246, 77.4723, 0.0, -100.481, 0.0, 0.0, -69.1809, -77.4723, 0.0, -100.481, -183.246, 0.0, -183.246],
                            [201.755, -92.3116, -201.755, 84.672, -84.672, -125.211, 0.0, -77.4723, -100.481, -84.672, -84.672, -125.21, -201.755, -92.3116, -201.755],
                            [237.861, -237.861, -237.861, 92.3116, -201.755, -201.755, 0.0, -183.246, -183.246, -92.3116, -201.755, -201.755, -237.861, -237.861, -237.861],
                        ];
                        let offset_normals = [
                            [-0.563819, -0.563819, 0.563819, -0.305397, -0.658052, 0.653738, 0.0, -0.690534, 0.690534, 0.303805, -0.656901, 0.655635, 0.563819, -0.563819, 0.563819],
                            [-0.655635, -0.303805, 0.656901, -0.296795, -0.269814, 0.890386, -0.00062678, -0.334578, 0.917459, 0.294445, -0.294445, 0.883334, 0.662669, -0.303812, 0.649801],
                            [-0.690534, 0.0, 0.690534, -0.332479, -0.00122531, 0.918221, 0.0, 0.0, 0.976562, 0.334578, 0.00062678, 0.917459, 0.690534, 0.0, 0.690534],
                            [-0.653738, 0.305397, 0.658053, -0.265198, 0.265198, 0.901673, 0.00122531, 0.332479, 0.918221, 0.269814, 0.296795, 0.890386, 0.652252, 0.305367, 0.65954],
                            [-0.563819, 0.563819, 0.563819, -0.303812, 0.649801, 0.662669, 0.0, 0.690534, 0.690534, 0.305366, 0.65954, 0.652252, 0.563819, 0.563819, 0.563819],
                        ];
                        (offsets, offset_normals)
                    }
                };
                let [size_x, size_y, size_z] = part.size.array();
                for row in &mut offsets {
                    row[0] *= size_x * map_scale / 1000.0;
                    row[1] *= size_y * map_scale / 1000.0;
                    row[2] *= size_z * map_scale / 1000.0;
                    row[3] *= size_x * map_scale / 1000.0;
                    row[4] *= size_y * map_scale / 1000.0;
                    row[5] *= size_z * map_scale / 1000.0;
                    row[6] *= size_x * map_scale / 1000.0;
                    row[7] *= size_y * map_scale / 1000.0;
                    row[8] *= size_z * map_scale / 1000.0;
                    row[9] *= size_x * map_scale / 1000.0;
                    row[10] *= size_y * map_scale / 1000.0;
                    row[11] *= size_z * map_scale / 1000.0;
                    row[12] *= size_x * map_scale / 1000.0;
                    row[13] *= size_y * map_scale / 1000.0;
                    row[14] *= size_z * map_scale / 1000.0;
                }

                Some(Displacement {
                    offsets,
                    offset_normals,
                    start_position: to_source_coordinates({
                        let mut x = f64::MAX;
                        let mut y = f64::MAX;
                        let mut z = f64::MIN;

                        for vector in plane {
                            x = x.min(vector.x);
                            y = y.min(vector.y);
                            z = z.max(vector.z);
                        }
                        Vector3 { x, y, z } * map_scale
                    }),
                })
            }
            PartShape::Block => None,
            PartShape::Cylinder => None,
        };

        let side = Side {
            id: *id,
            texture: texture_map.store(texture),
            texture_face,
            plane: [
                to_source_coordinates(plane[0] * map_scale),
                to_source_coordinates(plane[1] * map_scale),
                to_source_coordinates(plane[2] * map_scale)
            ],
            displacement,
        };
        *id += 1;
        side
    }).collect();
    *id += 6;
    sides
}



pub fn generate_skybox(part_id: &mut u32, side_id: &mut u32, bounding_box: BoundingBox, map_scale: f64, texture_map: &mut TextureMap<RobloxTexture>) -> [Solid; 6] {
    [
        Solid {
            id: {
                *part_id += 1;
                *part_id
            },
            sides: decompose_part(Part {
                part_type: PartType::Part,
                shape: PartShape::Block,
                is_detail: false,
                referent: "SKYBOX+X",
                size: Vector3 {
                    x: 1.0,
                    y: (bounding_box.y_max - bounding_box.y_min).abs(),
                    z: (bounding_box.z_max - bounding_box.z_min).abs(),
                },
                cframe: CFrame {
                    position: Vector3 {
                        x: (bounding_box.x_max + bounding_box.x_min) / 2.0 + (bounding_box.x_max - bounding_box.x_min).abs() / 2.0 + 0.5,
                        y: (bounding_box.y_max + bounding_box.y_min) / 2.0,
                        z: (bounding_box.z_max + bounding_box.z_min) / 2.0,
                    },
                    rot_matrix: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                },
                color: Color3::white(),
                transparency: 0.0,
                reflectance: 0.0,
                material: Material::Custom { texture: "tools/toolsskybox", fill: false, generate: false, size_x: 512.0, size_y: 512.0 },
                decals: [None, None, None, None, None, None],
            }, side_id, map_scale, texture_map),
        },
        Solid {
            id: {
                *part_id += 1;
                *part_id
            },
            sides: decompose_part(Part {
                part_type: PartType::Part,
                shape: PartShape::Block,
                is_detail: false,
                referent: "SKYBOX+Y",
                size: Vector3 {
                    x: (bounding_box.x_max - bounding_box.x_min).abs(),
                    y: 1.0,
                    z: (bounding_box.z_max - bounding_box.z_min).abs(),
                },
                cframe: CFrame {
                    position: Vector3 {
                        x: (bounding_box.x_max + bounding_box.x_min) / 2.0,
                        y: (bounding_box.y_max + bounding_box.y_min) / 2.0 + (bounding_box.y_max - bounding_box.y_min).abs() / 2.0 + 0.5,
                        z: (bounding_box.z_max + bounding_box.z_min) / 2.0,
                    },
                    rot_matrix: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                },
                color: Color3::white(),
                transparency: 0.0,
                reflectance: 0.0,
                material: Material::Custom { texture: "tools/toolsskybox", fill: false, generate: false, size_x: 512.0, size_y: 512.0 },
                decals: [None, None, None, None, None, None],
            }, side_id, map_scale, texture_map),
        },
        Solid {
            id: {
                *part_id += 1;
                *part_id
            },
            sides: decompose_part(Part {
                part_type: PartType::Part,
                shape: PartShape::Block,
                is_detail: false,
                referent: "SKYBOX+Z",
                size: Vector3 {
                    x: (bounding_box.x_max - bounding_box.x_min).abs(),
                    y: (bounding_box.y_max - bounding_box.y_min).abs(),
                    z: 1.0,
                },
                cframe: CFrame {
                    position: Vector3 {
                        x: (bounding_box.x_max + bounding_box.x_min) / 2.0,
                        y: (bounding_box.y_max + bounding_box.y_min) / 2.0,
                        z: (bounding_box.z_max + bounding_box.z_min) / 2.0 + (bounding_box.z_max - bounding_box.z_min).abs() / 2.0 + 0.5,
                    },
                    rot_matrix: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                },
                color: Color3::white(),
                transparency: 0.0,
                reflectance: 0.0,
                material: Material::Custom { texture: "tools/toolsskybox", fill: false, generate: false, size_x: 512.0, size_y: 512.0 },
                decals: [None, None, None, None, None, None],
            }, side_id, map_scale, texture_map),
        },
        Solid {
            id: {
                *part_id += 1;
                *part_id
            },
            sides: decompose_part(Part {
                part_type: PartType::Part,
                shape: PartShape::Block,
                is_detail: false,
                referent: "SKYBOX-X",
                size: Vector3 {
                    x: 1.0,
                    y: (bounding_box.y_max - bounding_box.y_min).abs(),
                    z: (bounding_box.z_max - bounding_box.z_min).abs(),
                },
                cframe: CFrame {
                    position: Vector3 {
                        x: (bounding_box.x_max + bounding_box.x_min) / 2.0 - (bounding_box.x_max - bounding_box.x_min).abs() / 2.0 - 0.5,
                        y: (bounding_box.y_max + bounding_box.y_min) / 2.0,
                        z: (bounding_box.z_max + bounding_box.z_min) / 2.0,
                    },
                    rot_matrix: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                },
                color: Color3::white(),
                transparency: 0.0,
                reflectance: 0.0,
                material: Material::Custom { texture: "tools/toolsskybox", fill: false, generate: false, size_x: 512.0, size_y: 512.0 },
                decals: [None, None, None, None, None, None],
            }, side_id, map_scale, texture_map),
        },
        Solid {
            id: {
                *part_id += 1;
                *part_id
            },
            sides: decompose_part(Part {
                part_type: PartType::Part,
                shape: PartShape::Block,
                is_detail: false,
                referent: "SKYBOX-Y",
                size: Vector3 {
                    x: (bounding_box.x_max - bounding_box.x_min).abs(),
                    y: 1.0,
                    z: (bounding_box.z_max - bounding_box.z_min).abs(),
                },
                cframe: CFrame {
                    position: Vector3 {
                        x: (bounding_box.x_max + bounding_box.x_min) / 2.0,
                        y: (bounding_box.y_max + bounding_box.y_min) / 2.0 - (bounding_box.y_max - bounding_box.y_min).abs() / 2.0 - 0.5,
                        z: (bounding_box.z_max + bounding_box.z_min) / 2.0,
                    },
                    rot_matrix: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                },
                color: Color3::white(),
                transparency: 0.0,
                reflectance: 0.0,
                material: Material::Custom { texture: "tools/toolsskybox", fill: false, generate: false, size_x: 512.0, size_y: 512.0 },
                decals: [None, None, None, None, None, None],
            }, side_id, map_scale, texture_map),
        },
        Solid {
            id: {
                *part_id += 1;
                *part_id
            },
            sides: decompose_part(Part {
                part_type: PartType::Part,
                shape: PartShape::Block,
                is_detail: false,
                referent: "SKYBOX-Z",
                size: Vector3 {
                    x: (bounding_box.x_max - bounding_box.x_min).abs(),
                    y: (bounding_box.y_max - bounding_box.y_min).abs(),
                    z: 1.0,
                },
                cframe: CFrame {
                    position: Vector3 {
                        x: (bounding_box.x_max + bounding_box.x_min) / 2.0,
                        y: (bounding_box.y_max + bounding_box.y_min) / 2.0,
                        z: (bounding_box.z_max + bounding_box.z_min) / 2.0 - (bounding_box.z_max - bounding_box.z_min).abs() / 2.0 - 0.5,
                    },
                    rot_matrix: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                },
                color: Color3::white(),
                transparency: 0.0,
                reflectance: 0.0,
                material: Material::Custom { texture: "tools/toolsskybox", fill: false, generate: false, size_x: 512.0, size_y: 512.0 },
                decals: [None, None, None, None, None, None],
            }, side_id, map_scale, texture_map),
        }
    ]
}