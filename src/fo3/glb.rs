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
    Scene, SceneAlphaMode, SceneAnimation, SceneAnimationChannel, SceneMaterial, SceneMesh,
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
            skin: None,
            weights: None,
        });
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
        let glow = source
            .glow_texture
            .as_deref()
            .map(|path| self.texture(path))
            .transpose()?
            .flatten();
        let texture_info = |index| json::texture::Info {
            index,
            tex_coord: 0,
            extensions: None,
            extras: Default::default(),
        };
        let extensions = (source.unlit || source.emissive_multiplier > 1.0).then(|| {
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
            json::extensions::material::Material {
                unlit: source.unlit.then_some(json::extensions::material::Unlit {}),
                emissive_strength: (source.emissive_multiplier > 1.0).then_some(
                    json::extensions::material::EmissiveStrength {
                        emissive_strength: json::extensions::material::EmissiveStrengthFactor(
                            source.emissive_multiplier,
                        ),
                    },
                ),
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
            emissive_factor: material::EmissiveFactor(source.emissive),
            extensions,
            extras: Default::default(),
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
        let decoded = image::load_from_memory(bytes).map_err(|error| GlbError::TextureDecode {
            path: path.to_owned(),
            message: error.to_string(),
        })?;
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

    #[test]
    fn position_bounds_cover_every_axis() {
        assert_eq!(
            position_bounds(&[[1.0, -2.0, 3.0], [-4.0, 5.0, 0.0]]),
            ([-4.0, -2.0, 0.0], [1.0, 5.0, 3.0])
        );
    }
}
