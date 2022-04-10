#![allow(non_snake_case)]
#![feature(try_blocks)]
#![feature(option_result_contains)]

use std::ffi::OsStr;
use std::fs::{File, Metadata};
use std::io::Read;
use std::path::Path;
use clap::{App, Arg};
use crate::conv::ConvertOptions;

mod rbx;
mod vmf;
mod conv;

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
        .arg(Arg::with_name("decal-size")
            .long("decal-size")
            .help("sets downloaded decal texture size")
            .default_value("256")
            .takes_value(true))
        .get_matches();

    conv::convert(CLIConvertOptions {
        input_name: matches.value_of("input").unwrap(),
        input_path: matches.value_of_os("input").unwrap(),
        output_path: matches.value_of_os("output").unwrap(),
        texture_output_folder: {
            let texture_folder = matches.value_of_os("texture-output").unwrap();
            if let Err(error) = std::fs::create_dir_all(Path::new(texture_folder).join("rbx")) {
                println!("error: could not create texture output directory {}", error);
                std::process::exit(-1)
            }
            texture_folder
        },
        is_texture_output_enabled: !matches.is_present("no-textures"),
        map_scale: match matches.value_of("map-scale").unwrap().parse() {
            Ok(f) => f,
            Err(_) => {
                println!("error: invalid map scale");
                std::process::exit(-1)
            }
        },
        auto_skybox_enabled: matches.is_present("auto-skybox"),
        skybox_clearance: matches.value_of("skybox-height").map(str::parse).and_then(Result::ok).unwrap_or(0f64),
        optimization_enabled: matches.is_present("optimize"),
        decal_size: match matches.value_of("decal-size").unwrap().parse() {
            Ok(size) => size,
            Err(_) => {
                println!("error: invalid decal size");
                std::process::exit(-1)
            }
        },
    });
}

struct CLIConvertOptions<'a> {
    input_name: &'a str,
    input_path: &'a OsStr,
    output_path: &'a OsStr,
    texture_output_folder: &'a OsStr,
    is_texture_output_enabled: bool,
    map_scale: f64,
    auto_skybox_enabled: bool,
    skybox_clearance: f64,
    optimization_enabled: bool,
    decal_size: u64
}

impl<'a> ConvertOptions<File> for CLIConvertOptions<'a> {
    fn input_name(&self) -> &str {
        &self.input_name
    }

    fn read_input_data(&self) -> String {
        let mut file = match File::open(self.input_path) {
            Ok(file) => file,
            Err(error) => {
                println!("error: Could not open input file: {}", error);
                std::process::exit(-1)
            }
        };
        let mut buffer = String::with_capacity(file.metadata().as_ref().map(Metadata::len).unwrap_or(0) as usize);
        match file.read_to_string(&mut buffer) {
            Ok(_) => {},
            Err(error) => {
                println!("error: Could not read input {}", error);
                std::process::exit(-1)
            }
        }
        buffer
    }

    fn vmf_output(&self) -> File {
        match File::create(self.output_path) {
            Ok(file) => file,
            Err(error) => {
                println!("error: Could not create output file {}", error);
                std::process::exit(-1)
            }
        }
    }

    fn texture_output(&self, path: &str) -> File {
        let texture_out_path = Path::new(self.texture_output_folder).join(path);
        match File::create(texture_out_path) {
            Ok(file) => file,
            Err(error) => {
                println!("error: Could not create file {}", error);
                std::process::exit(-1)
            }
        }
    }

    fn texture_output_enabled(&self) -> bool {
        self.is_texture_output_enabled
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
}