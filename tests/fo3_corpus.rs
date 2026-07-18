use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

fn collect_nifs(directory: &Path, output: &mut Vec<PathBuf>) {
    let mut entries = std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", directory.display()))
        .map(|entry| entry.expect("directory entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_nifs(&path, output);
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("nif"))
        {
            output.push(path);
        }
    }
}

/// Local acceptance only. The corpus contains Bethesda-derived data and must
/// never be checked into this repository.
#[test]
#[ignore = "set NIFTY_FO3_CORPUS to a local FO3/FNV NIF directory"]
fn parses_local_fo3_corpus_without_header_or_block_boundary_failures() {
    let root = PathBuf::from(
        std::env::var_os("NIFTY_FO3_CORPUS")
            .expect("NIFTY_FO3_CORPUS must point to a local NIF corpus"),
    );
    let mut files = Vec::new();
    collect_nifs(&root, &mut files);
    assert!(
        !files.is_empty(),
        "no NIF files found under {}",
        root.display()
    );

    let mut failures = Vec::new();
    for path in &files {
        let bytes = std::fs::read(path).expect("read NIF");
        if let Err(error) = nif::fo3::parse(&bytes) {
            failures.push(format!("{}: {error}", path.display()));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} NIFs failed:\n{}",
        failures.len(),
        files.len(),
        failures.join("\n")
    );
    println!("parsed {} FO3/FNV NIFs", files.len());
}

/// Local typed-decoder acceptance. Unsupported block types are deliberately
/// retained as raw bytes; every block type handled by the props milestone must
/// consume its complete size-bounded payload.
#[test]
#[ignore = "set NIFTY_FO3_CORPUS to a local FO3/FNV NIF directory"]
fn decodes_supported_blocks_in_local_fo3_corpus() {
    let root = PathBuf::from(
        std::env::var_os("NIFTY_FO3_CORPUS")
            .expect("NIFTY_FO3_CORPUS must point to a local NIF corpus"),
    );
    let mut files = Vec::new();
    collect_nifs(&root, &mut files);
    assert!(
        !files.is_empty(),
        "no NIF files found under {}",
        root.display()
    );

    let mut failures = Vec::new();
    let mut decoded = 0usize;
    for path in &files {
        let bytes = std::fs::read(path).expect("read NIF");
        let document = match nif::fo3::parse(&bytes) {
            Ok(document) => document,
            Err(error) => {
                failures.push(format!("{}: {error}", path.display()));
                continue;
            }
        };
        for index in 0..document.blocks.len() {
            match document.decode_block(index) {
                Ok(nif::fo3::TypedBlock::Unsupported) => {}
                Ok(_) => decoded += 1,
                Err(error) => failures.push(format!(
                    "{} block {index} {}: {error}",
                    path.display(),
                    document.blocks[index].type_name
                )),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} typed blocks failed across {} NIFs:\n{}",
        failures.len(),
        files.len(),
        failures.join("\n")
    );
    println!(
        "decoded {decoded} supported blocks across {} FO3/FNV NIFs",
        files.len()
    );
}

#[test]
#[ignore = "set NIFTY_FO3_CORPUS to a local FO3/FNV NIF directory"]
fn extracts_props_scenes_from_local_fo3_corpus() {
    let root = PathBuf::from(
        std::env::var_os("NIFTY_FO3_CORPUS")
            .expect("NIFTY_FO3_CORPUS must point to a local NIF corpus"),
    );
    let mut files = Vec::new();
    collect_nifs(&root, &mut files);
    assert!(
        !files.is_empty(),
        "no NIF files found under {}",
        root.display()
    );

    let mut failures = Vec::new();
    let mut lossy = Vec::new();
    let mut meshes = 0usize;
    let mut triangles = 0usize;
    for path in &files {
        let bytes = std::fs::read(path).expect("read NIF");
        let document = nif::fo3::parse(&bytes).expect("raw FO3 parser acceptance runs first");
        match nif::fo3::extract_scene(&document) {
            Ok(scene) => {
                meshes += scene.statistics.source_meshes;
                triangles += scene.statistics.source_triangles;
                if !scene.issues.is_empty() {
                    lossy.push(format!(
                        "{}: {}",
                        path.display(),
                        scene
                            .issues
                            .iter()
                            .map(|issue| format!(
                                "block {} {}: {}",
                                issue.source_block, issue.type_name, issue.message
                            ))
                            .collect::<Vec<_>>()
                            .join("; ")
                    ));
                }
            }
            Err(error) => failures.push(format!("{}: {error}", path.display())),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} NIF scenes failed:\n{}",
        failures.len(),
        files.len(),
        failures.join("\n")
    );
    println!(
        "extracted {meshes} meshes / {triangles} triangles across {} NIFs; {} files reported lossy reachable blocks",
        files.len(),
        lossy.len()
    );
    for issue in lossy.iter().take(50) {
        println!("lossy: {issue}");
    }
}

#[test]
#[ignore = "set NIFTY_FO3_SAMPLE to a local FO3/FNV NIF file"]
fn writes_a_valid_glb_for_a_local_fo3_sample() {
    let path = PathBuf::from(
        std::env::var_os("NIFTY_FO3_SAMPLE")
            .expect("NIFTY_FO3_SAMPLE must point to a local NIF file"),
    );
    let bytes = std::fs::read(&path).expect("read NIF");
    let document = nif::fo3::parse(&bytes).expect("parse NIF");
    let scene = nif::fo3::extract_scene(&document).expect("extract scene");
    assert!(
        scene.statistics.source_meshes > 0,
        "sample has no render meshes"
    );
    let output = nif::fo3::encode_glb(
        &scene,
        &BTreeMap::new(),
        &nif::fo3::GlbOptions {
            source_name: path.file_name().unwrap().to_string_lossy().into_owned(),
            allow_missing_textures: true,
        },
    )
    .expect("encode GLB");
    let gltf = gltf::Gltf::from_slice(&output.bytes).expect("validate GLB");
    assert_eq!(
        gltf.document.meshes().count(),
        scene.statistics.source_meshes
    );
    assert_eq!(gltf.document.animations().count(), scene.animations.len());
    assert!(gltf.blob.is_some(), "GLB has no binary chunk");
    println!(
        "wrote {} bytes with {} meshes / {} triangles; {} texture files omitted",
        output.bytes.len(),
        scene.statistics.source_meshes,
        scene.statistics.source_triangles,
        output.missing_textures.len()
    );
}

#[test]
#[ignore = "set NIFTY_FO3_CORPUS to a local FO3/FNV NIF directory"]
fn reports_local_fo3_havok_block_coverage() {
    let root = PathBuf::from(
        std::env::var_os("NIFTY_FO3_CORPUS")
            .expect("NIFTY_FO3_CORPUS must point to a local NIF corpus"),
    );
    let mut files = Vec::new();
    collect_nifs(&root, &mut files);
    let mut counts = BTreeMap::<String, (usize, usize)>::new();
    for path in &files {
        let bytes = std::fs::read(path).expect("read NIF");
        let document = nif::fo3::parse(&bytes).expect("parse NIF");
        let mut seen = std::collections::BTreeSet::new();
        for block in document.blocks {
            if block.type_name.starts_with("bhk")
                || block.type_name.contains("PackedNiTriStrips")
                || block.type_name == "NiTriStripsShape"
            {
                counts.entry(block.type_name.clone()).or_default().0 += 1;
                seen.insert(block.type_name);
            }
        }
        for type_name in seen {
            counts.entry(type_name).or_default().1 += 1;
        }
    }
    for (type_name, (blocks, files)) in counts {
        println!("{type_name}: blocks={blocks} files={files}");
    }
}

#[test]
#[ignore = "set NIFTY_FO3_CORPUS to a local FO3/FNV NIF directory"]
fn reports_local_fo3_animation_block_coverage() {
    let root = PathBuf::from(
        std::env::var_os("NIFTY_FO3_CORPUS")
            .expect("NIFTY_FO3_CORPUS must point to a local NIF corpus"),
    );
    let mut files = Vec::new();
    collect_nifs(&root, &mut files);
    let mut counts = BTreeMap::<String, (usize, usize)>::new();
    for path in &files {
        let bytes = std::fs::read(path).expect("read NIF");
        let document = nif::fo3::parse(&bytes).expect("parse NIF");
        let mut seen = std::collections::BTreeSet::new();
        for block in document.blocks {
            if block.type_name.contains("Controller")
                || block.type_name.contains("Interpolator")
                || block.type_name.contains("TransformData")
                || block.type_name.contains("KeyframeData")
            {
                counts.entry(block.type_name.clone()).or_default().0 += 1;
                seen.insert(block.type_name);
            }
        }
        for type_name in seen {
            counts.entry(type_name).or_default().1 += 1;
        }
    }
    for (type_name, (blocks, files)) in counts {
        println!("{type_name}: blocks={blocks} files={files}");
    }
}

#[test]
#[ignore = "set NIFTY_FO3_CORPUS to a local FO3/FNV NIF directory"]
fn extracts_local_fo3_animation_tracks() {
    let root = PathBuf::from(
        std::env::var_os("NIFTY_FO3_CORPUS")
            .expect("NIFTY_FO3_CORPUS must point to a local NIF corpus"),
    );
    let mut files = Vec::new();
    collect_nifs(&root, &mut files);
    let mut animations = 0usize;
    let mut channels = 0usize;
    let mut keyframes = 0usize;
    let mut failures = Vec::new();
    for path in &files {
        let bytes = std::fs::read(path).expect("read NIF");
        let document = nif::fo3::parse(&bytes).expect("parse NIF");
        let scene = nif::fo3::extract_scene(&document).expect("extract scene");
        animations += scene.animations.len();
        for animation in scene.animations {
            channels += animation.channels.len();
            for channel in animation.channels {
                keyframes +=
                    channel.translations.len() + channel.rotations.len() + channel.scales.len();
                let finite = channel.translations.iter().all(|key| {
                    key.time.is_finite() && key.value.iter().all(|value| value.is_finite())
                }) && channel.rotations.iter().all(|key| {
                    key.time.is_finite() && key.value.iter().all(|value| value.is_finite())
                }) && channel
                    .scales
                    .iter()
                    .all(|key| key.time.is_finite() && key.value.is_finite());
                if !finite {
                    failures.push(format!("{}: non-finite animation key", path.display()));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(animations > 0 && channels > 0 && keyframes > 0);
    println!(
        "extracted {animations} animations / {channels} channels / {keyframes} keyframes across {} NIFs",
        files.len()
    );
}

#[test]
#[ignore = "set NIFTY_FO3_CORPUS to a local FO3/FNV NIF directory"]
fn extracts_authored_physics_from_local_fo3_corpus() {
    let root = PathBuf::from(
        std::env::var_os("NIFTY_FO3_CORPUS")
            .expect("NIFTY_FO3_CORPUS must point to a local NIF corpus"),
    );
    let mut files = Vec::new();
    collect_nifs(&root, &mut files);
    let mut failures = Vec::new();
    let mut issues = Vec::new();
    let mut bodies = 0usize;
    let mut shapes = 0usize;
    for path in &files {
        let bytes = std::fs::read(path).expect("read NIF");
        let document = nif::fo3::parse(&bytes).expect("parse NIF");
        match nif::fo3::extract_physics(&document) {
            Ok(physics) => {
                bodies += physics.bodies.len();
                for body in physics.bodies {
                    shapes += body.shapes.len();
                    for shape in body.shapes {
                        let description = format!("{shape:?}");
                        let usable = match shape {
                            nif::fo3::ConvertedPhysicsShape::Box {
                                center,
                                half_extents,
                                rotation_xyzw,
                            } => {
                                center
                                    .iter()
                                    .chain(&half_extents)
                                    .chain(&rotation_xyzw)
                                    .all(|value| value.is_finite())
                                    && half_extents.iter().all(|value| *value > 0.0)
                            }
                            nif::fo3::ConvertedPhysicsShape::Sphere { center, radius } => {
                                center.into_iter().all(f32::is_finite)
                                    && radius.is_finite()
                                    && radius > 0.0
                            }
                            nif::fo3::ConvertedPhysicsShape::Capsule {
                                point1,
                                point2,
                                radius,
                            } => {
                                point1.into_iter().chain(point2).all(f32::is_finite)
                                    && radius.is_finite()
                                    && radius > 0.0
                            }
                            nif::fo3::ConvertedPhysicsShape::ConvexHull { points } => {
                                points.len() >= 4
                                    && points.iter().flatten().all(|value| value.is_finite())
                            }
                            nif::fo3::ConvertedPhysicsShape::TriangleMesh { vertices, indices } => {
                                vertices.len() >= 3
                                    && indices.len() >= 3
                                    && indices.len() % 3 == 0
                                    && vertices.iter().flatten().all(|value| value.is_finite())
                                    && indices
                                        .iter()
                                        .all(|index| (*index as usize) < vertices.len())
                            }
                        };
                        if !usable {
                            failures.push(format!(
                                "{}: unusable converted shape {description}",
                                path.display()
                            ));
                        }
                    }
                }
                for issue in physics.issues {
                    issues.push(format!(
                        "{} block {} {}: {}",
                        path.display(),
                        issue.source_block,
                        issue.type_name,
                        issue.message
                    ));
                }
            }
            Err(error) => failures.push(format!("{}: {error}", path.display())),
        }
    }
    assert!(
        failures.is_empty(),
        "{} physics failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
    println!(
        "extracted {bodies} bodies / {shapes} shapes across {} NIFs; {} issues",
        files.len(),
        issues.len()
    );
    for issue in issues.iter().take(50) {
        println!("physics issue: {issue}");
    }
}
