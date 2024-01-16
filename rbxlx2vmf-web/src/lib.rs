extern crate wee_alloc;
extern crate wasm_bindgen;

use std::rc::Rc;
use std::cell::RefCell;
use std::fmt::Arguments;
use std::io::{Cursor, IoSlice, Write};
use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;
use zip::write::FileOptions;
use zip::ZipWriter;
use rbxlx2vmf::conv;
use rbxlx2vmf::conv::{ConvertOptions, OwnedOrMut, OwnedOrRef};
use rbxlx2vmf::rbx::Material;

// Use `wee_alloc` as the global allocator
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);

    fn html_log(s: &str);
    fn html_log_error(s: &str);

    fn alert(s: &str);
}

struct WebLogger {
    pub buffer: Rc<RefCell<Vec<u8>>>,
    pub log_target: fn(&str),
    pub clear_buffer: bool,
    pub write_on_drop: bool
}

impl Write for WebLogger {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        RefCell::borrow_mut(&self.buffer).write(buf)
    }

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> std::io::Result<usize> {
        RefCell::borrow_mut(&self.buffer).write_vectored(bufs)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let mut buffer = RefCell::borrow_mut(&self.buffer);
        let string = String::from_utf8_lossy(&*buffer);
        (self.log_target)(string.as_ref());
        if self.clear_buffer { buffer.clear(); }
        Ok(())
    }

    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        RefCell::borrow_mut(&self.buffer).write_all(buf)
    }

    fn write_fmt(&mut self, fmt: Arguments<'_>) -> std::io::Result<()> {
        RefCell::borrow_mut(&self.buffer).write_fmt(fmt)
    }
}

impl Drop for WebLogger {
    fn drop(&mut self) {
        if self.write_on_drop {
            let buffer = RefCell::borrow_mut(&self.buffer);
            let string = String::from_utf8_lossy(&*buffer);
            (self.log_target)(string.as_ref())
        }
    }
}

struct JSConvertOptions<'a> {
    print_buffer: Rc<RefCell<Vec<u8>>>,
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
    skybox_name: &'a str,
    web_origin: &'a str
}

impl<'a> ConvertOptions<ZipWriter<Cursor<&'a mut Vec<u8>>>> for JSConvertOptions<'a> {
    fn print_output(&self) -> Box<dyn std::io::Write> {
        Box::new(WebLogger { buffer: self.print_buffer.clone(), log_target: html_log, clear_buffer: false, write_on_drop: true })
    }
    fn error_output(&self) -> Box<dyn std::io::Write> {
        Box::new(WebLogger { buffer: self.print_buffer.clone(), log_target: html_log_error, clear_buffer: false, write_on_drop: false })
    }

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

    async fn texture_input(&mut self, texture: Material) -> Option<Result<Vec<u8>, String>> {
        let path = format!("{}/textures/{}.png", self.web_origin(), texture);
        let http_client = reqwest::Client::new();

        match http_client.get(path).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    match response.bytes().await {
                        Ok(bytes) => Some(Ok(bytes.to_vec())),
                        Err(error) => Some(Err(format!(" FAILED ({})", error))),
                    }
                } else {
                    // TODO: Maybe return None and skip texture generation for HTTP 404?
                    Some(Err(format!(" FAILED (HTTP {})", response.status())))
                }
            }
            Err(error) => Some(Err(format!(" FAILED ({})", error)))
        }
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

    fn web_origin(&self) -> &str {
        self.web_origin
    }
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
    skyname: String,
    web_origin: String
) -> Result<Uint8Array, JsValue> {
    let mut zip_buffer = Vec::new();
    let zip_writer = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_buffer));

    log("Starting conversion...");
    let result = conv::convert(JSConvertOptions {
        print_buffer: Rc::new(RefCell::new(Vec::new())),
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
            _ => "default_skybox_fixme" // The only guard against invalid values here is HTML form validation, but as we're a clientside application, just substitute in a placeholder value
        },
        web_origin: &web_origin
    }).await;
    match result {
        Ok(0) => {
            log("Conversion complete...");
            Ok(js_sys::Uint8Array::from(&*zip_buffer))
        },
        Ok(_) => {
            alert("Conversion failed, see log");
            Err(JsValue::from("Conversion failed, see log"))
        },
        Err(error) => {
            let message = format!("Conversion failed: {}", error);
            alert(&*message);
            Err(JsValue::from(&*message))
        }
    }

}