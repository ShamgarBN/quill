//! Phase 7 — Second-brain service: Character Bible + Idea Park stores
//! plus cross-link queries that join brain entries to canon chunks and
//! manuscript scenes.

mod cross_link;
mod store;

pub use cross_link::find_cross_links;
pub use store::{CharacterStore, IdeaStore};
