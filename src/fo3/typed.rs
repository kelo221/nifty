use super::{
    decode_physics_block, CollisionObject, Document, Fo3Error, PackedTriStripsData, PhysicsShape,
    RawBlock, Reader, RigidBody, SimpleShapePhantom,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyType {
    Linear,
    Quadratic,
    Tbc,
    XyzRotation,
    Const,
    Unknown(u32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnimationKey<T> {
    pub time: f32,
    pub value: T,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnimationKeyGroup<T> {
    pub interpolation: Option<KeyType>,
    pub keys: Vec<AnimationKey<T>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransformData {
    pub rotations: Vec<AnimationKey<[f32; 4]>>,
    pub xyz_rotations: Option<[AnimationKeyGroup<f32>; 3]>,
    pub translations: AnimationKeyGroup<[f32; 3]>,
    pub scales: AnimationKeyGroup<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransformInterpolator {
    pub transform: Transform,
    pub data: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransformController {
    pub target: i32,
    pub interpolator: i32,
    pub start_time: f32,
    pub stop_time: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ControllerManager {
    pub sequences: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ControlledBlock {
    pub interpolator: i32,
    pub controller: i32,
    pub node_name: String,
    pub property_type: String,
    pub controller_type: String,
    pub controller_id: String,
    pub interpolator_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ControllerSequence {
    pub name: String,
    pub start_time: f32,
    pub stop_time: f32,
    pub controlled_blocks: Vec<ControlledBlock>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub translation: [f32; 3],
    /// Row-major NIF rotation matrix.
    pub rotation: [f32; 9],
    pub scale: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectNet {
    pub name: Option<String>,
    pub extra_data: Vec<i32>,
    pub controller: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AvObject {
    pub object: ObjectNet,
    pub flags: u32,
    pub transform: Transform,
    pub properties: Vec<i32>,
    pub collision_object: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub base: AvObject,
    pub children: Vec<i32>,
    pub effects: Vec<i32>,
    pub ordered_alpha_sort_bound: Option<[f32; 4]>,
    pub ordered_static_bound: Option<bool>,
    pub range: Option<[u8; 3]>,
    pub billboard_mode: Option<u16>,
    pub switch: Option<(u16, u32)>,
    pub lod_data: Option<i32>,
    pub value: Option<(u32, u8)>,
    pub multi_bound: Option<i32>,
    pub sorting_mode: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MaterialData {
    pub names: Vec<Option<String>>,
    pub extra_data: Vec<i32>,
    pub active: i32,
    pub needs_update: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Geometry {
    pub base: AvObject,
    pub data: i32,
    pub skin_instance: i32,
    pub materials: MaterialData,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeometryData {
    pub group_id: i32,
    pub vertices: Vec<[f32; 3]>,
    pub keep_flags: u8,
    pub compress_flags: u8,
    pub data_flags: u16,
    pub normals: Vec<[f32; 3]>,
    pub tangents: Vec<[f32; 3]>,
    pub bitangents: Vec<[f32; 3]>,
    pub bound_center: [f32; 3],
    pub bound_radius: f32,
    pub colors: Vec<[f32; 4]>,
    pub uv_sets: Vec<Vec<[f32; 2]>>,
    pub consistency_flags: u16,
    pub additional_data: i32,
    pub triangle_count: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TriStripsData {
    pub geometry: GeometryData,
    pub strips: Vec<Vec<u16>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TriShapeData {
    pub geometry: GeometryData,
    pub triangles: Vec<[u16; 3]>,
    pub match_groups: Vec<Vec<u16>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MaterialProperty {
    pub object: ObjectNet,
    pub ambient: Option<[f32; 3]>,
    pub diffuse: Option<[f32; 3]>,
    pub specular: [f32; 3],
    pub emissive: [f32; 3],
    pub glossiness: f32,
    pub alpha: f32,
    pub emissive_multiplier: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlphaProperty {
    pub object: ObjectNet,
    pub flags: u16,
    pub threshold: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShaderProperty {
    pub object: ObjectNet,
    pub shade_flags: u16,
    pub shader_type: u32,
    pub shader_flags: u32,
    pub shader_flags_2: u32,
    pub environment_map_scale: f32,
    pub texture_clamp_mode: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PpLightingProperty {
    pub base: ShaderProperty,
    pub texture_set: i32,
    pub refraction_strength: Option<f32>,
    pub refraction_fire_period: Option<i32>,
    pub parallax_max_passes: Option<f32>,
    pub parallax_scale: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NoLightingProperty {
    pub base: ShaderProperty,
    pub file_name: String,
    pub falloff: Option<[f32; 4]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShaderTextureSet {
    pub textures: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypedBlock {
    Node(Node),
    Geometry(Geometry),
    TriStripsData(TriStripsData),
    TriShapeData(TriShapeData),
    MaterialProperty(MaterialProperty),
    AlphaProperty(AlphaProperty),
    PpLightingProperty(PpLightingProperty),
    NoLightingProperty(NoLightingProperty),
    ShaderTextureSet(ShaderTextureSet),
    CollisionObject(CollisionObject),
    RigidBody(RigidBody),
    SimpleShapePhantom(SimpleShapePhantom),
    PhysicsShape(PhysicsShape),
    PackedTriStripsData(PackedTriStripsData),
    TransformController(TransformController),
    TransformInterpolator(TransformInterpolator),
    TransformData(TransformData),
    ControllerManager(ControllerManager),
    ControllerSequence(ControllerSequence),
    Unsupported,
}

impl Document {
    pub fn decode_block(&self, index: usize) -> Result<TypedBlock, Fo3Error> {
        let block = self
            .blocks
            .get(index)
            .ok_or(Fo3Error::InvalidRootReference {
                root_index: 0,
                block_index: index as i32,
                block_count: self.blocks.len(),
            })?;
        let mut reader = Reader::new(&block.bytes);
        let decoded = if let Some(decoded) = decode_physics_block(self, block, &mut reader) {
            decoded?
        } else {
            match block.type_name.as_str() {
                "NiNode" | "NiBillboardNode" | "NiSwitchNode" | "NiLODNode"
                | "NiSortAdjustNode" | "BSFadeNode" | "BSOrderedNode" | "BSRangeNode"
                | "BSBlastNode" | "BSDamageStage" | "BSValueNode" | "BSMultiBoundNode" => {
                    TypedBlock::Node(parse_node(self, block, &mut reader)?)
                }
                "NiTriStrips" | "NiTriShape" | "BSSegmentedTriShape" => {
                    TypedBlock::Geometry(parse_geometry(self, block, &mut reader)?)
                }
                "NiTriStripsData" => {
                    TypedBlock::TriStripsData(parse_tri_strips_data(block, &mut reader)?)
                }
                "NiTriShapeData" => {
                    TypedBlock::TriShapeData(parse_tri_shape_data(block, &mut reader)?)
                }
                "NiTransformController" => {
                    TypedBlock::TransformController(parse_transform_controller(&mut reader)?)
                }
                "NiTransformInterpolator" | "BSRotAccumTransfInterpolator" => {
                    TypedBlock::TransformInterpolator(parse_transform_interpolator(&mut reader)?)
                }
                "NiTransformData" | "NiKeyframeData" => {
                    TypedBlock::TransformData(parse_transform_data(block, &mut reader)?)
                }
                "NiControllerManager" => {
                    TypedBlock::ControllerManager(parse_controller_manager(block, &mut reader)?)
                }
                "NiControllerSequence" => TypedBlock::ControllerSequence(
                    parse_controller_sequence(self, block, &mut reader)?,
                ),
                "NiMaterialProperty" => {
                    TypedBlock::MaterialProperty(parse_material_property(self, block, &mut reader)?)
                }
                "NiAlphaProperty" => {
                    TypedBlock::AlphaProperty(parse_alpha_property(self, block, &mut reader)?)
                }
                "BSShaderPPLightingProperty" => TypedBlock::PpLightingProperty(
                    parse_pp_lighting_property(self, block, &mut reader)?,
                ),
                "BSShaderNoLightingProperty" => TypedBlock::NoLightingProperty(
                    parse_no_lighting_property(self, block, &mut reader)?,
                ),
                "BSShaderTextureSet" => {
                    TypedBlock::ShaderTextureSet(parse_texture_set(block, &mut reader)?)
                }
                _ => return Ok(TypedBlock::Unsupported),
            }
        };
        if reader.remaining() != 0 {
            return Err(Fo3Error::UnparsedBlockBytes {
                block: index,
                type_name: block.type_name.clone(),
                remaining: reader.remaining(),
            });
        }
        Ok(decoded)
    }

    pub fn decode_supported_blocks(&self) -> Result<Vec<TypedBlock>, Fo3Error> {
        (0..self.blocks.len())
            .map(|index| self.decode_block(index))
            .collect()
    }
}

fn parse_object_net(
    document: &Document,
    block: &RawBlock,
    reader: &mut Reader<'_>,
) -> Result<ObjectNet, Fo3Error> {
    let name_index = reader.read_i32("object name")?;
    let name = resolve_string(document, block, name_index)?;
    let extra_count = reader.read_u32("extra data count")? as usize;
    let extra_count = checked_count(block, reader, extra_count, 4, "extra data")?;
    let mut extra_data = Vec::with_capacity(extra_count);
    for _ in 0..extra_count {
        extra_data.push(reader.read_i32("extra data reference")?);
    }
    Ok(ObjectNet {
        name,
        extra_data,
        controller: reader.read_i32("controller reference")?,
    })
}

fn parse_av_object(
    document: &Document,
    block: &RawBlock,
    reader: &mut Reader<'_>,
) -> Result<AvObject, Fo3Error> {
    let object = parse_object_net(document, block, reader)?;
    let flags = if document.header.bethesda.version > 26 {
        reader.read_u32("AV flags")?
    } else {
        reader.read_u16("AV flags")? as u32
    };
    let translation = read_vec3(reader, "translation")?;
    let mut rotation = [0.0; 9];
    for value in &mut rotation {
        *value = reader.read_f32("rotation")?;
    }
    let scale = reader.read_f32("scale")?;
    let property_count = reader.read_u32("property count")? as usize;
    let property_count = checked_count(block, reader, property_count, 4, "property")?;
    let mut properties = Vec::with_capacity(property_count);
    for _ in 0..property_count {
        properties.push(reader.read_i32("property reference")?);
    }
    Ok(AvObject {
        object,
        flags,
        transform: Transform {
            translation,
            rotation,
            scale,
        },
        properties,
        collision_object: reader.read_i32("collision object reference")?,
    })
}

fn parse_node(
    document: &Document,
    block: &RawBlock,
    reader: &mut Reader<'_>,
) -> Result<Node, Fo3Error> {
    let base = parse_av_object(document, block, reader)?;
    let child_count = reader.read_u32("child count")? as usize;
    let child_count = checked_count(block, reader, child_count, 4, "child")?;
    let mut children = Vec::with_capacity(child_count);
    for _ in 0..child_count {
        children.push(reader.read_i32("child reference")?);
    }
    let effect_count = reader.read_u32("effect count")? as usize;
    let effect_count = checked_count(block, reader, effect_count, 4, "effect")?;
    let mut effects = Vec::with_capacity(effect_count);
    for _ in 0..effect_count {
        effects.push(reader.read_i32("effect reference")?);
    }
    let (ordered_alpha_sort_bound, ordered_static_bound) = if block.type_name == "BSOrderedNode" {
        let mut bound = [0.0; 4];
        for value in &mut bound {
            *value = reader.read_f32("alpha sort bound")?;
        }
        (Some(bound), Some(reader.read_u8("static bound")? != 0))
    } else {
        (None, None)
    };
    let range = matches!(
        block.type_name.as_str(),
        "BSRangeNode" | "BSBlastNode" | "BSDamageStage"
    )
    .then(|| {
        Ok::<_, Fo3Error>([
            reader.read_u8("range minimum")?,
            reader.read_u8("range maximum")?,
            reader.read_u8("range current")?,
        ])
    })
    .transpose()?;
    let billboard_mode = (block.type_name == "NiBillboardNode")
        .then(|| reader.read_u16("billboard mode"))
        .transpose()?;
    let switch = matches!(block.type_name.as_str(), "NiSwitchNode" | "NiLODNode")
        .then(|| {
            Ok::<_, Fo3Error>((
                reader.read_u16("switch node flags")?,
                reader.read_u32("active child index")?,
            ))
        })
        .transpose()?;
    let lod_data = (block.type_name == "NiLODNode")
        .then(|| reader.read_i32("LOD data reference"))
        .transpose()?;
    let value = (block.type_name == "BSValueNode")
        .then(|| {
            Ok::<_, Fo3Error>((
                reader.read_u32("value")?,
                reader.read_u8("value node flags")?,
            ))
        })
        .transpose()?;
    let multi_bound = (block.type_name == "BSMultiBoundNode")
        .then(|| reader.read_i32("multi bound reference"))
        .transpose()?;
    let sorting_mode = (block.type_name == "NiSortAdjustNode")
        .then(|| reader.read_u32("sorting mode"))
        .transpose()?;
    Ok(Node {
        base,
        children,
        effects,
        ordered_alpha_sort_bound,
        ordered_static_bound,
        range,
        billboard_mode,
        switch,
        lod_data,
        value,
        multi_bound,
        sorting_mode,
    })
}

fn parse_geometry(
    document: &Document,
    block: &RawBlock,
    reader: &mut Reader<'_>,
) -> Result<Geometry, Fo3Error> {
    let base = parse_av_object(document, block, reader)?;
    let data = reader.read_i32("geometry data reference")?;
    let skin_instance = reader.read_i32("skin instance reference")?;
    let material_count = reader.read_u32("material count")? as usize;
    let material_count = checked_count(block, reader, material_count, 8, "material")?;
    let mut names = Vec::with_capacity(material_count);
    for _ in 0..material_count {
        let index = reader.read_i32("material name")?;
        names.push(resolve_string(document, block, index)?);
    }
    let mut extra_data = Vec::with_capacity(material_count);
    for _ in 0..material_count {
        extra_data.push(reader.read_i32("material extra data")?);
    }
    let materials = MaterialData {
        names,
        extra_data,
        active: reader.read_i32("active material")?,
        needs_update: reader.read_u8("material needs update")? != 0,
    };
    Ok(Geometry {
        base,
        data,
        skin_instance,
        materials,
    })
}

fn parse_geometry_data(
    block: &RawBlock,
    reader: &mut Reader<'_>,
) -> Result<GeometryData, Fo3Error> {
    let group_id = reader.read_i32("geometry group id")?;
    let vertex_count = reader.read_u16("vertex count")? as usize;
    let keep_flags = reader.read_u8("keep flags")?;
    let compress_flags = reader.read_u8("compress flags")?;
    let has_vertices = reader.read_u8("has vertices")? != 0;
    let mut vertices = Vec::with_capacity(if has_vertices { vertex_count } else { 0 });
    if has_vertices {
        checked_count(block, reader, vertex_count, 12, "vertex")?;
        for _ in 0..vertex_count {
            vertices.push(read_vec3(reader, "vertex")?);
        }
    }
    let data_flags = reader.read_u16("geometry data flags")?;
    let has_normals = reader.read_u8("has normals")? != 0;
    let mut normals = Vec::new();
    let mut tangents = Vec::new();
    let mut bitangents = Vec::new();
    if has_normals {
        checked_count(block, reader, vertex_count, 12, "normal")?;
        normals.reserve(vertex_count);
        for _ in 0..vertex_count {
            normals.push(read_vec3(reader, "normal")?);
        }
        if data_flags & 0x1000 != 0 {
            checked_count(block, reader, vertex_count, 24, "tangent pair")?;
            tangents.reserve(vertex_count);
            bitangents.reserve(vertex_count);
            for _ in 0..vertex_count {
                tangents.push(read_vec3(reader, "tangent")?);
            }
            for _ in 0..vertex_count {
                bitangents.push(read_vec3(reader, "bitangent")?);
            }
        }
    }
    let bound_center = read_vec3(reader, "bound center")?;
    let bound_radius = reader.read_f32("bound radius")?;
    let has_colors = reader.read_u8("has vertex colors")? != 0;
    let mut colors = Vec::new();
    if has_colors {
        checked_count(block, reader, vertex_count, 16, "vertex color")?;
        colors.reserve(vertex_count);
        for _ in 0..vertex_count {
            colors.push(read_vec4(reader, "vertex color")?);
        }
    }
    let uv_count = usize::from(data_flags & 1);
    let mut uv_sets = Vec::with_capacity(uv_count);
    for _ in 0..uv_count {
        checked_count(block, reader, vertex_count, 8, "texture coordinate")?;
        let mut values = Vec::with_capacity(vertex_count);
        for _ in 0..vertex_count {
            values.push([
                reader.read_f32("texture coordinate u")?,
                reader.read_f32("texture coordinate v")?,
            ]);
        }
        uv_sets.push(values);
    }
    Ok(GeometryData {
        group_id,
        vertices,
        keep_flags,
        compress_flags,
        data_flags,
        normals,
        tangents,
        bitangents,
        bound_center,
        bound_radius,
        colors,
        uv_sets,
        consistency_flags: reader.read_u16("consistency flags")?,
        additional_data: reader.read_i32("additional data reference")?,
        triangle_count: reader.read_u16("triangle count")?,
    })
}

fn parse_tri_strips_data(
    block: &RawBlock,
    reader: &mut Reader<'_>,
) -> Result<TriStripsData, Fo3Error> {
    let geometry = parse_geometry_data(block, reader)?;
    let strip_count = reader.read_u16("strip count")? as usize;
    checked_count(block, reader, strip_count, 2, "strip length")?;
    let mut lengths = Vec::with_capacity(strip_count);
    for _ in 0..strip_count {
        lengths.push(reader.read_u16("strip length")? as usize);
    }
    let has_points = reader.read_u8("has strip points")? != 0;
    let mut strips = Vec::with_capacity(if has_points { strip_count } else { 0 });
    if has_points {
        for length in lengths {
            checked_count(block, reader, length, 2, "strip point")?;
            let mut strip = Vec::with_capacity(length);
            for _ in 0..length {
                strip.push(reader.read_u16("strip point")?);
            }
            strips.push(strip);
        }
    }
    Ok(TriStripsData { geometry, strips })
}

fn parse_tri_shape_data(
    block: &RawBlock,
    reader: &mut Reader<'_>,
) -> Result<TriShapeData, Fo3Error> {
    let geometry = parse_geometry_data(block, reader)?;
    let _triangle_point_count = reader.read_u32("triangle point count")?;
    let has_triangles = reader.read_u8("has triangles")? != 0;
    let mut triangles = Vec::new();
    if has_triangles {
        let triangle_count = geometry.triangle_count as usize;
        checked_count(block, reader, triangle_count, 6, "triangle")?;
        triangles.reserve(triangle_count);
        for _ in 0..triangle_count {
            triangles.push([
                reader.read_u16("triangle index")?,
                reader.read_u16("triangle index")?,
                reader.read_u16("triangle index")?,
            ]);
        }
    }
    let group_count = reader.read_u16("match group count")? as usize;
    let mut match_groups = Vec::with_capacity(group_count);
    for _ in 0..group_count {
        let count = reader.read_u16("match group vertex count")? as usize;
        checked_count(block, reader, count, 2, "match group vertex")?;
        let mut group = Vec::with_capacity(count);
        for _ in 0..count {
            group.push(reader.read_u16("match group vertex")?);
        }
        match_groups.push(group);
    }
    Ok(TriShapeData {
        geometry,
        triangles,
        match_groups,
    })
}

fn parse_material_property(
    document: &Document,
    block: &RawBlock,
    reader: &mut Reader<'_>,
) -> Result<MaterialProperty, Fo3Error> {
    let object = parse_object_net(document, block, reader)?;
    let include_ambient_diffuse = document.header.bethesda.version < 26;
    let ambient = include_ambient_diffuse
        .then(|| read_vec3(reader, "ambient color"))
        .transpose()?;
    let diffuse = include_ambient_diffuse
        .then(|| read_vec3(reader, "diffuse color"))
        .transpose()?;
    let specular = read_vec3(reader, "specular color")?;
    let emissive = read_vec3(reader, "emissive color")?;
    let glossiness = reader.read_f32("glossiness")?;
    let alpha = reader.read_f32("alpha")?;
    let emissive_multiplier = if document.header.bethesda.version > 21 {
        reader.read_f32("emissive multiplier")?
    } else {
        1.0
    };
    Ok(MaterialProperty {
        object,
        ambient,
        diffuse,
        specular,
        emissive,
        glossiness,
        alpha,
        emissive_multiplier,
    })
}

fn parse_alpha_property(
    document: &Document,
    block: &RawBlock,
    reader: &mut Reader<'_>,
) -> Result<AlphaProperty, Fo3Error> {
    Ok(AlphaProperty {
        object: parse_object_net(document, block, reader)?,
        flags: reader.read_u16("alpha flags")?,
        threshold: reader.read_u8("alpha threshold")?,
    })
}

fn parse_shader_property(
    document: &Document,
    block: &RawBlock,
    reader: &mut Reader<'_>,
) -> Result<ShaderProperty, Fo3Error> {
    Ok(ShaderProperty {
        object: parse_object_net(document, block, reader)?,
        shade_flags: reader.read_u16("shade flags")?,
        shader_type: reader.read_u32("shader type")?,
        shader_flags: reader.read_u32("shader flags")?,
        shader_flags_2: reader.read_u32("shader flags 2")?,
        environment_map_scale: reader.read_f32("environment map scale")?,
        texture_clamp_mode: reader.read_u32("texture clamp mode")?,
    })
}

fn parse_pp_lighting_property(
    document: &Document,
    block: &RawBlock,
    reader: &mut Reader<'_>,
) -> Result<PpLightingProperty, Fo3Error> {
    let base = parse_shader_property(document, block, reader)?;
    let texture_set = reader.read_i32("texture set reference")?;
    let (refraction_strength, refraction_fire_period) = if document.header.bethesda.version > 14 {
        (
            Some(reader.read_f32("refraction strength")?),
            Some(reader.read_i32("refraction fire period")?),
        )
    } else {
        (None, None)
    };
    let (parallax_max_passes, parallax_scale) = if document.header.bethesda.version > 24 {
        (
            Some(reader.read_f32("parallax max passes")?),
            Some(reader.read_f32("parallax scale")?),
        )
    } else {
        (None, None)
    };
    Ok(PpLightingProperty {
        base,
        texture_set,
        refraction_strength,
        refraction_fire_period,
        parallax_max_passes,
        parallax_scale,
    })
}

fn parse_no_lighting_property(
    document: &Document,
    block: &RawBlock,
    reader: &mut Reader<'_>,
) -> Result<NoLightingProperty, Fo3Error> {
    let base = parse_shader_property(document, block, reader)?;
    let file_name = reader.read_sized_string("unlit texture file name")?;
    let falloff = if document.header.bethesda.version > 26 {
        Some([
            reader.read_f32("falloff start angle")?,
            reader.read_f32("falloff stop angle")?,
            reader.read_f32("falloff start opacity")?,
            reader.read_f32("falloff stop opacity")?,
        ])
    } else {
        None
    };
    Ok(NoLightingProperty {
        base,
        file_name,
        falloff,
    })
}

fn parse_texture_set(
    block: &RawBlock,
    reader: &mut Reader<'_>,
) -> Result<ShaderTextureSet, Fo3Error> {
    let count = reader.read_u32("texture count")? as usize;
    let count = checked_count(block, reader, count, 4, "texture")?;
    let mut textures = Vec::with_capacity(count);
    for _ in 0..count {
        textures.push(reader.read_sized_string("texture path")?);
    }
    Ok(ShaderTextureSet { textures })
}

fn parse_transform_controller(reader: &mut Reader<'_>) -> Result<TransformController, Fo3Error> {
    let _next = reader.read_i32("next controller")?;
    let _flags = reader.read_u16("controller flags")?;
    let _frequency = reader.read_f32("controller frequency")?;
    let _phase = reader.read_f32("controller phase")?;
    let start_time = reader.read_f32("controller start time")?;
    let stop_time = reader.read_f32("controller stop time")?;
    let target = reader.read_i32("controller target")?;
    let interpolator = reader.read_i32("controller interpolator")?;
    Ok(TransformController {
        target,
        interpolator,
        start_time,
        stop_time,
    })
}

fn parse_transform_interpolator(
    reader: &mut Reader<'_>,
) -> Result<TransformInterpolator, Fo3Error> {
    let translation = read_vec3(reader, "interpolator translation")?;
    let rotation = read_vec4(reader, "interpolator rotation")?;
    let scale = reader.read_f32("interpolator scale")?;
    let data = reader.read_i32("interpolator data")?;
    Ok(TransformInterpolator {
        transform: Transform {
            translation,
            rotation: quaternion_to_matrix(rotation),
            scale,
        },
        data,
    })
}

fn parse_transform_data(
    block: &RawBlock,
    reader: &mut Reader<'_>,
) -> Result<TransformData, Fo3Error> {
    let rotation_count = reader.read_u32("rotation key count")? as usize;
    let rotation_type = (rotation_count > 0)
        .then(|| read_key_type(reader, "rotation key type"))
        .transpose()?;
    let mut rotations = Vec::with_capacity(rotation_count);
    if rotation_type != Some(KeyType::XyzRotation) {
        checked_count(block, reader, rotation_count, 20, "rotation key")?;
        for _ in 0..rotation_count {
            let time = reader.read_f32("rotation key time")?;
            let value = read_vec4(reader, "rotation key quaternion")?;
            skip_key_tangent(reader, rotation_type, 4, "rotation key tangent")?;
            rotations.push(AnimationKey { time, value });
        }
    }
    let xyz_rotations = if rotation_type == Some(KeyType::XyzRotation) {
        Some([
            parse_scalar_group(block, reader, "x rotation")?,
            parse_scalar_group(block, reader, "y rotation")?,
            parse_scalar_group(block, reader, "z rotation")?,
        ])
    } else {
        None
    };
    let translations = parse_vec3_group(block, reader, "translation")?;
    let scales = parse_scalar_group(block, reader, "scale")?;
    Ok(TransformData {
        rotations,
        xyz_rotations,
        translations,
        scales,
    })
}

fn parse_controller_manager(
    block: &RawBlock,
    reader: &mut Reader<'_>,
) -> Result<ControllerManager, Fo3Error> {
    let _next = reader.read_i32("manager next controller")?;
    let _flags = reader.read_u16("manager flags")?;
    let _frequency = reader.read_f32("manager frequency")?;
    let _phase = reader.read_f32("manager phase")?;
    let _start = reader.read_f32("manager start time")?;
    let _stop = reader.read_f32("manager stop time")?;
    let _target = reader.read_i32("manager target")?;
    let _cumulative = reader.read_u8("manager cumulative")?;
    let count = reader.read_u32("controller sequence count")? as usize;
    let count = checked_count(block, reader, count, 4, "controller sequence")?;
    let mut sequences = Vec::with_capacity(count);
    for _ in 0..count {
        sequences.push(reader.read_i32("controller sequence reference")?);
    }
    let _palette = reader.read_i32("manager object palette")?;
    Ok(ControllerManager { sequences })
}

fn parse_controller_sequence(
    document: &Document,
    block: &RawBlock,
    reader: &mut Reader<'_>,
) -> Result<ControllerSequence, Fo3Error> {
    let name = resolve_string(document, block, reader.read_i32("sequence name")?)?
        .unwrap_or_else(|| format!("sequence#{}", block.index));
    let count = reader.read_u32("controlled block count")? as usize;
    let count = checked_count(block, reader, count, 12, "controlled block")?;
    let _array_grow_by = reader.read_u32("controlled block array grow by")?;
    let mut controlled_blocks = Vec::with_capacity(count);
    for _ in 0..count {
        let interpolator = reader.read_i32("controlled block interpolator")?;
        let controller = reader.read_i32("controlled block controller")?;
        let _priority = reader.read_u8("controlled block priority")?;
        let mut string_ref = |field| {
            reader
                .read_i32(field)
                .and_then(|index| resolve_string(document, block, index))
        };
        controlled_blocks.push(ControlledBlock {
            interpolator,
            controller,
            node_name: string_ref("controlled block node name")?.unwrap_or_default(),
            property_type: string_ref("controlled block property type")?.unwrap_or_default(),
            controller_type: string_ref("controlled block controller type")?.unwrap_or_default(),
            controller_id: string_ref("controlled block controller id")?.unwrap_or_default(),
            interpolator_id: string_ref("controlled block interpolator id")?.unwrap_or_default(),
        });
    }
    let _weight = reader.read_f32("sequence weight")?;
    let _text_keys = reader.read_i32("sequence text keys")?;
    let _cycle_type = reader.read_u32("sequence cycle type")?;
    let _frequency = reader.read_f32("sequence frequency")?;
    let start_time = reader.read_f32("sequence start time")?;
    let stop_time = reader.read_f32("sequence stop time")?;
    let _manager = reader.read_i32("sequence manager")?;
    let _accum_root = resolve_string(
        document,
        block,
        reader.read_i32("sequence accumulation root")?,
    )?;
    if document.header.bethesda.version > 28 {
        let note_count = reader.read_u16("animation note array count")? as usize;
        let _ = checked_count(block, reader, note_count, 4, "animation note array")?;
        for _ in 0..note_count {
            reader.read_i32("animation note array")?;
        }
    }
    Ok(ControllerSequence {
        name,
        start_time,
        stop_time,
        controlled_blocks,
    })
}

fn parse_scalar_group(
    block: &RawBlock,
    reader: &mut Reader<'_>,
    field: &'static str,
) -> Result<AnimationKeyGroup<f32>, Fo3Error> {
    let count = reader.read_u32("key count")? as usize;
    let interpolation = (count > 0)
        .then(|| read_key_type(reader, "key interpolation"))
        .transpose()?;
    let count = checked_count(block, reader, count, 8, field)?;
    let mut keys = Vec::with_capacity(count);
    for _ in 0..count {
        let time = reader.read_f32("key time")?;
        let value = reader.read_f32("key value")?;
        skip_key_tangent(reader, interpolation, 1, "key tangent")?;
        keys.push(AnimationKey { time, value });
    }
    Ok(AnimationKeyGroup {
        interpolation,
        keys,
    })
}

fn parse_vec3_group(
    block: &RawBlock,
    reader: &mut Reader<'_>,
    field: &'static str,
) -> Result<AnimationKeyGroup<[f32; 3]>, Fo3Error> {
    let count = reader.read_u32("key count")? as usize;
    let interpolation = (count > 0)
        .then(|| read_key_type(reader, "key interpolation"))
        .transpose()?;
    let count = checked_count(block, reader, count, 16, field)?;
    let mut keys = Vec::with_capacity(count);
    for _ in 0..count {
        let time = reader.read_f32("key time")?;
        let value = read_vec3(reader, "key value")?;
        skip_key_tangent(reader, interpolation, 3, "key tangent")?;
        keys.push(AnimationKey { time, value });
    }
    Ok(AnimationKeyGroup {
        interpolation,
        keys,
    })
}

fn read_key_type(reader: &mut Reader<'_>, field: &'static str) -> Result<KeyType, Fo3Error> {
    Ok(match reader.read_u32(field)? {
        1 => KeyType::Linear,
        2 => KeyType::Quadratic,
        3 => KeyType::Tbc,
        4 => KeyType::XyzRotation,
        5 => KeyType::Const,
        value => KeyType::Unknown(value),
    })
}

fn skip_key_tangent(
    reader: &mut Reader<'_>,
    interpolation: Option<KeyType>,
    components: usize,
    field: &'static str,
) -> Result<(), Fo3Error> {
    let extra = match interpolation {
        Some(KeyType::Quadratic) => components * 2,
        Some(KeyType::Tbc) => 3,
        _ => 0,
    };
    reader.take(extra * 4, field).map(|_| ())
}

fn quaternion_to_matrix(quaternion: [f32; 4]) -> [f32; 9] {
    let [w, x, y, z] = quaternion;
    [
        1.0 - 2.0 * (y * y + z * z),
        2.0 * (x * y - z * w),
        2.0 * (x * z + y * w),
        2.0 * (x * y + z * w),
        1.0 - 2.0 * (x * x + z * z),
        2.0 * (y * z - x * w),
        2.0 * (x * z - y * w),
        2.0 * (y * z + x * w),
        1.0 - 2.0 * (x * x + y * y),
    ]
}

fn resolve_string(
    document: &Document,
    block: &RawBlock,
    index: i32,
) -> Result<Option<String>, Fo3Error> {
    if index == -1 {
        return Ok(None);
    }
    document
        .header
        .strings
        .get(index as usize)
        .cloned()
        .map(Some)
        .ok_or_else(|| Fo3Error::InvalidStringIndex {
            block: block.index as usize,
            type_name: block.type_name.clone(),
            string_index: index,
            string_count: document.header.strings.len(),
        })
}

fn checked_count(
    block: &RawBlock,
    reader: &Reader<'_>,
    count: usize,
    item_size: usize,
    field: &'static str,
) -> Result<usize, Fo3Error> {
    let needed = count
        .checked_mul(item_size)
        .ok_or(Fo3Error::Overflow(field))?;
    if needed > reader.remaining() {
        Err(Fo3Error::InvalidBlockCount {
            block: block.index as usize,
            type_name: block.type_name.clone(),
            field,
            count,
            remaining: reader.remaining(),
        })
    } else {
        Ok(count)
    }
}

fn read_vec3(reader: &mut Reader<'_>, field: &'static str) -> Result<[f32; 3], Fo3Error> {
    Ok([
        reader.read_f32(field)?,
        reader.read_f32(field)?,
        reader.read_f32(field)?,
    ])
}

fn read_vec4(reader: &mut Reader<'_>, field: &'static str) -> Result<[f32; 4], Fo3Error> {
    Ok([
        reader.read_f32(field)?,
        reader.read_f32(field)?,
        reader.read_f32(field)?,
        reader.read_f32(field)?,
    ])
}
