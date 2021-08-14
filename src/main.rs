#![allow(non_snake_case)]
#![feature(try_blocks)]
#![feature(option_result_contains)]
#![feature(array_map)]

use clap::{App, Arg};
use std::fs::{File, Metadata};
use std::io::{Read, Write};
use crate::rbx::{Model, Part, CFrame, Vector3, BoundingBox, Material, Color3, PartType};
use roxmltree::{Document, Node};
use crate::vmf::{VMFBuilder, Solid, Side, TextureFace, TextureMap, VMFTexture};
use std::fmt::{Display, Formatter};
use std::path::Path;
use image::{EncodableLayout, GenericImageView, ColorType, ImageFormat};

mod rbx;
mod vmf;

fn main() {
    let matches = App::new("RBXLX2VMF")
        .version("1.0")
        .about("Converts Roblox RBXLX files to Valve VMF files.")
        .arg(Arg::with_name("input")
            .long("input")
            .short("i")
            .value_name("FILE")
            .help("Sets input file")
            .takes_value(true)
            .required(true))
        .arg(Arg::with_name("texture-input")
            .long("texture-input")
            .short("ti")
            .value_name("FOLDER")
            .help("Sets texture input folder")
            .takes_value(true)
            .required(true))
        .arg(Arg::with_name("output")
            .long("output")
            .short("o")
            .value_name("FILE")
            .help("Sets output file")
            .default_value("rbxlx_out.vmf")
            .takes_value(true))
        .arg(Arg::with_name("texture-output")
            .long("texture-output")
            .short("to")
            .value_name("FOLDER")
            .help("Sets texture output folder")
            .default_value("./textures-out")
            .takes_value(true))
        .arg(Arg::with_name("auto-skybox")
            .long("auto-skybox")
            .help("enables automatic skybox (Warning: Results in highly unoptimized map)")
            .takes_value(false))
        .arg(Arg::with_name("skybox-height")
            .long("skybox-height")
            .help("sets additional auto-skybox height clearance")
            .takes_value(true))
        .arg(Arg::with_name("map-scale")
            .long("map-scale")
            .help("sets map scale")
            .default_value("15")
            .takes_value(true))
        .get_matches();


    let input = matches.value_of_os("input").unwrap();
    let output = matches.value_of_os("output").unwrap();
    let texture_input = matches.value_of_os("texture-input").unwrap();
    let texture_output = matches.value_of_os("texture-output").unwrap();

    let auto_skybox_enabled = matches.is_present("auto-skybox");
    let map_scale: f64 = match matches.value_of("map-scale").unwrap().parse() {
        Ok(f) => f,
        Err(_) => {
            println!("error: invalid map scale");
            std::process::exit(-1)
        }
    };
    let skybox_height_clearance = matches.value_of("skybox-height").map(str::parse).and_then(Result::ok).unwrap_or(0f64);

    println!("Converting {} to {}", input.to_string_lossy(), output.to_string_lossy());
    println!("Using map scale: {}×", map_scale);
    println!("Auto-skybox [{}]\n", if auto_skybox_enabled { "ENABLED" } else { "DISABLED" });

    let mut input = match File::open(input) {
        Ok(file) => file,
        Err(error) => {
            println!("error: Could not open input file: {}", error);
            std::process::exit(-1)
        }
    };
    let output = match File::create(output) {
        Ok(file) => file,
        Err(error) => {
            println!("error: Could not open output file {}", error);
            std::process::exit(-1)
        }
    };

    print!("Reading input... ");    // We need to flush print! manually, as it is usually line-buffered.
    std::io::stdout().flush().unwrap_or_default();  // Error discarded; Failed flush causes no problems.
    let mut buffer = String::with_capacity(input.metadata().as_ref().map(Metadata::len).unwrap_or(0) as usize);
    match input.read_to_string(&mut buffer) {
        Ok(_) => println!("DONE"),
        Err(error) => {
            println!("error: Could not read input {}", error);
            std::process::exit(-1)
        }
    }

    print!("Parsing XML...   ");
    std::io::stdout().flush().unwrap_or_default();  // Error discarded; Failed flush causes no problems.
    match Document::parse(buffer.as_str()) {
        Ok(document) => {
            let mut parts = Vec::new();
            let mut models = Vec::new();
            parse_xml(document.root_element(), &mut parts, &mut models, false);

            let result: std::io::Result<()> = try {
                let mut part_id = 35000 * 0;    // At most 32768 faces may exist in a VMF map, so ID blocks are sized according
                let mut side_id = 35000 * 1;
                let mut entity_id = 35000 * 2;

                let mut bounding_box = parts.iter()
                    .chain(models.iter().flat_map(<&Model>::into_iter).map(|(part, _)| part))
                    .copied()
                    .fold(BoundingBox::zeros(), BoundingBox::include);

                let mut texture_map = TextureMap::new();

                println!("{} parts found!", bounding_box.part_count);
                print!("Writing VMF...   ");
                std::io::stdout().flush().unwrap_or_default();  // Error discarded; Failed flush causes no problems.

                let mut world_solids = Vec::with_capacity(bounding_box.part_count as usize);
                let mut detail_solids = Vec::new();

                parts.iter()
                    .filter(|part| !part.is_detail)
                    .map(|part| {
                        Solid {
                            id: {
                                part_id += 1;
                                part_id
                            },
                            sides: decompose_part(*part, &mut side_id, map_scale, &mut texture_map),
                        }
                    })
                    .for_each(|s| world_solids.push(s));

                models.iter()
                    .flat_map(<&Model>::into_iter)
                    .map(|(part, _)| part)
                    .filter(|part| !part.is_detail)
                    .map(|part| {
                        Solid {
                            id: {
                                part_id += 1;
                                part_id
                            },
                            sides: decompose_part(*part, &mut side_id, map_scale, &mut texture_map),
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
                                sides: decompose_part(*part, &mut side_id, map_scale, &mut texture_map),
                            }
                        )
                    })
                    .for_each(|s| detail_solids.push(s));

                models.iter()
                    .flat_map(<&Model>::into_iter)
                    .map(|(part, _)| part)
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
                                sides: decompose_part(*part, &mut side_id, map_scale, &mut texture_map),
                            }
                        )
                    })
                    .for_each(|s| detail_solids.push(s));

                if auto_skybox_enabled {
                    bounding_box.y_max += skybox_height_clearance;
                    world_solids.extend(generate_skybox(&mut part_id, &mut side_id, bounding_box, map_scale, &mut texture_map));
                }

                VMFBuilder(output)
                    .version_info(400, 3325, 0, false)? // Defaults from https://developer.valvesoftware.com/wiki/Valve_Map_Format
                    .visgroups()?
                    .viewsettings()?
                    .world(0, "sky_tf2_04", world_solids, &texture_map)?
                    .detail(detail_solids, &texture_map)?
                    .flush()?;
                println!("DONE");


                print!("Writing textures...\n");
                std::io::stdout().flush().unwrap_or_default();  // Error discarded; Failed flush causes no problems.

                let texture_input_folder = Path::new(texture_input);
                let texture_output_folder = Path::new(texture_output);
                if let Err(error) = std::fs::create_dir_all(texture_output_folder) {
                    println!("error: could not create texture output directory {}", error);
                    std::process::exit(-1)
                }
                for texture in texture_map.iter().filter(|t| if let Material::Custom { generate, .. } = t.material { generate } else { true }) {
                    let path = texture_input_folder.join(texture.material.texture()).with_extension("png");
                    print!("\ttexture:{}...", texture);
                    std::io::stdout().flush().unwrap_or_default();  // Error discarded; Failed flush causes no problems.
                    match image::io::Reader::open(path) {
                        Ok(image_reader) => {
                            match image_reader.decode() {
                                Ok(image) => {
                                    let width = image.width();
                                    let height = image.height();
                                    let rgba_image = image.into_rgba8();    // Optimization hack: We're guesstimating the images are RGBA
                                    let input_buf = rgba_image.as_bytes();


                                    let mut output_buf = Vec::with_capacity((width * height * 3) as usize);
                                    for index in 0..(input_buf.len() / 4) {
                                        let red = input_buf[index * 4 + 0];
                                        let green = input_buf[index * 4 + 1];
                                        let blue = input_buf[index * 4 + 2];

                                        let out_red = (texture.color.red as u64 * red as u64 / 255) as u8;
                                        let out_green = (texture.color.green as u64 * green as u64 / 255) as u8;
                                        let out_blue = (texture.color.blue as u64 * blue as u64 / 255) as u8;

                                        output_buf.push(out_red);
                                        output_buf.push(out_green);
                                        output_buf.push(out_blue);
                                    }

                                    let image_out_path = texture_output_folder.join(format!("{}_{}-{}-{}", texture.material.texture(), texture.color.red, texture.color.blue, texture.color.green))
                                        .with_extension("png");
                                    let vmt_out_path = texture_output_folder.join(format!("{}_{}-{}-{}", texture.material.texture(), texture.color.red, texture.color.blue, texture.color.green))
                                        .with_extension("vmt");

                                    match image::save_buffer_with_format(image_out_path, &*output_buf, width, height, ColorType::Rgb8, ImageFormat::Png) {
                                        Ok(_) => {
                                            println!(" SAVED")
                                        }
                                        Err(error) => {
                                            println!("error: could not write texture file {}", error);
                                            std::process::exit(-1)
                                        }
                                    }

                                    if let Err(error) = File::create(vmt_out_path).and_then(|mut file| {
                                        write!(file,
                                               "\"LightmappedGeneric\"\n\
                                                {{\n\
                                                \"$basetexture\" \"{}\"\n\
                                                }}\n",
                                               texture
                                        )
                                    }) {
                                        println!("\t\twarning: could not write VMT: {}", error);
                                    }
                                }
                                Err(error) => {
                                    println!("error: could not read texture file {}", error);
                                    std::process::exit(-1)
                                }
                            }
                        }
                        Err(error) => {
                            println!("error: could not read texture file {}", error);
                            std::process::exit(-1)
                        }
                    };
                }
            };
            if let Err(error) = result {
                println!("error: could not write VMF {}", error);
                std::process::exit(-1)
            }
        }
        Err(error) => {
            println!("error: invalid XML {}", error);
            std::process::exit(-1)
        }
    }
}

/// Convenience trait; Provides methods for searching for specific children of a node
pub trait NodeExtensions<'a> {
    type Output;

    fn get_child_with_name(self, tag_name: &'a str) -> Option<Self::Output>;
    fn get_child_with_attribute(self, tag_name: &'a str, attribute_name: &'a str, attribute_value: &'a str) -> Option<Self::Output>;
    fn get_child_text(self, tag_name: &'a str) -> Option<&'a str>;
}

impl<'a, 'input> NodeExtensions<'a> for Node<'a, 'input> {
    type Output = Node<'a, 'input>;

    fn get_child_with_name(self, tag_name: &str) -> Option<Node<'a, 'input>> {
        self.children()
            .filter(|node| node.tag_name().name().eq(tag_name))
            .next()
    }

    fn get_child_with_attribute(self, tag_name: &str, attribute_name: &str, attribute_value: &str) -> Option<Node<'a, 'input>> {
        self.children()
            .filter(|node| node.tag_name().name().eq(tag_name))
            .filter(|node| node.attribute(attribute_name).contains(&attribute_value))
            .next()
    }

    fn get_child_text(self, tag_name: &'a str) -> Option<&'a str> {
        self.get_child_with_name(tag_name)?.text()
    }
}

/// Recursively parses XML
/// Expects machine-generated RBXLX files as input, and skips any malformed items.
fn parse_xml<'a>(node: Node<'a, '_>, parts: &mut Vec<Part<'a>>, models: &mut Vec<Model<'a>>, is_detail: bool) {
    match node.attribute("class") {
        Some(class @ "Part") | Some(class @ "SpawnLocation") | Some(class @ "TrussPart") => {
            let option: Option<()> = try {
                let referent = node.attribute("referent")?;
                let properties = node.get_child_with_name("Properties")?;

                let size_node = properties.get_child_with_attribute("Vector3", "name", "size")?;
                let position_node = properties.get_child_with_attribute("CoordinateFrame", "name", "CFrame")?;

                let color = Color3::from(
                    properties.get_child_with_name("Color3uint8")?
                        .text()?
                        .parse::<u32>()
                        .ok()?
                );

                let material = Material::from_id(
                    properties.get_child_with_attribute("token", "name", "Material")?
                        .text()?
                        .parse::<u32>()
                        .ok()?
                )?;

                const DECAL_FRONT: usize = 5;
                const DECAL_BACK: usize = 2;
                const DECAL_TOP: usize = 1;
                const DECAL_BOTTOM: usize = 4;
                const DECAL_RIGHT: usize = 0;
                const DECAL_LEFT: usize = 3;

                let mut decals = [None; 6];

                fn decal_for_side(properties: Node, decals: &mut [Option<Material>; 6], side_name: &str, side_enum: usize) {
                    if let Some(surface) = properties.get_child_with_attribute("token", "name", side_name).and_then(|node| node.text()) {
                        let decal = match surface.parse() {
                            Ok(3u8) => Some(Material::Custom { texture: "studs", generate: true }),    // Studs,
                            Ok(4u8) => Some(Material::Custom { texture: "inlet", generate: true }),    // Inlet,
                            _ => None
                        };
                        decals[side_enum] = decal;
                    }
                }
                decal_for_side(properties, &mut decals, "FrontSurface", DECAL_FRONT);
                decal_for_side(properties, &mut decals, "BackSurface", DECAL_BACK);
                decal_for_side(properties, &mut decals, "TopSurface", DECAL_TOP);
                decal_for_side(properties, &mut decals, "BottomSurface", DECAL_BOTTOM);
                decal_for_side(properties, &mut decals, "RightSurface", DECAL_RIGHT);
                decal_for_side(properties, &mut decals, "LeftSurface", DECAL_LEFT);

                node.children()
                    .filter(|child_node| child_node.tag_name().name() == "Item" && child_node.attribute("class").contains(&"Decal"))
                    .filter_map(|child_node| child_node.get_child_with_name("Properties"))
                    .filter_map(|properties| properties.get_child_with_attribute("token", "name", "Face"))
                    .filter_map(|node| node.text())
                    .filter_map(|text| text.parse::<u8>().ok())
                    .for_each(|side| {
                        if side < 6 {
                            // We could probably retrieve the actual decal through https://assetdelivery.roblox.com/docs#!/AssetFetch/get_v1_assetId_assetId
                            decals[side as usize] = Some(Material::Custom { texture: "decal", generate: true })
                        }
                    });

                if class == "SpawnLocation" {
                    decals[DECAL_TOP] = Some(Material::Custom { texture: "spawnlocation", generate: true })
                }

                let part_type = match class {
                    "Part" => PartType::Part,
                    "SpawnLocation" => PartType::SpawnLocation,
                    "TrussPart" => PartType::Truss,
                    _ => unreachable!()
                };

                parts.push(Part {
                    part_type,
                    is_detail,
                    referent,
                    size: Vector3 {
                        x: size_node.get_child_text("X")?.parse().ok()?,
                        y: size_node.get_child_text("Y")?.parse().ok()?,
                        z: size_node.get_child_text("Z")?.parse().ok()?,
                    },
                    cframe: CFrame {
                        position: Vector3 {
                            x: position_node.get_child_text("X")?.parse().ok()?,
                            y: position_node.get_child_text("Y")?.parse().ok()?,
                            z: position_node.get_child_text("Z")?.parse().ok()?,
                        },
                        rot_matrix: [
                            [position_node.get_child_text("R00")?.parse().ok()?, position_node.get_child_text("R10")?.parse().ok()?, position_node.get_child_text("R20")?.parse().ok()?],
                            [position_node.get_child_text("R01")?.parse().ok()?, position_node.get_child_text("R11")?.parse().ok()?, position_node.get_child_text("R21")?.parse().ok()?],
                            [position_node.get_child_text("R02")?.parse().ok()?, position_node.get_child_text("R12")?.parse().ok()?, position_node.get_child_text("R22")?.parse().ok()?],
                        ],
                    },
                    color,
                    material,
                    decals: if decals.iter().any(Option::is_some) {
                        Some(decals)
                    } else {
                        None
                    },
                });
            };
            if option.is_none() {
                println!("Skipping malformed Part: {}-{}", node.range().start, node.range().end)
            }
        }
        Some("Model") => {
            let option: Option<()> = try {
                let referent = node.attribute("referent")?;
                let properties = node.get_child_with_name("Properties")?;
                let name = properties.get_child_with_attribute("string", "name", "Name")?.text()?;

                let is_model_detail = is_detail |
                    node.children()
                        .filter(|p| {
                            p.attribute("class")
                                .map(|s| s == "StringValue")
                                .unwrap_or(false)
                        })
                        .any(|node| {
                            if let Some(properties) = node.get_child_with_name("Properties") {
                                properties.get_child_with_attribute("string", "name", "Name").as_ref().and_then(Node::text).contains(&"func_detail")
                                    | properties.get_child_with_attribute("string", "name", "Value").as_ref().and_then(Node::text).contains(&"func_detail")
                            } else {
                                false
                            }
                        });

                let mut child_models = Vec::new();
                let mut child_parts = Vec::new();
                for child in node.children() {
                    parse_xml(child, &mut child_parts, &mut child_models, is_model_detail)
                }
                models.push(Model { name, referent, models: child_models, parts: child_parts })
            };
            if option.is_none() {
                println!("Skipping malformed Model: {}-{}", node.range().start, node.range().end)
            }
        }
        _ => {
            for child in node.children() {
                parse_xml(child, parts, models, is_detail)
            }
        }
    }
}

#[derive(PartialEq, Copy, Clone)]
pub struct RobloxTexture {
    pub material: Material,
    pub color: Color3,
    pub scale_x: f64,
    pub scale_z: f64,
}

impl VMFTexture for RobloxTexture {
    fn scale_x(&self) -> f64 {
        self.scale_x
    }

    fn scale_z(&self) -> f64 {
        self.scale_z
    }
}

impl Display for RobloxTexture {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Material::Custom { texture, generate } = self.material {
            if !generate {
                write!(f, "{}", texture)
            } else {
                write!(f, "rbx/{}_{}-{}-{}", self.material.texture(), self.color.red, self.color.blue, self.color.green)
            }
        } else {
            write!(f, "rbx/{}_{}-{}-{}", self.material.texture(), self.color.red, self.color.blue, self.color.green)
        }
    }
}

/// Decomposes a Roblox part into it's polyhedron faces, and returns them as source engine Sides
/// Currently only supports box-shaped parts
fn decompose_part(part: Part, id: &mut u32, map_scale: f64, texture_map: &mut TextureMap<RobloxTexture>) -> Vec<Side> {
    let boundaries = part.boundaries();

    const DECAL_FRONT: usize = 5;
    const DECAL_BACK: usize = 2;
    const DECAL_TOP: usize = 1;
    const DECAL_BOTTOM: usize = 4;
    const DECAL_RIGHT: usize = 0;
    const DECAL_LEFT: usize = 3;

    // First three boundaries of a plane form the defining points, in the order required by source engine
    let planes = [
        ([boundaries[5], boundaries[7], boundaries[4], boundaries[6]], DECAL_TOP),      // +Y
        ([boundaries[0], boundaries[2], boundaries[1], boundaries[3]], DECAL_BOTTOM),   // -Y
        ([boundaries[2], boundaries[7], boundaries[6], boundaries[3]], DECAL_RIGHT),    // -X
        ([boundaries[5], boundaries[0], boundaries[1], boundaries[4]], DECAL_LEFT),     // +X
        ([boundaries[3], boundaries[4], boundaries[7], boundaries[0]], DECAL_FRONT),    // -Z
        ([boundaries[6], boundaries[1], boundaries[2], boundaries[5]], DECAL_BACK)      // +Z
    ];

    let part_centroid = part.centroid();

    let sides = std::array::IntoIter::new(planes).map(|(plane, decal_side)| {
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

        // Determine which cardinal direction the plane normal vector points; This is the direction from which the texture is rendered.
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
            if let Some(decals) = part.decals {
                if let Some(side_decal) = decals[decal_side] {
                    if part.part_type == PartType::SpawnLocation && decal_side == DECAL_TOP {    // Bit of a hack, to resize spawnlocation textures, TODO: Move scale into the decal type
                        RobloxTexture {
                            material: side_decal,
                            color: part.color,
                            scale_x: (vector_b.magnitude() / 256.0) * map_scale,    // TODO: Spawnlocation textures need to be shifted in the X/Z direction to be fully correct
                            scale_z: (vector_b.magnitude() / 256.0) * map_scale,
                        }
                    } else {
                        RobloxTexture {
                            material: side_decal,
                            color: part.color,
                            scale_x: (1.0 / 32.0) * map_scale,
                            scale_z: (1.0 / 32.0) * map_scale
                        }
                    }
                } else {
                    RobloxTexture {
                        material: part.material,
                        color: part.color,
                        scale_x: 0.25,
                        scale_z: 0.25
                    }
                }
            } else {
                RobloxTexture {
                    material: part.material,
                    color: part.color,
                    scale_x: 0.25,
                    scale_z: 0.25
                }
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
        };
        *id += 1;
        side
    }).collect();
    *id += 6;
    sides
}


/// Converts roblox coordinates to source engine coordinates
pub fn to_source_coordinates(vector: Vector3) -> [f64; 3] {
    [
        vector.x,
        -vector.z, // Negation corrects for mirroring in hammer/VMF
        vector.y
    ]
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
                material: Material::Custom { texture: "tools/toolsskybox", generate: false },
                decals: None,
            }, side_id, map_scale, texture_map),
        },
        Solid {
            id: {
                *part_id += 1;
                *part_id
            },
            sides: decompose_part(Part {
                part_type: PartType::Part,
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
                material: Material::Custom { texture: "tools/toolsskybox", generate: false },
                decals: None,
            }, side_id, map_scale, texture_map),
        },
        Solid {
            id: {
                *part_id += 1;
                *part_id
            },
            sides: decompose_part(Part {
                part_type: PartType::Part,
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
                material: Material::Custom { texture: "tools/toolsskybox", generate: false },
                decals: None,
            }, side_id, map_scale, texture_map),
        },
        Solid {
            id: {
                *part_id += 1;
                *part_id
            },
            sides: decompose_part(Part {
                part_type: PartType::Part,
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
                material: Material::Custom { texture: "tools/toolsskybox", generate: false },
                decals: None,
            }, side_id, map_scale, texture_map),
        },
        Solid {
            id: {
                *part_id += 1;
                *part_id
            },
            sides: decompose_part(Part {
                part_type: PartType::Part,
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
                material: Material::Custom { texture: "tools/toolsskybox", generate: false },
                decals: None,
            }, side_id, map_scale, texture_map),
        },
        Solid {
            id: {
                *part_id += 1;
                *part_id
            },
            sides: decompose_part(Part {
                part_type: PartType::Part,
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
                material: Material::Custom { texture: "tools/toolsskybox", generate: false },
                decals: None,
            }, side_id, map_scale, texture_map),
        }
    ]
}