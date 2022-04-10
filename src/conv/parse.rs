use roxmltree::Node;
use crate::rbx::{Part, Color3, PartShape, Material, PartType, Vector3, CFrame};

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
pub fn parse_xml<'a>(node: Node<'a, '_>, parts: &mut Vec<Part<'a>>, is_detail: bool, decal_size: u64) {
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
                            Ok(3u8) => Some(Material::Custom { texture: "studs", fill: false, generate: true, size_x: 32, size_y: 32 }),    // Studs,    TODO: other surfaces
                            Ok(4u8) => Some(Material::Custom { texture: "inlet", fill: false, generate: true, size_x: 32, size_y: 32 }),    // Inlet,
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
                                decals[face as usize] = Some(Material::Decal { id, size_x: decal_size, size_y: decal_size })
                            } else {
                                decals[face as usize] = Some(Material::Custom { texture: "decal", fill: false, generate: true, size_x: 32, size_y: 32 })
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
                                decals[face as usize] = Some(Material::Texture { id, size_x: decal_size, size_y: decal_size, studs_per_u, studs_per_v, offset_u, offset_v })
                            } else {
                                decals[face as usize] = Some(Material::Custom { texture: "decal", fill: false, generate: true, size_x: 32, size_y: 32 })
                            }
                        }
                    });

                if class == "SpawnLocation" {
                    decals[DECAL_TOP] = Some(Material::Custom { texture: "spawnlocation", fill: true, generate: true, size_x: 256, size_y: 256 })
                }

                let part_type = match class {
                    "Part" => PartType::Part,
                    "SpawnLocation" => PartType::SpawnLocation,
                    "TrussPart" => PartType::Truss,
                    _ => unreachable!() // We match on class earlier, and only permit the above three options
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
                    parse_xml(child, parts, is_model_detail, decal_size)
                }
            };
            if option.is_none() {
                println!("Skipping malformed Model: {}-{}", node.range().start, node.range().end)
            }
        }
        _ => {
            for child in node.children() {
                parse_xml(child, parts, is_detail, decal_size)
            }
        }
    }
}
