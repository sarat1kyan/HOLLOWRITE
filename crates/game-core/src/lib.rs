//! Authoritative HOLLOWRITE simulation.
//!
//! Everything here is deterministic for a given seed and sequence of
//! inputs: no wall clock, no I/O beyond loading content, no threads.

pub mod ai;
pub mod character;
pub mod combat;
pub mod content;
pub mod geom;
pub mod map;
pub mod rng;
pub mod world;

pub use character::CharacterData;
pub use content::Content;
pub use geom::Pos;
pub use world::{Out, SaveRequest, World};

/// Location of the repository content directory, for tests and tools.
pub fn default_content_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}
