#![allow(non_snake_case)]
#![feature(try_blocks)]

use std::ffi::{OsStr, OsString};
use std::fs::{File, Metadata};
use std::io::{Read, Write};
use std::path::Path;
use std::process::ExitCode;
use clap::{Arg, ArgAction, Command};
use clap::builder::OsStringValueParser;
use crate::conv::{ConvertOptions, OwnedOrMut, OwnedOrRef};
use crate::rbx::Material;

mod rbx;
mod vmf;
mod conv;

fn main() -> ExitCode {
    let matches = Command::new("RBXLX2VMF")
        .version("1.0")
        .about("Converts Roblox RBXLX files to Valve VMF files.")
        .arg(Arg::new("input")
            .long("input")
            .short('i')
            .value_name("FILE")
            .help("Sets input file")
            .required(true)
            .num_args(1)
            .value_parser(OsStringValueParser::new()))
        .arg(Arg::new("output")
            .long("output")
            .short('o')
            .value_name("FILE")
            .help("Sets output file")
            .default_value("rbxlx_out.vmf")
            .required(false)
            .num_args(1)
            .value_parser(OsStringValueParser::new()))
        .arg(Arg::new("texture-output")
            .long("texture-output")
            .value_name("FOLDER")
            .help("Sets texture output folder")
            .default_value("./textures-out")
            .required(false)
            .num_args(1)
            .value_parser(OsStringValueParser::new()))
        .arg(Arg::new("no-textures")
            .long("no-textures")
            .help("disables texture generation")
            .action(ArgAction::SetTrue))
        .arg(Arg::new("dev-textures")
            .long("dev-textures")
            .help("use developer textures instead of roblox textures")
            .action(ArgAction::SetTrue))
        .arg(Arg::new("auto-skybox")
            .long("auto-skybox")
            .help("enables automatic skybox (Warning: Results in highly unoptimized map)")
            .action(ArgAction::SetTrue))
        .arg(Arg::new("optimize")
            .long("optimize")
            .help("enables part-count reduction by joining adjacent parts")
            .action(ArgAction::SetTrue))
        .arg(Arg::new("skybox-height")
            .long("skybox-height")
            .help("sets additional auto-skybox height clearance")
            .value_parser(|input: &str| input.parse::<f64>())
            .required(false)
            .num_args(1))
        .arg(Arg::new("map-scale")
            .long("map-scale")
            .help("sets map scale")
            .default_value("15")
            .value_parser(|input: &str| input.parse::<f64>())
            .required(false)
            .num_args(1))
        .arg(Arg::new("decal-size")
            .long("decal-size")
            .help("sets downloaded decal texture size")
            .value_parser(|input: &str| input.parse::<u64>())
            .required(false)
            .default_value("256")
            .num_args(1))
        .arg(Arg::new("game")
            .long("game")
            .short('g')
            .help("sets target source engine game")
            .required(true)
            .value_parser(["css", "csgo", "gmod", "hl2", "hl2e1", "hl2e2", "hl", "hls", "l4d", "l4d2", "portal2", "portal", "tf2"])
            .num_args(1)
        )
        .get_matches();

    let exit_code = async_std::task::block_on(
        conv::convert(CLIConvertOptions {
            input_name: &matches.get_one::<OsString>("input").unwrap().as_os_str().to_string_lossy(),
            input_path: matches.get_one::<OsString>("input").unwrap(),
            output_path: matches.get_one::<OsString>("output").unwrap(),
            texture_output_folder: {
                let texture_folder = matches.get_one::<OsString>("texture-output").unwrap();
                if let Err(error) = std::fs::create_dir_all(Path::new(texture_folder).join("rbx")) {
                    println!("error: could not create texture output directory {}", error);
                    std::process::exit(-1)
                }
                texture_folder
            },
            is_texture_output_enabled: !matches.get_one("no-textures").unwrap_or(&false),
            use_developer_textures: *matches.get_one("dev-textures").unwrap_or(&false),
            map_scale: *matches.get_one("map-scale").unwrap(),
            auto_skybox_enabled: *matches.get_one("auto-skybox").unwrap_or(&false),
            skybox_clearance: *matches.get_one("skybox-height").unwrap_or(&0f64),
            optimization_enabled: *matches.get_one("optimize").unwrap_or(&false),
            decal_size: *matches.get_one("decal-size").unwrap(),
            skybox_name: match matches.get_one::<String>("game").unwrap().as_str() {
                "css" => "sky_day01_05",
                "csgo" => "sky_day02_05",
                "gmod" => "painted",
                "hl2" => "sky_day01_04",
                "hl2e1" => "sky_ep01_01",
                "hl2e2" => "sky_ep02_01_hdr",
                "hl" => "city",
                "hls" => "sky_wasteland02",
                "l4d" => "river_hdr",
                "l4d2" => "sky_l4d_c1_2_hdr",
                "portal2" => "sky_day01_01",
                "portal" => "sky_day01_05_hdr",
                "tf2" => "sky_day01_01",
                _ => "default_skybox_fixme" // The only guard against invalid values here is HTML form validation, but as we're a clientside application, just substitute in a placeholder value
            }
        })
    );

    return match exit_code {
        Ok(code) => ExitCode::from(code),
        // Error writing to STDIO
        Err(error) => {
            eprintln!("{}", error);
            ExitCode::FAILURE
        }
    }
}

struct CLIConvertOptions<'a> {
    input_name: &'a str,
    input_path: &'a OsStr,
    output_path: &'a OsStr,
    texture_output_folder: &'a OsStr,
    is_texture_output_enabled: bool,
    use_developer_textures: bool,
    map_scale: f64,
    auto_skybox_enabled: bool,
    skybox_clearance: f64,
    optimization_enabled: bool,
    decal_size: u64,
    skybox_name: &'a str
}

impl<'a> ConvertOptions<File> for CLIConvertOptions<'a> {
    fn print_output(&self) -> Box<dyn Write> {
        Box::new(std::io::stdout())
    }
    fn error_output(&self) -> Box<dyn Write> {
        Box::new(std::io::stderr())
    }

    fn input_name(&self) -> &str {
        &self.input_name
    }

    fn read_input_data(&self) ->  OwnedOrRef<'_, String> {
        let mut file = match File::open(self.input_path) {
            Ok(file) => file,
            Err(error) => {
                println!("error: Could not open input file: {}", error);
                std::process::exit(-1)
            }
        };
        let mut buffer = String::with_capacity(file.metadata().as_ref().map(Metadata::len).unwrap_or(0) as usize);
        match file.read_to_string(&mut buffer) {
            Ok(_) => {}
            Err(error) => {
                println!("error: Could not read input {}", error);
                std::process::exit(-1)
            }
        }
        OwnedOrRef::Owned(buffer)
    }

    fn vmf_output(&mut self) -> OwnedOrMut<'_, File> {
        match File::create(self.output_path) {
            Ok(file) => OwnedOrMut::Owned(file),
            Err(error) => {
                println!("error: Could not create output file {}", error);
                std::process::exit(-1)
            }
        }
    }

    async fn texture_input(&mut self, texture: Material) -> Option<Result<Vec<u8>, String>> {
        Some(Ok(Vec::from(
            match texture {
                Material::Plastic => crate::rbx::textures::PLASTIC,
                Material::Wood => crate::rbx::textures::WOOD,
                Material::Slate => crate::rbx::textures::SLATE,
                Material::Concrete => crate::rbx::textures::CONCRETE,
                Material::CorrodedMetal => crate::rbx::textures::RUST,
                Material::DiamondPlate => crate::rbx::textures::DIAMONDPLATE,
                Material::Foil => crate::rbx::textures::ALUMINIUM,
                Material::Grass => crate::rbx::textures::GRASS,
                Material::Ice => crate::rbx::textures::ICE,
                Material::Marble => crate::rbx::textures::MARBLE,
                Material::Granite => crate::rbx::textures::GRANITE,
                Material::Brick => crate::rbx::textures::BRICK,
                Material::Pebble => crate::rbx::textures::PEBBLE,
                Material::Sand => crate::rbx::textures::SAND,
                Material::Fabric => crate::rbx::textures::FABRIC,
                Material::SmoothPlastic => crate::rbx::textures::SMOOTHPLASTIC,
                Material::Metal => crate::rbx::textures::METAL,
                Material::WoodPlanks => crate::rbx::textures::WOODPLANKS,
                Material::Cobblestone => crate::rbx::textures::COBBLESTONE,
                Material::Glass => crate::rbx::textures::GLASS,
                Material::ForceField => crate::rbx::textures::FORCEFIELD,
                Material::Custom { texture: "decal", .. } => crate::rbx::textures::DECAL,
                Material::Custom { texture: "studs", .. } => crate::rbx::textures::STUDS,
                Material::Custom { texture: "inlet", .. } => crate::rbx::textures::INLET,
                Material::Custom { texture: "spawnlocation", .. } => crate::rbx::textures::SPAWNLOCATION,
                Material::Custom { .. } | Material::Decal { .. } | Material::Texture { .. } => return None,
            }
        )))
    }

    fn texture_output(&mut self, path: &str) -> OwnedOrMut<'_, File> {
        let texture_out_path = Path::new(self.texture_output_folder).join(path);
        match File::create(texture_out_path) {
            Ok(file) => OwnedOrMut::Owned(file),
            Err(error) => {
                println!("error: Could not create file {}", error);
                std::process::exit(-1)
            }
        }
    }

    fn texture_output_enabled(&self) -> bool {
        self.is_texture_output_enabled
    }

    fn use_dev_textures(&self) -> bool {
        self.use_developer_textures
    }

    fn map_scale(&self) -> f64 {
        self.map_scale
    }

    fn auto_skybox_enabled(&self) -> bool {
        self.auto_skybox_enabled
    }

    fn skybox_clearance(&self) -> f64 {
        self.skybox_clearance
    }

    fn optimization_enabled(&self) -> bool {
        self.optimization_enabled
    }

    fn decal_size(&self) -> u64 {
        self.decal_size
    }

    fn skybox_name(&self) -> &str {
        self.skybox_name
    }

    fn web_origin(&self) -> &str {
        ""  // Unused in CLI version; TODO: Remove when async-trait functions are available.
    }
}