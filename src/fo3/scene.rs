use std::collections::{HashMap, HashSet};

use glam::{EulerRot, Mat3, Mat4, Quat, Vec3};
use thiserror::Error;

use super::{
    AlphaProperty, AnimationKey, AnimationKeyGroup, AvObject, Document, Geometry, GeometryData,
    MaterialProperty, NoLightingProperty, Node, ShaderProperty, ShaderTextureSet, SkinData,
    SkinInstance, SkinPartitionData, TextKeyExtraData, Transform, TransformData,
    TransformInterpolator, TriStripsData, TypedBlock,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneAlphaMode {
    Opaque,
    Mask,
    Blend,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneMaterial {
    pub name: String,
    pub base_color: [f32; 4],
    pub emissive: [f32; 3],
    pub emissive_multiplier: f32,
    pub roughness: f32,
    pub alpha_mode: SceneAlphaMode,
    pub alpha_cutoff: Option<f32>,
    pub double_sided: bool,
    pub unlit: bool,
    pub diffuse_texture: Option<String>,
    pub normal_texture: Option<String>,
    pub specular_texture: Option<String>,
    pub glow_texture: Option<String>,
    pub height_texture: Option<String>,
    pub environment_texture: Option<String>,
    pub environment_mask: Option<String>,
    pub shader_type: u32,
    pub shader_flags_1: u32,
    pub shader_flags_2: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneMesh {
    pub name: String,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub tangents: Vec<[f32; 4]>,
    pub colors: Vec<[f32; 4]>,
    pub tex_coords: Vec<[f32; 2]>,
    pub joints: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
    pub indices: Vec<u16>,
    pub material: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneNode {
    pub source_block: usize,
    pub name: String,
    pub transform: Transform,
    pub children: Vec<usize>,
    pub mesh: Option<SceneMesh>,
    pub skin: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneSkin {
    pub name: String,
    pub joints: Vec<usize>,
    pub inverse_bind_matrices: Vec<[f32; 16]>,
    pub skeleton: Option<usize>,
}

type VertexSkinInfluences = (Vec<[u16; 4]>, Vec<[f32; 4]>);

#[derive(Debug, Clone, PartialEq)]
pub struct SceneAnimation {
    pub name: String,
    pub start_time: f32,
    pub stop_time: f32,
    pub channels: Vec<SceneAnimationChannel>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneAnimationChannel {
    pub node: usize,
    pub translations: Vec<AnimationKey<[f32; 3]>>,
    pub rotations: Vec<AnimationKey<[f32; 4]>>,
    pub scales: Vec<AnimationKey<f32>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneAnimationSoundCue {
    pub sequence: String,
    pub time: f32,
    pub editor_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneIssue {
    pub source_block: usize,
    pub type_name: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SceneStatistics {
    pub source_meshes: usize,
    pub source_vertices: usize,
    pub source_triangles: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub nodes: Vec<SceneNode>,
    pub roots: Vec<usize>,
    pub materials: Vec<SceneMaterial>,
    pub skins: Vec<SceneSkin>,
    pub issues: Vec<SceneIssue>,
    pub statistics: SceneStatistics,
    pub animations: Vec<SceneAnimation>,
    pub animation_sound_cues: Vec<SceneAnimationSoundCue>,
}

impl Scene {
    pub fn is_lossless(&self) -> bool {
        self.issues.is_empty()
    }

    pub fn has_visible_geometry(&self) -> bool {
        self.nodes.iter().any(|node| {
            node.mesh
                .as_ref()
                .is_some_and(|mesh| !mesh.positions.is_empty() && !mesh.indices.is_empty())
        })
    }

    pub fn has_visible_weighted_geometry(&self) -> bool {
        self.nodes.iter().any(|node| {
            node.skin.is_some()
                && node.mesh.as_ref().is_some_and(|mesh| {
                    !mesh.positions.is_empty()
                        && !mesh.indices.is_empty()
                        && mesh.joints.len() == mesh.positions.len()
                        && mesh.weights.len() == mesh.positions.len()
                })
        })
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ActorSceneMergeError {
    #[error("actor part skin joint {joint:?} is absent from the shared skeleton")]
    MissingJoint { joint: String },
    #[error("actor part skeleton root {root:?} is absent from the shared skeleton")]
    MissingSkeletonRoot { root: String },
    #[error("actor scene has an invalid rest transform at node {node:?}")]
    InvalidRestTransform { node: String },
    #[error("actor attachment node {attachment:?} is absent from the shared skeleton")]
    MissingAttachment { attachment: String },
}

/// Merges a visual actor part onto `actor`'s shared skeleton.
///
/// Joint node indices are remapped by normalized bone name, but the part's
/// authored inverse bind matrices are retained. Bethesda actor-part skin data
/// uses a part-local bind space which cannot be reconstructed solely from the
/// shared skeleton hierarchy without visibly deforming the intact mesh.
pub fn merge_actor_scene(actor: &mut Scene, part: &Scene) -> Result<(), ActorSceneMergeError> {
    let mut merged = actor.clone();
    merge_actor_scene_inner(&mut merged, part, None)?;
    *actor = merged;
    Ok(())
}

/// Merges a visual actor part whose independent roots are authored in the
/// local space of a shared skeleton node.
///
/// FO3 head accessories such as hair and eyes are separate NIFs. Their roots
/// have head-local coordinates, so treating those roots as actor roots leaves
/// the geometry at the actor origin. Roots that already match a shared node
/// still reuse that node normally; only newly appended roots are parented to
/// `attachment`.
pub fn merge_actor_scene_attached(
    actor: &mut Scene,
    part: &Scene,
    attachment: &str,
) -> Result<(), ActorSceneMergeError> {
    let mut merged = actor.clone();
    merge_actor_scene_inner(&mut merged, part, Some(attachment))?;
    *actor = merged;
    Ok(())
}

/// Rebuilds inverse bind matrices from scene-node rest transforms.
///
/// This is suitable for synthetic scenes whose mesh bind space is exactly the
/// scene-node hierarchy. Do not apply it after [`merge_actor_scene`]: imported
/// Bethesda actor parts carry authoritative part-local matrices.
pub fn recalculate_actor_inverse_bind_matrices(
    actor: &mut Scene,
) -> Result<(), ActorSceneMergeError> {
    let mut parents = vec![None; actor.nodes.len()];
    for (parent, node) in actor.nodes.iter().enumerate() {
        for &child in &node.children {
            if let Some(slot) = parents.get_mut(child) {
                *slot = Some(parent);
            }
        }
    }
    let mut globals = vec![None; actor.nodes.len()];
    let mut visiting = HashSet::new();
    for node in 0..actor.nodes.len() {
        actor_global_transform(actor, &parents, &mut globals, &mut visiting, node)?;
    }
    for (skin_index, skin) in actor.skins.iter_mut().enumerate() {
        let mesh_node = actor
            .nodes
            .iter()
            .position(|node| node.skin == Some(skin_index))
            .and_then(|node| globals[node])
            .unwrap_or(Mat4::IDENTITY);
        let mut inverse_bind_matrices = Vec::with_capacity(skin.joints.len());
        for &joint in &skin.joints {
            let Some(joint_global) = globals.get(joint).copied().flatten() else {
                return Err(ActorSceneMergeError::InvalidRestTransform {
                    node: format!("node#{joint}"),
                });
            };
            let inverse_bind = joint_global.inverse() * mesh_node;
            if !inverse_bind.is_finite() {
                return Err(ActorSceneMergeError::InvalidRestTransform {
                    node: actor.nodes[joint].name.clone(),
                });
            }
            inverse_bind_matrices.push(inverse_bind.to_cols_array());
        }
        skin.inverse_bind_matrices = inverse_bind_matrices;
    }
    Ok(())
}

fn actor_global_transform(
    actor: &Scene,
    parents: &[Option<usize>],
    globals: &mut [Option<Mat4>],
    visiting: &mut HashSet<usize>,
    node: usize,
) -> Result<Mat4, ActorSceneMergeError> {
    if let Some(global) = globals.get(node).copied().flatten() {
        return Ok(global);
    }
    if !visiting.insert(node) {
        return Err(ActorSceneMergeError::InvalidRestTransform {
            node: actor
                .nodes
                .get(node)
                .map(|node| node.name.clone())
                .unwrap_or_else(|| format!("node#{node}")),
        });
    }
    let local = actor
        .nodes
        .get(node)
        .map(|node| Mat4::from_cols_array(&transform_matrix(&node.transform)))
        .ok_or_else(|| ActorSceneMergeError::InvalidRestTransform {
            node: format!("node#{node}"),
        })?;
    let global = if let Some(parent) = parents.get(node).copied().flatten() {
        actor_global_transform(actor, parents, globals, visiting, parent)? * local
    } else {
        local
    };
    visiting.remove(&node);
    if !global.is_finite() {
        return Err(ActorSceneMergeError::InvalidRestTransform {
            node: actor.nodes[node].name.clone(),
        });
    }
    globals[node] = Some(global);
    Ok(global)
}

fn merge_actor_scene_inner(
    actor: &mut Scene,
    part: &Scene,
    attachment: Option<&str>,
) -> Result<(), ActorSceneMergeError> {
    let shared_node_count = actor.nodes.len();
    let attachment_node = attachment
        .map(|attachment| {
            let key = actor_node_key(attachment);
            actor
                .nodes
                .iter()
                .enumerate()
                .find_map(|(index, node)| {
                    (node.mesh.is_none() && actor_node_key(&node.name) == key).then_some(index)
                })
                .ok_or_else(|| ActorSceneMergeError::MissingAttachment {
                    attachment: attachment.to_owned(),
                })
        })
        .transpose()?;
    let material_offset = actor.materials.len();
    actor.materials.extend(part.materials.iter().cloned());

    let mut reusable = HashMap::<String, usize>::new();
    for (index, node) in actor.nodes.iter().enumerate() {
        if node.mesh.is_none() {
            reusable.entry(actor_node_key(&node.name)).or_insert(index);
        }
    }
    let mut node_map = vec![usize::MAX; part.nodes.len()];
    let mut appended_part_nodes = vec![false; part.nodes.len()];
    let mut appended = Vec::new();
    for (index, node) in part.nodes.iter().enumerate() {
        if node.mesh.is_none() {
            if let Some(&existing) = reusable.get(&actor_node_key(&node.name)) {
                node_map[index] = existing;
                continue;
            }
        }
        node_map[index] = actor.nodes.len();
        appended_part_nodes[index] = true;
        appended.push((index, actor.nodes.len()));
        let mut node = node.clone();
        node.children.clear();
        node.skin = None;
        if let Some(mesh) = &mut node.mesh {
            if let Some(material) = &mut mesh.material {
                *material += material_offset;
            }
        }
        actor.nodes.push(node);
    }

    let skin_offset = actor.skins.len();
    for skin in &part.skins {
        let mut remapped = skin.clone();
        remapped.joints = skin
            .joints
            .iter()
            .map(|&joint| {
                node_map
                    .get(joint)
                    .copied()
                    .filter(|&node| node < shared_node_count)
                    .ok_or_else(|| ActorSceneMergeError::MissingJoint {
                        joint: part
                            .nodes
                            .get(joint)
                            .map(|node| node.name.clone())
                            .unwrap_or_else(|| format!("node#{joint}")),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        remapped.skeleton = skin
            .skeleton
            .map(|root| {
                node_map
                    .get(root)
                    .copied()
                    .filter(|&node| node < shared_node_count)
                    .ok_or_else(|| ActorSceneMergeError::MissingSkeletonRoot {
                        root: part
                            .nodes
                            .get(root)
                            .map(|node| node.name.clone())
                            .unwrap_or_else(|| format!("node#{root}")),
                    })
            })
            .transpose()?;
        actor.skins.push(remapped);
    }

    for (part_index, actor_index) in appended {
        actor.nodes[actor_index].children = part.nodes[part_index]
            .children
            .iter()
            .filter(|&&child| appended_part_nodes.get(child).copied().unwrap_or(false))
            .filter_map(|&child| node_map.get(child).copied())
            .filter(|&child| child != usize::MAX)
            .collect();
        actor.nodes[actor_index].skin = part.nodes[part_index].skin.map(|skin| skin + skin_offset);
    }
    for (part_index, node) in part.nodes.iter().enumerate() {
        let actor_index = node_map[part_index];
        if actor_index >= actor.nodes.len() || actor.nodes[actor_index].mesh.is_some() {
            continue;
        }
        for &child in &node.children {
            if !appended_part_nodes.get(child).copied().unwrap_or(false) {
                continue;
            }
            if let Some(&child) = node_map.get(child) {
                if child != usize::MAX && !actor.nodes[actor_index].children.contains(&child) {
                    actor.nodes[actor_index].children.push(child);
                }
            }
        }
    }
    for &root in &part.roots {
        if !appended_part_nodes.get(root).copied().unwrap_or(false) {
            continue;
        }
        if let Some(&root) = node_map.get(root) {
            if root != usize::MAX && !actor.nodes.iter().any(|node| node.children.contains(&root)) {
                if let Some(attachment_node) = attachment_node {
                    if !actor.nodes[attachment_node].children.contains(&root) {
                        actor.nodes[attachment_node].children.push(root);
                    }
                } else if !actor.roots.contains(&root) {
                    actor.roots.push(root);
                }
            }
        }
    }
    actor.issues.extend(part.issues.iter().cloned());
    actor.statistics.source_meshes += part.statistics.source_meshes;
    actor.statistics.source_vertices += part.statistics.source_vertices;
    actor.statistics.source_triangles += part.statistics.source_triangles;
    Ok(())
}

fn actor_node_key(name: &str) -> String {
    let mut parts = name
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    let side = (parts.len() >= 3 && parts.first().is_some_and(|part| part.starts_with("bip")))
        .then(|| parts.pop_if(|part| part == "l" || part == "r"))
        .flatten();
    if let Some(side) = side {
        parts.insert(1, side);
    }
    parts.concat()
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SceneError {
    #[error("NIF has no usable scene roots")]
    NoRoots,
    #[error(
        "block {source_block} field {field} references invalid block {reference} (block count {block_count})"
    )]
    InvalidBlockReference {
        source_block: usize,
        field: &'static str,
        reference: i32,
        block_count: usize,
    },
    #[error("cycle detected while traversing scene block {block}")]
    SceneCycle { block: usize },
    #[error("could not decode block {block}: {message}")]
    Decode { block: usize, message: String },
}

pub fn extract_scene(document: &Document) -> Result<Scene, SceneError> {
    let mut builder = SceneBuilder {
        document,
        scene: Scene {
            nodes: Vec::new(),
            roots: Vec::new(),
            materials: Vec::new(),
            skins: Vec::new(),
            issues: Vec::new(),
            statistics: SceneStatistics::default(),
            animations: Vec::new(),
            animation_sound_cues: Vec::new(),
        },
        node_by_block: HashMap::new(),
        visiting: HashSet::new(),
    };

    for &root in &document.roots {
        if root < 0 {
            continue;
        }
        if let Some(node) = builder.visit(root as usize)? {
            builder.scene.roots.push(node);
        }
    }
    builder.scene.roots.sort_unstable();
    builder.scene.roots.dedup();
    if builder.scene.roots.is_empty() {
        return Err(SceneError::NoRoots);
    }
    builder.scene.animations = extract_animations(document, &builder.scene.nodes)?;
    builder.scene.animation_sound_cues = extract_animation_sound_cues(document)?;
    Ok(builder.scene)
}

fn extract_animation_sound_cues(
    document: &Document,
) -> Result<Vec<SceneAnimationSoundCue>, SceneError> {
    let mut cues = Vec::new();
    for index in 0..document.blocks.len() {
        let sequence = match document
            .decode_block(index)
            .map_err(|error| SceneError::Decode {
                block: index,
                message: error.to_string(),
            })? {
            TypedBlock::ControllerSequence(sequence) => sequence,
            _ => continue,
        };
        if sequence.text_keys < 0 {
            continue;
        }
        let text_key_index = sequence.text_keys as usize;
        let text_keys =
            document
                .decode_block(text_key_index)
                .map_err(|error| SceneError::Decode {
                    block: text_key_index,
                    message: error.to_string(),
                })?;
        let TypedBlock::TextKeyExtraData(TextKeyExtraData { keys }) = text_keys else {
            return Err(SceneError::InvalidBlockReference {
                source_block: index,
                field: "sequence text keys",
                reference: sequence.text_keys,
                block_count: document.blocks.len(),
            });
        };
        for key in keys {
            append_animation_sound_cues(&mut cues, &sequence.name, key.time, &key.value);
        }
    }
    cues.sort_by(|left, right| {
        left.sequence
            .to_ascii_lowercase()
            .cmp(&right.sequence.to_ascii_lowercase())
            .then_with(|| left.time.total_cmp(&right.time))
            .then_with(|| {
                left.editor_id
                    .to_ascii_lowercase()
                    .cmp(&right.editor_id.to_ascii_lowercase())
            })
            .then_with(|| left.editor_id.cmp(&right.editor_id))
    });
    Ok(cues)
}

fn append_animation_sound_cues(
    cues: &mut Vec<SceneAnimationSoundCue>,
    sequence: &str,
    time: f32,
    text: &str,
) {
    for line in text.replace('\r', "\n").split('\n') {
        let Some((prefix, value)) = line.split_once(':') else {
            continue;
        };
        if prefix.trim().eq_ignore_ascii_case("sound") {
            let editor_id = value.trim();
            if !editor_id.is_empty() {
                cues.push(SceneAnimationSoundCue {
                    sequence: sequence.to_owned(),
                    time,
                    editor_id: editor_id.to_owned(),
                });
            }
        }
    }
}

fn extract_animations(
    document: &Document,
    nodes: &[SceneNode],
) -> Result<Vec<SceneAnimation>, SceneError> {
    let mut by_source = HashMap::new();
    let mut by_name = HashMap::<String, usize>::new();
    for (index, node) in nodes.iter().enumerate() {
        by_source.insert(node.source_block, index);
        by_name.entry(node.name.clone()).or_insert(index);
        if let Some((base, _)) = node_base(document, node.source_block)? {
            if let Some(name) = &base.object.name {
                by_name.entry(name.clone()).or_insert(index);
            }
        }
    }

    let mut animations = Vec::new();
    let mut sequence_nodes = HashSet::new();
    for index in 0..document.blocks.len() {
        let sequence = match document
            .decode_block(index)
            .map_err(|error| SceneError::Decode {
                block: index,
                message: error.to_string(),
            })? {
            TypedBlock::ControllerSequence(sequence) => sequence,
            _ => continue,
        };
        let mut channels = Vec::new();
        for controlled in sequence.controlled_blocks {
            let node = by_name.get(&controlled.node_name).copied().or_else(|| {
                controlled
                    .node_name
                    .split_once(':')
                    .and_then(|(name, _)| by_name.get(name).copied())
            });
            let Some(node) = node else { continue };
            let Some(data) = transform_data(document, controlled.interpolator)? else {
                continue;
            };
            sequence_nodes.insert(node);
            merge_animation_channel(&mut channels, node, data);
        }
        if !channels.is_empty() {
            let stop_time = if sequence.stop_time > sequence.start_time {
                sequence.stop_time
            } else {
                channels
                    .iter()
                    .flat_map(|channel| {
                        channel
                            .translations
                            .iter()
                            .map(|key| key.time)
                            .chain(channel.rotations.iter().map(|key| key.time))
                            .chain(channel.scales.iter().map(|key| key.time))
                    })
                    .fold(sequence.start_time, f32::max)
            };
            animations.push(SceneAnimation {
                name: sequence.name,
                start_time: sequence.start_time,
                stop_time,
                channels,
            });
        }
    }

    // Some props carry a controller directly on the node and no controller sequence. Preserve
    // those single-track animations under a stable synthetic name.
    for (node_index, node) in nodes.iter().enumerate() {
        if sequence_nodes.contains(&node_index) {
            continue;
        }
        let Some((base, _)) = node_base(document, node.source_block)? else {
            continue;
        };
        let controller = base.object.controller;
        if controller < 0 {
            continue;
        }
        let controller = match document
            .decode_block(controller as usize)
            .map_err(|error| SceneError::Decode {
                block: controller as usize,
                message: error.to_string(),
            })? {
            TypedBlock::TransformController(controller) => controller,
            _ => continue,
        };
        let Some(data) = transform_data(document, controller.interpolator)? else {
            continue;
        };
        let mut channels = Vec::new();
        merge_animation_channel(&mut channels, node_index, data);
        let stop_time = controller.stop_time.max(controller.start_time);
        animations.push(SceneAnimation {
            name: format!("{}:direct", node.name),
            start_time: controller.start_time,
            stop_time,
            channels,
        });
    }
    Ok(animations)
}

fn node_base(
    document: &Document,
    source_block: usize,
) -> Result<Option<(AvObject, bool)>, SceneError> {
    let block = document
        .decode_block(source_block)
        .map_err(|error| SceneError::Decode {
            block: source_block,
            message: error.to_string(),
        })?;
    Ok(match block {
        TypedBlock::Node(node) => Some((node.base, true)),
        TypedBlock::Geometry(geometry) => Some((geometry.base, false)),
        _ => None,
    })
}

fn transform_data(
    document: &Document,
    interpolator_reference: i32,
) -> Result<Option<TransformData>, SceneError> {
    if interpolator_reference < 0 {
        return Ok(None);
    }
    let interpolator = document
        .decode_block(interpolator_reference as usize)
        .map_err(|error| SceneError::Decode {
            block: interpolator_reference as usize,
            message: error.to_string(),
        })?;
    let TypedBlock::TransformInterpolator(TransformInterpolator { data, .. }) = interpolator else {
        return Ok(None);
    };
    if data < 0 {
        return Ok(None);
    }
    let data = document
        .decode_block(data as usize)
        .map_err(|error| SceneError::Decode {
            block: data as usize,
            message: error.to_string(),
        })?;
    Ok(match data {
        TypedBlock::TransformData(data) => Some(data),
        _ => None,
    })
}

fn merge_animation_channel(
    channels: &mut Vec<SceneAnimationChannel>,
    node: usize,
    data: TransformData,
) {
    let TransformData {
        rotations: nif_wxyz_rotations,
        xyz_rotations,
        translations,
        scales,
    } = data;
    let mut rotations = nif_wxyz_rotations
        .into_iter()
        .map(nif_wxyz_rotation_key_to_gltf_xyzw)
        .collect::<Vec<_>>();
    if let Some(xyz_rotations) = xyz_rotations {
        rotations.extend(xyz_rotation_keys(&xyz_rotations));
    }
    if translations.keys.is_empty() && rotations.is_empty() && scales.keys.is_empty() {
        return;
    }
    let Some(channel) = channels.iter_mut().find(|channel| channel.node == node) else {
        channels.push(SceneAnimationChannel {
            node,
            translations: translations.keys,
            rotations,
            scales: scales.keys,
        });
        return;
    };
    channel.translations.extend(translations.keys);
    channel.rotations.extend(rotations);
    channel.scales.extend(scales.keys);
}

/// Converts the NIF quaternion convention (WXYZ) to glTF's XYZW convention.
///
/// XYZ Euler tracks are constructed with `glam::Quat` and already use XYZW,
/// so they must bypass this source-format conversion.
fn nif_wxyz_rotation_key_to_gltf_xyzw(key: AnimationKey<[f32; 4]>) -> AnimationKey<[f32; 4]> {
    let [w, x, y, z] = key.value;
    AnimationKey {
        time: key.time,
        value: [x, y, z, w],
    }
}

fn xyz_rotation_keys(groups: &[AnimationKeyGroup<f32>; 3]) -> Vec<AnimationKey<[f32; 4]>> {
    let mut times = groups
        .iter()
        .flat_map(|group| group.keys.iter().map(|key| key.time))
        .collect::<Vec<_>>();
    times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    times.dedup_by(|a, b| (*a - *b).abs() <= 1.0e-6);
    times
        .into_iter()
        .map(|time| {
            let angles = groups
                .iter()
                .map(|group| sample_scalar_group(group, time))
                .collect::<Vec<_>>();
            AnimationKey {
                time,
                value: Quat::from_euler(EulerRot::XYZ, angles[0], angles[1], angles[2]).to_array(),
            }
        })
        .collect()
}

fn sample_scalar_group(group: &AnimationKeyGroup<f32>, time: f32) -> f32 {
    let Some(first) = group.keys.first() else {
        return 0.0;
    };
    if time <= first.time {
        return first.value;
    }
    let Some(last) = group.keys.last() else {
        return first.value;
    };
    if time >= last.time {
        return last.value;
    }
    for pair in group.keys.windows(2) {
        let [left, right] = pair else { continue };
        if time <= right.time {
            let span = right.time - left.time;
            let weight = if span.abs() <= f32::EPSILON {
                0.0
            } else {
                (time - left.time) / span
            };
            return left.value + (right.value - left.value) * weight;
        }
    }
    last.value
}

struct SceneBuilder<'a> {
    document: &'a Document,
    scene: Scene,
    node_by_block: HashMap<usize, usize>,
    visiting: HashSet<usize>,
}

impl SceneBuilder<'_> {
    fn visit(&mut self, block_index: usize) -> Result<Option<usize>, SceneError> {
        if let Some(&node) = self.node_by_block.get(&block_index) {
            return Ok(Some(node));
        }
        if !self.visiting.insert(block_index) {
            return Err(SceneError::SceneCycle { block: block_index });
        }
        let block = self.block(block_index, block_index, "scene child")?;
        let result = match self.decode(block_index)? {
            TypedBlock::Node(node) => self.visit_node(block_index, node),
            TypedBlock::Geometry(geometry) => self.visit_geometry(block_index, geometry),
            TypedBlock::Unsupported => {
                self.issue(
                    block_index,
                    format!(
                        "reachable {} is not supported by the props converter",
                        block.type_name
                    ),
                );
                Ok(None)
            }
            _ => {
                self.issue(
                    block_index,
                    format!(
                        "reachable {} is data rather than a scene object",
                        block.type_name
                    ),
                );
                Ok(None)
            }
        };
        self.visiting.remove(&block_index);
        result
    }

    fn visit_node(&mut self, block_index: usize, node: Node) -> Result<Option<usize>, SceneError> {
        let scene_index = self.push_node(block_index, &node.base, None);
        let children = if node.lod_data.is_some() {
            node.children
                .first()
                .copied()
                .into_iter()
                .collect::<Vec<_>>()
        } else if let Some((_, active)) = node.switch {
            usize::try_from(active)
                .ok()
                .and_then(|index| node.children.get(index).copied())
                .into_iter()
                .collect()
        } else {
            node.children
        };
        let mut scene_children = Vec::new();
        for child in children {
            if child < 0 {
                continue;
            }
            self.block(block_index, child as usize, "child")?;
            if let Some(child) = self.visit(child as usize)? {
                scene_children.push(child);
            }
        }
        self.scene.nodes[scene_index].children = scene_children;
        Ok(Some(scene_index))
    }

    fn visit_geometry(
        &mut self,
        block_index: usize,
        geometry: Geometry,
    ) -> Result<Option<usize>, SceneError> {
        let data_index = self.reference(block_index, geometry.data, "geometry data")?;
        let data_type = self.document.blocks[data_index].type_name.clone();
        let (data, mut triangles) = match self.decode(data_index)? {
            TypedBlock::TriShapeData(data) => {
                let triangles = data.triangles.clone();
                (data.geometry, triangles)
            }
            TypedBlock::TriStripsData(data) => {
                let triangles = triangulate_strips(&data);
                (data.geometry, triangles)
            }
            _ => {
                self.issue(
                    data_index,
                    format!("geometry data type {data_type} is not supported"),
                );
                return Ok(None);
            }
        };
        if data.vertices.is_empty() || triangles.is_empty() {
            self.issue(data_index, "geometry has no renderable triangles".into());
            return Ok(None);
        }
        let vertex_count = data.vertices.len();
        let mut skin = None;
        let (joints, weights) = if geometry.skin_instance >= 0 {
            let skin_index =
                self.reference(block_index, geometry.skin_instance, "skin instance")?;
            let instance = match self.decode(skin_index)? {
                TypedBlock::SkinInstance(instance) => instance,
                _ => {
                    self.issue(skin_index, "skin instance has an unsupported type".into());
                    return Ok(None);
                }
            };
            let skin_data_index = self.reference(skin_index, instance.data, "skin data")?;
            let skin_data = match self.decode(skin_data_index)? {
                TypedBlock::SkinData(data) => data,
                _ => {
                    self.issue(skin_data_index, "skin data has an unsupported type".into());
                    return Ok(None);
                }
            };
            if instance.bones.len() != skin_data.bones.len() {
                self.issue(
                    skin_index,
                    format!(
                        "skin instance has {} bone references but skin data has {} bones",
                        instance.bones.len(),
                        skin_data.bones.len()
                    ),
                );
                return Ok(None);
            }
            if instance.skin_partition >= 0 {
                let partition_index =
                    self.reference(skin_index, instance.skin_partition, "skin partition")?;
                if let TypedBlock::SkinPartitionData(partitions) = self.decode(partition_index)? {
                    triangles = visible_partition_triangles(&partitions, &instance, vertex_count);
                    if triangles.is_empty() {
                        self.issue(
                            block_index,
                            "skinned geometry has no editor-visible partitions".into(),
                        );
                        return Ok(None);
                    }
                }
            }
            let Some((joints, weights)) = skin_vertex_influences(vertex_count, &skin_data) else {
                self.issue(skin_data_index, "skin has no usable vertex weights".into());
                return Ok(None);
            };
            let mut joint_nodes = Vec::with_capacity(instance.bones.len());
            for &bone in &instance.bones {
                let bone_index = self.reference(skin_index, bone, "skin bone")?;
                let Some(node) = self.visit(bone_index)? else {
                    self.issue(bone_index, "skin bone is not a scene node".into());
                    return Ok(None);
                };
                joint_nodes.push(node);
            }
            let skeleton = if instance.skeleton_root >= 0 {
                let root = self.reference(skin_index, instance.skeleton_root, "skeleton root")?;
                self.visit(root)?
            } else {
                None
            };
            let skin_scene_index = self.scene.skins.len();
            self.scene.skins.push(SceneSkin {
                name: format!(
                    "{} Skin",
                    geometry.base.object.name.as_deref().unwrap_or("Mesh")
                ),
                joints: joint_nodes,
                inverse_bind_matrices: skin_data
                    .bones
                    .iter()
                    .map(|bone| transform_matrix(&bone.skin_transform))
                    .collect(),
                skeleton,
            });
            skin = Some(skin_scene_index);
            (joints, weights)
        } else {
            (Vec::new(), Vec::new())
        };
        let indices = triangles.into_iter().flatten().collect::<Vec<_>>();
        if indices.iter().any(|&index| index as usize >= vertex_count) {
            self.issue(
                data_index,
                "geometry contains an out-of-range vertex index".into(),
            );
            return Ok(None);
        }

        let material = self.extract_material(block_index, &geometry)?;
        let tangents = tangents(&data);
        let mesh = SceneMesh {
            name: geometry.base.object.name.clone().unwrap_or_else(|| {
                format!(
                    "{}#{block_index}",
                    self.document.blocks[block_index].type_name
                )
            }),
            positions: data.vertices,
            normals: exact_attribute(data.normals, vertex_count),
            tangents,
            colors: exact_attribute(data.colors, vertex_count),
            tex_coords: data
                .uv_sets
                .into_iter()
                .next()
                .filter(|values| values.len() == vertex_count)
                .unwrap_or_default(),
            joints,
            weights,
            indices,
            material,
        };
        self.scene.statistics.source_meshes += 1;
        self.scene.statistics.source_vertices += mesh.positions.len();
        self.scene.statistics.source_triangles += mesh.indices.len() / 3;
        let node = self.push_node(block_index, &geometry.base, Some(mesh));
        self.scene.nodes[node].skin = skin;
        Ok(Some(node))
    }

    fn extract_material(
        &mut self,
        block_index: usize,
        geometry: &Geometry,
    ) -> Result<Option<usize>, SceneError> {
        let mut material_property = None;
        let mut alpha_property = None;
        let mut pp_shader = None;
        let mut no_lighting = None;

        for &property in &geometry.base.properties {
            if property < 0 {
                continue;
            }
            let property = self.reference(block_index, property, "property")?;
            match self.decode(property)? {
                TypedBlock::MaterialProperty(value) => material_property = Some(value),
                TypedBlock::AlphaProperty(value) => alpha_property = Some(value),
                TypedBlock::PpLightingProperty(value) => pp_shader = Some(value),
                TypedBlock::NoLightingProperty(value) => no_lighting = Some(value),
                TypedBlock::Unsupported => {}
                _ => {}
            }
        }

        if material_property.is_none()
            && alpha_property.is_none()
            && pp_shader.is_none()
            && no_lighting.is_none()
        {
            return Ok(None);
        }

        let textures = match pp_shader.as_ref().map(|shader| shader.texture_set) {
            Some(reference) if reference >= 0 => {
                let texture_set = self.reference(block_index, reference, "shader texture set")?;
                match self.decode(texture_set)? {
                    TypedBlock::ShaderTextureSet(value) => Some(value),
                    _ => {
                        self.issue(
                            texture_set,
                            "shader texture set has an unsupported type".into(),
                        );
                        None
                    }
                }
            }
            _ => None,
        };
        let shader = pp_shader
            .as_ref()
            .map(|value| &value.base)
            .or_else(|| no_lighting.as_ref().map(|value| &value.base));
        let material = make_material(
            geometry,
            material_property.as_ref(),
            alpha_property.as_ref(),
            shader,
            no_lighting.as_ref(),
            textures.as_ref(),
        );
        self.scene.materials.push(material);
        Ok(Some(self.scene.materials.len() - 1))
    }

    fn push_node(
        &mut self,
        block_index: usize,
        object: &AvObject,
        mesh: Option<SceneMesh>,
    ) -> usize {
        let scene_index = self.scene.nodes.len();
        self.scene.nodes.push(SceneNode {
            source_block: block_index,
            name: object.object.name.clone().unwrap_or_else(|| {
                format!(
                    "{}#{block_index}",
                    self.document.blocks[block_index].type_name
                )
            }),
            transform: object.transform,
            children: Vec::new(),
            mesh,
            skin: None,
        });
        self.node_by_block.insert(block_index, scene_index);
        scene_index
    }

    fn decode(&self, block: usize) -> Result<TypedBlock, SceneError> {
        self.document
            .decode_block(block)
            .map_err(|error| SceneError::Decode {
                block,
                message: error.to_string(),
            })
    }

    fn reference(
        &self,
        source_block: usize,
        reference: i32,
        field: &'static str,
    ) -> Result<usize, SceneError> {
        if reference < 0 {
            return Err(SceneError::InvalidBlockReference {
                source_block,
                field,
                reference,
                block_count: self.document.blocks.len(),
            });
        }
        let reference = reference as usize;
        self.block(source_block, reference, field)?;
        Ok(reference)
    }

    fn block(
        &self,
        source_block: usize,
        reference: usize,
        field: &'static str,
    ) -> Result<&super::RawBlock, SceneError> {
        self.document
            .blocks
            .get(reference)
            .ok_or(SceneError::InvalidBlockReference {
                source_block,
                field,
                reference: reference as i32,
                block_count: self.document.blocks.len(),
            })
    }

    fn issue(&mut self, source_block: usize, message: String) {
        let type_name = self
            .document
            .blocks
            .get(source_block)
            .map(|block| block.type_name.clone())
            .unwrap_or_else(|| "<missing>".into());
        self.scene.issues.push(SceneIssue {
            source_block,
            type_name,
            message,
        });
    }
}

const PF_EDITOR_VISIBLE: u16 = 0x0001;

fn visible_partition_triangles(
    partitions: &SkinPartitionData,
    instance: &SkinInstance,
    vertex_count: usize,
) -> Vec<[u16; 3]> {
    let has_dismember_metadata = !instance.partitions.is_empty();
    partitions
        .partitions
        .iter()
        .enumerate()
        .filter(|(index, _)| {
            !has_dismember_metadata
                || instance
                    .partitions
                    .get(*index)
                    .is_some_and(|partition| partition.flags & PF_EDITOR_VISIBLE != 0)
        })
        .flat_map(|(_, partition)| {
            partition.triangles.iter().filter_map(|triangle| {
                let mapped = triangle.map(|index| {
                    partition
                        .vertex_map
                        .get(index as usize)
                        .copied()
                        .unwrap_or(index)
                });
                mapped
                    .iter()
                    .all(|&index| (index as usize) < vertex_count)
                    .then_some(mapped)
            })
        })
        .collect()
}

fn skin_vertex_influences(
    vertex_count: usize,
    skin_data: &SkinData,
) -> Option<VertexSkinInfluences> {
    let mut influences = vec![Vec::<(u16, f32)>::new(); vertex_count];
    for (bone_index, bone) in skin_data.bones.iter().enumerate() {
        let bone_index = u16::try_from(bone_index).ok()?;
        for &(vertex, weight) in &bone.vertex_weights {
            if weight.is_finite() && weight > 0.0 {
                if let Some(vertex_influences) = influences.get_mut(vertex as usize) {
                    vertex_influences.push((bone_index, weight));
                }
            }
        }
    }
    if influences.iter().any(Vec::is_empty) {
        return None;
    }
    let mut joints = Vec::with_capacity(vertex_count);
    let mut weights = Vec::with_capacity(vertex_count);
    for mut vertex in influences {
        vertex.sort_by(|left, right| {
            right
                .1
                .partial_cmp(&left.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.0.cmp(&right.0))
        });
        vertex.truncate(4);
        let total = vertex.iter().map(|(_, weight)| *weight).sum::<f32>();
        if total <= f32::EPSILON {
            return None;
        }
        let mut vertex_joints = [0; 4];
        let mut vertex_weights = [0.0; 4];
        for (slot, (joint, weight)) in vertex.into_iter().enumerate() {
            vertex_joints[slot] = joint;
            vertex_weights[slot] = weight / total;
        }
        joints.push(vertex_joints);
        weights.push(vertex_weights);
    }
    Some((joints, weights))
}

fn transform_matrix(transform: &Transform) -> [f32; 16] {
    let rotation =
        Quat::from_mat3(&Mat3::from_cols_array(&transform.rotation).transpose()).normalize();
    Mat4::from_scale_rotation_translation(
        Vec3::splat(transform.scale),
        rotation,
        Vec3::from_array(transform.translation),
    )
    .to_cols_array()
}

fn exact_attribute<T>(values: Vec<T>, vertex_count: usize) -> Vec<T> {
    if values.len() == vertex_count {
        values
    } else {
        Vec::new()
    }
}

fn tangents(data: &GeometryData) -> Vec<[f32; 4]> {
    if data.tangents.len() != data.vertices.len()
        || data.bitangents.len() != data.vertices.len()
        || data.normals.len() != data.vertices.len()
    {
        return Vec::new();
    }
    data.tangents
        .iter()
        .zip(&data.bitangents)
        .zip(&data.normals)
        .map(|((&tangent, &bitangent), &normal)| {
            let cross = [
                normal[1] * tangent[2] - normal[2] * tangent[1],
                normal[2] * tangent[0] - normal[0] * tangent[2],
                normal[0] * tangent[1] - normal[1] * tangent[0],
            ];
            let handedness = if cross[0] * bitangent[0]
                + cross[1] * bitangent[1]
                + cross[2] * bitangent[2]
                < 0.0
            {
                -1.0
            } else {
                1.0
            };
            [tangent[0], tangent[1], tangent[2], handedness]
        })
        .collect()
}

fn triangulate_strips(data: &TriStripsData) -> Vec<[u16; 3]> {
    let mut triangles = Vec::with_capacity(data.geometry.triangle_count as usize);
    for strip in &data.strips {
        for (offset, window) in strip.windows(3).enumerate() {
            let triangle = if offset % 2 == 0 {
                [window[0], window[1], window[2]]
            } else {
                [window[1], window[0], window[2]]
            };
            if triangle[0] != triangle[1]
                && triangle[1] != triangle[2]
                && triangle[0] != triangle[2]
            {
                triangles.push(triangle);
            }
        }
    }
    triangles
}

fn make_material(
    geometry: &Geometry,
    material: Option<&MaterialProperty>,
    alpha: Option<&AlphaProperty>,
    shader: Option<&ShaderProperty>,
    no_lighting: Option<&NoLightingProperty>,
    textures: Option<&ShaderTextureSet>,
) -> SceneMaterial {
    let name = geometry
        .materials
        .names
        .get(usize::try_from(geometry.materials.active).unwrap_or(usize::MAX))
        .and_then(|value| value.clone())
        .or_else(|| material.and_then(|value| value.object.name.clone()))
        .unwrap_or_else(|| "Material".into());
    let alpha_value = material.map_or(1.0, |value| value.alpha.clamp(0.0, 1.0));
    let (alpha_mode, alpha_cutoff) = alpha_policy(
        alpha.map(|value| (value.flags, value.threshold)),
        alpha_value,
    );
    let texture = |slot: usize| {
        textures
            .and_then(|value| value.textures.get(slot))
            .map(|path| normalize_texture_path(path))
            .filter(|path| !path.is_empty())
    };
    let shader_type = shader.map_or(0, |value| value.shader_type);
    let shader_flags_1 = shader.map_or(0, |value| value.shader_flags);
    let shader_flags_2 = shader.map_or(0, |value| value.shader_flags_2);
    let features =
        super::FalloutShaderFeatures::from_flags(shader_type, shader_flags_1, shader_flags_2);
    let diffuse_color = material.and_then(|value| value.diffuse).unwrap_or([1.0; 3]);
    SceneMaterial {
        name,
        base_color: [
            diffuse_color[0].max(0.0),
            diffuse_color[1].max(0.0),
            diffuse_color[2].max(0.0),
            alpha_value,
        ],
        emissive: material.map_or([0.0; 3], |value| value.emissive),
        emissive_multiplier: material.map_or(1.0, |value| value.emissive_multiplier.max(0.0)),
        roughness: material_roughness_policy(material.map(|value| value.glossiness)),
        alpha_mode,
        alpha_cutoff,
        double_sided: features.double_sided,
        unlit: no_lighting.is_some(),
        diffuse_texture: no_lighting
            .map(|value| normalize_texture_path(&value.file_name))
            .filter(|path| !path.is_empty())
            .or_else(|| texture(0)),
        normal_texture: texture(1),
        specular_texture: features.specular.then(|| texture(7)).flatten(),
        // Preserve slot 2 even when its shader flag is absent. The GLB writer
        // will keep it out of emissiveTexture, but its presence is evidence
        // that a constant authored emission must not be promoted on an
        // unflagged environment-map material (RadAway's `_g` case).
        glow_texture: texture(2),
        height_texture: features.parallax.then(|| texture(3)).flatten(),
        environment_texture: features.environment_mapping.then(|| texture(4)).flatten(),
        environment_mask: features.environment_mapping.then(|| texture(5)).flatten(),
        shader_type,
        shader_flags_1,
        shader_flags_2,
    }
}

fn material_roughness_policy(_glossiness: Option<f32>) -> f32 {
    0.5
}

fn alpha_policy(alpha: Option<(u16, u8)>, material_alpha: f32) -> (SceneAlphaMode, Option<f32>) {
    match alpha {
        Some((flags, _)) if flags & 0x0001 != 0 => (SceneAlphaMode::Blend, None),
        Some((flags, threshold)) if flags & 0x0200 != 0 => {
            (SceneAlphaMode::Mask, Some(f32::from(threshold) / 255.0))
        }
        _ if material_alpha < 1.0 => (SceneAlphaMode::Blend, None),
        _ => (SceneAlphaMode::Opaque, None),
    }
}

pub fn normalize_texture_path(path: &str) -> String {
    path.trim_matches('\0')
        .replace('\\', "/")
        .trim_start_matches('/')
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_cues_are_case_insensitive_and_normalize_carriage_returns() {
        let mut cues = Vec::new();
        append_animation_sound_cues(
            &mut cues,
            "Open",
            0.25,
            "start\rSound: DRSAmmoBoxOpen\r\nsound: DRSRefrigeratorOpen\r",
        );
        assert_eq!(
            cues,
            vec![
                SceneAnimationSoundCue {
                    sequence: "Open".into(),
                    time: 0.25,
                    editor_id: "DRSAmmoBoxOpen".into(),
                },
                SceneAnimationSoundCue {
                    sequence: "Open".into(),
                    time: 0.25,
                    editor_id: "DRSRefrigeratorOpen".into(),
                },
            ]
        );
    }

    fn scalar_keys(values: &[(f32, f32)]) -> AnimationKeyGroup<f32> {
        AnimationKeyGroup {
            interpolation: Some(super::super::KeyType::Linear),
            keys: values
                .iter()
                .map(|&(time, value)| AnimationKey { time, value })
                .collect(),
        }
    }

    fn empty_vec3_keys() -> AnimationKeyGroup<[f32; 3]> {
        AnimationKeyGroup {
            interpolation: None,
            keys: Vec::new(),
        }
    }

    fn empty_scalar_keys() -> AnimationKeyGroup<f32> {
        AnimationKeyGroup {
            interpolation: None,
            keys: Vec::new(),
        }
    }

    fn assert_quaternion_close(actual: [f32; 4], expected: [f32; 4]) {
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!(
                (actual - expected).abs() <= 1.0e-6,
                "{actual} != {expected}"
            );
        }
    }

    #[test]
    fn zero_xyz_rotation_is_identity_xyzw() {
        let groups = [
            scalar_keys(&[(0.0, 0.0)]),
            scalar_keys(&[(0.0, 0.0)]),
            scalar_keys(&[(0.0, 0.0)]),
        ];
        let rotations = xyz_rotation_keys(&groups);
        assert_eq!(rotations.len(), 1);
        assert_quaternion_close(rotations[0].value, [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn ninety_degree_z_xyz_rotation_is_xyzw() {
        let groups = [
            scalar_keys(&[(0.0, 0.0)]),
            scalar_keys(&[(0.0, 0.0)]),
            scalar_keys(&[(0.0, std::f32::consts::FRAC_PI_2)]),
        ];
        let rotations = xyz_rotation_keys(&groups);
        let half_angle = std::f32::consts::FRAC_PI_4;
        assert_quaternion_close(
            rotations[0].value,
            [0.0, 0.0, half_angle.sin(), half_angle.cos()],
        );
    }

    #[test]
    fn mixed_xyz_rotation_matches_glam_xyzw() {
        let angles = [0.25, -0.5, 0.75];
        let groups = [
            scalar_keys(&[(0.0, angles[0])]),
            scalar_keys(&[(0.0, angles[1])]),
            scalar_keys(&[(0.0, angles[2])]),
        ];
        let rotations = xyz_rotation_keys(&groups);
        assert_quaternion_close(
            rotations[0].value,
            Quat::from_euler(EulerRot::XYZ, angles[0], angles[1], angles[2]).to_array(),
        );
    }

    #[test]
    fn nif_wxyz_rotation_is_reordered_exactly_once() {
        let converted = nif_wxyz_rotation_key_to_gltf_xyzw(AnimationKey {
            time: 0.0,
            value: [0.5, 0.1, 0.2, 0.3],
        });
        assert_eq!(converted.value, [0.1, 0.2, 0.3, 0.5]);
    }

    #[test]
    fn quaternion_and_xyz_controller_sources_merge_to_same_xyzw_rotation() {
        let half_angle = std::f32::consts::FRAC_PI_4;
        let quaternion_data = TransformData {
            rotations: vec![AnimationKey {
                time: 0.0,
                value: [half_angle.cos(), 0.0, 0.0, half_angle.sin()],
            }],
            xyz_rotations: None,
            translations: empty_vec3_keys(),
            scales: empty_scalar_keys(),
        };
        let xyz_data = TransformData {
            rotations: Vec::new(),
            xyz_rotations: Some([
                scalar_keys(&[(0.0, 0.0)]),
                scalar_keys(&[(0.0, 0.0)]),
                scalar_keys(&[(0.0, std::f32::consts::FRAC_PI_2)]),
            ]),
            translations: empty_vec3_keys(),
            scales: empty_scalar_keys(),
        };
        let mut quaternion_channels = Vec::new();
        merge_animation_channel(&mut quaternion_channels, 7, quaternion_data);
        let mut xyz_channels = Vec::new();
        merge_animation_channel(&mut xyz_channels, 7, xyz_data);
        assert_quaternion_close(
            quaternion_channels[0].rotations[0].value,
            xyz_channels[0].rotations[0].value,
        );
    }

    #[test]
    fn material_roughness_matches_the_blender_converter_baseline() {
        assert_eq!(material_roughness_policy(None), 0.5);
        assert_eq!(material_roughness_policy(Some(0.0)), 0.5);
        assert_eq!(material_roughness_policy(Some(70.0)), 0.5);
        assert_eq!(material_roughness_policy(Some(100.0)), 0.5);
    }

    #[test]
    fn alpha_policy_prefers_blend_when_blend_and_test_are_both_authored() {
        assert_eq!(
            alpha_policy(Some((0x0001 | 0x0200, 128)), 1.0),
            (SceneAlphaMode::Blend, None)
        );
    }

    #[test]
    fn alpha_policy_uses_blend_when_only_blending_is_authored() {
        assert_eq!(
            alpha_policy(Some((0x0001, 128)), 1.0),
            (SceneAlphaMode::Blend, None)
        );
    }

    #[test]
    fn alpha_policy_preserves_mask_cutoff_when_only_testing_is_authored() {
        assert_eq!(
            alpha_policy(Some((0x0200, 120)), 1.0),
            (SceneAlphaMode::Mask, Some(120.0 / 255.0))
        );
    }

    #[test]
    fn alpha_policy_uses_material_alpha_only_without_authored_alpha_flags() {
        assert_eq!(alpha_policy(None, 0.5), (SceneAlphaMode::Blend, None));
        assert_eq!(
            alpha_policy(Some((0, 200)), 1.0),
            (SceneAlphaMode::Opaque, None)
        );
    }

    #[test]
    fn triangle_strips_alternate_winding_and_drop_degenerates() {
        let data = TriStripsData {
            geometry: GeometryData {
                group_id: 0,
                vertices: vec![[0.0; 3]; 5],
                keep_flags: 0,
                compress_flags: 0,
                data_flags: 0,
                normals: Vec::new(),
                tangents: Vec::new(),
                bitangents: Vec::new(),
                bound_center: [0.0; 3],
                bound_radius: 0.0,
                colors: Vec::new(),
                uv_sets: Vec::new(),
                consistency_flags: 0,
                additional_data: -1,
                triangle_count: 2,
            },
            strips: vec![vec![0, 1, 2, 3], vec![3, 3, 4]],
        };
        assert_eq!(triangulate_strips(&data), [[0, 1, 2], [2, 1, 3]]);
    }

    #[test]
    fn texture_paths_are_canonical_and_portable() {
        assert_eq!(
            normalize_texture_path("\\Textures\\Clutter\\Desk.DDS\0"),
            "textures/clutter/desk.dds"
        );
    }

    fn identity_transform() -> Transform {
        Transform {
            translation: [0.0; 3],
            rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            scale: 1.0,
        }
    }

    fn empty_scene(nodes: Vec<SceneNode>, roots: Vec<usize>) -> Scene {
        Scene {
            nodes,
            roots,
            materials: Vec::new(),
            skins: Vec::new(),
            issues: Vec::new(),
            statistics: SceneStatistics::default(),
            animations: Vec::new(),
            animation_sound_cues: Vec::new(),
        }
    }

    #[test]
    fn actor_merge_preserves_authoritative_part_inverse_bind_matrices() {
        let node = |name: &str, children: Vec<usize>| SceneNode {
            source_block: 0,
            name: name.into(),
            transform: identity_transform(),
            children,
            mesh: None,
            skin: None,
        };
        let mut actor = empty_scene(
            vec![node("Scene Root", vec![1]), node("Bip01 Spine", Vec::new())],
            vec![0],
        );
        let mesh = SceneMesh {
            name: "Outfit".into(),
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: Vec::new(),
            tangents: Vec::new(),
            colors: Vec::new(),
            tex_coords: Vec::new(),
            joints: vec![[0, 0, 0, 0]; 3],
            weights: vec![[1.0, 0.0, 0.0, 0.0]; 3],
            indices: vec![0, 1, 2],
            material: None,
        };
        let mut part = empty_scene(
            vec![
                node("Scene Root", vec![1, 2]),
                node("Bip01 Spine", Vec::new()),
                SceneNode {
                    source_block: 2,
                    name: "Outfit".into(),
                    transform: identity_transform(),
                    children: Vec::new(),
                    mesh: Some(mesh),
                    skin: Some(0),
                },
            ],
            vec![0],
        );
        let authored = Mat4::from_translation(Vec3::new(3.0, 5.0, 7.0)).to_cols_array();
        part.skins.push(SceneSkin {
            name: "Outfit Skin".into(),
            joints: vec![1],
            inverse_bind_matrices: vec![authored],
            skeleton: Some(0),
        });

        merge_actor_scene(&mut actor, &part).unwrap();

        assert_eq!(actor.skins.len(), 1);
        assert_eq!(actor.skins[0].joints, vec![1]);
        assert_eq!(actor.skins[0].skeleton, Some(0));
        assert_eq!(actor.skins[0].inverse_bind_matrices, vec![authored]);
        assert_eq!(actor.nodes[2].skin, Some(0));
    }

    #[test]
    fn actor_merge_attaches_head_local_roots_to_the_shared_head_bone() {
        let node = |name: &str, children: Vec<usize>| SceneNode {
            source_block: 0,
            name: name.into(),
            transform: identity_transform(),
            children,
            mesh: None,
            skin: None,
        };
        let mut actor = empty_scene(
            vec![node("Scene Root", vec![1]), node("Bip01 Head", Vec::new())],
            vec![0],
        );
        let hair = SceneMesh {
            name: "NoHat".into(),
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: Vec::new(),
            tangents: Vec::new(),
            colors: Vec::new(),
            tex_coords: Vec::new(),
            joints: Vec::new(),
            weights: Vec::new(),
            indices: vec![0, 1, 2],
            material: None,
        };
        let part = empty_scene(
            vec![
                node("HairBase", vec![1]),
                SceneNode {
                    source_block: 1,
                    name: "NoHat".into(),
                    transform: identity_transform(),
                    children: Vec::new(),
                    mesh: Some(hair),
                    skin: None,
                },
            ],
            vec![0],
        );

        merge_actor_scene_attached(&mut actor, &part, "Bip01 Head").unwrap();

        assert_eq!(actor.roots, vec![0]);
        assert_eq!(actor.nodes[1].children, vec![2]);
        assert_eq!(actor.nodes[2].name, "HairBase");
        assert_eq!(actor.nodes[2].children, vec![3]);
        assert_eq!(actor.nodes[3].name, "NoHat");
    }
}
