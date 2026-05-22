//! Voice fingerprint commands.

use crate::error::Result;
use crate::services::voice::{
    compute_drift, extract_features, DriftReport, ReferencePin, ReferencePinStore, VoiceFeatures,
    VoiceFingerprint,
};
use crate::state::AppState;
use serde::Deserialize;
use tauri::State;

#[tauri::command]
pub fn voice_pins_list(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<ReferencePin>> {
    ReferencePinStore::new(&state.projects).list(&project_id)
}

#[tauri::command]
pub fn voice_pins_create(
    state: State<'_, AppState>,
    project_id: String,
    label: String,
    passage: String,
) -> Result<ReferencePin> {
    ReferencePinStore::new(&state.projects).create(&project_id, &label, &passage)
}

#[tauri::command]
pub fn voice_pins_delete(state: State<'_, AppState>, project_id: String, id: String) -> Result<()> {
    ReferencePinStore::new(&state.projects).delete(&project_id, &id)
}

#[derive(Deserialize)]
pub struct PinPatch {
    pub label: Option<String>,
    pub author: Option<Option<String>>,
    pub source: Option<Option<String>>,
    pub passage: Option<String>,
    pub weight: Option<f32>,
    pub enabled: Option<bool>,
}

#[tauri::command]
pub fn voice_pins_update(
    state: State<'_, AppState>,
    project_id: String,
    id: String,
    patch: PinPatch,
) -> Result<ReferencePin> {
    ReferencePinStore::new(&state.projects).update(&project_id, &id, |p| {
        if let Some(v) = patch.label {
            p.label = v;
        }
        if let Some(v) = patch.author {
            p.author = v;
        }
        if let Some(v) = patch.source {
            p.source = v;
        }
        if let Some(v) = patch.passage {
            p.passage = v;
        }
        if let Some(v) = patch.weight {
            p.weight = v.clamp(0.0, 10.0);
        }
        if let Some(v) = patch.enabled {
            p.enabled = v;
        }
    })
}

#[tauri::command]
pub fn voice_fingerprint(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<VoiceFingerprint> {
    ReferencePinStore::new(&state.projects).fingerprint(&project_id)
}

#[tauri::command]
pub fn voice_extract(text: String) -> Result<VoiceFeatures> {
    Ok(extract_features(&text))
}

#[tauri::command]
pub fn voice_drift(
    state: State<'_, AppState>,
    project_id: String,
    candidate: String,
    top_n: Option<usize>,
) -> Result<DriftReport> {
    let fp = ReferencePinStore::new(&state.projects).fingerprint(&project_id)?;
    Ok(compute_drift(&fp, &candidate, top_n.unwrap_or(8)))
}
