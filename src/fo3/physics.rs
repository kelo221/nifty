use std::collections::{HashMap, HashSet};

use glam::{Mat3, Mat4, Quat, Vec3};
use thiserror::Error;

use super::{AvObject, Document, Fo3Error, RawBlock, Reader, Transform, TypedBlock};

const HAVOK_TO_METRES: f32 = 0.1;
const GAME_UNITS_TO_METRES: f32 = 1.0 / 70.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HavokFilter {
    pub layer: u8,
    pub flags: u8,
    pub group: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CollisionObject {
    pub target: i32,
    pub flags: u16,
    pub body: i32,
    pub phantom: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RigidBody {
    pub shape: i32,
    pub body_filter: HavokFilter,
    pub info_filter: HavokFilter,
    pub translation: [f32; 4],
    pub rotation: [f32; 4],
    pub linear_velocity: [f32; 4],
    pub angular_velocity: [f32; 4],
    pub inertia: [[f32; 3]; 3],
    pub center: [f32; 4],
    pub mass: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub friction: f32,
    pub restitution: f32,
    pub max_linear_velocity: f32,
    pub max_angular_velocity: f32,
    pub penetration_depth: f32,
    pub motion_system: u8,
    pub deactivator_type: u8,
    pub solver_deactivation: u8,
    pub quality_type: u8,
    pub constraints: Vec<i32>,
    pub body_flags: u32,
    pub transformed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimpleShapePhantom {
    pub shape: i32,
    pub filter: HavokFilter,
    pub transform: [f32; 16],
}

#[derive(Debug, Clone, PartialEq)]
pub struct PackedSubShape {
    pub filter: HavokFilter,
    pub vertex_count: u32,
    pub material: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PackedTriStripsData {
    pub triangles: Vec<[u16; 3]>,
    pub vertices: Vec<[f32; 3]>,
    pub sub_shapes: Vec<PackedSubShape>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PhysicsShape {
    Box {
        material: u32,
        radius: f32,
        half_extents: [f32; 3],
    },
    Sphere {
        material: u32,
        radius: f32,
    },
    Capsule {
        material: u32,
        radius: f32,
        first: [f32; 3],
        second: [f32; 3],
    },
    ConvexVertices {
        material: u32,
        radius: f32,
        vertices: Vec<[f32; 3]>,
    },
    Transform {
        shape: i32,
        material: u32,
        transform: [f32; 16],
    },
    List {
        shapes: Vec<i32>,
        material: u32,
    },
    ConvexList {
        shapes: Vec<i32>,
        material: u32,
    },
    Mopp {
        shape: i32,
    },
    PackedTriStrips {
        scale: [f32; 4],
        data: i32,
    },
    NiTriStrips {
        material: u32,
        scale: [f32; 4],
        data: Vec<i32>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PhysicsScene {
    pub bodies: Vec<PhysicsBody>,
    pub joints: Vec<PhysicsJoint>,
    pub issues: Vec<PhysicsIssue>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PhysicsJoint {
    pub source_block: usize,
    pub kind: String,
    pub body_a: u32,
    pub body_b: u32,
    pub anchor_a: [f32; 3],
    pub anchor_b: [f32; 3],
    pub frame_a_rotation_xyzw: [f32; 4],
    pub frame_b_rotation_xyzw: [f32; 4],
    pub lower_limit: Option<f32>,
    pub upper_limit: Option<f32>,
    pub cone_limit: Option<f32>,
    pub plane_lower_limit: Option<f32>,
    pub plane_upper_limit: Option<f32>,
    pub twist_lower_limit: Option<f32>,
    pub twist_upper_limit: Option<f32>,
    pub malleable_strength: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Constraint {
    pub entity_a: i32,
    pub entity_b: i32,
    pub data: ConstraintData,
    pub malleable_strength: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConstraintData {
    Ragdoll(RagdollConstraint),
    LimitedHinge(LimitedHingeConstraint),
    Hinge(HingeConstraint),
    BallAndSocket(BallAndSocketConstraint),
    Prismatic(PrismaticConstraint),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RagdollConstraint {
    pub twist_a: [f32; 4],
    pub plane_a: [f32; 4],
    pub pivot_a: [f32; 4],
    pub twist_b: [f32; 4],
    pub plane_b: [f32; 4],
    pub pivot_b: [f32; 4],
    pub cone_max_angle: f32,
    pub plane_min_angle: f32,
    pub plane_max_angle: f32,
    pub twist_min_angle: f32,
    pub twist_max_angle: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LimitedHingeConstraint {
    pub axis_a: [f32; 4],
    pub perpendicular_a: [f32; 4],
    pub pivot_a: [f32; 4],
    pub axis_b: [f32; 4],
    pub perpendicular_b: [f32; 4],
    pub pivot_b: [f32; 4],
    pub min_angle: f32,
    pub max_angle: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HingeConstraint {
    pub axis_a: [f32; 4],
    pub perpendicular_a: [f32; 4],
    pub pivot_a: [f32; 4],
    pub axis_b: [f32; 4],
    pub perpendicular_b: [f32; 4],
    pub pivot_b: [f32; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct BallAndSocketConstraint {
    pub pivot_a: [f32; 4],
    pub pivot_b: [f32; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrismaticConstraint {
    pub sliding_a: [f32; 4],
    pub rotation_a: [f32; 4],
    pub pivot_a: [f32; 4],
    pub sliding_b: [f32; 4],
    pub rotation_b: [f32; 4],
    pub pivot_b: [f32; 4],
    pub min_distance: f32,
    pub max_distance: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PhysicsBody {
    pub source_block: usize,
    pub group_id: u32,
    pub node: Option<String>,
    pub motion_type: String,
    pub quality_type: String,
    pub mass: f32,
    pub center_of_mass: [f32; 3],
    pub inertia: [[f32; 3]; 3],
    pub linear_velocity: [f32; 3],
    pub angular_velocity: [f32; 3],
    pub gravity_factor: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub friction: f32,
    pub restitution: f32,
    pub max_linear_velocity: f32,
    pub max_angular_velocity: f32,
    pub sleep_enabled: bool,
    pub ccd_enabled: bool,
    pub layer: u8,
    pub filter_flags: u8,
    pub material: Option<u32>,
    pub material_name: Option<String>,
    pub phantom: bool,
    pub constrained: bool,
    pub shapes: Vec<ConvertedPhysicsShape>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConvertedPhysicsShape {
    Box {
        center: [f32; 3],
        half_extents: [f32; 3],
        rotation_xyzw: [f32; 4],
    },
    Sphere {
        center: [f32; 3],
        radius: f32,
    },
    Capsule {
        point1: [f32; 3],
        point2: [f32; 3],
        radius: f32,
    },
    ConvexHull {
        points: Vec<[f32; 3]>,
    },
    TriangleMesh {
        vertices: Vec<[f32; 3]>,
        indices: Vec<u32>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicsIssue {
    pub source_block: usize,
    pub type_name: String,
    pub message: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PhysicsError {
    #[error("physics block {block} references invalid {field} block {reference}")]
    InvalidReference {
        block: usize,
        field: &'static str,
        reference: i32,
    },
    #[error("physics shape graph contains a cycle at block {block}")]
    ShapeCycle { block: usize },
    #[error("could not decode physics block {block}: {message}")]
    Decode { block: usize, message: String },
}

pub fn extract_physics(document: &Document) -> Result<PhysicsScene, PhysicsError> {
    let parents = av_parent_map(document);
    let mut world_cache = HashMap::new();
    let mut scene = PhysicsScene {
        bodies: Vec::new(),
        joints: Vec::new(),
        issues: Vec::new(),
    };
    let mut seen_bodies = HashSet::new();
    let mut body_frames = HashMap::new();

    for index in 0..document.blocks.len() {
        let collision = match decode(document, index)? {
            TypedBlock::CollisionObject(value) => value,
            _ => continue,
        };
        if collision.body < 0 {
            continue;
        }
        let body_index = reference(document, index, collision.body, "body")?;
        if !seen_bodies.insert(body_index) {
            continue;
        }
        let target_transform = if collision.target >= 0 {
            let target = reference(document, index, collision.target, "target")?;
            av_world_transform(
                document,
                target,
                &parents,
                &mut world_cache,
                &mut HashSet::new(),
            )
        } else {
            Mat4::IDENTITY
        };
        let node = collision
            .target
            .try_into()
            .ok()
            .and_then(|target| av_name(document, target));
        let group_id = scene.bodies.len() as u32;
        match decode(document, body_index)? {
            TypedBlock::RigidBody(body) => {
                let body_transform = if body.transformed {
                    target_transform * rigid_transform(&body)
                } else {
                    target_transform
                };
                let mut material = None;
                let shapes = collect_shapes(
                    document,
                    body.shape,
                    body_transform,
                    &mut material,
                    &mut scene.issues,
                    &mut HashSet::new(),
                )?;
                if shapes.is_empty() {
                    issue(
                        document,
                        &mut scene.issues,
                        body_index,
                        "rigid body produced no supported collision shapes".into(),
                    );
                }
                body_frames.insert(body_index, (group_id, body_transform));
                scene.bodies.push(PhysicsBody {
                    source_block: body_index,
                    group_id,
                    node,
                    motion_type: motion_system_name(body.motion_system).into(),
                    quality_type: quality_type_name(body.quality_type).into(),
                    mass: body.mass,
                    center_of_mass: rigid_body_point(body.center, body_transform),
                    inertia: convert_inertia(body.inertia),
                    linear_velocity: basis_scaled(vec3(body.linear_velocity), HAVOK_TO_METRES),
                    angular_velocity: basis_scaled(vec3(body.angular_velocity), 1.0),
                    gravity_factor: 1.0,
                    linear_damping: body.linear_damping.max(0.0),
                    angular_damping: body.angular_damping.max(0.0),
                    friction: body.friction.max(0.0),
                    restitution: body.restitution.max(0.0),
                    max_linear_velocity: (body.max_linear_velocity * HAVOK_TO_METRES).max(0.0),
                    max_angular_velocity: body.max_angular_velocity.max(0.0),
                    sleep_enabled: body.deactivator_type != 1,
                    ccd_enabled: body.quality_type == 6,
                    layer: body.info_filter.layer,
                    filter_flags: body.body_filter.flags,
                    material,
                    material_name: material.map(havok_material_name).map(str::to_owned),
                    phantom: collision.phantom,
                    constrained: !body.constraints.is_empty(),
                    shapes,
                });
            }
            TypedBlock::SimpleShapePhantom(phantom) => {
                let transform = target_transform * scaled_havok_matrix(phantom.transform);
                let mut material = None;
                let shapes = collect_shapes(
                    document,
                    phantom.shape,
                    transform,
                    &mut material,
                    &mut scene.issues,
                    &mut HashSet::new(),
                )?;
                scene.bodies.push(PhysicsBody {
                    source_block: body_index,
                    group_id,
                    node,
                    motion_type: "MO_SYS_FIXED".into(),
                    quality_type: "MO_QUAL_FIXED".into(),
                    mass: 0.0,
                    center_of_mass: [0.0; 3],
                    inertia: [[0.0; 3]; 3],
                    linear_velocity: [0.0; 3],
                    angular_velocity: [0.0; 3],
                    gravity_factor: 1.0,
                    linear_damping: 0.0,
                    angular_damping: 0.0,
                    friction: 0.8,
                    restitution: 0.0,
                    max_linear_velocity: 0.0,
                    max_angular_velocity: 0.0,
                    sleep_enabled: true,
                    ccd_enabled: false,
                    layer: phantom.filter.layer,
                    filter_flags: phantom.filter.flags,
                    material,
                    material_name: material.map(havok_material_name).map(str::to_owned),
                    phantom: true,
                    constrained: false,
                    shapes,
                });
            }
            _ => issue(
                document,
                &mut scene.issues,
                body_index,
                "collision object body type is not supported".into(),
            ),
        }
    }
    for index in 0..document.blocks.len() {
        let constraint = match decode(document, index)? {
            TypedBlock::Constraint(value) => value,
            _ => continue,
        };
        let Some(&(body_a, frame_a)) = usize::try_from(constraint.entity_a)
            .ok()
            .and_then(|entity| body_frames.get(&entity))
        else {
            issue(
                document,
                &mut scene.issues,
                index,
                format!(
                    "constraint entity A {} is not an extracted rigid body",
                    constraint.entity_a
                ),
            );
            continue;
        };
        let Some(&(body_b, frame_b)) = usize::try_from(constraint.entity_b)
            .ok()
            .and_then(|entity| body_frames.get(&entity))
        else {
            issue(
                document,
                &mut scene.issues,
                index,
                format!(
                    "constraint entity B {} is not an extracted rigid body",
                    constraint.entity_b
                ),
            );
            continue;
        };
        if let Some(joint) = convert_constraint(index, body_a, body_b, frame_a, frame_b, constraint)
        {
            scene.joints.push(joint);
        }
    }
    Ok(scene)
}

fn convert_constraint(
    source_block: usize,
    body_a: u32,
    body_b: u32,
    body_frame_a: Mat4,
    body_frame_b: Mat4,
    constraint: Constraint,
) -> Option<PhysicsJoint> {
    let mut joint = PhysicsJoint {
        source_block,
        kind: "spherical".into(),
        body_a,
        body_b,
        anchor_a: [0.0; 3],
        anchor_b: [0.0; 3],
        frame_a_rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        frame_b_rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        lower_limit: None,
        upper_limit: None,
        cone_limit: None,
        plane_lower_limit: None,
        plane_upper_limit: None,
        twist_lower_limit: None,
        twist_upper_limit: None,
        malleable_strength: constraint.malleable_strength,
    };
    match constraint.data {
        ConstraintData::Ragdoll(value) => {
            joint.anchor_a = constraint_point(value.pivot_a, body_frame_a);
            joint.anchor_b = constraint_point(value.pivot_b, body_frame_b);
            joint.frame_a_rotation_xyzw =
                constraint_frame(value.twist_a, value.plane_a, body_frame_a)?;
            joint.frame_b_rotation_xyzw =
                constraint_frame(value.twist_b, value.plane_b, body_frame_b)?;
            joint.cone_limit = Some(value.cone_max_angle);
            joint.plane_lower_limit = Some(value.plane_min_angle);
            joint.plane_upper_limit = Some(value.plane_max_angle);
            joint.twist_lower_limit = Some(value.twist_min_angle);
            joint.twist_upper_limit = Some(value.twist_max_angle);
        }
        ConstraintData::LimitedHinge(value) => {
            joint.kind = "revolute".into();
            joint.anchor_a = constraint_point(value.pivot_a, body_frame_a);
            joint.anchor_b = constraint_point(value.pivot_b, body_frame_b);
            joint.frame_a_rotation_xyzw =
                constraint_frame(value.axis_a, value.perpendicular_a, body_frame_a)?;
            joint.frame_b_rotation_xyzw =
                constraint_frame(value.axis_b, value.perpendicular_b, body_frame_b)?;
            joint.lower_limit = Some(value.min_angle);
            joint.upper_limit = Some(value.max_angle);
        }
        ConstraintData::Hinge(value) => {
            joint.kind = "revolute".into();
            joint.anchor_a = constraint_point(value.pivot_a, body_frame_a);
            joint.anchor_b = constraint_point(value.pivot_b, body_frame_b);
            joint.frame_a_rotation_xyzw =
                constraint_frame(value.axis_a, value.perpendicular_a, body_frame_a)?;
            joint.frame_b_rotation_xyzw =
                constraint_frame(value.axis_b, value.perpendicular_b, body_frame_b)?;
        }
        ConstraintData::BallAndSocket(value) => {
            joint.anchor_a = constraint_point(value.pivot_a, body_frame_a);
            joint.anchor_b = constraint_point(value.pivot_b, body_frame_b);
        }
        ConstraintData::Prismatic(value) => {
            joint.kind = "prismatic".into();
            joint.anchor_a = constraint_point(value.pivot_a, body_frame_a);
            joint.anchor_b = constraint_point(value.pivot_b, body_frame_b);
            joint.frame_a_rotation_xyzw =
                constraint_frame(value.sliding_a, value.rotation_a, body_frame_a)?;
            joint.frame_b_rotation_xyzw =
                constraint_frame(value.sliding_b, value.rotation_b, body_frame_b)?;
            joint.lower_limit = Some(value.min_distance * HAVOK_TO_METRES);
            joint.upper_limit = Some(value.max_distance * HAVOK_TO_METRES);
        }
    }
    let separation = Vec3::from_array(joint.anchor_a).distance(Vec3::from_array(joint.anchor_b));
    (separation.is_finite() && separation <= 0.05).then_some(joint)
}

fn constraint_point(value: [f32; 4], body_frame: Mat4) -> [f32; 3] {
    rigid_body_point(value, body_frame)
}

fn rigid_body_point(value: [f32; 4], body_frame: Mat4) -> [f32; 3] {
    basis(body_frame.transform_point3(vec3(value) * HAVOK_TO_METRES))
}

fn constraint_frame(axis: [f32; 4], reference: [f32; 4], body_frame: Mat4) -> Option<[f32; 4]> {
    let axis = Vec3::from_array(basis(body_frame.transform_vector3(vec3(axis))));
    let reference = Vec3::from_array(basis(body_frame.transform_vector3(vec3(reference))));
    joint_frame_quaternion(axis, reference)
}

fn joint_frame_quaternion(axis_z: Vec3, reference_x: Vec3) -> Option<[f32; 4]> {
    if !axis_z.is_finite() || !reference_x.is_finite() || axis_z.length_squared() < 1.0e-10 {
        return None;
    }
    let z = axis_z.normalize();
    let projected_x = reference_x - z * reference_x.dot(z);
    if projected_x.length_squared() < 1.0e-10 {
        return None;
    }
    let mut x = projected_x.normalize();
    let y = z.cross(x).normalize();
    x = y.cross(z).normalize();
    let mut frame = Quat::from_mat3(&Mat3::from_cols(x, y, z))
        .normalize()
        .to_array();
    if frame[3] < 0.0 {
        for component in &mut frame {
            *component = -*component;
        }
    }
    Some(frame)
}

fn collect_shapes(
    document: &Document,
    shape_reference: i32,
    transform: Mat4,
    material: &mut Option<u32>,
    issues: &mut Vec<PhysicsIssue>,
    visiting: &mut HashSet<usize>,
) -> Result<Vec<ConvertedPhysicsShape>, PhysicsError> {
    if shape_reference < 0 {
        return Ok(Vec::new());
    }
    let shape_index = reference(document, shape_reference as usize, shape_reference, "shape")?;
    if !visiting.insert(shape_index) {
        return Err(PhysicsError::ShapeCycle { block: shape_index });
    }
    let result = match decode(document, shape_index)? {
        TypedBlock::PhysicsShape(shape) => match shape {
            PhysicsShape::Box {
                material: shape_material,
                radius: _,
                half_extents,
            } => {
                material.get_or_insert(shape_material);
                let (scale, rotation, translation) = transform.to_scale_rotation_translation();
                vec![ConvertedPhysicsShape::Box {
                    center: basis(translation),
                    half_extents: [
                        half_extents[0] * HAVOK_TO_METRES * scale.x.abs(),
                        half_extents[2] * HAVOK_TO_METRES * scale.z.abs(),
                        half_extents[1] * HAVOK_TO_METRES * scale.y.abs(),
                    ],
                    rotation_xyzw: basis_rotation(rotation).to_array(),
                }]
            }
            PhysicsShape::Sphere {
                material: shape_material,
                radius,
            } => {
                material.get_or_insert(shape_material);
                let (scale, _, translation) = transform.to_scale_rotation_translation();
                vec![ConvertedPhysicsShape::Sphere {
                    center: basis(translation),
                    radius: radius * HAVOK_TO_METRES * scale.abs().max_element(),
                }]
            }
            PhysicsShape::Capsule {
                material: shape_material,
                radius,
                first,
                second,
            } => {
                material.get_or_insert(shape_material);
                let (scale, _, _) = transform.to_scale_rotation_translation();
                vec![ConvertedPhysicsShape::Capsule {
                    point1: basis(
                        transform.transform_point3(Vec3::from_array(first) * HAVOK_TO_METRES),
                    ),
                    point2: basis(
                        transform.transform_point3(Vec3::from_array(second) * HAVOK_TO_METRES),
                    ),
                    radius: radius * HAVOK_TO_METRES * scale.abs().max_element(),
                }]
            }
            PhysicsShape::ConvexVertices {
                material: shape_material,
                vertices,
                ..
            } => {
                material.get_or_insert(shape_material);
                vec![ConvertedPhysicsShape::ConvexHull {
                    points: vertices
                        .into_iter()
                        .map(|point| {
                            basis(
                                transform
                                    .transform_point3(Vec3::from_array(point) * HAVOK_TO_METRES),
                            )
                        })
                        .collect(),
                }]
            }
            PhysicsShape::Transform {
                shape,
                material: shape_material,
                transform: nested,
            } => {
                material.get_or_insert(shape_material);
                collect_shapes(
                    document,
                    shape,
                    transform * scaled_havok_matrix(nested),
                    material,
                    issues,
                    visiting,
                )?
            }
            PhysicsShape::List {
                shapes,
                material: shape_material,
            }
            | PhysicsShape::ConvexList {
                shapes,
                material: shape_material,
            } => {
                material.get_or_insert(shape_material);
                let mut output = Vec::new();
                for shape in shapes {
                    output.extend(collect_shapes(
                        document, shape, transform, material, issues, visiting,
                    )?);
                }
                output
            }
            PhysicsShape::Mopp { shape } => {
                collect_shapes(document, shape, transform, material, issues, visiting)?
            }
            PhysicsShape::PackedTriStrips { scale, data } => {
                let data_index = reference(document, shape_index, data, "packed data")?;
                match decode(document, data_index)? {
                    TypedBlock::PackedTriStripsData(data) => packed_meshes(
                        data,
                        transform * Mat4::from_scale(Vec3::new(scale[0], scale[1], scale[2])),
                        material,
                    ),
                    _ => {
                        issue(
                            document,
                            issues,
                            data_index,
                            "packed shape data type is not supported".into(),
                        );
                        Vec::new()
                    }
                }
            }
            PhysicsShape::NiTriStrips {
                material: shape_material,
                scale,
                data,
            } => {
                material.get_or_insert(shape_material);
                let transform =
                    transform * Mat4::from_scale(Vec3::new(scale[0], scale[1], scale[2]));
                let mut output = Vec::new();
                for data in data {
                    let data_index = reference(document, shape_index, data, "tri strips data")?;
                    match decode(document, data_index)? {
                        TypedBlock::TriStripsData(data) => {
                            let vertices = data
                                .geometry
                                .vertices
                                .into_iter()
                                .map(|point| {
                                    basis(transform.transform_point3(
                                        Vec3::from_array(point) * GAME_UNITS_TO_METRES,
                                    ))
                                })
                                .collect();
                            let indices = collision_strip_triangles(&data.strips)
                                .into_iter()
                                .flatten()
                                .map(u32::from)
                                .collect();
                            output.push(ConvertedPhysicsShape::TriangleMesh { vertices, indices });
                        }
                        _ => issue(
                            document,
                            issues,
                            data_index,
                            "tri strips collision data type is not supported".into(),
                        ),
                    }
                }
                output
            }
        },
        TypedBlock::Unsupported => {
            issue(
                document,
                issues,
                shape_index,
                "collision shape type is not supported".into(),
            );
            Vec::new()
        }
        _ => {
            issue(
                document,
                issues,
                shape_index,
                "collision shape reference points to a non-shape block".into(),
            );
            Vec::new()
        }
    };
    visiting.remove(&shape_index);
    Ok(result)
}

fn packed_meshes(
    data: PackedTriStripsData,
    transform: Mat4,
    body_material: &mut Option<u32>,
) -> Vec<ConvertedPhysicsShape> {
    // Triangle indices address the packed data's complete vertex buffer. Sub-shape vertex counts
    // describe Havok material/filter partitions, but are not guaranteed to form independent
    // triangle meshes (some authored assets contain triangles spanning those ranges). Match the
    // reference importer and preserve the packed collision as one mesh.
    if let Some(sub_shape) = data.sub_shapes.first() {
        body_material.get_or_insert(sub_shape.material);
    }
    vec![packed_mesh(&data.vertices, &data.triangles, transform)]
}

fn packed_mesh(
    vertices: &[[f32; 3]],
    triangles: &[[u16; 3]],
    transform: Mat4,
) -> ConvertedPhysicsShape {
    let vertices = vertices
        .iter()
        .map(|&point| basis(transform.transform_point3(Vec3::from_array(point) * HAVOK_TO_METRES)))
        .collect::<Vec<_>>();
    let indices = triangles
        .iter()
        .flat_map(|triangle| triangle.map(u32::from))
        .collect();
    ConvertedPhysicsShape::TriangleMesh { vertices, indices }
}

fn av_parent_map(document: &Document) -> HashMap<usize, usize> {
    let mut parents = HashMap::new();
    for index in 0..document.blocks.len() {
        if let Ok(TypedBlock::Node(node)) = document.decode_block(index) {
            for child in node.children.into_iter().filter(|value| *value >= 0) {
                parents.entry(child as usize).or_insert(index);
            }
        }
    }
    parents
}

fn av_world_transform(
    document: &Document,
    index: usize,
    parents: &HashMap<usize, usize>,
    cache: &mut HashMap<usize, Mat4>,
    visiting: &mut HashSet<usize>,
) -> Mat4 {
    if let Some(value) = cache.get(&index) {
        return *value;
    }
    if !visiting.insert(index) {
        return Mat4::IDENTITY;
    }
    let local = av_object(document, index)
        .map(|object| av_transform(object.transform))
        .unwrap_or(Mat4::IDENTITY);
    let world = parents.get(&index).map_or(local, |parent| {
        av_world_transform(document, *parent, parents, cache, visiting) * local
    });
    visiting.remove(&index);
    cache.insert(index, world);
    world
}

fn av_object(document: &Document, index: usize) -> Option<AvObject> {
    match document.decode_block(index).ok()? {
        TypedBlock::Node(value) => Some(value.base),
        TypedBlock::Geometry(value) => Some(value.base),
        _ => None,
    }
}

fn av_name(document: &Document, index: usize) -> Option<String> {
    av_object(document, index).and_then(|value| value.object.name)
}

fn av_transform(transform: Transform) -> Mat4 {
    let rotation =
        Quat::from_mat3(&Mat3::from_cols_array(&transform.rotation).transpose()).normalize();
    Mat4::from_scale_rotation_translation(
        Vec3::splat(transform.scale),
        rotation,
        Vec3::from_array(transform.translation) * GAME_UNITS_TO_METRES,
    )
}

fn rigid_transform(body: &RigidBody) -> Mat4 {
    Mat4::from_rotation_translation(
        Quat::from_array(body.rotation).normalize(),
        vec3(body.translation) * HAVOK_TO_METRES,
    )
}

fn scaled_havok_matrix(values: [f32; 16]) -> Mat4 {
    let mut matrix = Mat4::from_cols_array(&values);
    matrix.w_axis.x *= HAVOK_TO_METRES;
    matrix.w_axis.y *= HAVOK_TO_METRES;
    matrix.w_axis.z *= HAVOK_TO_METRES;
    matrix
}

fn basis(point: Vec3) -> [f32; 3] {
    [point.x, point.z, -point.y]
}

fn basis_scaled(point: Vec3, scale: f32) -> [f32; 3] {
    basis(point * scale)
}

fn basis_rotation(rotation: Quat) -> Quat {
    let basis = Mat3::from_cols(Vec3::X, -Vec3::Z, Vec3::Y);
    Quat::from_mat3(&(basis * Mat3::from_quat(rotation) * basis.transpose())).normalize()
}

fn convert_inertia(values: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let source =
        Mat3::from_cols_array(&values.concat().try_into().expect("nine values")).transpose();
    let basis = Mat3::from_cols(Vec3::X, -Vec3::Z, Vec3::Y);
    let converted = basis * source * basis.transpose() * (HAVOK_TO_METRES * HAVOK_TO_METRES);
    let rows = converted.transpose().to_cols_array();
    [
        [rows[0], rows[1], rows[2]],
        [rows[3], rows[4], rows[5]],
        [rows[6], rows[7], rows[8]],
    ]
}

fn vec3(values: [f32; 4]) -> Vec3 {
    Vec3::new(values[0], values[1], values[2])
}

fn decode(document: &Document, block: usize) -> Result<TypedBlock, PhysicsError> {
    document
        .decode_block(block)
        .map_err(|error| PhysicsError::Decode {
            block,
            message: error.to_string(),
        })
}

fn reference(
    document: &Document,
    block: usize,
    reference: i32,
    field: &'static str,
) -> Result<usize, PhysicsError> {
    if reference < 0 || reference as usize >= document.blocks.len() {
        Err(PhysicsError::InvalidReference {
            block,
            field,
            reference,
        })
    } else {
        Ok(reference as usize)
    }
}

fn issue(
    document: &Document,
    issues: &mut Vec<PhysicsIssue>,
    source_block: usize,
    message: String,
) {
    issues.push(PhysicsIssue {
        source_block,
        type_name: document
            .blocks
            .get(source_block)
            .map(|block| block.type_name.clone())
            .unwrap_or_else(|| "<missing>".into()),
        message,
    });
}

fn collision_strip_triangles(strips: &[Vec<u16>]) -> Vec<[u16; 3]> {
    strips
        .iter()
        .flat_map(|strip| {
            strip.windows(3).enumerate().filter_map(|(offset, window)| {
                let triangle = if offset % 2 == 0 {
                    [window[0], window[1], window[2]]
                } else {
                    [window[1], window[0], window[2]]
                };
                (triangle[0] != triangle[1]
                    && triangle[1] != triangle[2]
                    && triangle[0] != triangle[2])
                    .then_some(triangle)
            })
        })
        .collect()
}

pub fn havok_material_name(value: u32) -> &'static str {
    match value {
        0 => "FO_HAV_MAT_STONE",
        1 => "FO_HAV_MAT_CLOTH",
        2 => "FO_HAV_MAT_DIRT",
        3 => "FO_HAV_MAT_GLASS",
        4 => "FO_HAV_MAT_GRASS",
        5 => "FO_HAV_MAT_METAL",
        6 => "FO_HAV_MAT_ORGANIC",
        7 => "FO_HAV_MAT_SKIN",
        8 => "FO_HAV_MAT_WATER",
        9 => "FO_HAV_MAT_WOOD",
        10 => "FO_HAV_MAT_HEAVY_STONE",
        11 => "FO_HAV_MAT_HEAVY_METAL",
        12 => "FO_HAV_MAT_HEAVY_WOOD",
        13 => "FO_HAV_MAT_CHAIN",
        14 => "FO_HAV_MAT_BOTTLECAP",
        15 => "FO_HAV_MAT_ELEVATOR",
        16 => "FO_HAV_MAT_HOLLOW_METAL",
        17 => "FO_HAV_MAT_SHEET_METAL",
        18 => "FO_HAV_MAT_SAND",
        19 => "FO_HAV_MAT_BROKEN_CONCRETE",
        20 => "FO_HAV_MAT_VEHICLE_BODY",
        21 => "FO_HAV_MAT_VEHICLE_PART_SOLID",
        22 => "FO_HAV_MAT_VEHICLE_PART_HOLLOW",
        23 => "FO_HAV_MAT_BARREL",
        24 => "FO_HAV_MAT_BOTTLE",
        25 => "FO_HAV_MAT_SODA_CAN",
        26 => "FO_HAV_MAT_PISTOL",
        27 => "FO_HAV_MAT_RIFLE",
        28 => "FO_HAV_MAT_SHOPPING_CART",
        29 => "FO_HAV_MAT_LUNCHBOX",
        30 => "FO_HAV_MAT_BABY_RATTLE",
        31 => "FO_HAV_MAT_RUBBER_BALL",
        _ => "FO_HAV_MAT_UNKNOWN",
    }
}

pub(crate) fn decode_physics_block(
    _document: &Document,
    block: &RawBlock,
    reader: &mut Reader<'_>,
) -> Option<Result<TypedBlock, Fo3Error>> {
    let result = match block.type_name.as_str() {
        "bhkCollisionObject" | "bhkBlendCollisionObject" | "bhkSPCollisionObject" => {
            parse_collision_object(block, reader).map(TypedBlock::CollisionObject)
        }
        "bhkRigidBody" | "bhkRigidBodyT" => {
            parse_rigid_body(block, reader).map(TypedBlock::RigidBody)
        }
        "bhkRagdollConstraint"
        | "bhkLimitedHingeConstraint"
        | "bhkHingeConstraint"
        | "bhkBallAndSocketConstraint"
        | "bhkPrismaticConstraint"
        | "bhkMalleableConstraint" => {
            parse_constraint_block(block, reader).map(TypedBlock::Constraint)
        }
        "bhkSimpleShapePhantom" => {
            parse_simple_shape_phantom(block, reader).map(TypedBlock::SimpleShapePhantom)
        }
        "bhkBoxShape" => parse_box(reader).map(TypedBlock::PhysicsShape),
        "bhkSphereShape" => parse_sphere(reader).map(TypedBlock::PhysicsShape),
        "bhkCapsuleShape" => parse_capsule(reader).map(TypedBlock::PhysicsShape),
        "bhkConvexVerticesShape" => {
            parse_convex_vertices(block, reader).map(TypedBlock::PhysicsShape)
        }
        "bhkTransformShape" | "bhkConvexTransformShape" => {
            parse_transform(reader).map(TypedBlock::PhysicsShape)
        }
        "bhkListShape" => parse_list(block, reader).map(TypedBlock::PhysicsShape),
        "bhkConvexListShape" => parse_convex_list(block, reader).map(TypedBlock::PhysicsShape),
        "bhkMoppBvTreeShape" => parse_mopp(reader).map(TypedBlock::PhysicsShape),
        "bhkPackedNiTriStripsShape" => parse_packed_shape(reader).map(TypedBlock::PhysicsShape),
        "bhkNiTriStripsShape" => {
            parse_ni_tri_strips_shape(block, reader).map(TypedBlock::PhysicsShape)
        }
        "hkPackedNiTriStripsData" => {
            parse_packed_data(block, reader).map(TypedBlock::PackedTriStripsData)
        }
        _ => return None,
    };
    Some(result)
}

fn parse_collision_object(
    block: &RawBlock,
    reader: &mut Reader<'_>,
) -> Result<CollisionObject, Fo3Error> {
    let target = reader.read_i32("collision target")?;
    let flags = reader.read_u16("collision flags")?;
    let body = reader.read_i32("collision body")?;
    if block.type_name == "bhkBlendCollisionObject" {
        reader.read_f32("hierarchy gain")?;
        reader.read_f32("velocity gain")?;
    }
    Ok(CollisionObject {
        target,
        flags,
        body,
        phantom: block.type_name == "bhkSPCollisionObject",
    })
}

fn parse_rigid_body(block: &RawBlock, reader: &mut Reader<'_>) -> Result<RigidBody, Fo3Error> {
    let shape = reader.read_i32("world object shape")?;
    let body_filter = read_filter(reader)?;
    reader.take(20, "world object info")?;
    reader.take(4, "entity info")?;

    reader.take(4, "rigid body unused 1")?;
    let info_filter = read_filter(reader)?;
    reader.take(4, "rigid body unused 2")?;
    reader.take(4, "rigid body response info")?;
    reader.take(4, "rigid body unused 4")?;
    let translation = read_vec4(reader, "rigid body translation")?;
    let rotation = read_vec4(reader, "rigid body rotation")?;
    let linear_velocity = read_vec4(reader, "linear velocity")?;
    let angular_velocity = read_vec4(reader, "angular velocity")?;
    let mut matrix = [[0.0; 4]; 3];
    for row in &mut matrix {
        *row = read_vec4(reader, "inertia tensor")?;
    }
    let inertia = [
        [matrix[0][0], matrix[0][1], matrix[0][2]],
        [matrix[1][0], matrix[1][1], matrix[1][2]],
        [matrix[2][0], matrix[2][1], matrix[2][2]],
    ];
    let center = read_vec4(reader, "center of mass")?;
    let mass = reader.read_f32("mass")?;
    let linear_damping = reader.read_f32("linear damping")?;
    let angular_damping = reader.read_f32("angular damping")?;
    let friction = reader.read_f32("friction")?;
    let restitution = reader.read_f32("restitution")?;
    let max_linear_velocity = reader.read_f32("maximum linear velocity")?;
    let max_angular_velocity = reader.read_f32("maximum angular velocity")?;
    let penetration_depth = reader.read_f32("penetration depth")?;
    let motion_system = reader.read_u8("motion system")?;
    let deactivator_type = reader.read_u8("deactivator type")?;
    let solver_deactivation = reader.read_u8("solver deactivation")?;
    let quality_type = reader.read_u8("quality type")?;
    reader.take(12, "rigid body unused 5")?;
    let constraint_count = reader.read_u32("constraint count")? as usize;
    checked_count(block, reader, constraint_count, 4, "constraint")?;
    let mut constraints = Vec::with_capacity(constraint_count);
    for _ in 0..constraint_count {
        constraints.push(reader.read_i32("constraint reference")?);
    }
    let body_flags = reader.read_u32("body flags")?;
    Ok(RigidBody {
        shape,
        body_filter,
        info_filter,
        translation,
        rotation,
        linear_velocity,
        angular_velocity,
        inertia,
        center,
        mass,
        linear_damping,
        angular_damping,
        friction,
        restitution,
        max_linear_velocity,
        max_angular_velocity,
        penetration_depth,
        motion_system,
        deactivator_type,
        solver_deactivation,
        quality_type,
        constraints,
        body_flags,
        transformed: block.type_name == "bhkRigidBodyT",
    })
}

fn parse_constraint_block(
    block: &RawBlock,
    reader: &mut Reader<'_>,
) -> Result<Constraint, Fo3Error> {
    let outer = read_constraint_info(reader)?;
    let (data, malleable_strength) = match block.type_name.as_str() {
        "bhkRagdollConstraint" => (parse_ragdoll_constraint(reader)?, None),
        "bhkLimitedHingeConstraint" => (parse_limited_hinge_constraint(reader)?, None),
        "bhkHingeConstraint" => (parse_hinge_constraint(reader)?, None),
        "bhkBallAndSocketConstraint" => (parse_ball_and_socket_constraint(reader)?, None),
        "bhkPrismaticConstraint" => (parse_prismatic_constraint(reader)?, None),
        "bhkMalleableConstraint" => {
            let constraint_type = reader.read_u32("malleable constraint type")?;
            let _nested = read_constraint_info(reader)?;
            let data = match constraint_type {
                0 => parse_ball_and_socket_constraint(reader)?,
                1 => parse_hinge_constraint(reader)?,
                2 => parse_limited_hinge_constraint(reader)?,
                6 => parse_prismatic_constraint(reader)?,
                7 => parse_ragdoll_constraint(reader)?,
                value => {
                    return Err(Fo3Error::InvalidEnum {
                        field: "malleable constraint type",
                        value,
                    });
                }
            };
            (data, Some(reader.read_f32("malleable strength")?))
        }
        _ => unreachable!("constraint parser is only dispatched for known constraint blocks"),
    };
    Ok(Constraint {
        entity_a: outer.0,
        entity_b: outer.1,
        data,
        malleable_strength,
    })
}

fn read_constraint_info(reader: &mut Reader<'_>) -> Result<(i32, i32), Fo3Error> {
    reader.read_u32("constraint entity count")?;
    let entity_a = reader.read_i32("constraint entity A")?;
    let entity_b = reader.read_i32("constraint entity B")?;
    reader.read_u32("constraint priority")?;
    Ok((entity_a, entity_b))
}

fn parse_ragdoll_constraint(reader: &mut Reader<'_>) -> Result<ConstraintData, Fo3Error> {
    let twist_a = read_vec4(reader, "ragdoll twist A")?;
    let plane_a = read_vec4(reader, "ragdoll plane A")?;
    read_vec4(reader, "ragdoll motor A")?;
    let pivot_a = read_vec4(reader, "ragdoll pivot A")?;
    let twist_b = read_vec4(reader, "ragdoll twist B")?;
    let plane_b = read_vec4(reader, "ragdoll plane B")?;
    read_vec4(reader, "ragdoll motor B")?;
    let pivot_b = read_vec4(reader, "ragdoll pivot B")?;
    let data = RagdollConstraint {
        twist_a,
        plane_a,
        pivot_a,
        twist_b,
        plane_b,
        pivot_b,
        cone_max_angle: reader.read_f32("ragdoll cone maximum")?,
        plane_min_angle: reader.read_f32("ragdoll plane minimum")?,
        plane_max_angle: reader.read_f32("ragdoll plane maximum")?,
        twist_min_angle: reader.read_f32("ragdoll twist minimum")?,
        twist_max_angle: reader.read_f32("ragdoll twist maximum")?,
    };
    reader.read_f32("ragdoll maximum friction")?;
    skip_constraint_motor(reader)?;
    Ok(ConstraintData::Ragdoll(data))
}

fn parse_limited_hinge_constraint(reader: &mut Reader<'_>) -> Result<ConstraintData, Fo3Error> {
    let axis_a = read_vec4(reader, "limited hinge axis A")?;
    let perpendicular_a = read_vec4(reader, "limited hinge perpendicular A1")?;
    read_vec4(reader, "limited hinge perpendicular A2")?;
    let pivot_a = read_vec4(reader, "limited hinge pivot A")?;
    let axis_b = read_vec4(reader, "limited hinge axis B")?;
    let perpendicular_b = read_vec4(reader, "limited hinge perpendicular B1")?;
    read_vec4(reader, "limited hinge perpendicular B2")?;
    let pivot_b = read_vec4(reader, "limited hinge pivot B")?;
    let data = LimitedHingeConstraint {
        axis_a,
        perpendicular_a,
        pivot_a,
        axis_b,
        perpendicular_b,
        pivot_b,
        min_angle: reader.read_f32("limited hinge minimum")?,
        max_angle: reader.read_f32("limited hinge maximum")?,
    };
    reader.read_f32("limited hinge maximum friction")?;
    skip_constraint_motor(reader)?;
    Ok(ConstraintData::LimitedHinge(data))
}

fn parse_hinge_constraint(reader: &mut Reader<'_>) -> Result<ConstraintData, Fo3Error> {
    let axis_a = read_vec4(reader, "hinge axis A")?;
    let perpendicular_a = read_vec4(reader, "hinge perpendicular A1")?;
    read_vec4(reader, "hinge perpendicular A2")?;
    let pivot_a = read_vec4(reader, "hinge pivot A")?;
    let axis_b = read_vec4(reader, "hinge axis B")?;
    let perpendicular_b = read_vec4(reader, "hinge perpendicular B1")?;
    read_vec4(reader, "hinge perpendicular B2")?;
    let pivot_b = read_vec4(reader, "hinge pivot B")?;
    Ok(ConstraintData::Hinge(HingeConstraint {
        axis_a,
        perpendicular_a,
        pivot_a,
        axis_b,
        perpendicular_b,
        pivot_b,
    }))
}

fn parse_ball_and_socket_constraint(reader: &mut Reader<'_>) -> Result<ConstraintData, Fo3Error> {
    Ok(ConstraintData::BallAndSocket(BallAndSocketConstraint {
        pivot_a: read_vec4(reader, "ball and socket pivot A")?,
        pivot_b: read_vec4(reader, "ball and socket pivot B")?,
    }))
}

fn parse_prismatic_constraint(reader: &mut Reader<'_>) -> Result<ConstraintData, Fo3Error> {
    let sliding_a = read_vec4(reader, "prismatic sliding A")?;
    let rotation_a = read_vec4(reader, "prismatic rotation A")?;
    read_vec4(reader, "prismatic plane A")?;
    let pivot_a = read_vec4(reader, "prismatic pivot A")?;
    let sliding_b = read_vec4(reader, "prismatic sliding B")?;
    let rotation_b = read_vec4(reader, "prismatic rotation B")?;
    read_vec4(reader, "prismatic plane B")?;
    let pivot_b = read_vec4(reader, "prismatic pivot B")?;
    let data = PrismaticConstraint {
        sliding_a,
        rotation_a,
        pivot_a,
        sliding_b,
        rotation_b,
        pivot_b,
        min_distance: reader.read_f32("prismatic minimum distance")?,
        max_distance: reader.read_f32("prismatic maximum distance")?,
    };
    reader.read_f32("prismatic friction")?;
    skip_constraint_motor(reader)?;
    Ok(ConstraintData::Prismatic(data))
}

fn skip_constraint_motor(reader: &mut Reader<'_>) -> Result<(), Fo3Error> {
    let payload_size = match reader.read_u8("constraint motor type")? {
        0 => 0,
        1 => 25,
        2 => 18,
        3 => 17,
        value => {
            return Err(Fo3Error::InvalidEnum {
                field: "constraint motor type",
                value: u32::from(value),
            });
        }
    };
    reader.take(payload_size, "constraint motor")?;
    Ok(())
}

fn parse_simple_shape_phantom(
    _block: &RawBlock,
    reader: &mut Reader<'_>,
) -> Result<SimpleShapePhantom, Fo3Error> {
    let shape = reader.read_i32("phantom shape")?;
    let filter = read_filter(reader)?;
    reader.take(20, "phantom world object info")?;
    reader.take(8, "phantom unused")?;
    let transform = read_matrix44(reader, "phantom transform")?;
    Ok(SimpleShapePhantom {
        shape,
        filter,
        transform,
    })
}

fn parse_box(reader: &mut Reader<'_>) -> Result<PhysicsShape, Fo3Error> {
    let material = reader.read_u32("box material")?;
    let radius = reader.read_f32("box radius")?;
    reader.take(8, "box unused")?;
    let half_extents = read_vec3(reader, "box dimensions")?;
    reader.read_f32("box unused float")?;
    Ok(PhysicsShape::Box {
        material,
        radius,
        half_extents,
    })
}

fn parse_sphere(reader: &mut Reader<'_>) -> Result<PhysicsShape, Fo3Error> {
    Ok(PhysicsShape::Sphere {
        material: reader.read_u32("sphere material")?,
        radius: reader.read_f32("sphere radius")?,
    })
}

fn parse_capsule(reader: &mut Reader<'_>) -> Result<PhysicsShape, Fo3Error> {
    let material = reader.read_u32("capsule material")?;
    let radius = reader.read_f32("capsule radius")?;
    reader.take(8, "capsule unused")?;
    let first = read_vec3(reader, "capsule first point")?;
    reader.read_f32("capsule radius 1")?;
    let second = read_vec3(reader, "capsule second point")?;
    reader.read_f32("capsule radius 2")?;
    Ok(PhysicsShape::Capsule {
        material,
        radius,
        first,
        second,
    })
}

fn parse_convex_vertices(
    block: &RawBlock,
    reader: &mut Reader<'_>,
) -> Result<PhysicsShape, Fo3Error> {
    let material = reader.read_u32("convex material")?;
    let radius = reader.read_f32("convex radius")?;
    reader.take(24, "convex properties")?;
    let count = reader.read_u32("convex vertex count")? as usize;
    checked_count(block, reader, count, 16, "convex vertex")?;
    let mut vertices = Vec::with_capacity(count);
    for _ in 0..count {
        let value = read_vec4(reader, "convex vertex")?;
        vertices.push([value[0], value[1], value[2]]);
    }
    let normal_count = reader.read_u32("convex normal count")? as usize;
    checked_count(block, reader, normal_count, 16, "convex normal")?;
    reader.take(normal_count * 16, "convex normals")?;
    Ok(PhysicsShape::ConvexVertices {
        material,
        radius,
        vertices,
    })
}

fn parse_transform(reader: &mut Reader<'_>) -> Result<PhysicsShape, Fo3Error> {
    let shape = reader.read_i32("transformed shape")?;
    let material = reader.read_u32("transform material")?;
    reader.read_f32("transform radius")?;
    reader.take(8, "transform unused")?;
    let transform = read_matrix44(reader, "shape transform")?;
    Ok(PhysicsShape::Transform {
        shape,
        material,
        transform,
    })
}

fn parse_list(block: &RawBlock, reader: &mut Reader<'_>) -> Result<PhysicsShape, Fo3Error> {
    let shapes = read_references(block, reader, "list sub shape")?;
    let material = reader.read_u32("list material")?;
    reader.take(24, "list properties")?;
    let filter_count = reader.read_u32("list filter count")? as usize;
    checked_count(block, reader, filter_count, 4, "list filter")?;
    reader.take(filter_count * 4, "list filters")?;
    Ok(PhysicsShape::List { shapes, material })
}

fn parse_convex_list(block: &RawBlock, reader: &mut Reader<'_>) -> Result<PhysicsShape, Fo3Error> {
    let shapes = read_references(block, reader, "convex list sub shape")?;
    let material = reader.read_u32("convex list material")?;
    reader.read_f32("convex list radius")?;
    reader.read_u32("convex list unknown int")?;
    reader.read_f32("convex list unknown float")?;
    reader.take(12, "convex list child property")?;
    reader.read_u8("convex list cached AABB")?;
    reader.read_f32("convex list closest distance")?;
    Ok(PhysicsShape::ConvexList { shapes, material })
}

fn parse_mopp(reader: &mut Reader<'_>) -> Result<PhysicsShape, Fo3Error> {
    let shape = reader.read_i32("MOPP child shape")?;
    reader.take(12, "MOPP unused")?;
    reader.read_f32("MOPP scale")?;
    let size = reader.read_u32("MOPP data size")? as usize;
    reader.take(16, "MOPP offset")?;
    reader.take(size, "MOPP data")?;
    Ok(PhysicsShape::Mopp { shape })
}

fn parse_packed_shape(reader: &mut Reader<'_>) -> Result<PhysicsShape, Fo3Error> {
    reader.read_u32("packed user data")?;
    reader.take(4, "packed unused 1")?;
    reader.read_f32("packed radius")?;
    reader.take(4, "packed unused 2")?;
    let scale = read_vec4(reader, "packed scale")?;
    reader.read_f32("packed radius copy")?;
    reader.take(16, "packed scale copy")?;
    let data = reader.read_i32("packed data reference")?;
    Ok(PhysicsShape::PackedTriStrips { scale, data })
}

fn parse_ni_tri_strips_shape(
    block: &RawBlock,
    reader: &mut Reader<'_>,
) -> Result<PhysicsShape, Fo3Error> {
    let material = reader.read_u32("tri strips material")?;
    reader.read_f32("tri strips radius")?;
    reader.take(20, "tri strips unused")?;
    reader.read_u32("tri strips grow by")?;
    let scale = read_vec4(reader, "tri strips scale")?;
    let data = read_references(block, reader, "tri strips data")?;
    let filter_count = reader.read_u32("tri strips filter count")? as usize;
    checked_count(block, reader, filter_count, 4, "tri strips filter")?;
    reader.take(filter_count * 4, "tri strips filters")?;
    Ok(PhysicsShape::NiTriStrips {
        material,
        scale,
        data,
    })
}

fn parse_packed_data(
    block: &RawBlock,
    reader: &mut Reader<'_>,
) -> Result<PackedTriStripsData, Fo3Error> {
    let triangle_count = reader.read_u32("packed triangle count")? as usize;
    checked_count(block, reader, triangle_count, 8, "packed triangle")?;
    let mut triangles = Vec::with_capacity(triangle_count);
    for _ in 0..triangle_count {
        triangles.push([
            reader.read_u16("packed triangle index")?,
            reader.read_u16("packed triangle index")?,
            reader.read_u16("packed triangle index")?,
        ]);
        reader.read_u16("packed welding info")?;
    }
    let vertex_count = reader.read_u32("packed vertex count")? as usize;
    let compressed = reader.read_u8("packed vertices compressed")? != 0;
    checked_count(
        block,
        reader,
        vertex_count,
        if compressed { 6 } else { 12 },
        "packed vertex",
    )?;
    let mut vertices = Vec::with_capacity(vertex_count);
    for _ in 0..vertex_count {
        vertices.push(if compressed {
            [
                half::f16::from_bits(reader.read_u16("half vertex x")?).to_f32(),
                half::f16::from_bits(reader.read_u16("half vertex y")?).to_f32(),
                half::f16::from_bits(reader.read_u16("half vertex z")?).to_f32(),
            ]
        } else {
            read_vec3(reader, "packed vertex")?
        });
    }
    let sub_shape_count = reader.read_u16("packed sub shape count")? as usize;
    checked_count(block, reader, sub_shape_count, 12, "packed sub shape")?;
    let mut sub_shapes = Vec::with_capacity(sub_shape_count);
    for _ in 0..sub_shape_count {
        sub_shapes.push(PackedSubShape {
            filter: read_filter(reader)?,
            vertex_count: reader.read_u32("sub shape vertex count")?,
            material: reader.read_u32("sub shape material")?,
        });
    }
    Ok(PackedTriStripsData {
        triangles,
        vertices,
        sub_shapes,
    })
}

fn read_references(
    block: &RawBlock,
    reader: &mut Reader<'_>,
    field: &'static str,
) -> Result<Vec<i32>, Fo3Error> {
    let count = reader.read_u32(field)? as usize;
    checked_count(block, reader, count, 4, field)?;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(reader.read_i32(field)?);
    }
    Ok(values)
}

fn read_filter(reader: &mut Reader<'_>) -> Result<HavokFilter, Fo3Error> {
    Ok(HavokFilter {
        layer: reader.read_u8("Havok layer")?,
        flags: reader.read_u8("Havok filter flags")?,
        group: reader.read_u16("Havok collision group")?,
    })
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

fn read_matrix44(reader: &mut Reader<'_>, field: &'static str) -> Result<[f32; 16], Fo3Error> {
    let mut values = [0.0; 16];
    for value in &mut values {
        *value = reader.read_f32(field)?;
    }
    Ok(values)
}

fn checked_count(
    block: &RawBlock,
    reader: &Reader<'_>,
    count: usize,
    element_size: usize,
    field: &'static str,
) -> Result<usize, Fo3Error> {
    if count
        .checked_mul(element_size)
        .is_none_or(|required| required > reader.remaining())
    {
        return Err(Fo3Error::InvalidBlockCount {
            block: block.index as usize,
            type_name: block.type_name.clone(),
            field,
            count,
            remaining: reader.remaining(),
        });
    }
    Ok(count)
}

pub fn motion_system_name(value: u8) -> &'static str {
    match value {
        1 => "MO_SYS_DYNAMIC",
        2 => "MO_SYS_SPHERE_INERTIA",
        3 => "MO_SYS_SPHERE_STABILIZED",
        4 => "MO_SYS_BOX_INERTIA",
        5 => "MO_SYS_BOX_STABILIZED",
        6 => "MO_SYS_KEYFRAMED",
        7 => "MO_SYS_FIXED",
        8 => "MO_SYS_THIN_BOX",
        9 => "MO_SYS_CHARACTER",
        _ => "MO_SYS_INVALID",
    }
}

pub fn quality_type_name(value: u8) -> &'static str {
    match value {
        1 => "MO_QUAL_FIXED",
        2 => "MO_QUAL_KEYFRAMED",
        3 => "MO_QUAL_DEBRIS",
        4 => "MO_QUAL_MOVING",
        5 => "MO_QUAL_CRITICAL",
        6 => "MO_QUAL_BULLET",
        7 => "MO_QUAL_USER",
        8 => "MO_QUAL_CHARACTER",
        9 => "MO_QUAL_KEYFRAMED_REPORT",
        _ => "MO_QUAL_INVALID",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn push_i32(bytes: &mut Vec<u8>, value: i32) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn push_f32(bytes: &mut Vec<u8>, value: f32) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn push_vec4(bytes: &mut Vec<u8>, value: [f32; 4]) {
        for component in value {
            push_f32(bytes, component);
        }
    }

    fn push_constraint_info(bytes: &mut Vec<u8>, body_a: i32, body_b: i32) {
        push_u32(bytes, 2);
        push_i32(bytes, body_a);
        push_i32(bytes, body_b);
        push_u32(bytes, 1);
    }

    fn block(type_name: &str, bytes: Vec<u8>) -> RawBlock {
        RawBlock {
            index: 7,
            type_name: type_name.into(),
            bytes,
        }
    }

    fn ragdoll_payload(bytes: &mut Vec<u8>) {
        for value in [
            [0.0, 0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [1.0, 2.0, 3.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [4.0, 5.0, 6.0, 0.0],
        ] {
            push_vec4(bytes, value);
        }
        for value in [1.2, -0.4, 0.5, -0.7, 0.8, 100.0] {
            push_f32(bytes, value);
        }
        bytes.push(0); // MOTOR_NONE
    }

    fn limited_hinge_payload(bytes: &mut Vec<u8>) {
        for value in [
            [0.0, 0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [1.0, 2.0, 3.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [4.0, 5.0, 6.0, 0.0],
        ] {
            push_vec4(bytes, value);
        }
        for value in [-0.2, 1.4, 100.0] {
            push_f32(bytes, value);
        }
        bytes.push(0); // MOTOR_NONE
    }

    #[test]
    fn direct_ragdoll_constraint_decodes_fo3_field_order() {
        let mut bytes = Vec::new();
        push_constraint_info(&mut bytes, 12, 34);
        ragdoll_payload(&mut bytes);
        let block = block("bhkRagdollConstraint", bytes);
        let mut reader = Reader::new(&block.bytes);

        let constraint = parse_constraint_block(&block, &mut reader).unwrap();

        assert_eq!((constraint.entity_a, constraint.entity_b), (12, 34));
        let ConstraintData::Ragdoll(data) = constraint.data else {
            panic!("expected ragdoll constraint");
        };
        assert_eq!(data.pivot_a, [1.0, 2.0, 3.0, 0.0]);
        assert_eq!(data.pivot_b, [4.0, 5.0, 6.0, 0.0]);
        assert_eq!(data.cone_max_angle, 1.2);
        assert_eq!(data.twist_max_angle, 0.8);
        assert_eq!(constraint.malleable_strength, None);
        assert_eq!(reader.remaining(), 0);
    }

    #[test]
    fn malleable_limited_hinge_decodes_nested_info_and_strength() {
        let mut bytes = Vec::new();
        push_constraint_info(&mut bytes, 12, 34);
        push_u32(&mut bytes, 2); // LIMITED_HINGE
        push_constraint_info(&mut bytes, 12, 34);
        limited_hinge_payload(&mut bytes);
        push_f32(&mut bytes, 0.9);
        let block = block("bhkMalleableConstraint", bytes);
        let mut reader = Reader::new(&block.bytes);

        let constraint = parse_constraint_block(&block, &mut reader).unwrap();

        let ConstraintData::LimitedHinge(data) = constraint.data else {
            panic!("expected limited hinge constraint");
        };
        assert_eq!(data.axis_a, [0.0, 0.0, 1.0, 0.0]);
        assert_eq!(data.pivot_b, [4.0, 5.0, 6.0, 0.0]);
        assert_eq!((data.min_angle, data.max_angle), (-0.2, 1.4));
        assert_eq!(constraint.malleable_strength, Some(0.9));
        assert_eq!(reader.remaining(), 0);
    }

    #[test]
    fn joint_frame_places_authored_axis_on_local_z() {
        let frame = joint_frame_quaternion(Vec3::Z, Vec3::X).unwrap();
        let rotation = Quat::from_array(frame);
        assert!((rotation * Vec3::Z - Vec3::Z).length() < 1.0e-6);
        assert!((rotation * Vec3::X - Vec3::X).length() < 1.0e-6);
        assert!(frame[3] >= 0.0);
    }

    #[test]
    fn rigid_body_center_of_mass_is_flattened_with_its_body_frame() {
        let frame = Mat4::from_rotation_translation(
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            Vec3::new(1.0, 2.0, 3.0),
        );

        let center = rigid_body_point([10.0, 0.0, 0.0, 0.0], frame);

        let expected = Vec3::new(1.0, 3.0, -(2.0 + 10.0 * HAVOK_TO_METRES));
        assert!((Vec3::from_array(center) - expected).length() < 1.0e-6);
    }
}
