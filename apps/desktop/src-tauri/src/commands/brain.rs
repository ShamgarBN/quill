//! Phase 7 — Character Bible + Idea Park commands.

use crate::error::{QuillError, Result};
use crate::models::brain::{
    Character, CharacterPatch, CrossLink, Idea, IdeaPatch, Thread, ThreadPatch, WorldEntry,
    WorldEntryPatch, WorldKind,
};
use crate::services::brain::{
    find_cross_links, CharacterStore, IdeaStore, ThreadStore, WorldStore,
};
use crate::state::AppState;
use tauri::State;

// ---------- Characters ----------

#[tauri::command]
pub fn brain_characters_list(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<Character>> {
    CharacterStore::new(&state.projects).list(&project_id)
}

#[tauri::command]
pub fn brain_character_create(
    state: State<'_, AppState>,
    project_id: String,
    name: String,
) -> Result<Character> {
    CharacterStore::new(&state.projects).create(&project_id, &name)
}

#[tauri::command]
pub fn brain_character_update(
    state: State<'_, AppState>,
    project_id: String,
    id: String,
    patch: CharacterPatch,
) -> Result<Character> {
    CharacterStore::new(&state.projects).update(&project_id, &id, patch)
}

#[tauri::command]
pub fn brain_character_delete(
    state: State<'_, AppState>,
    project_id: String,
    id: String,
) -> Result<()> {
    CharacterStore::new(&state.projects).delete(&project_id, &id)
}

#[tauri::command]
pub async fn brain_character_cross_links(
    state: State<'_, AppState>,
    project_id: String,
    id: String,
) -> Result<Vec<CrossLink>> {
    let store = CharacterStore::new(&state.projects);
    let chars = store.list(&project_id)?;
    let character = chars
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| QuillError::NotFound(format!("character {id}")))?;
    find_cross_links(&character, &state.projects, state.vectors.as_ref()).await
}

// ---------- Ideas ----------

#[tauri::command]
pub fn brain_ideas_list(state: State<'_, AppState>, project_id: String) -> Result<Vec<Idea>> {
    IdeaStore::new(&state.projects).list(&project_id)
}

#[tauri::command]
pub fn brain_idea_create(
    state: State<'_, AppState>,
    project_id: String,
    text: String,
) -> Result<Idea> {
    IdeaStore::new(&state.projects).create(&project_id, &text)
}

#[tauri::command]
pub fn brain_idea_update(
    state: State<'_, AppState>,
    project_id: String,
    id: String,
    patch: IdeaPatch,
) -> Result<Idea> {
    IdeaStore::new(&state.projects).update(&project_id, &id, patch)
}

#[tauri::command]
pub fn brain_idea_delete(state: State<'_, AppState>, project_id: String, id: String) -> Result<()> {
    IdeaStore::new(&state.projects).delete(&project_id, &id)
}

// ---------- Threads ----------

#[tauri::command]
pub fn brain_threads_list(state: State<'_, AppState>, project_id: String) -> Result<Vec<Thread>> {
    ThreadStore::new(&state.projects).list(&project_id)
}

#[tauri::command]
pub fn brain_thread_create(
    state: State<'_, AppState>,
    project_id: String,
    title: String,
) -> Result<Thread> {
    ThreadStore::new(&state.projects).create(&project_id, &title)
}

#[tauri::command]
pub fn brain_thread_update(
    state: State<'_, AppState>,
    project_id: String,
    id: String,
    patch: ThreadPatch,
) -> Result<Thread> {
    ThreadStore::new(&state.projects).update(&project_id, &id, patch)
}

#[tauri::command]
pub fn brain_thread_delete(
    state: State<'_, AppState>,
    project_id: String,
    id: String,
) -> Result<()> {
    ThreadStore::new(&state.projects).delete(&project_id, &id)
}

// ---------- World Bible (places / factions / lore) ----------

#[tauri::command]
pub fn brain_world_list(state: State<'_, AppState>, project_id: String) -> Result<Vec<WorldEntry>> {
    // One-shot, idempotent migration: earlier builds routed extracted
    // locations/factions/lore into the Idea Park (tagged world/faction/
    // lore). Relocate those AI-suggested ideas into the World Bible the
    // first time this list is requested, so existing projects don't lose
    // their extracted world material when the storage moved.
    migrate_legacy_world_ideas(&state.projects, &project_id)?;
    WorldStore::new(&state.projects).list(&project_id)
}

#[tauri::command]
pub fn brain_world_create(
    state: State<'_, AppState>,
    project_id: String,
    name: String,
    kind: WorldKind,
) -> Result<WorldEntry> {
    WorldStore::new(&state.projects).create(&project_id, &name, kind)
}

#[tauri::command]
pub fn brain_world_update(
    state: State<'_, AppState>,
    project_id: String,
    id: String,
    patch: WorldEntryPatch,
) -> Result<WorldEntry> {
    WorldStore::new(&state.projects).update(&project_id, &id, patch)
}

#[tauri::command]
pub fn brain_world_delete(
    state: State<'_, AppState>,
    project_id: String,
    id: String,
) -> Result<()> {
    WorldStore::new(&state.projects).delete(&project_id, &id)
}

/// Move AI-suggested Idea Park entries tagged `world` / `faction` /
/// `lore` into the World Bible. Idempotent: once moved, the ideas are
/// gone so subsequent calls are no-ops. The user's own (non-AI) ideas
/// are never touched, even if they happen to carry one of those tags.
fn migrate_legacy_world_ideas(
    projects: &crate::services::storage::ProjectStore,
    project_id: &str,
) -> Result<()> {
    let idea_store = IdeaStore::new(projects);
    let ideas = idea_store.list(project_id)?;

    let is_legacy = |i: &Idea| -> Option<WorldKind> {
        if !i.ai_suggested {
            return None;
        }
        // Match the tags the old extraction path wrote.
        for t in &i.tags {
            match t.trim().to_lowercase().as_str() {
                "faction" => return Some(WorldKind::Faction),
                "lore" => return Some(WorldKind::Lore),
                "world" => return Some(WorldKind::Location),
                _ => {}
            }
        }
        None
    };

    if !ideas.iter().any(|i| is_legacy(i).is_some()) {
        return Ok(()); // nothing to migrate — fast path
    }

    let world_store = WorldStore::new(projects);
    let mut world = world_store.list(project_id)?;
    let mut existing_keys: std::collections::HashSet<(WorldKind, String)> = world
        .iter()
        .map(|w| (w.kind, w.name.to_lowercase()))
        .collect();

    let mut keep_ideas: Vec<Idea> = Vec::new();
    for idea in ideas {
        match is_legacy(&idea) {
            Some(kind) => {
                // The old format stored "**Name** — description" in text.
                let (name, description) = split_legacy_idea_text(&idea.text);
                let key = (kind, name.to_lowercase());
                if name.is_empty() || existing_keys.contains(&key) {
                    continue; // drop duplicate / empty
                }
                existing_keys.insert(key);
                let mut w = WorldEntry::fresh(project_id, &name, kind);
                w.description = description;
                w.ai_suggested = true;
                w.source_doc_id = idea.source_doc_id.clone();
                w.created_at = idea.created_at;
                world.push(w);
            }
            None => keep_ideas.push(idea),
        }
    }

    world_store.save(project_id, &world)?;
    idea_store.save(project_id, &keep_ideas)?;
    Ok(())
}

/// Parse the legacy "**Name** — description" idea text back into its
/// (name, description) parts. Falls back to treating the whole string as
/// the name if it doesn't match the expected shape.
fn split_legacy_idea_text(text: &str) -> (String, String) {
    let t = text.trim();
    // Expected: **Name** — description
    if let Some(rest) = t.strip_prefix("**") {
        if let Some(end) = rest.find("**") {
            let name = rest[..end].trim().to_string();
            let after = rest[end + 2..].trim_start();
            let desc = after.trim_start_matches(['—', '-', ':']).trim().to_string();
            return (name, desc);
        }
    }
    (t.to_string(), String::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::brain::IdeaPatch;
    use crate::services::storage::ProjectStore;

    /// Seed an idea the way the pre-WorldStore extraction wrote them.
    fn seed_legacy_idea(
        store: &IdeaStore<'_>,
        pid: &str,
        text: &str,
        tag: &str,
        ai: bool,
    ) -> String {
        let i = store.create(pid, text).unwrap();
        store
            .update(
                pid,
                &i.id,
                IdeaPatch {
                    tags: Some(vec![tag.to_string()]),
                    ..IdeaPatch::default()
                },
            )
            .unwrap();
        if ai {
            // ai_suggested isn't patchable (by design); flip it via save.
            let mut all = store.list(pid).unwrap();
            if let Some(x) = all.iter_mut().find(|x| x.id == i.id) {
                x.ai_suggested = true;
                x.source_doc_id = Some("doc_legacy".into());
            }
            store.save(pid, &all).unwrap();
        }
        i.id
    }

    #[test]
    fn migration_moves_ai_world_ideas_and_keeps_user_ideas() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let p = projects.create("Demo").unwrap();
        let ideas = IdeaStore::new(&projects);

        seed_legacy_idea(
            &ideas,
            &p.id,
            "**Aevis** — A continent of six regions.",
            "world",
            true,
        );
        seed_legacy_idea(
            &ideas,
            &p.id,
            "**Coven of Shadows** — Morn's cult.",
            "faction",
            true,
        );
        seed_legacy_idea(
            &ideas,
            &p.id,
            "**The Seal** — Stone tablet prison.",
            "lore",
            true,
        );
        // User's own note with a colliding tag — must NOT migrate.
        seed_legacy_idea(
            &ideas,
            &p.id,
            "my own worldbuilding thought",
            "world",
            false,
        );

        migrate_legacy_world_ideas(&projects, &p.id).unwrap();

        let world = WorldStore::new(&projects).list(&p.id).unwrap();
        assert_eq!(world.len(), 3);
        let aevis = world.iter().find(|w| w.name == "Aevis").unwrap();
        assert_eq!(aevis.kind, WorldKind::Location);
        assert_eq!(aevis.description, "A continent of six regions.");
        assert!(aevis.ai_suggested);
        assert_eq!(aevis.source_doc_id.as_deref(), Some("doc_legacy"));
        assert!(world.iter().any(|w| w.kind == WorldKind::Faction));
        assert!(world.iter().any(|w| w.kind == WorldKind::Lore));

        let remaining = ideas.list(&p.id).unwrap();
        assert_eq!(remaining.len(), 1, "user's own idea stays in the park");
        assert_eq!(remaining[0].text, "my own worldbuilding thought");

        // Idempotent: second run changes nothing.
        migrate_legacy_world_ideas(&projects, &p.id).unwrap();
        assert_eq!(WorldStore::new(&projects).list(&p.id).unwrap().len(), 3);
        assert_eq!(ideas.list(&p.id).unwrap().len(), 1);
    }

    #[test]
    fn split_legacy_parses_bold_name_em_dash_desc() {
        let (name, desc) =
            split_legacy_idea_text("**Aevis** — The continent containing six regions.");
        assert_eq!(name, "Aevis");
        assert_eq!(desc, "The continent containing six regions.");
    }

    #[test]
    fn split_legacy_handles_plain_hyphen_and_colon() {
        assert_eq!(
            split_legacy_idea_text("**Cinterra** - capital city"),
            ("Cinterra".into(), "capital city".into())
        );
        assert_eq!(
            split_legacy_idea_text("**The Seal**: an artifact"),
            ("The Seal".into(), "an artifact".into())
        );
    }

    #[test]
    fn split_legacy_name_only_when_no_description() {
        assert_eq!(
            split_legacy_idea_text("**Volara**"),
            ("Volara".into(), String::new())
        );
    }

    #[test]
    fn split_legacy_falls_back_to_whole_text_as_name() {
        // No bold markers → treat the entire string as the name.
        assert_eq!(
            split_legacy_idea_text("just a plain note"),
            ("just a plain note".into(), String::new())
        );
    }
}
