pub mod parse;
pub mod texture;

use std::io;
use std::io::{Read, Write};
use image::{EncodableLayout, ImageFormat};
use roxmltree::Document;
use crate::conv::texture::RobloxTexture;
use crate::rbx::{BoundingBox, Material, Part, PartShape};
use crate::vmf::{Solid, TextureMap, VMFBuilder, VMFTexture};
use crate::rbx::{Vector3, CFrame, PartType, Color3};
use crate::conv::texture::TextureScale;
use crate::vmf::{Side, TextureFace, Displacement};


const MAX_PART_COUNT: usize = 32768;    // VMF format limitations
const ID_BLOCK_SIZE: u32 = 35000;

/// AsRef variant with explicit lifetime
#[allow(unused)]    // We use one variant at a time in the binary and wasm
pub enum OwnedOrRef<'a, T> {
    Owned(T),
    Ref(&'a T)
}

impl<'a, T> OwnedOrRef<'a, T> {
    pub fn as_ref(&'a self) -> &'a T {
        match self {
            OwnedOrRef::Owned(o) => o,
            OwnedOrRef::Ref(r) => r
        }
    }
}

/// AsMut variant with explicit lifetime
#[allow(unused)]    // We use one variant at a time in the binary and wasm
pub enum OwnedOrMut<'a, T> {
    Owned(T),
    Ref(&'a mut T)
}

impl<'a, T> OwnedOrMut<'a, T> {
    pub fn as_mut(&'a mut self) -> &'a mut T {
        match self {
            OwnedOrMut::Owned(o) => o,
            OwnedOrMut::Ref(r) => r
        }
    }
}

pub trait ConvertOptions<R: Read, W: Write> {
    fn print_output(&self) -> Box<dyn Write>;
    fn error_output(&self) -> Box<dyn Write>;

    fn input_name(&self) -> &str;
    fn read_input_data<'a>(&'a self) -> OwnedOrRef<'a, String>;

    fn vmf_output<'a>(&'a mut self) -> OwnedOrMut<'a, W>;
    fn texture_input<'a>(&'a mut self, texture: Material) -> Option<OwnedOrMut<'a, R>>;
    fn texture_output<'a>(&'a mut self, path: &str) -> OwnedOrMut<'a, W>;
    fn texture_output_enabled(&self) -> bool;
    fn use_dev_textures(&self) -> bool;

    fn map_scale(&self) -> f64;
    fn auto_skybox_enabled(&self) -> bool;
    fn skybox_clearance(&self) -> f64;
    fn optimization_enabled(&self) -> bool;

    fn decal_size(&self) -> u64;
    fn skybox_name(&self) -> &str;

    fn web_origin(&self) -> &str;
}

pub async fn convert<R: Read, W: Write, O: ConvertOptions<R, W>>(mut options: O) -> Result<u8, std::io::Error> {
    let mut print_out = options.print_output();
    let mut error_out = options.error_output();
    writeln!(print_out, "Converting {}", options.input_name())?;
    writeln!(print_out, "Using map scale: {}×", options.map_scale())?;
    writeln!(print_out, "Auto-skybox [{}]", if options.auto_skybox_enabled() { "ENABLED" } else { "DISABLED" })?;
    writeln!(print_out, "Part-count optimization [{}]", if options.optimization_enabled() { "ENABLED" } else { "DISABLED" })?;
    writeln!(print_out)?;

    write!(print_out, "Reading input...    ")?;    // We need to flush print! manually, as it is usually line-buffered.
    print_out.flush().unwrap_or_default();  // Error discarded; Failed flush causes no problems.
    writeln!(print_out, "DONE")?;

    write!(print_out, "Parsing XML...      ")?;
    print_out.flush().unwrap_or_default();
    match Document::parse(options.read_input_data().as_ref()) {
        Ok(document) => {
            let mut parts = Vec::new();
            parse::parse_xml(document.root_element(), &mut parts, false, options.decal_size());
            writeln!(print_out, "{} parts found!", parts.len())?;

            if options.optimization_enabled() {
                write!(print_out, "Optimizing...\n")?;
                print_out.flush().unwrap_or_default();
                let old_count = parts.len();
                parts = Part::join_adjacent(parts, true, &mut print_out);
                writeln!(print_out, "Reduced part count to {} (-{})", parts.len(), old_count - parts.len())?;
            }

            if parts.len() > MAX_PART_COUNT {
                writeln!(error_out, "error: Too many parts, found: {} parts, must be fewer than {}", parts.len(), MAX_PART_COUNT + 1)?;
                return Ok(1)
            }

            // Hack: Source engine does not support surface-displacement on detail
            parts.iter_mut().for_each(|part| if part.shape != PartShape::Block { part.is_detail = false });

            let result: std::io::Result<()> = try {
                let mut part_id = ID_BLOCK_SIZE * 0;    // IDs split into blocks to avoid overlap
                let mut side_id = ID_BLOCK_SIZE * 1;
                let mut entity_id = ID_BLOCK_SIZE * 2;

                let mut bounding_box = parts.iter()
                    .copied()
                    .fold(BoundingBox::zeros(), BoundingBox::include);

                let mut texture_map = TextureMap::new();

                write!(print_out, "Writing VMF...      ")?;
                print_out.flush().unwrap_or_default();

                let mut world_solids = Vec::with_capacity(parts.len());
                let mut detail_solids = Vec::new();

                parts.iter()
                    .filter(|part| !part.is_detail)
                    .map(|part| {
                        Solid {
                            id: {
                                part_id += 1;
                                part_id
                            },
                            sides: decompose_part(*part, &mut side_id, options.map_scale(), options.use_dev_textures(), &mut texture_map),
                        }
                    })
                    .for_each(|s| world_solids.push(s));

                parts.iter()
                    .filter(|part| part.is_detail)
                    .map(|part| {
                        (
                            {
                                entity_id += 1;
                                entity_id
                            },
                            Solid {
                                id: {
                                    part_id += 1;
                                    part_id
                                },
                                sides: decompose_part(*part, &mut side_id, options.map_scale(), options.use_dev_textures(), &mut texture_map),
                            }
                        )
                    })
                    .for_each(|s| detail_solids.push(s));

                if options.auto_skybox_enabled() {
                    bounding_box.y_max += options.skybox_clearance();
                    world_solids.extend(generate_skybox(&mut part_id, &mut side_id, bounding_box, options.map_scale(), &mut texture_map));
                }

                let skyname = options.skybox_name().to_string();  // Make owned copy; We want to borrow options mutable as well

                VMFBuilder(options.vmf_output().as_mut())
                    .version_info(400, 3325, 0, false)? // Defaults from https://developer.valvesoftware.com/wiki/Valve_Map_Format
                    .visgroups()?
                    .viewsettings()?
                    .world(0, &*skyname, world_solids, &texture_map)?
                    .detail(detail_solids, &texture_map)?
                    .flush()?;
                writeln!(print_out, "DONE")?;

                if options.texture_output_enabled() {
                    write!(print_out, "Writing textures...\n")?;
                    print_out.flush().unwrap_or_default();

                    let mut textures_to_copy = Vec::new();  // We don't want to hash Material, and the low amount of entries in this Vec makes checking pretty fast.

                    let http_client = reqwest::Client::new();

                    for texture in texture_map.into_iter().filter(RobloxTexture::must_generate) {
                        if let Material::Decal { id, .. } | Material::Texture { id, .. } = texture.material {
                            write!(print_out, "\tdecal: {}...", id)?;
                            print_out.flush().unwrap_or_default();
                            match texture::fetch_texture(&http_client, id, texture, texture.dimension_x as u32, texture.dimension_y as u32).await {
                                Ok(image) => {
                                    let image_out_path = format!("{}.png", texture.name());
                                    match image.write_to(options.texture_output(&*image_out_path).as_mut(), ImageFormat::Png) {
                                        Ok(_) => writeln!(print_out, " SAVED")?,
                                        Err(error) => {
                                            writeln!(error_out, "error: could not write texture file {}", error)?;
                                            return Ok(1)
                                        }
                                    }

                                    let vmt_out_path = format!("{}.vmt", texture.name());
                                    let mut temp = options.texture_output(&*vmt_out_path);
                                    let file = temp.as_mut();
                                    let result: Result<(), io::Error> = try {
                                        write!(file,
                                               "\"LightmappedGeneric\"\n\
                                           {{\n\
                                           \t$basetexture \"{}\"\n",
                                               texture.name()
                                        )?;
                                        if texture.transparency != 255 {
                                            write!(file, "\t$translucent 1\n")?;
                                        }
                                        if texture.reflectance != 0 {
                                            write!(file, "\t$envmap env_cubemap\n")?;
                                            write!(file, "\t$envmaptint \"[{reflectance} {reflectance} {reflectance}]\"\n", reflectance = 1.0 / (255.0 / (texture.reflectance as f64)))?;
                                        }
                                        write!(file, "}}\n")?;
                                    };
                                    if let Err(error) = result {
                                        writeln!(print_out, "\t\twarning: could not write VMT: {}", error)?;
                                    }
                                }
                                Err(error) => writeln!(error_out, "error loading decal: {}", error)?,
                            }
                        } else {
                            write!(print_out, "\ttexture: {}...", texture.name())?;
                            print_out.flush().unwrap_or_default();

                            if !(textures_to_copy.contains(&texture.material)) {
                                debug_assert!(!(matches!(texture.material, Material::Decal { .. }) && matches!(texture.material, Material::Texture { .. })));
                                textures_to_copy.push(texture.material);
                            }

                            let vmt_out_path = format!("{}.vmt", texture.name());
                            let mut temp = options.texture_output(&*vmt_out_path);
                            let file = temp.as_mut();
                            let result: Result<(), io::Error> = try {
                                write!(file,
                                       "\"LightmappedGeneric\"\n\
                                           {{\n\
                                           \t$basetexture \"rbx/{}\"\n\
                                           \t$color \"[{} {} {}]\"\n",
                                       texture.material,
                                       ((texture.color.red as f64) / 255.0).powf(2.2),  // Pow for gamma adjustment
                                       ((texture.color.green as f64) / 255.0).powf(2.2),
                                       ((texture.color.blue as f64) / 255.0).powf(2.2)
                                )?;
                                if texture.transparency != 255 {
                                    write!(file, "\t$alpha {}\n", texture.transparency as f64 / 255.0)?;
                                }
                                if texture.reflectance != 0 {
                                    write!(file, "\t$envmap env_cubemap\n")?;
                                    write!(file, "\t$envmaptint \"[{reflectance} {reflectance} {reflectance}]\"\n", reflectance = 1.0 / (255.0 / (texture.reflectance as f64)))?;
                                }
                                write!(file, "}}\n")?;
                            };
                            if let Err(error) = result {
                                writeln!(error_out, "\t\twarning: could not write VMT: {}", error)?;
                            } else {
                                writeln!(print_out, " SAVED")?;
                            }
                        };
                    }

                    write!(print_out, "Copying textures...\n")?;
                    print_out.flush().unwrap_or_default();
                    for texture in textures_to_copy {
                        write!(print_out, "\ttexture: {}...", texture)?;
                        print_out.flush().unwrap_or_default();

                        let mut bytes = Vec::new();
                        if cfg!(all(target_arch = "wasm32", target_os = "unknown")) {   // TODO: Remove this hack once trait functions can be async
                            let path = format!("{}/textures/{}.png", options.web_origin(), texture);

                            match http_client.get(path).send().await {
                                Ok(response) => {
                                    if response.status().is_success() {
                                        match response.bytes().await {
                                            Ok(bytes) => {
                                                let texture_path = format!("rbx/{}.png", texture);
                                                let mut temp = options.texture_output(&*texture_path);
                                                let file = temp.as_mut();
                                                if let Err(error) = file.write_all(bytes.as_bytes()) {
                                                    writeln!(error_out, "\t\twarning: could not copy texture file {}: {}", texture, error)?;
                                                } else {
                                                    writeln!(print_out, " COPIED")?;
                                                }
                                            }
                                            Err(error) => writeln!(print_out, " FAILED ({})", error)?,
                                        }
                                    } else {
                                        writeln!(print_out, " FAILED (HTTP {})", response.status())?;
                                    }
                                }
                                Err(error) => writeln!(print_out, " FAILED ({})", error)?
                            }
                        } else {
                            let input = options.texture_input(texture);
                            if let Some(mut file) = input {
                                if let Err(error) = file.as_mut().read_to_end(&mut bytes) {
                                    writeln!(error_out, "\t\twarning: could not read texture file {}: {}", texture, error)?;
                                } else {
                                    let texture_path = format!("rbx/{}.png", texture);
                                    let mut temp = options.texture_output(&*texture_path);
                                    let file = temp.as_mut();
                                    if let Err(error) = file.write_all(&*bytes) {
                                        writeln!(error_out, "\t\twarning: could not copy texture file {}: {}", texture, error)?;
                                    } else {
                                        writeln!(print_out, " COPIED")?;
                                    }
                                }
                            } else {
                                writeln!(print_out, " SKIPPED")?;
                            }
                        }
                    }
                }
            };
            if let Err(error) = result {
                writeln!(error_out, "error: could not write VMF {}", error)?;
                return Ok(1);
            }
            Ok(0)
        }
        Err(error) => {
            writeln!(error_out, "error: invalid XML {}", error)?;
            return Ok(1);
        }
    }
}

/// Converts roblox coordinates to source engine coordinates
fn to_source_coordinates(vector: Vector3) -> [f64; 3] {
    [
        vector.x,
        -vector.z, // Negation corrects for mirroring in hammer/VMF
        vector.y
    ]
}

/// Decomposes a Roblox part into it's polyhedron faces, and returns them as source engine Sides
fn decompose_part(part: Part, id: &mut u32, map_scale: f64, use_dev_textures: bool, texture_map: &mut TextureMap<RobloxTexture>) -> Vec<Side> {
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
            if use_dev_textures {
                match part.material {
                    Material::Plastic => {
                        RobloxTexture {
                            material: Material::Custom {
                                texture: "dev/dev_measuregeneric01",
                                fill: false,
                                generate: false,
                                size_x: 64,
                                size_y: 64
                            },
                            color: Color3::white(),
                            transparency: 255,
                            reflectance: 0,
                            scale: TextureScale::FIXED { scale_x: 0.25, scale_z: 0.25 },
                            no_offset: true,
                            dimension_x: 64,
                            dimension_y: 64
                        }
                    },
                    Material::DiamondPlate => {
                        RobloxTexture {
                            material: Material::Custom {
                                texture: "dev/dev_measuregeneric01b",
                                fill: false,
                                generate: false,
                                size_x: 64,
                                size_y: 64
                            },
                            color: Color3::white(),
                            transparency: 255,
                            reflectance: 0,
                            scale: TextureScale::FIXED { scale_x: 0.25, scale_z: 0.25 },
                            no_offset: true,
                            dimension_x: 64,
                            dimension_y: 64
                        }
                    },
                    Material::Wood => {
                        RobloxTexture {
                            material: Material::Custom {
                                texture: "customdev/dev_measuregeneric01red",
                                fill: false,
                                generate: false,
                                size_x: 64,
                                size_y: 64
                            },
                            color: Color3::white(),
                            transparency: 255,
                            reflectance: 0,
                            scale: TextureScale::FIXED { scale_x: 0.25, scale_z: 0.25 },
                            no_offset: true,
                            dimension_x: 64,
                            dimension_y: 64
                        }
                    },
                    Material::Brick => {
                        RobloxTexture {
                            material: Material::Custom {
                                texture: "customdev/dev_measuregeneric01blu",
                                fill: false,
                                generate: false,
                                size_x: 64,
                                size_y: 64
                            },
                            color: Color3::white(),
                            transparency: 255,
                            reflectance: 0,
                            scale: TextureScale::FIXED { scale_x: 0.25, scale_z: 0.25 },
                            no_offset: true,
                            dimension_x: 64,
                            dimension_y: 64
                        }
                    },
                    Material::ForceField => {
                        RobloxTexture {
                            material: Material::Custom {
                                texture: "tools/toolsclip",
                                fill: false,
                                generate: false,
                                size_x: 64,
                                size_y: 64
                            },
                            color: Color3::white(),
                            transparency: 255,
                            reflectance: 0,
                            scale: TextureScale::FIXED { scale_x: 0.25, scale_z: 0.25 },
                            no_offset: true,
                            dimension_x: 64,
                            dimension_y: 64
                        }
                    },
                    Material::Glass => {
                        RobloxTexture {
                            material: Material::Custom {
                                texture: "tools/toolsskybox",
                                fill: false,
                                generate: false,
                                size_x: 64,
                                size_y: 64
                            },
                            color: Color3::white(),
                            transparency: 255,
                            reflectance: 0,
                            scale: TextureScale::FIXED { scale_x: 0.25, scale_z: 0.25 },
                            no_offset: true,
                            dimension_x: 64,
                            dimension_y: 64
                        }
                    },
                    _ => {
                        RobloxTexture {
                            material: Material::Custom {
                                texture: "dev/graygrid",
                                fill: false,
                                generate: false,
                                size_x: 64,
                                size_y: 64
                            },
                            color: Color3::white(),
                            transparency: 255,
                            reflectance: 0,
                            scale: TextureScale::FIXED { scale_x: 0.25, scale_z: 0.25 },
                            no_offset: true,
                            dimension_x: 64,
                            dimension_y: 64
                        }
                    }
                }
            } else if let Some(side_decal) = part.decals[decal_side] {
                let (color, transparency) = if let Material::Custom { texture: "decal", .. } = &side_decal {    // Slight hack: Do not color "decal" textures
                    (Color3::white(), 255)
                } else {
                    (part.color, (255.0 * (1.0 - part.transparency)) as u8)
                };
                RobloxTexture {
                    material: side_decal,
                    color,
                    transparency,
                    reflectance: (255.0 * part.reflectance) as u8,
                    scale: match side_decal {
                        Material::Decal { .. } | Material::Custom { fill: true, .. } => TextureScale::FILL,
                        Material::Texture { size_x, size_y, studs_per_u, studs_per_v, .. } => {
                            TextureScale::FIXED {
                                scale_x: map_scale * studs_per_u / (size_x as f64),
                                scale_z: map_scale * studs_per_v / (size_y as f64),
                            }
                        }
                        _ => TextureScale::FIXED { scale_x: map_scale / 32.0, scale_z: map_scale / 32.0 },
                    },
                    no_offset: false,
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
                    no_offset: false,
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
            PartShape::Cylinder => None,
            PartShape::Block => None,
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

fn generate_skybox(part_id: &mut u32, side_id: &mut u32, bounding_box: BoundingBox, map_scale: f64, texture_map: &mut TextureMap<RobloxTexture>) -> [Solid; 6] {
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
                material: Material::Custom { texture: "tools/toolsskybox", fill: false, generate: false, size_x: 512, size_y: 512 },
                decals: [None, None, None, None, None, None],
            }, side_id, map_scale, false, texture_map),
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
                material: Material::Custom { texture: "tools/toolsskybox", fill: false, generate: false, size_x: 512, size_y: 512 },
                decals: [None, None, None, None, None, None],
            }, side_id, map_scale, false, texture_map),
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
                material: Material::Custom { texture: "tools/toolsskybox", fill: false, generate: false, size_x: 512, size_y: 512 },
                decals: [None, None, None, None, None, None],
            }, side_id, map_scale, false, texture_map),
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
                material: Material::Custom { texture: "tools/toolsskybox", fill: false, generate: false, size_x: 512, size_y: 512 },
                decals: [None, None, None, None, None, None],
            }, side_id, map_scale, false, texture_map),
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
                material: Material::Custom { texture: "tools/toolsskybox", fill: false, generate: false, size_x: 512, size_y: 512 },
                decals: [None, None, None, None, None, None],
            }, side_id, map_scale, false, texture_map),
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
                material: Material::Custom { texture: "tools/toolsskybox", fill: false, generate: false, size_x: 512, size_y: 512 },
                decals: [None, None, None, None, None, None],
            }, side_id, map_scale, false, texture_map),
        }
    ]
}