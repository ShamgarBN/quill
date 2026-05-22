//! Serde-serialized data structures shared across services and exposed over IPC.
//!
//! Field naming: snake_case to match TypeScript expectations (we don't rename
//! at the wire boundary; the Rust types ARE the wire types).

mod commit;
mod project;
pub mod settings;

pub use commit::CommitInfo;
pub use project::Project;
#[allow(unused_imports)]
pub use settings::{FontPreference, GenerationMode, Settings, ThemePreference};
