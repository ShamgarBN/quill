//! Manuscript content service.
//!
//! Stores scene prose as plain Markdown files, one per scene, on disk:
//!
//!   <project>/manuscript/<NNNN>-<scene-id>.md
//!
//! - The numeric prefix mirrors `scene.order` so a directory listing reads
//!   in narrative order (handy when the user wants to grep, diff, or open
//!   the file in any other editor).
//! - The filename ends with the scene id so renaming the title doesn't move
//!   the file (which would confuse Git history).
//! - Word counts are computed on save and propagated back into the scene
//!   metadata so the beat sheet's "drafted vs. target" math stays accurate.
//!
//! Discipline: this module owns the content; `services::structure::store`
//! owns the metadata. Both are joined at the command-handler layer.

mod store;

pub use store::{CompileOptions, CompileReport, ManuscriptStore, SceneContent};
