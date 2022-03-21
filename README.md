# Roblox to Source Engine map converter


Converts Roblox XML-format maps (*.rbxlx) to source engine (*.vmf) maps.

## Usage

| Option                    | Explanation                                                                                                                                               |
|---------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------|
| -i --input <FILE>         | Input RBXLX file to convert                                                                                                                               |
| -o --output <FILE>        | (optional) Output file, default: "./rbxlx_out.vmf"                                                                                                        |
| --texture-input <FOLDER>  | (optional) Texture input folder, default: "./textures"                                                                                                    |
| --texture-output <FOLDER> | (optional) Texture output folder, default: "./textures-out/"                                                                                              |
| --map-scale <scale>       | (optional) Scale conversion from Roblox studs to Source Engine Hammer Units, default: 15.0 HU/stud                                                        |
| --no-textures             | Disables texture generation & output                                                                                                                      |
| --auto-skybox             | Include automatically generated skybox                                                                                                                    |
| --skybox-height <height>  | Adds margin space between the top of the map and the skybox, height in Roblox studs                                                                       |
| --optimize                | Enables part-count optimization by joining identical adjecent parts into a single map brush<br/>**WARNING: This may take a very long time on large maps** |

### Recommended process

1. Design or open Roblox map
2. Add StringValue with name or value 'func_detail' to detail models (Note: Child-models are also marked detail)
3. Save map in XML (*.xbxlx) format
4. Run conversion tool
5. Convert the generated texture PNGs to Valve VTF format. (The VTFEdit tool provides bulk 'Convert Folder' functionality)
6. Move VTF and VMT texture files to game material folder