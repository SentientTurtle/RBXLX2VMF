# Roblox to Source Engine map converter

Converts Roblox XML-format maps (*.rbxlx) to source engine (*.vmf) maps.

[Web App version found here](https://sentientturtle.github.io/rbxlx2vmf.html)  
Note: The web-app runs entirely in your local browser, performance is dependant on your device.

Feel free to open issues/discussions for feature requests or other improvements.

### Recommended process

1. Design or open Roblox map
2. Add StringValue with name or value 'func_detail' to detail models (Note: Nested models are also marked detail)
3. Save map in XML (*.xbxlx) format
4. Run conversion tool
5. Convert the generated texture PNGs to Valve VTF format.
6. Move VTF and VMT texture files to game material folder

**What you get:**

* Part geometry converted to brushes.
* Basic support for func_detail and compiling before the heat-death of the universe
* Textures (VMT + PNG)
* (Optional) Basic optimization by joining adjacent parts
* (Optional) bounding box skybox

(Note: No support for Meshes or terrain. Cylindrical and truss parts get converted into cuboid brushes. Spherical parts get converted into displacements)

## Command-line options

| Option                    | Explanation                                                                                                                                               |
|---------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------|
| -i --input <FILE>         | Input RBXLX file to convert                                                                                                                               |
| -o --output <FILE>        | (optional) Output file, default: "./rbxlx_out.vmf"                                                                                                        |
| --texture-output <FOLDER> | (optional) Texture output folder, default: "./textures-out/"                                                                                              |
| --dev-textures            | (optional) Use source engine developer textures instead of generating map textures                                                                        
| --map-scale <scale>       | (optional) Scale conversion from Roblox studs to Source Engine Hammer Units, default: 15.0 HU/stud                                                        |
| --no-textures             | Disables texture generation & output                                                                                                                      |
| --auto-skybox             | Include automatically generated skybox                                                                                                                    |
| --skybox-height <height>  | Adds margin space between the top of the map and the skybox, height in Roblox studs                                                                       |
| --optimize                | Enables part-count optimization by joining identical adjecent parts into a single map brush<br/>**WARNING: This may take a very long time on large maps** |
| -g --game <GAME>          | Selects which version of source engine to generate map for                                                                                                |


## Building

### CLI Version

* Run `cargo build --release` in top level project directory

### Webassembly Version

* Run `build.bat` or `build.sh` shell script with 'rbxlx2vmf-web' as current-directory.

Or manually, in 'rbxlx2vmf-web' directory:

1. Run `cargo build --target wasm32-unknown-unknown --release`
2. Run `wasm-bindgen --target web --no-typescript --out-dir . "./target/wasm32-unknown-unknown/release/rbxlx2vmf_web.wasm"`
3. Run `wasm-gc "./rbxlx2vmf_web_bg.wasm"`
4. Move `"./rbxlx2vmf_web_bg.wasm"` to `"./html/rbxlx2vmf_web_bg.wasm"`
4. Move `"./rbxlx2vmf_web.js"` to `"./html/rbxlx2vmf_web.js"`
