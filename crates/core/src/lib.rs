//! iv_core: every rule of Iron Vector.
//!
//! Builder requirement: this crate performs no input or output. It takes
//! values and returns values, so `cargo test` reaches every rule without a
//! browser. `tests/boundary.rs` enforces that, along with the dependency
//! allowlist and the reference ban on floats, hash maps and clocks.

pub mod campaign;
pub mod combat;
pub mod content;
pub mod fx;
pub mod game;
pub mod garage;
pub mod geom;
pub mod map;
pub mod mech;
pub mod mesh;
pub mod mission;
pub mod model;
pub mod parts;
pub mod pilot;
pub mod render;
pub mod rng;
pub mod starmap;
pub mod world;
