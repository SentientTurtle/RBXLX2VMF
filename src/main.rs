#![allow(non_snake_case)]
#![feature(try_blocks)]
#![feature(option_result_contains)]

use std::collections::HashSet;
use clap::{App, Arg};
use std::fs::{File, Metadata};
use std::io::{Read, Write};
use crate::rbx::{Part, CFrame, Vector3, BoundingBox, Material, Color3, PartType, PartShape};
use roxmltree::{Document};
use crate::vmf::{VMFBuilder, Solid, Side, TextureFace, TextureMap, VMFTexture, Displacement};
use std::path::Path;
use image::{EncodableLayout, GenericImageView, ColorType, ImageFormat};
use crate::conv::texture::{RobloxTexture, TextureScale};

mod rbx;
mod vmf;
mod conv;

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
            conv::parse::parse_xml(document.root_element(), &mut parts, false);
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
                            sides: conv::decompose_part(*part, &mut side_id, map_scale, &mut texture_map),
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
                                sides: conv::decompose_part(*part, &mut side_id, map_scale, &mut texture_map),
                            }
                        )
                    })
                    .for_each(|s| detail_solids.push(s));

                if auto_skybox_enabled {
                    bounding_box.y_max += skybox_height_clearance;
                    world_solids.extend(conv::generate_skybox(&mut part_id, &mut side_id, bounding_box, map_scale, &mut texture_map));
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
                            match conv::texture::fetch_texture(id, texture, ROBLOX_DECAL_MAX_WIDTH, ROBLOX_DECAL_MAX_HEIGHT) {
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