# nifty

Cross-platform Rust NIF tooling. The legacy parser remains available for its
original 20.0.0.4 assets; `nif::fo3` is the focused, size-bounded parser and
typed scene/physics/animation extraction path for Fallout 3 and Fallout: New
Vegas NIF `20.2.0.7`. The `fo3_glb` feature emits self-contained GLB files
through `gltf-json`/`gltf`, without Blender or PyNifly.
