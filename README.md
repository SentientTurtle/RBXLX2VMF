# Roblox to Source Engine map converter


Converts Roblox XML-format maps (*.rbxlx) to source engine (*.vmf) maps.

## Usage

| Option                        | Explanation                                                                                    |
|-------------------------------|------------------------------------------------------------------------------------------------|
| -i --input <FILE>             | Input RBXLX file to convert                                                                    |
| -ti --texture-input <FOLDER>  | Texture input folder                                                                           |
| -o --output <FILE>            | (optional) Output file, default: "./rbxlx_out.vmf"                                             |
| -to --texture-output <FOLDER> | (optional) Texture output folder, default: "./textures-out/"                                   |
| --auto-skybox                 | (optional) Include automatically generated skybox                                              |
| --skybox-height <height>      | (optional) Adds margin space between the top of the map and the skybox, height in Roblox studs |
| --map-scale <scale>           | (optional) Scale conversion from Roblox studs to Source Engine Hammer Units, default: 15.0     |

### Recommended process

1. Design or open Roblox map
2. Add StringValue with name or value 'func_detail' to detail models (Note: Child-models are also marked detail)
3. Save map in XML (*.xbxlx) format
4. Run conversion tool

Convert textures
5. Convert the generated texture PNGs to Valve VTF format. (VTFEdit provides bulk 'Convert Folder' functionality)
6. Move VTF and VMT texture files to game material folder

Correct texture alignment
8. Open generated VMF file in Hammer
9. Select all brushes
10. Open Hammer Texture Application
11. Align to T(op) and R(ight)
