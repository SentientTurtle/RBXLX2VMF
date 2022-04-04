#![allow(non_snake_case)]
#![feature(try_blocks)]
#![feature(option_result_contains)]

use std::collections::{HashMap, HashSet};
use clap::{App, Arg};
use std::fs::{File, Metadata};
use std::io::{Read, Write};
use crate::rbx::{Part, CFrame, Vector3, BoundingBox, Material, Color3, PartType, PartShape};
use roxmltree::{Document, Node};
use crate::vmf::{VMFBuilder, Solid, Side, TextureFace, TextureMap, VMFTexture, Displacement};
use std::path::Path;
use flate2::read::GzDecoder;
use image::{EncodableLayout, GenericImageView, ColorType, ImageFormat, DynamicImage, ImageBuffer, Rgba};
use image::imageops::FilterType;

mod rbx;
mod vmf;

const MAX_PART_COUNT: usize = 32768;
const ID_BLOCK_SIZE: u32 = 35000;
const ROBLOX_DECAL_MAX_WIDTH: u32 = 1024;
const ROBLOX_DECAL_MAX_HEIGHT: u32 = 1024;

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
            .value_name("FOLDER")
            .help("Sets texture input folder")
            .takes_value(true)
            .default_value("./textures"))
        .arg(Arg::with_name("output")
            .long("output")
            .short("o")
            .value_name("FILE")
            .help("Sets output file")
            .default_value("rbxlx_out.vmf")
            .takes_value(true))
        .arg(Arg::with_name("texture-output")
            .long("texture-output")
            .value_name("FOLDER")
            .help("Sets texture output folder")
            .default_value("./textures-out")
            .takes_value(true))
        .arg(Arg::with_name("no-textures")
            .long("no-textures")
            .help("disables texture generation")
            .takes_value(false))
        .arg(Arg::with_name("auto-skybox")
            .long("auto-skybox")
            .help("enables automatic skybox (Warning: Results in highly unoptimized map)")
            .takes_value(false))
        .arg(Arg::with_name("optimize")
            .long("optimize")
            .help("enables part-count reduction by joining adjacent parts")
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
    let texture_export_enabled = !matches.is_present("no-textures");

    let auto_skybox_enabled = matches.is_present("auto-skybox");
    let part_optimization_enabled = matches.is_present("optimize");
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
    println!("Auto-skybox [{}]", if auto_skybox_enabled { "ENABLED" } else { "DISABLED" });
    println!("Part-count optimization [{}]", if part_optimization_enabled { "ENABLED" } else { "DISABLED" });
    println!();

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

    print!("Reading input...    ");    // We need to flush print! manually, as it is usually line-buffered.
    std::io::stdout().flush().unwrap_or_default();  // Error discarded; Failed flush causes no problems.
    let mut buffer = String::with_capacity(input.metadata().as_ref().map(Metadata::len).unwrap_or(0) as usize);
    match input.read_to_string(&mut buffer) {
        Ok(_) => println!("DONE"),
        Err(error) => {
            println!("error: Could not read input {}", error);
            std::process::exit(-1)
        }
    }

    print!("Parsing XML...      ");
    std::io::stdout().flush().unwrap_or_default();
    match Document::parse(buffer.as_str()) {
        Ok(document) => {
            let mut parts = Vec::new();
            parse_xml(document.root_element(), &mut parts, false);
            println!("{} parts found!", parts.len());

            if part_optimization_enabled {
                print!("Optimizing...       ");
                std::io::stdout().flush().unwrap_or_default();
                let old_count = parts.len();
                parts = Part::join_adjacent(parts, true);
                println!("\nReduced part count to {} (-{})", parts.len(), old_count - parts.len());
            }

            if parts.len() > MAX_PART_COUNT {
                println!("error: Too many parts, found: {} parts, must be fewer than {}", parts.len(), MAX_PART_COUNT + 1);
                std::process::exit(-1)
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

                print!("Writing VMF...      ");
                std::io::stdout().flush().unwrap_or_default();

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

                if texture_export_enabled {
                    print!("Writing textures...\n");
                    std::io::stdout().flush().unwrap_or_default();

                    let texture_input_folder = Path::new(texture_input);
                    let texture_output_folder = Path::new(texture_output);
                    if let Err(error) = std::fs::create_dir_all(texture_output_folder.join("rbx")) {
                        println!("error: could not create texture output directory {}", error);
                        std::process::exit(-1)
                    }

                    let mut textures_to_copy = HashSet::new();

                    for texture in texture_map.into_iter().filter(RobloxTexture::must_generate) {
                        if let Material::Decal { id, .. } | Material::Texture { id, .. } = texture.material {
                            print!("\tdecal: {}...", id);
                            std::io::stdout().flush().unwrap_or_default();

                            let result: Result<DynamicImage, String> = try {
                                let response = reqwest::blocking::get(format!("https://assetdelivery.roblox.com/v1/assetId/{}", id))
                                    .map_err(|err| format!("{}", err))?
                                    .json::<HashMap<String, serde_json::Value>>()
                                    .map_err(|err| format!("{}", err))?;
                                let location = response.get("location")
                                    .and_then(|value| value.as_str())
                                    .ok_or("No location specified!".to_string())?;

                                let bytes = reqwest::blocking::get(location)
                                    .map_err(|err| format!("{}", err))?
                                    .bytes()
                                    .map_err(|err| format!("{}", err))?;

                                let mut buffer = Vec::with_capacity(bytes.len());   // reqwest supports automatic deflating, but that does not function reliably with the roblox api
                                let bytes = match GzDecoder::new(bytes.as_bytes()).read_to_end(&mut buffer) {
                                    Ok(_) => &buffer[..],
                                    Err(_) => bytes.as_bytes(),
                                };

                                let image = image::load_from_memory_with_format(bytes.as_bytes(), ImageFormat::Png)
                                    .or_else(|_| image::load_from_memory_with_format(bytes.as_bytes(), ImageFormat::Jpeg))
                                    .map_err(|err| {
                                        File::create(format!("./{}-Error.png", id)).unwrap().write_all(bytes.as_bytes()).unwrap();
                                        format!("{}", err)
                                    })?;

                                // Resize image; Source engine only supports power-of-two sized images. Decals are also assumed to have a fixed height
                                let image = image.resize_exact(ROBLOX_DECAL_MAX_WIDTH, ROBLOX_DECAL_MAX_HEIGHT, FilterType::Lanczos3);

                                let mut buf = ImageBuffer::from_pixel(image.width(), image.height(), Rgba([texture.color.red, texture.color.green, texture.color.blue, texture.transparency]));

                                image::imageops::overlay(&mut buf, &image, 0, 0);

                                DynamicImage::ImageRgba8(buf)
                            };
                            match result {
                                Ok(image) => {
                                    let image_out_path = texture_output_folder.join(texture.name())
                                        .with_extension("png");

                                    let width = image.width();
                                    let height = image.height();

                                    match image::save_buffer_with_format(image_out_path, image.into_rgba8().as_bytes(), width, height, ColorType::Rgba8, ImageFormat::Png) {
                                        Ok(_) => println!(" SAVED"),
                                        Err(error) => {
                                            println!("error: could not write texture file {}", error);
                                            std::process::exit(-1)
                                        }
                                    }

                                    let vmt_out_path = texture_output_folder.join(texture.name()).with_extension("vmt");

                                    if let Err(error) = File::create(vmt_out_path).and_then(|mut file| try {
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
                                    }) {
                                        println!("\t\twarning: could not write VMT: {}", error);
                                    }
                                }
                                Err(error) => println!("error loading decal: {}", error),
                            }
                        } else {
                            print!("\ttexture: {}...", texture.name());
                            std::io::stdout().flush().unwrap_or_default();

                            textures_to_copy.insert(format!("{}", texture.material));

                            let vmt_out_path = texture_output_folder.join(texture.name()).with_extension("vmt");

                            if let Err(error) = File::create(vmt_out_path).and_then(|mut file| try {
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
                                println!(" SAVED");
                            }) {
                                println!("\t\twarning: could not write VMT: {}", error);
                            }
                        };
                    }

                    print!("Copying textures...\n");
                    std::io::stdout().flush().unwrap_or_default();
                    for texture in textures_to_copy {
                        print!("\ttexture: {}...", texture);
                        std::io::stdout().flush().unwrap_or_default();
                        let inpath = texture_input_folder.join(format!("{}", texture)).with_extension("png");
                        let outpath = texture_output_folder.join("rbx").join(format!("{}", texture)).with_extension("png");
                        if let Err(error) = std::fs::copy(inpath, outpath) {
                            println!("\t\twarning: could not copy texture file {}: {}", texture, error);
                        } else {
                            println!(" COPIED");
                        }
                    }
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
fn parse_xml<'a>(node: Node<'a, '_>, parts: &mut Vec<Part<'a>>, is_detail: bool) {
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

                let transparency = properties.get_child_with_attribute("float", "name", "Transparency")?
                    .text()?
                    .parse::<f64>()
                    .ok()?;

                let reflectance = properties.get_child_with_attribute("float", "name", "Reflectance")?
                    .text()?
                    .parse::<f64>()
                    .ok()?;

                let material = Material::from_id(
                    properties.get_child_with_attribute("token", "name", "Material")?
                        .text()?
                        .parse::<u32>()
                        .ok()?
                )?;

                // Truss parts do not have a shape field, so this field is not required
                let shape = match properties.get_child_with_attribute("token", "name", "shape")
                    .as_ref()
                    .and_then(Node::text)
                    .and_then(|text| text.parse::<u32>().ok())
                {
                    Some(0) => PartShape::Sphere,
                    Some(2) => PartShape::Cylinder,
                    Some(1) | _ => PartShape::Block,  // Default to block
                };

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
                            Ok(3u8) => Some(Material::Custom { texture: "studs", fill: false, generate: true, size_x: 32.0, size_y: 32.0 }),    // Studs,    TODO: other surfaces
                            Ok(4u8) => Some(Material::Custom { texture: "inlet", fill: false, generate: true, size_x: 32.0, size_y: 32.0 }),    // Inlet,
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
                    .filter_map(|properties| {
                        if let (Some(face), Some(texture)) = (
                            properties.get_child_with_attribute("token", "name", "Face").as_ref().and_then(Node::text).and_then(|text| text.parse::<u8>().ok()),
                            properties.get_child_with_attribute("Content", "name", "Texture").and_then(|node| node.get_child_with_name("url")).as_ref().and_then(Node::text)
                        ) {
                            Some((face, texture))
                        } else {
                            None
                        }
                    })
                    .for_each(|(face, texture)| {
                        if face < 6 {
                            if let Some(id) = texture.split_once("?id=").and_then(|(_, id)| id.parse::<u64>().ok()) {
                                decals[face as usize] = Some(Material::Decal { id, size_x: ROBLOX_DECAL_MAX_WIDTH as f64, size_y: ROBLOX_DECAL_MAX_HEIGHT as f64 })
                            } else {
                                decals[face as usize] = Some(Material::Custom { texture: "decal", fill: false, generate: true, size_x: 32.0, size_y: 32.0 })
                            }
                        }
                    });

                node.children()
                    .filter(|child_node| child_node.tag_name().name() == "Item" && child_node.attribute("class").contains(&"Texture"))
                    .filter_map(|child_node| child_node.get_child_with_name("Properties"))
                    .filter_map(|properties| {
                        if let (Some(face), Some(texture), Some(studs_per_u), Some(studs_per_v), Some(offset_studs_per_u), Some(offset_studs_per_v)) = (
                            properties.get_child_with_attribute("token", "name", "Face").as_ref().and_then(Node::text).and_then(|text| text.parse::<u8>().ok()),
                            properties.get_child_with_attribute("Content", "name", "Texture").and_then(|node| node.get_child_with_name("url")).as_ref().and_then(Node::text),
                            properties.get_child_with_attribute("float", "name", "StudsPerTileU").as_ref().and_then(Node::text).and_then(|text| text.parse::<f64>().ok()),
                            properties.get_child_with_attribute("float", "name", "StudsPerTileV").as_ref().and_then(Node::text).and_then(|text| text.parse::<f64>().ok()),
                            properties.get_child_with_attribute("float", "name", "OffsetStudsU").as_ref().and_then(Node::text).and_then(|text| text.parse::<f64>().ok()),
                            properties.get_child_with_attribute("float", "name", "OffsetStudsV").as_ref().and_then(Node::text).and_then(|text| text.parse::<f64>().ok()),
                        ) {
                            Some((face, texture, studs_per_u.abs(), studs_per_v.abs(), offset_studs_per_u, offset_studs_per_v))
                        } else {
                            None
                        }
                    })
                    .for_each(|(face, texture, studs_per_u, studs_per_v, offset_u, offset_v)| {
                        if face < 6 {
                            if let Some(id) = texture.split_once("?id=").and_then(|(_, id)| id.parse::<u64>().ok()) {
                                decals[face as usize] = Some(Material::Texture { id, size_x: ROBLOX_DECAL_MAX_WIDTH as f64, size_y: ROBLOX_DECAL_MAX_HEIGHT as f64, studs_per_u, studs_per_v, offset_u, offset_v })
                            } else {
                                decals[face as usize] = Some(Material::Custom { texture: "decal", fill: false, generate: true, size_x: 32.0, size_y: 32.0 })
                            }
                        }
                    });

                if class == "SpawnLocation" {
                    decals[DECAL_TOP] = Some(Material::Custom { texture: "spawnlocation", fill: true, generate: true, size_x: 256.0, size_y: 256.0 })
                }

                let part_type = match class {
                    "Part" => PartType::Part,
                    "SpawnLocation" => PartType::SpawnLocation,
                    "TrussPart" => PartType::Truss,
                    _ => unreachable!()
                };

                parts.push(Part {
                    part_type,
                    shape,
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
                    transparency,
                    reflectance,
                    material,
                    decals,
                });
            };
            if option.is_none() {
                println!("Skipping malformed Part: {}-{}", node.range().start, node.range().end)
            }
        }
        Some("Model") => {
            let option: Option<()> = try {
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

                for child in node.children() {
                    parse_xml(child, parts, is_model_detail)
                }
            };
            if option.is_none() {
                println!("Skipping malformed Model: {}-{}", node.range().start, node.range().end)
            }
        }
        _ => {
            for child in node.children() {
                parse_xml(child, parts, is_detail)
            }
        }
    }
}

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
    pub dimension_x: f64,
    pub dimension_y: f64,
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
        format!("rbx/{}_{:x}-{:x}-{:x}-{:x}-{:x}", self.material, self.color.red, self.color.blue, self.color.green, self.transparency, self.reflectance)
    }

    fn scale_x(&self, side: Side) -> f64 {
        match self.scale {
            TextureScale::FILL => (Vector3::from_array(side.plane[2]) - Vector3::from_array(side.plane[1])).magnitude() / self.dimension_x,
            TextureScale::FIXED { scale_x, .. } => scale_x
        }
    }

    fn scale_z(&self, side: Side) -> f64 {
        match self.scale {
            TextureScale::FILL => (Vector3::from_array(side.plane[2]) - Vector3::from_array(side.plane[0])).magnitude() / self.dimension_y,
            TextureScale::FIXED { scale_z, .. } => scale_z
        }
    }

    fn offset_x(&self, side: Side) -> f64 {
        let position = match side.texture_face {
            TextureFace::X_POS => -side.plane[2][1],
            TextureFace::X_NEG => side.plane[2][1],
            TextureFace::Z_POS => -side.plane[2][0],
            TextureFace::Z_NEG => side.plane[2][0],
            TextureFace::Y_POS => -side.plane[2][1],
            TextureFace::Y_NEG => side.plane[2][1]
        };
        (position / self.scale_x(side)) % self.dimension_x
    }

    fn offset_y(&self, side: Side) -> f64 {
        let position = match side.texture_face {
            TextureFace::X_POS => side.plane[2][2],
            TextureFace::X_NEG => side.plane[2][2],
            TextureFace::Z_POS => side.plane[2][2],
            TextureFace::Z_NEG => -side.plane[2][2],
            TextureFace::Y_POS => -side.plane[2][0],
            TextureFace::Y_NEG => -side.plane[2][0]
        };
        (position / self.scale_z(side)) % self.dimension_y
    }
}

/// Decomposes a Roblox part into it's polyhedron faces, and returns them as source engine Sides
fn decompose_part(part: Part, id: &mut u32, map_scale: f64, texture_map: &mut TextureMap<RobloxTexture>) -> Vec<Side> {
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