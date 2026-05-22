//! Serde-serialized data structures shared across services and exposed over IPC.
//!
//! Field naming: snake_case to match TypeScript expectations (we don't rename
//! at the wire boundary; the Rust types ARE the wire types).

pub mod brain;
pub mod canon;
mod commit;
mod project;
pub mod settings;
pub mod structure;

#[allow(unused_imports)]
pub use brain::{Character, CharacterPatch, CharacterRole, CrossLink, Idea, IdeaPatch};
#[allow(unused_imports)]
pub use canon::{CanonChunk, CanonDocument, CanonKind, ChunkRef, ChunkSensitivity};
pub use commit::CommitInfo;
pub use project::Project;
#[allow(unused_imports)]
pub use settings::{FontPreference, GenerationMode, Settings, ThemePreference};
#[allow(unused_imports)]
pub use structure::{Beat, BeatId, BeatSheet, Scene, SceneList, SceneStatus};
