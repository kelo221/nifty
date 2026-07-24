use std::{borrow::Cow, collections::BTreeMap, io::Cursor};

use gltf_json as json;
use json::{
    accessor, buffer, material, mesh,
    scene::UnitQuaternion,
    validation::{Checked::Valid, USize64},
    Index,
};
use thiserror::Error;

use super::{
    FalloutShaderFeatures, FALLOUT_EMISSIVE_SCALE, SHADER_TYPE_HAIR_TINT, SHADER_TYPE_SKIN_TINT,
};
use super::{
    Scene, SceneAlphaMode, SceneAnimation, SceneAnimationChannel, SceneMaterial, SceneMesh,
    SceneSkin,
};

const NIF_UNITS_PER_METRE: f32 = 70.0;

#[derive(Debug, Clone)]
pub struct GlbOptions {
    pub source_name: String,
    pub allow_missing_textures: bool,
}

impl Default for GlbOptions {
    fn default() -> Self {
        Self {
            source_name: "scene.nif".into(),
            allow_missing_textures: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlbOutput {
    pub bytes: Vec<u8>,
    pub missing_textures: Vec<String>,
}

#[derive(Debug, Error)]
pub enum GlbError {
    #[error("texture {0} was referenced by the NIF but not supplied")]
    MissingTexture(String),
    #[error("texture {path} could not be decoded: {message}")]
    TextureDecode { path: String, message: String },
    #[error("could not encode a texture as PNG: {0}")]
    TextureEncode(String),
    #[error("could not serialize glTF JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("could not write GLB: {0}")]
    Glb(String),
    #[error("generated GLB did not pass gltf validation: {0}")]
    Validation(String),
}

pub fn encode_glb(
    scene: &Scene,
    textures: &BTreeMap<String, Vec<u8>>,
    options: &GlbOptions,
) -> Result<GlbOutput, GlbError> {
    let mut writer = Writer {
        root: json::Root::default(),
        bin: Vec::new(),
        textures,
        texture_indices: BTreeMap::new(),
        missing_textures: Vec::new(),
        allow_missing_textures: options.allow_missing_textures,
    };

    for material in &scene.materials {
        let material = writer.material(material)?;
        writer.root.materials.push(material);
    }

    for node in &scene.nodes {
        let mesh = node
            .mesh
            .as_ref()
            .map(|mesh| writer.mesh(mesh))
            .transpose()?;
        let rotation = glam::Quat::from_mat3(
            &glam::Mat3::from_cols_array(&node.transform.rotation).transpose(),
        )
        .normalize();
        writer.root.nodes.push(json::Node {
            camera: None,
            children: (!node.children.is_empty()).then(|| {
                node.children
                    .iter()
                    .map(|&index| Index::new(index as u32))
                    .collect()
            }),
            extensions: None,
            extras: extras(serde_json::json!({
                "bevyout_nif_block": node.source_block,
            }))?,
            matrix: None,
            mesh,
            name: Some(node.name.clone()),
            rotation: Some(UnitQuaternion(rotation.to_array())),
            scale: Some([node.transform.scale; 3]),
            translation: Some(node.transform.translation),
            skin: node.skin.map(|index| Index::new(index as u32)),
            weights: None,
        });
    }

    for skin in &scene.skins {
        let skin = writer.skin(skin);
        writer.root.skins.push(skin);
    }

    for animation in &scene.animations {
        let encoded = writer.animation(animation);
        writer.root.animations.push(encoded);
    }

    let scene_roots = scene
        .roots
        .iter()
        .map(|&index| Index::new(index as u32))
        .collect::<Vec<_>>();
    let animation_sound_cues = scene
        .animation_sound_cues
        .iter()
        .map(|cue| {
            serde_json::json!({
                "sequence": cue.sequence,
                "time": cue.time,
                "editor_id": cue.editor_id,
            })
        })
        .collect::<Vec<_>>();
    let animation_sound_cues = serde_json::to_string(&animation_sound_cues)?;
    let coordinate_root = writer.root.nodes.len();
    writer.root.nodes.push(json::Node {
        camera: None,
        children: Some(scene_roots),
        extensions: None,
        extras: extras(serde_json::json!({
            "bevyout_nif_units_per_metre": NIF_UNITS_PER_METRE,
            "bevyout_nif_axis_conversion": "Z-up to glTF Y-up",
            "bevyout_source_model": options.source_name,
            "bevyout_source_render_meshes": scene.statistics.source_meshes,
            "bevyout_source_render_vertices": scene.statistics.source_vertices,
            "bevyout_source_render_triangles": scene.statistics.source_triangles,
            "bevyout_root_transform_policy": "preserve",
            "bevyout_native_nif_converter": true,
            "bevyout_animation_sound_cues": animation_sound_cues,
        }))?,
        matrix: None,
        mesh: None,
        name: Some(options.source_name.clone()),
        rotation: Some(UnitQuaternion([
            -std::f32::consts::FRAC_1_SQRT_2,
            0.0,
            0.0,
            std::f32::consts::FRAC_1_SQRT_2,
        ])),
        scale: Some([1.0 / NIF_UNITS_PER_METRE; 3]),
        translation: None,
        skin: None,
        weights: None,
    });

    writer.root.scenes.push(json::Scene {
        extensions: None,
        extras: extras(serde_json::json!({
            "bevyout_native_nif_converter": true,
        }))?,
        name: Some(options.source_name.clone()),
        nodes: vec![Index::new(coordinate_root as u32)],
    });
    writer.root.scene = Some(Index::new(0));
    writer.root.buffers.push(json::Buffer {
        byte_length: writer.bin.len().into(),
        name: Some(format!("{} binary", options.source_name)),
        uri: None,
        extensions: None,
        extras: Default::default(),
    });

    if writer
        .root
        .extensions_used
        .iter()
        .any(|value| value == "KHR_materials_unlit")
        && !writer
            .root
            .extensions_required
            .iter()
            .any(|value| value == "KHR_materials_unlit")
    {
        writer
            .root
            .extensions_required
            .push("KHR_materials_unlit".into());
    }

    let json = json::serialize::to_vec(&writer.root)?;
    let glb = gltf::binary::Glb {
        header: gltf::binary::Header {
            magic: *b"glTF",
            version: 2,
            length: 0,
        },
        json: Cow::Owned(json),
        bin: Some(Cow::Owned(writer.bin)),
    }
    .to_vec()
    .map_err(|error| GlbError::Glb(error.to_string()))?;
    gltf::Gltf::from_slice(&glb).map_err(|error| GlbError::Validation(error.to_string()))?;
    Ok(GlbOutput {
        bytes: glb,
        missing_textures: writer.missing_textures,
    })
}

struct Writer<'a> {
    root: json::Root,
    bin: Vec<u8>,
    textures: &'a BTreeMap<String, Vec<u8>>,
    texture_indices: BTreeMap<String, Index<json::Texture>>,
    missing_textures: Vec<String>,
    allow_missing_textures: bool,
}

impl Writer<'_> {
    fn animation(&mut self, source: &SceneAnimation) -> json::animation::Animation {
        let mut channels = Vec::new();
        let mut samplers = Vec::new();
        for channel in &source.channels {
            self.add_animation_property(
                &mut channels,
                &mut samplers,
                channel,
                json::animation::Property::Translation,
                &channel.translations,
                |key| key.value.to_vec(),
                accessor::Type::Vec3,
            );
            self.add_animation_property(
                &mut channels,
                &mut samplers,
                channel,
                json::animation::Property::Rotation,
                &channel.rotations,
                |key| key.value.to_vec(),
                accessor::Type::Vec4,
            );
            self.add_animation_property(
                &mut channels,
                &mut samplers,
                channel,
                json::animation::Property::Scale,
                &channel.scales,
                |key| [key.value, key.value, key.value].to_vec(),
                accessor::Type::Vec3,
            );
        }
        json::animation::Animation {
            extensions: None,
            extras: Default::default(),
            channels,
            name: Some(source.name.clone()),
            samplers,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add_animation_property<T, F>(
        &mut self,
        channels: &mut Vec<json::animation::Channel>,
        samplers: &mut Vec<json::animation::Sampler>,
        channel: &SceneAnimationChannel,
        property: json::animation::Property,
        keys: &[super::AnimationKey<T>],
        values: F,
        type_: accessor::Type,
    ) where
        F: Fn(&super::AnimationKey<T>) -> Vec<f32>,
    {
        if keys.is_empty() {
            return;
        }
        let mut key_indices = (0..keys.len()).collect::<Vec<_>>();
        key_indices.sort_by(|&a, &b| {
            keys[a]
                .time
                .partial_cmp(&keys[b].time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let times = key_indices.iter().map(|&index| keys[index].time);
        let values = key_indices.iter().flat_map(|&index| values(&keys[index]));
        let input = self.f32_accessor(
            times,
            keys.len(),
            accessor::Type::Scalar,
            None,
            buffer::Target::ArrayBuffer,
            format!("{} {:?} times", channel.node, property),
        );
        let output = self.f32_accessor(
            values,
            keys.len(),
            type_,
            None,
            buffer::Target::ArrayBuffer,
            format!("{} {:?} values", channel.node, property),
        );
        let sampler = Index::new(samplers.len() as u32);
        samplers.push(json::animation::Sampler {
            extensions: None,
            extras: Default::default(),
            input,
            interpolation: Valid(json::animation::Interpolation::Linear),
            output,
        });
        channels.push(json::animation::Channel {
            sampler,
            target: json::animation::Target {
                extensions: None,
                extras: Default::default(),
                node: Index::new(channel.node as u32),
                path: Valid(property),
            },
            extensions: None,
            extras: Default::default(),
        });
    }

    fn mesh(&mut self, source: &SceneMesh) -> Result<Index<json::Mesh>, GlbError> {
        let positions = self.f32_accessor(
            source.positions.iter().flatten().copied(),
            source.positions.len(),
            accessor::Type::Vec3,
            Some(position_bounds(&source.positions)),
            buffer::Target::ArrayBuffer,
            format!("{} positions", source.name),
        );
        let mut attributes = BTreeMap::new();
        attributes.insert(Valid(mesh::Semantic::Positions), positions);
        if !source.normals.is_empty() {
            let normals = self.f32_accessor(
                source.normals.iter().flatten().copied(),
                source.normals.len(),
                accessor::Type::Vec3,
                None,
                buffer::Target::ArrayBuffer,
                format!("{} normals", source.name),
            );
            attributes.insert(Valid(mesh::Semantic::Normals), normals);
        }
        if !source.tangents.is_empty() {
            let tangents = self.f32_accessor(
                source.tangents.iter().flatten().copied(),
                source.tangents.len(),
                accessor::Type::Vec4,
                None,
                buffer::Target::ArrayBuffer,
                format!("{} tangents", source.name),
            );
            attributes.insert(Valid(mesh::Semantic::Tangents), tangents);
        }
        if !source.colors.is_empty() {
            let colors = self.f32_accessor(
                source.colors.iter().flatten().copied(),
                source.colors.len(),
                accessor::Type::Vec4,
                None,
                buffer::Target::ArrayBuffer,
                format!("{} colors", source.name),
            );
            attributes.insert(Valid(mesh::Semantic::Colors(0)), colors);
        }
        if !source.tex_coords.is_empty() {
            let tex_coords = self.f32_accessor(
                source.tex_coords.iter().flatten().copied(),
                source.tex_coords.len(),
                accessor::Type::Vec2,
                None,
                buffer::Target::ArrayBuffer,
                format!("{} texture coordinates", source.name),
            );
            attributes.insert(Valid(mesh::Semantic::TexCoords(0)), tex_coords);
        }
        if !source.joints.is_empty() {
            let joints = self.u16_vector_accessor(
                source.joints.iter().flatten().copied(),
                source.joints.len(),
                accessor::Type::Vec4,
                buffer::Target::ArrayBuffer,
                format!("{} joints", source.name),
            );
            attributes.insert(Valid(mesh::Semantic::Joints(0)), joints);
        }
        if !source.weights.is_empty() {
            let weights = self.f32_accessor(
                source.weights.iter().flatten().copied(),
                source.weights.len(),
                accessor::Type::Vec4,
                None,
                buffer::Target::ArrayBuffer,
                format!("{} weights", source.name),
            );
            attributes.insert(Valid(mesh::Semantic::Weights(0)), weights);
        }
        let indices = self.u16_accessor(&source.indices, format!("{} indices", source.name));
        let primitive = json::mesh::Primitive {
            attributes,
            extensions: None,
            extras: Default::default(),
            indices: Some(indices),
            material: source.material.map(|index| Index::new(index as u32)),
            mode: Valid(mesh::Mode::Triangles),
            targets: None,
        };
        self.root.meshes.push(json::Mesh {
            extensions: None,
            extras: Default::default(),
            name: Some(source.name.clone()),
            primitives: vec![primitive],
            weights: None,
        });
        Ok(Index::new((self.root.meshes.len() - 1) as u32))
    }

    fn skin(&mut self, source: &SceneSkin) -> json::Skin {
        let inverse_bind_matrices = (!source.inverse_bind_matrices.is_empty()).then(|| {
            self.f32_accessor(
                source.inverse_bind_matrices.iter().flatten().copied(),
                source.inverse_bind_matrices.len(),
                accessor::Type::Mat4,
                None,
                buffer::Target::ArrayBuffer,
                format!("{} inverse bind matrices", source.name),
            )
        });
        json::Skin {
            extensions: None,
            extras: Default::default(),
            inverse_bind_matrices,
            joints: source
                .joints
                .iter()
                .map(|&index| Index::new(index as u32))
                .collect(),
            name: Some(source.name.clone()),
            skeleton: source.skeleton.map(|index| Index::new(index as u32)),
        }
    }

    #[allow(clippy::needless_update)]
    fn material(&mut self, source: &SceneMaterial) -> Result<json::Material, GlbError> {
        let diffuse = source
            .diffuse_texture
            .as_deref()
            .map(|path| self.texture(path))
            .transpose()?
            .flatten();
        let normal = source
            .normal_texture
            .as_deref()
            .map(|path| self.texture(path))
            .transpose()?
            .flatten();
        let specular = source
            .specular_texture
            .as_deref()
            .map(|path| self.texture(path))
            .transpose()?
            .flatten();
        let glow = source
            .glow_texture
            .as_deref()
            .map(|path| self.texture(path))
            .transpose()?
            .flatten();
        let features = FalloutShaderFeatures::from_flags(
            source.shader_type,
            source.shader_flags_1,
            source.shader_flags_2,
        );
        let translucency_strength = if features.back_lighting {
            0.35
        } else if features.soft_lighting {
            0.2
        } else if matches!(
            source.shader_type,
            SHADER_TYPE_SKIN_TINT | SHADER_TYPE_HAIR_TINT
        ) {
            0.15
        } else {
            0.0
        };
        let texture_info = |index| json::texture::Info {
            index,
            tex_coord: 0,
            extensions: None,
            extras: Default::default(),
        };
        let extensions = (source.unlit || source.emissive_multiplier > 1.0 || specular.is_some())
            .then(|| {
                if source.unlit
                    && !self
                        .root
                        .extensions_used
                        .iter()
                        .any(|value| value == "KHR_materials_unlit")
                {
                    self.root.extensions_used.push("KHR_materials_unlit".into());
                }
                if source.emissive_multiplier > 1.0
                    && !self
                        .root
                        .extensions_used
                        .iter()
                        .any(|value| value == "KHR_materials_emissive_strength")
                {
                    self.root
                        .extensions_used
                        .push("KHR_materials_emissive_strength".into());
                }
                if specular.is_some()
                    && !self
                        .root
                        .extensions_used
                        .iter()
                        .any(|value| value == "KHR_materials_specular")
                {
                    self.root
                        .extensions_used
                        .push("KHR_materials_specular".into());
                }
                json::extensions::material::Material {
                    unlit: source.unlit.then_some(json::extensions::material::Unlit {}),
                    emissive_strength: (source.emissive_multiplier > 1.0).then_some(
                        json::extensions::material::EmissiveStrength {
                            emissive_strength: json::extensions::material::EmissiveStrengthFactor(
                                source.emissive_multiplier,
                            ),
                        },
                    ),
                    specular: specular.map(|index| json::extensions::material::Specular {
                        specular_texture: Some(texture_info(index)),
                        ..Default::default()
                    }),
                    ..Default::default()
                }
            });
        Ok(json::Material {
            alpha_cutoff: source.alpha_cutoff.map(material::AlphaCutoff),
            alpha_mode: Valid(match source.alpha_mode {
                SceneAlphaMode::Opaque => material::AlphaMode::Opaque,
                SceneAlphaMode::Mask => material::AlphaMode::Mask,
                SceneAlphaMode::Blend => material::AlphaMode::Blend,
            }),
            double_sided: source.double_sided,
            name: Some(source.name.clone()),
            pbr_metallic_roughness: material::PbrMetallicRoughness {
                base_color_factor: material::PbrBaseColorFactor(source.base_color),
                base_color_texture: diffuse.map(texture_info),
                metallic_factor: material::StrengthFactor(0.0),
                roughness_factor: material::StrengthFactor(source.roughness),
                metallic_roughness_texture: None,
                extensions: None,
                extras: Default::default(),
            },
            normal_texture: normal.map(|index| material::NormalTexture {
                index,
                scale: 1.0,
                tex_coord: 0,
                extensions: None,
                extras: Default::default(),
            }),
            occlusion_texture: None,
            emissive_texture: glow.map(texture_info),
            emissive_factor: material::EmissiveFactor([
                source.emissive[0] * FALLOUT_EMISSIVE_SCALE,
                source.emissive[1] * FALLOUT_EMISSIVE_SCALE,
                source.emissive[2] * FALLOUT_EMISSIVE_SCALE,
            ]),
            extensions,
            extras: extras(serde_json::json!({
                "bevyout_fallout_material": {
                    "schema": 1,
                    "shader_type": source.shader_type,
                    "shader_flags_1": source.shader_flags_1,
                    "shader_flags_2": source.shader_flags_2,
                    "features": {
                        "glow_map": features.glow_map,
                        "specular": features.specular,
                        "parallax": features.parallax,
                        "environment_mapping": features.environment_mapping,
                        "double_sided": features.double_sided,
                        "vertex_colors": features.vertex_colors,
                        "vertex_alpha": features.vertex_alpha,
                        "soft_lighting": features.soft_lighting,
                        "back_lighting": features.back_lighting,
                    },
                    "translucency_enabled": features.translucent_candidate,
                    "translucency_strength": translucency_strength,
                    "emissive_multiplier": source.emissive_multiplier,
                    "emissive_scale": FALLOUT_EMISSIVE_SCALE,
                    "environment_texture": source.environment_texture,
                    "environment_mask": source.environment_mask,
                    "height_texture": source.height_texture,
                }
            }))?,
        })
    }

    fn texture(&mut self, path: &str) -> Result<Option<Index<json::Texture>>, GlbError> {
        if let Some(&index) = self.texture_indices.get(path) {
            return Ok(Some(index));
        }
        let Some(bytes) = self.textures.get(path) else {
            if !self.missing_textures.iter().any(|value| value == path) {
                self.missing_textures.push(path.to_owned());
            }
            return if self.allow_missing_textures {
                Ok(None)
            } else {
                Err(GlbError::MissingTexture(path.to_owned()))
            };
        };
        let decoded = decode_texture(path, bytes)?;
        let mut png = Cursor::new(Vec::new());
        decoded
            .write_to(&mut png, image::ImageFormat::Png)
            .map_err(|error| GlbError::TextureEncode(error.to_string()))?;
        let view = self.view(&png.into_inner(), None, Some(format!("{path} PNG")));
        self.root.images.push(json::Image {
            buffer_view: Some(view),
            mime_type: Some(json::image::MimeType("image/png".into())),
            name: Some(path.to_owned()),
            uri: None,
            extensions: None,
            extras: Default::default(),
        });
        let image = Index::new((self.root.images.len() - 1) as u32);
        self.root.textures.push(json::Texture {
            name: Some(path.to_owned()),
            sampler: None,
            source: image,
            extensions: None,
            extras: Default::default(),
        });
        let texture = Index::new((self.root.textures.len() - 1) as u32);
        self.texture_indices.insert(path.to_owned(), texture);
        Ok(Some(texture))
    }

    fn f32_accessor(
        &mut self,
        values: impl IntoIterator<Item = f32>,
        count: usize,
        type_: accessor::Type,
        bounds: Option<([f32; 3], [f32; 3])>,
        target: buffer::Target,
        name: String,
    ) -> Index<json::Accessor> {
        let mut bytes = Vec::new();
        for value in values {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        let view = self.view(&bytes, Some(target), Some(name));
        self.root.accessors.push(json::Accessor {
            buffer_view: Some(view),
            byte_offset: Some(USize64(0)),
            count: count.into(),
            component_type: Valid(accessor::GenericComponentType(accessor::ComponentType::F32)),
            extensions: None,
            extras: Default::default(),
            type_: Valid(type_),
            min: bounds.map(|(minimum, _)| serde_json::json!(minimum)),
            max: bounds.map(|(_, maximum)| serde_json::json!(maximum)),
            name: None,
            normalized: false,
            sparse: None,
        });
        Index::new((self.root.accessors.len() - 1) as u32)
    }

    fn u16_accessor(&mut self, values: &[u16], name: String) -> Index<json::Accessor> {
        let mut bytes = Vec::with_capacity(values.len() * 2);
        for &value in values {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        let view = self.view(&bytes, Some(buffer::Target::ElementArrayBuffer), Some(name));
        self.root.accessors.push(json::Accessor {
            buffer_view: Some(view),
            byte_offset: Some(USize64(0)),
            count: values.len().into(),
            component_type: Valid(accessor::GenericComponentType(accessor::ComponentType::U16)),
            extensions: None,
            extras: Default::default(),
            type_: Valid(accessor::Type::Scalar),
            min: values.iter().min().map(|value| serde_json::json!([value])),
            max: values.iter().max().map(|value| serde_json::json!([value])),
            name: None,
            normalized: false,
            sparse: None,
        });
        Index::new((self.root.accessors.len() - 1) as u32)
    }

    fn u16_vector_accessor(
        &mut self,
        values: impl IntoIterator<Item = u16>,
        count: usize,
        type_: accessor::Type,
        target: buffer::Target,
        name: String,
    ) -> Index<json::Accessor> {
        let mut bytes = Vec::new();
        for value in values {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        let view = self.view(&bytes, Some(target), Some(name));
        self.root.accessors.push(json::Accessor {
            buffer_view: Some(view),
            byte_offset: Some(USize64(0)),
            count: count.into(),
            component_type: Valid(accessor::GenericComponentType(accessor::ComponentType::U16)),
            extensions: None,
            extras: Default::default(),
            type_: Valid(type_),
            min: None,
            max: None,
            name: None,
            normalized: false,
            sparse: None,
        });
        Index::new((self.root.accessors.len() - 1) as u32)
    }

    fn view(
        &mut self,
        bytes: &[u8],
        target: Option<buffer::Target>,
        name: Option<String>,
    ) -> Index<json::buffer::View> {
        while !self.bin.len().is_multiple_of(4) {
            self.bin.push(0);
        }
        let offset = self.bin.len();
        self.bin.extend_from_slice(bytes);
        self.root.buffer_views.push(json::buffer::View {
            buffer: Index::new(0),
            byte_length: bytes.len().into(),
            byte_offset: Some(USize64(offset as u64)),
            byte_stride: None,
            name,
            target: target.map(Valid),
            extensions: None,
            extras: Default::default(),
        });
        Index::new((self.root.buffer_views.len() - 1) as u32)
    }
}

fn decode_texture(path: &str, bytes: &[u8]) -> Result<image::DynamicImage, GlbError> {
    let layout = bc1_layout(bytes).map_err(|message| GlbError::TextureDecode {
        path: path.to_owned(),
        message,
    })?;
    let decode_bytes = match layout {
        Some(layout) if !layout.width.is_multiple_of(4) || !layout.height.is_multiple_of(4) => {
            let mut padded = bytes.to_vec();
            padded[12..16].copy_from_slice(&layout.height.next_multiple_of(4).to_le_bytes());
            padded[16..20].copy_from_slice(&layout.width.next_multiple_of(4).to_le_bytes());
            Cow::Owned(padded)
        }
        _ => Cow::Borrowed(bytes),
    };
    let mut decoded =
        image::load_from_memory(&decode_bytes).map_err(|error| GlbError::TextureDecode {
            path: path.to_owned(),
            message: error.to_string(),
        })?;
    let Some(layout) = layout else {
        return Ok(decoded);
    };
    if decoded.width() != layout.width || decoded.height() != layout.height {
        decoded = decoded.crop_imm(0, 0, layout.width, layout.height);
    }

    let mut rgba = decoded.to_rgba8();
    if rgba.width() != layout.width || rgba.height() != layout.height {
        return Err(GlbError::TextureDecode {
            path: path.to_owned(),
            message: format!(
                "BC1 header dimensions {}x{} do not match decoded image {}x{}",
                layout.width,
                layout.height,
                rgba.width(),
                rgba.height()
            ),
        });
    }

    let block_width = layout.width.div_ceil(4);
    let block_height = layout.height.div_ceil(4);
    for block_y in 0..block_height {
        for block_x in 0..block_width {
            let block_index = u64::from(block_y) * u64::from(block_width) + u64::from(block_x);
            let byte_offset = layout.offset
                + usize::try_from(block_index * 8).map_err(|_| GlbError::TextureDecode {
                    path: path.to_owned(),
                    message: "BC1 block offset exceeds the addressable range".into(),
                })?;
            let block = &bytes[byte_offset..byte_offset + 8];
            let color_0 = u16::from_le_bytes([block[0], block[1]]);
            let color_1 = u16::from_le_bytes([block[2], block[3]]);
            let selectors = u32::from_le_bytes([block[4], block[5], block[6], block[7]]);
            let has_transparent_selector = color_0 <= color_1;

            for local_y in 0..4 {
                let y = block_y * 4 + local_y;
                if y >= layout.height {
                    break;
                }
                for local_x in 0..4 {
                    let x = block_x * 4 + local_x;
                    if x >= layout.width {
                        break;
                    }
                    let selector_index = local_y * 4 + local_x;
                    let selector = (selectors >> (selector_index * 2)) & 0x3;
                    rgba.get_pixel_mut(x, y)[3] = if has_transparent_selector && selector == 3 {
                        0
                    } else {
                        255
                    };
                }
            }
        }
    }
    Ok(image::DynamicImage::ImageRgba8(rgba))
}

#[derive(Debug, Clone, Copy)]
struct Bc1Layout {
    offset: usize,
    width: u32,
    height: u32,
}

fn bc1_layout(bytes: &[u8]) -> Result<Option<Bc1Layout>, String> {
    if !bytes.starts_with(b"DDS ") {
        return Ok(None);
    }
    if bytes.len() < 128 {
        return Err("DDS header is truncated".into());
    }
    let read_u32 = |offset: usize| {
        u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ])
    };
    let height = read_u32(12);
    let width = read_u32(16);
    if width == 0 || height == 0 {
        return Err("DDS dimensions must be nonzero".into());
    }

    let four_cc = &bytes[84..88];
    let offset = if four_cc == b"DXT1" {
        128
    } else if four_cc == b"DX10" {
        if bytes.len() < 148 {
            return Err("DDS DX10 header is truncated".into());
        }
        if !matches!(read_u32(128), 70..=72) {
            return Ok(None);
        }
        148
    } else {
        return Ok(None);
    };

    let block_width = u64::from(width.div_ceil(4));
    let block_height = u64::from(height.div_ceil(4));
    let byte_length = block_width
        .checked_mul(block_height)
        .and_then(|blocks| blocks.checked_mul(8))
        .ok_or_else(|| "BC1 payload length overflowed".to_owned())?;
    let end = u64::try_from(offset)
        .ok()
        .and_then(|offset| offset.checked_add(byte_length))
        .and_then(|end| usize::try_from(end).ok())
        .ok_or_else(|| "BC1 payload range overflowed".to_owned())?;
    if end > bytes.len() {
        return Err(format!(
            "BC1 payload is truncated: need {end} bytes, found {}",
            bytes.len()
        ));
    }
    Ok(Some(Bc1Layout {
        offset,
        width,
        height,
    }))
}

fn position_bounds(positions: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    let mut minimum = [f32::INFINITY; 3];
    let mut maximum = [f32::NEG_INFINITY; 3];
    for position in positions {
        for axis in 0..3 {
            minimum[axis] = minimum[axis].min(position[axis]);
            maximum[axis] = maximum[axis].max(position[axis]);
        }
    }
    (minimum, maximum)
}

fn extras(value: serde_json::Value) -> Result<json::Extras, serde_json::Error> {
    serde_json::value::to_raw_value(&value).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dds_block(four_cc: &[u8; 4], width: u32, height: u32, block: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0; 128];
        bytes[0..4].copy_from_slice(b"DDS ");
        bytes[4..8].copy_from_slice(&124u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&0x0002_100fu32.to_le_bytes());
        bytes[12..16].copy_from_slice(&height.to_le_bytes());
        bytes[16..20].copy_from_slice(&width.to_le_bytes());
        bytes[20..24].copy_from_slice(&(block.len() as u32).to_le_bytes());
        bytes[76..80].copy_from_slice(&32u32.to_le_bytes());
        bytes[80..84].copy_from_slice(&4u32.to_le_bytes());
        bytes[84..88].copy_from_slice(four_cc);
        bytes[108..112].copy_from_slice(&0x1000u32.to_le_bytes());
        bytes.extend_from_slice(block);
        bytes
    }

    fn dxt1_block(color_0: u16, color_1: u16, selectors: u32) -> [u8; 8] {
        let mut block = [0; 8];
        block[0..2].copy_from_slice(&color_0.to_le_bytes());
        block[2..4].copy_from_slice(&color_1.to_le_bytes());
        block[4..8].copy_from_slice(&selectors.to_le_bytes());
        block
    }

    fn dx10_bc1(width: u32, height: u32, block: &[u8]) -> Vec<u8> {
        let mut bytes = dds_block(b"DX10", width, height, &[]);
        bytes.extend_from_slice(&[0; 20]);
        bytes[128..132].copy_from_slice(&71u32.to_le_bytes());
        bytes[132..136].copy_from_slice(&3u32.to_le_bytes());
        bytes[140..144].copy_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(block);
        bytes
    }

    #[test]
    fn dxt1_three_color_selector_three_restores_binary_alpha() {
        let bytes = dds_block(b"DXT1", 4, 4, &dxt1_block(0, u16::MAX, u32::MAX));
        let decoded = decode_texture("cutout.dds", &bytes).expect("decode DXT1 cutout");
        assert!(decoded.to_rgba8().pixels().all(|pixel| pixel[3] == 0));
    }

    #[test]
    fn dxt1_four_color_selector_three_stays_opaque() {
        let bytes = dds_block(b"DXT1", 4, 4, &dxt1_block(u16::MAX, 0, u32::MAX));
        let decoded = decode_texture("opaque.dds", &bytes).expect("decode opaque DXT1");
        assert!(decoded.to_rgba8().pixels().all(|pixel| pixel[3] == 255));
    }

    #[test]
    fn dx10_bc1_restores_binary_alpha() {
        let bytes = dx10_bc1(4, 4, &dxt1_block(0, u16::MAX, u32::MAX));
        let decoded = decode_texture("cutout-dx10.dds", &bytes).expect("decode DX10 BC1 cutout");
        assert!(decoded.to_rgba8().pixels().all(|pixel| pixel[3] == 0));
    }

    #[test]
    fn dxt1_alpha_crops_padded_pixels_for_non_block_dimensions() {
        let bytes = dds_block(b"DXT1", 3, 2, &dxt1_block(0, u16::MAX, u32::MAX));
        let decoded = decode_texture("cropped.dds", &bytes).expect("decode cropped DXT1");
        assert_eq!((decoded.width(), decoded.height()), (3, 2));
        assert_eq!(decoded.to_rgba8().pixels().count(), 6);
        assert!(decoded.to_rgba8().pixels().all(|pixel| pixel[3] == 0));
    }

    #[test]
    fn truncated_dxt1_payload_is_a_structured_texture_error() {
        let mut bytes = dds_block(b"DXT1", 4, 4, &dxt1_block(0, u16::MAX, 0));
        bytes.pop();
        let error = decode_texture("truncated.dds", &bytes).unwrap_err();
        assert!(matches!(
            error,
            GlbError::TextureDecode { path, .. } if path == "truncated.dds"
        ));
    }

    #[test]
    fn dxt3_and_dxt5_keep_the_image_decoder_alpha() {
        let mut dxt3 = [0; 16];
        dxt3[8..16].copy_from_slice(&dxt1_block(u16::MAX, 0, 0));
        let decoded =
            decode_texture("dxt3.dds", &dds_block(b"DXT3", 4, 4, &dxt3)).expect("decode DXT3");
        assert!(decoded.to_rgba8().pixels().all(|pixel| pixel[3] == 0));

        let mut dxt5 = [0; 16];
        dxt5[0] = 0;
        dxt5[1] = 255;
        dxt5[8..16].copy_from_slice(&dxt1_block(u16::MAX, 0, 0));
        let decoded =
            decode_texture("dxt5.dds", &dds_block(b"DXT5", 4, 4, &dxt5)).expect("decode DXT5");
        assert!(decoded.to_rgba8().pixels().all(|pixel| pixel[3] == 0));
    }

    #[test]
    fn encoded_glb_validates_and_embeds_corrected_rgba() {
        let texture_path = "textures/cutout.dds";
        let scene = Scene {
            nodes: Vec::new(),
            roots: Vec::new(),
            materials: vec![SceneMaterial {
                name: "Cutout".into(),
                base_color: [1.0; 4],
                emissive: [0.8, 0.4, 0.2],
                emissive_multiplier: 1.0,
                roughness: 1.0,
                alpha_mode: SceneAlphaMode::Mask,
                alpha_cutoff: Some(0.5),
                double_sided: false,
                unlit: false,
                diffuse_texture: Some(texture_path.into()),
                normal_texture: None,
                specular_texture: None,
                glow_texture: None,
                height_texture: None,
                environment_texture: None,
                environment_mask: None,
                shader_type: 0,
                shader_flags_1: super::super::SHADER_FLAG1_SPECULAR,
                shader_flags_2: super::super::SHADER_FLAG2_GLOW_MAP,
            }],
            skins: Vec::new(),
            issues: Vec::new(),
            statistics: super::super::SceneStatistics::default(),
            animations: Vec::new(),
            animation_sound_cues: Vec::new(),
        };
        let mut textures = BTreeMap::new();
        textures.insert(
            texture_path.into(),
            dds_block(b"DXT1", 4, 4, &dxt1_block(0, u16::MAX, u32::MAX)),
        );
        let output = encode_glb(&scene, &textures, &GlbOptions::default()).expect("encode GLB");
        let gltf = gltf::Gltf::from_slice(&output.bytes).expect("validate GLB");
        let json_length = u32::from_le_bytes(output.bytes[12..16].try_into().unwrap()) as usize;
        let document: serde_json::Value =
            serde_json::from_slice(&output.bytes[20..20 + json_length]).unwrap();
        assert_eq!(
            document["materials"][0]["extras"]["bevyout_fallout_material"]["features"]["glow_map"],
            true
        );
        assert_eq!(
            document["materials"][0]["extras"]["bevyout_fallout_material"]["features"]["specular"],
            true
        );
        assert_eq!(
            document["materials"][0]["emissiveFactor"],
            serde_json::json!([0.2, 0.1, 0.05])
        );
        assert_eq!(
            gltf.document.materials().next().unwrap().alpha_mode(),
            gltf::material::AlphaMode::Mask
        );
        let blob = gltf.blob.as_deref().expect("GLB binary chunk");
        let source = gltf.document.images().next().unwrap().source();
        let gltf::image::Source::View { view, .. } = source else {
            panic!("embedded image must use a buffer view");
        };
        let png = &blob[view.offset()..view.offset() + view.length()];
        let image = image::load_from_memory(png)
            .expect("decode embedded PNG")
            .to_rgba8();
        assert!(image.pixels().all(|pixel| pixel[3] == 0));
    }

    #[test]
    fn encoded_glb_preserves_skin_joints_and_weights() {
        let identity = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let transform = super::super::Transform {
            translation: [0.0; 3],
            rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            scale: 1.0,
        };
        let scene = Scene {
            nodes: vec![
                super::super::SceneNode {
                    source_block: 0,
                    name: "Root".into(),
                    transform,
                    children: vec![1],
                    mesh: None,
                    skin: None,
                },
                super::super::SceneNode {
                    source_block: 1,
                    name: "Bone".into(),
                    transform,
                    children: vec![2],
                    mesh: None,
                    skin: None,
                },
                super::super::SceneNode {
                    source_block: 2,
                    name: "Skinned".into(),
                    transform,
                    children: Vec::new(),
                    mesh: Some(SceneMesh {
                        name: "Skinned".into(),
                        positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                        normals: Vec::new(),
                        tangents: Vec::new(),
                        colors: Vec::new(),
                        tex_coords: Vec::new(),
                        joints: vec![[0, 0, 0, 0]; 3],
                        weights: vec![[1.0, 0.0, 0.0, 0.0]; 3],
                        indices: vec![0, 1, 2],
                        material: None,
                    }),
                    skin: Some(0),
                },
            ],
            roots: vec![0],
            materials: Vec::new(),
            skins: vec![SceneSkin {
                name: "Skin".into(),
                joints: vec![1],
                inverse_bind_matrices: vec![identity],
                skeleton: Some(0),
            }],
            issues: Vec::new(),
            statistics: super::super::SceneStatistics {
                source_meshes: 1,
                source_vertices: 3,
                source_triangles: 1,
            },
            animations: Vec::new(),
            animation_sound_cues: Vec::new(),
        };
        let output = encode_glb(&scene, &BTreeMap::new(), &GlbOptions::default())
            .expect("encode skinned GLB");
        let gltf = gltf::Gltf::from_slice(&output.bytes).expect("validate skinned GLB");
        let skin = gltf.document.skins().next().expect("skin");
        assert_eq!(skin.joints().count(), 1);
        let primitive = gltf
            .document
            .meshes()
            .next()
            .unwrap()
            .primitives()
            .next()
            .unwrap();
        assert!(primitive.get(&gltf::Semantic::Joints(0)).is_some());
        assert!(primitive.get(&gltf::Semantic::Weights(0)).is_some());
    }

    #[test]
    fn encoded_glb_preserves_xyzw_animation_rotations() {
        let transform = super::super::Transform {
            translation: [0.0; 3],
            rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            scale: 1.0,
        };
        let half_angle = std::f32::consts::FRAC_PI_4;
        let expected = [0.0, 0.0, half_angle.sin(), half_angle.cos()];
        let scene = Scene {
            nodes: vec![super::super::SceneNode {
                source_block: 0,
                name: "Door".into(),
                transform,
                children: Vec::new(),
                mesh: None,
                skin: None,
            }],
            roots: vec![0],
            materials: Vec::new(),
            skins: Vec::new(),
            issues: Vec::new(),
            statistics: super::super::SceneStatistics::default(),
            animations: vec![SceneAnimation {
                name: "Open".into(),
                start_time: 0.0,
                stop_time: 1.0,
                channels: vec![SceneAnimationChannel {
                    node: 0,
                    translations: Vec::new(),
                    rotations: vec![super::super::AnimationKey {
                        time: 0.0,
                        value: expected,
                    }],
                    scales: Vec::new(),
                }],
            }],
            animation_sound_cues: Vec::new(),
        };
        let output = encode_glb(&scene, &BTreeMap::new(), &GlbOptions::default())
            .expect("encode animated GLB");
        let gltf = gltf::Gltf::from_slice(&output.bytes).expect("validate animated GLB");
        let blob = gltf.blob.as_deref().expect("GLB binary chunk");
        let channel = gltf
            .document
            .animations()
            .next()
            .expect("animation")
            .channels()
            .next()
            .expect("rotation channel");
        let reader = channel.reader(|_| Some(blob));
        let gltf::animation::util::ReadOutputs::Rotations(rotations) =
            reader.read_outputs().expect("rotation values")
        else {
            panic!("expected rotation output");
        };
        let values = rotations.into_f32().collect::<Vec<_>>();
        assert_eq!(values.len(), 1);
        for (actual, expected) in values[0].into_iter().zip(expected) {
            assert!(
                (actual - expected).abs() <= 1.0e-6,
                "{actual} != {expected}"
            );
        }
    }

    #[test]
    fn position_bounds_cover_every_axis() {
        assert_eq!(
            position_bounds(&[[1.0, -2.0, 3.0], [-4.0, 5.0, 0.0]]),
            ([-4.0, -2.0, 0.0], [1.0, 5.0, 3.0])
        );
    }
}
