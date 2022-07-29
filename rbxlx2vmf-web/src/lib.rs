extern crate wee_alloc;
extern crate wasm_bindgen;

use std::io::Cursor;
use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;
use zip::write::FileOptions;
use zip::ZipWriter;
use rbxlx2vmf::conv;
use rbxlx2vmf::conv::{ConvertOptions, OwnedOrMut, OwnedOrRef};

// Use `wee_alloc` as the global allocator
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

struct JSConvertOptions<'a> {
    input_name: &'a str,
    input_data: String,
    zip_writer: ZipWriter<Cursor<&'a mut Vec<u8>>>,
    is_texture_output_enabled: bool,
    use_developer_textures: bool,
    map_scale: f64,
    auto_skybox_enabled: bool,
    skybox_clearance: f64,
    optimization_enabled: bool,
    decal_size: u64,
    skybox_name: &'a str
}

impl<'a> ConvertOptions<&'a [u8], ZipWriter<Cursor<&'a mut Vec<u8>>>> for JSConvertOptions<'a> {
    fn input_name(&self) -> &str {
        &self.input_name
    }

    fn read_input_data(&self) -> OwnedOrRef<'_, String> {
        OwnedOrRef::Ref(&self.input_data)
    }

    fn vmf_output(&mut self) -> OwnedOrMut<'_, ZipWriter<Cursor<&'a mut Vec<u8>>>> {
        self.zip_writer.start_file("map.vmf", FileOptions::default()).unwrap();
        OwnedOrMut::Ref(&mut self.zip_writer)
    }

    fn texture_input(&mut self, _texture: rbxlx2vmf::rbx::Material) -> Option<OwnedOrMut<'_, &'a [u8]>> {
        None    // TODO: Rust does not yet support async trait functions, so this implementation has been moved into ::conv
    }

    fn texture_output(&mut self, path: &str) -> OwnedOrMut<'_, ZipWriter<Cursor<&'a mut Vec<u8>>>> {
        self.zip_writer.start_file(path, FileOptions::default()).unwrap();
        OwnedOrMut::Ref(&mut self.zip_writer)
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
}

#[wasm_bindgen]
extern "C" {
    // Use `js_namespace` here to bind `console.log(..)` instead of just
    // `log(..)`
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

#[wasm_bindgen]
pub async fn convert_map(
    input_name: String,
    input_data: String,
    is_texture_output_enabled: bool,
    use_developer_textures: bool,
    map_scale: f64,
    auto_skybox_enabled: bool,
    skybox_clearance: f64,
    optimization_enabled: bool,
    skyname: String
) -> Uint8Array {
    let mut zip_buffer = Vec::new();
    let zip_writer = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_buffer));

    log("Starting conversion...");
    conv::convert(JSConvertOptions {
        input_name: &*input_name,
        input_data,
        zip_writer,
        is_texture_output_enabled,
        use_developer_textures,
        map_scale,
        auto_skybox_enabled,
        skybox_clearance,
        optimization_enabled,
        decal_size: 256,
        skybox_name: match &*skyname {
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
            _ => "default_skybox_fixme"
        }
    }).await;
    log("Conversion complete...");
    js_sys::Uint8Array::from(&*zip_buffer)
}