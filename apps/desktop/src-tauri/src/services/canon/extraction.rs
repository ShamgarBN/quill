//! Canon entity extraction.
//!
//! Pipeline: take the chunks of one ingested document, send their text
//! to a chat LLM with a structured-output prompt, parse the JSON, and
//! merge the candidate entities into the Character Bible, Idea Park,
//! and Plot Threads — flagging each one `ai_suggested = true` with a
//! back-link to the source doc.
//!
//! Privacy:
//! - Chunks with sensitivity `DoNotSend` are dropped before the prompt
//!   is assembled. They never leave the machine.
//! - The output text is also clamped to a sensible character budget so
//!   we don't blow past the model's context window or the user's
//!   token budget on a giant document.
//!
//! Dedup:
//! - Characters by case-insensitive name match against existing
//!   Character store entries.
//! - Threads by case-insensitive title match.
//! - Locations / factions / lore concepts land in Idea Park; dedup on
//!   exact-text-after-normalization to keep the bar low (the LLM
//!   reliably restates the same name on re-runs).
//!
//! Re-running on the same doc is safe — the dedup step keeps new items
//! distinct from already-merged ones. The on-disk source_doc_id link
//! also lets the UI offer a "remove all from this doc" action later.

use crate::error::{QuillError, Result};
use crate::models::brain::{Character, CharacterRole, Thread, WorldEntry, WorldKind};
use crate::models::canon::{CanonChunk, ChunkSensitivity};
use crate::services::brain::{CharacterStore, ThreadStore, WorldStore};
use crate::services::canon::docs::DocMetaStore;
use crate::services::llm::{ChatMessage, ChatProvider, ChatRequest, ChatResponse, ChatRole};
use crate::services::storage::ProjectStore;
use serde::{Deserialize, Serialize};

/// Cap on how much chunk text we feed the model at once. Most ingested
/// docs sit well under this; longer docs get truncated with a note so
/// the user knows a single pass didn't see the tail. Conservative — a
/// Gemini 2.5 Pro call at this size lands well inside the free-tier
/// quota even for a vault of dozens of files.
const MAX_PROMPT_CHARS: usize = 24_000;

#[derive(Debug, Clone, Default, Serialize)]
pub struct ExtractionReport {
    pub doc_id: String,
    pub characters_added: u32,
    /// Existing characters updated with new facts (aliases merged, empty
    /// fields filled, or untouched same-doc entries refreshed).
    pub characters_enriched: u32,
    /// Places + factions + lore concepts added to the World Bible.
    pub world_added: u32,
    /// Existing world entries updated with new facts.
    pub world_enriched: u32,
    pub threads_added: u32,
    /// True when chunks existed but were all filtered out (do-not-send),
    /// so the LLM was never called. The UI can show "skipped" instead
    /// of "0 found".
    pub skipped_do_not_send: bool,
    /// True when the source text exceeded MAX_PROMPT_CHARS and was
    /// truncated for this run. The user can re-run after splitting.
    pub truncated: bool,
    /// Total chunks the doc had at extraction time. 0 means the
    /// extractor was called against a doc with no chunks (race
    /// condition or bad doc_id).
    pub chunks_total: u32,
    /// Chunks actually sent to the model (after do-not-send filter
    /// and truncation). `chunks_sent < chunks_total` is a hint that
    /// something didn't make it in.
    pub chunks_sent: u32,
    /// Raw count of candidates the model returned, pre-dedup. Gap
    /// between `*_returned` and `*_added` = how many the dedup
    /// dropped because they already existed.
    pub characters_returned: u32,
    pub world_returned: u32,
    pub threads_returned: u32,
}

/// Run the extraction pass for one doc's chunks. Idempotent — re-running
/// won't duplicate entities that already exist (matched case-insensitively
/// by name/title).
pub async fn extract_and_merge(
    project_id: &str,
    doc_id: &str,
    chunks: &[CanonChunk],
    chat: &dyn ChatProvider,
    projects: &ProjectStore,
) -> Result<ExtractionReport> {
    let mut report = ExtractionReport {
        doc_id: doc_id.to_string(),
        chunks_total: chunks.len() as u32,
        ..Default::default()
    };

    // Drop anything we promised never to send.
    let eligible: Vec<&CanonChunk> = chunks
        .iter()
        .filter(|c| c.sensitivity != ChunkSensitivity::DoNotSend)
        .collect();

    if chunks.is_empty() {
        return Ok(report);
    }
    if eligible.is_empty() {
        report.skipped_do_not_send = true;
        // Still stamp so the UI doesn't keep showing "never extracted".
        DocMetaStore::new(projects).mark_extracted(project_id, doc_id)?;
        return Ok(report);
    }

    let (corpus, truncated, chunks_used) = assemble_corpus(&eligible);
    report.truncated = truncated;
    report.chunks_sent = chunks_used as u32;

    // Call the model. Temperature 0 for the most-deterministic output we
    // can get — this is fact extraction, not creative writing.
    //
    // `json_mode` forces well-formed JSON (no prose, no code fences).
    // `disable_thinking` is critical for Gemini 2.5 Flash: its default
    // thinking pass consumes the output-token budget, which silently
    // truncates a long entity list mid-stream. Disabling it sends the
    // whole budget to the answer. max_tokens is set near the model cap
    // so a rich worldbuilding doc fits in one pass.
    let req = ChatRequest {
        messages: vec![
            ChatMessage {
                role: ChatRole::System,
                content: SYSTEM_PROMPT.to_string(),
            },
            ChatMessage {
                role: ChatRole::User,
                content: build_user_prompt(&corpus),
            },
        ],
        temperature: 0.0,
        max_tokens: 8_192,
        stop: Vec::new(),
        json_mode: true,
        disable_thinking: true,
    };
    let resp = chat_with_retry(chat, &req).await?;
    tracing::info!(
        chunks_sent = report.chunks_sent,
        response_chars = resp.content.len(),
        "canon extraction: LLM round-trip complete"
    );
    let payload = parse_json_payload(&resp.content)?;
    report.characters_returned = payload.characters.len() as u32;
    report.world_returned = (payload.locations_factions.len() + payload.lore_concepts.len()) as u32;
    report.threads_returned = payload.threads.len() as u32;
    tracing::info!(
        characters = report.characters_returned,
        world = report.world_returned,
        threads = report.threads_returned,
        "canon extraction: parsed candidates"
    );

    merge_into_stores(project_id, doc_id, &payload, projects, &mut report)?;
    DocMetaStore::new(projects).mark_extracted(project_id, doc_id)?;
    Ok(report)
}

/// Backoff schedule (ms) for transient LLM failures. Gemini Flash returns
/// HTTP 503 ("high demand") and 429 (per-minute rate limit) under load;
/// both are worth waiting out. Total worst-case added latency ≈ 23s, which
/// is fine for a background task that already shows a spinner.
const RETRY_BACKOFFS_MS: [u64; 3] = [2_000, 6_000, 15_000];

/// Call the chat provider, retrying transient (overload / rate-limit)
/// errors with exponential backoff. Non-transient errors (bad request,
/// missing model, auth) fail fast — retrying them is pointless.
async fn chat_with_retry(chat: &dyn ChatProvider, req: &ChatRequest) -> Result<ChatResponse> {
    let mut attempt = 0usize;
    loop {
        match chat.chat(req).await {
            Ok(r) => return Ok(r),
            Err(e) if is_transient(&e) && attempt < RETRY_BACKOFFS_MS.len() => {
                let wait = RETRY_BACKOFFS_MS[attempt];
                tracing::warn!(
                    attempt = attempt + 1,
                    wait_ms = wait,
                    error = %e,
                    "canon extraction: transient LLM error, backing off and retrying"
                );
                tokio::time::sleep(std::time::Duration::from_millis(wait)).await;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

/// Heuristic: is this error worth retrying? Matches the provider-agnostic
/// markers our error strings carry for server overload and rate limiting.
fn is_transient(e: &QuillError) -> bool {
    let s = e.to_string();
    s.contains("503")
        || s.contains("UNAVAILABLE")
        || s.contains("high demand")
        || s.contains("overloaded")
        || s.contains("500")
        || s.contains("INTERNAL")
        || s.contains("429")
        || s.contains("RESOURCE_EXHAUSTED")
        || s.contains("Too Many Requests")
}

/// Concatenate eligible chunk texts into a single corpus string,
/// labeled with their heading trail so the model has structural cues.
/// Returns (corpus, was_truncated, chunks_consumed).
fn assemble_corpus(chunks: &[&CanonChunk]) -> (String, bool, usize) {
    let mut out = String::new();
    let mut truncated = false;
    let mut used = 0usize;
    for c in chunks {
        let header = if c.headings.is_empty() {
            String::new()
        } else {
            format!("\n## {}\n", c.headings.join(" › "))
        };
        let next_len = out.len() + header.len() + c.text.len() + 2;
        if next_len > MAX_PROMPT_CHARS {
            truncated = true;
            break;
        }
        out.push_str(&header);
        out.push_str(&c.text);
        out.push_str("\n\n");
        used += 1;
    }
    (out, truncated, used)
}

const SYSTEM_PROMPT: &str = r#"You are an extraction assistant for a YA fantasy writer's worldbuilding notes. Read the provided text and identify named entities the writer will need to track while drafting.

Output STRICT JSON ONLY — no markdown, no commentary, no code fences. The JSON must match this schema:

{
  "characters": [
    {
      "name": "Primary name as it appears in the text",
      "aliases": ["Other names, nicknames, titles"],
      "role_guess": "protagonist | antagonist | mentor | ally | love-interest | family | foil | supporting | minor",
      "motivation": "One-sentence motivation if stated or strongly implied",
      "voice_notes": "Short notes on how they speak / act, if stated",
      "arc_one_liner": "One-sentence arc if the text suggests one"
    }
  ],
  "locations_factions": [
    {
      "name": "Place name or organization name",
      "kind": "location | faction",
      "description": "1-3 sentence summary grounded in the text"
    }
  ],
  "lore_concepts": [
    {
      "name": "Magic system, artifact, prophecy, rule of the world, etc.",
      "description": "1-3 sentence summary grounded in the text"
    }
  ],
  "threads": [
    {
      "title": "An unresolved promise, debt, mystery, or setup the text raises",
      "description": "1-2 sentences: what was set up, what closure would look like"
    }
  ]
}

Rules:
- Extract ONLY what the text states or strongly implies. Do not invent.
- A "thread" is a setup that demands payoff — a debt, vow, mystery, prophecy, foreshadowing, an open question. NOT every plot point.
- If a section has nothing for a category, return an empty array.
- Be conservative. Quality over quantity. One well-grounded entry beats five guesses.
- Use the names as they appear in the source — don't translate or reformat them.
"#;

fn build_user_prompt(corpus: &str) -> String {
    format!(
        "Extract entities from the following worldbuilding notes. Return JSON only.\n\n---\n{corpus}\n---\n"
    )
}

/// Deserialize a field that may be absent, present, OR explicitly `null`.
///
/// LLMs routinely emit `"voice_notes": null` for fields they have no
/// value for, rather than omitting the key or sending `""`. Plain
/// `#[serde(default)]` only covers the *absent* case — deserializing a
/// JSON `null` into `String`/`Vec` is a hard type error that aborts the
/// entire parse and discards every entity in the response. Routing every
/// field through this helper (paired with `#[serde(default)]` for the
/// truly-absent case) makes a single stray `null` harmless.
fn null_default<'de, D, T>(de: D) -> std::result::Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(de)?.unwrap_or_default())
}

/// The full shape we expect back from the model. Every field tolerates
/// missing keys AND explicit nulls (see `null_default`) because model
/// JSON is only loosely contract-bound.
#[derive(Debug, Default, Deserialize)]
struct Payload {
    #[serde(default, deserialize_with = "null_default")]
    characters: Vec<CandidateCharacter>,
    #[serde(default, deserialize_with = "null_default")]
    locations_factions: Vec<CandidateLocOrFaction>,
    #[serde(default, deserialize_with = "null_default")]
    lore_concepts: Vec<CandidateLore>,
    #[serde(default, deserialize_with = "null_default")]
    threads: Vec<CandidateThread>,
}

#[derive(Debug, Default, Deserialize)]
struct CandidateCharacter {
    #[serde(default, deserialize_with = "null_default")]
    name: String,
    #[serde(default, deserialize_with = "null_default")]
    aliases: Vec<String>,
    #[serde(default, deserialize_with = "null_default")]
    role_guess: String,
    #[serde(default, deserialize_with = "null_default")]
    motivation: String,
    #[serde(default, deserialize_with = "null_default")]
    voice_notes: String,
    #[serde(default, deserialize_with = "null_default")]
    arc_one_liner: String,
}

#[derive(Debug, Default, Deserialize)]
struct CandidateLocOrFaction {
    #[serde(default, deserialize_with = "null_default")]
    name: String,
    #[serde(default, deserialize_with = "null_default")]
    kind: String,
    #[serde(default, deserialize_with = "null_default")]
    description: String,
}

#[derive(Debug, Default, Deserialize)]
struct CandidateLore {
    #[serde(default, deserialize_with = "null_default")]
    name: String,
    #[serde(default, deserialize_with = "null_default")]
    description: String,
}

#[derive(Debug, Default, Deserialize)]
struct CandidateThread {
    #[serde(default, deserialize_with = "null_default")]
    title: String,
    #[serde(default, deserialize_with = "null_default")]
    description: String,
}

/// Models sometimes wrap JSON in ```json ... ``` fences despite the
/// system prompt forbidding it. Strip leading/trailing junk and parse
/// what looks like a JSON object. If the JSON is truncated (e.g. the
/// model hit the token cap mid-array), fall back to salvaging the
/// complete leading objects of each array rather than discarding the
/// whole response.
fn parse_json_payload(raw: &str) -> Result<Payload> {
    let trimmed = raw.trim();
    // Strip code fences if present.
    let cleaned = if let Some(rest) = trimmed.strip_prefix("```") {
        let rest = rest.trim_start_matches("json").trim_start_matches('\n');
        rest.trim_end_matches("```").trim()
    } else {
        trimmed
    };
    // Find the outermost {...} just in case the model added prose around it.
    let start = cleaned.find('{');
    let end = cleaned.rfind('}');
    let candidate = match (start, end) {
        (Some(s), Some(e)) if e > s => &cleaned[s..=e],
        _ => cleaned,
    };
    match serde_json::from_str::<Payload>(candidate) {
        Ok(p) => Ok(p),
        Err(strict_err) => {
            // Salvage path: repair a truncated object by closing dangling
            // arrays/strings, then re-parse. Recovers the complete entities
            // that landed before the cutoff.
            if let Some(repaired) = repair_truncated_json(candidate) {
                if let Ok(p) = serde_json::from_str::<Payload>(&repaired) {
                    tracing::warn!("canon extraction: recovered from truncated JSON via salvage");
                    return Ok(p);
                }
            }
            Err(QuillError::Internal(format!(
                "extraction: model returned unparseable JSON ({strict_err}); first 200 chars: {}",
                cleaned.chars().take(200).collect::<String>()
            )))
        }
    }
}

/// Best-effort repair of a truncated JSON object. Walks the string
/// tracking string/escape state and bracket depth, drops any trailing
/// partial token, then appends the closing brackets needed to balance.
/// Returns None if the input doesn't even start with `{`.
fn repair_truncated_json(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'{') {
        return None;
    }
    let mut stack: Vec<u8> = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    // Index just past the last byte that sits at a "safe" boundary — i.e.
    // the end of a fully-closed value (`}`, `]`, `"`, digit) outside a string.
    let mut last_safe = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
                last_safe = i + 1;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => stack.push(b'}'),
            b'[' => stack.push(b']'),
            b'}' | b']' => {
                stack.pop();
                last_safe = i + 1;
            }
            b'0'..=b'9' | b'e' | b'E' | b'.' | b'-' | b'+' => last_safe = i + 1,
            b',' => last_safe = i, // keep up to but not including a dangling comma
            _ => {}
        }
    }
    // Recompute the bracket stack for the truncated-at-last_safe prefix so
    // the closers we append actually match.
    let prefix = &s[..last_safe];
    let mut closers: Vec<u8> = Vec::new();
    let mut in_str = false;
    let mut esc = false;
    for &b in prefix.as_bytes() {
        if in_str {
            if esc {
                esc = false;
            } else if b == b'\\' {
                esc = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' => closers.push(b'}'),
            b'[' => closers.push(b']'),
            b'}' | b']' => {
                closers.pop();
            }
            _ => {}
        }
    }
    let mut out = String::with_capacity(prefix.len() + closers.len());
    out.push_str(prefix.trim_end_matches(|c: char| c == ',' || c.is_whitespace()));
    while let Some(c) = closers.pop() {
        out.push(c as char);
    }
    Some(out)
}

fn merge_into_stores(
    project_id: &str,
    doc_id: &str,
    payload: &Payload,
    projects: &ProjectStore,
    report: &mut ExtractionReport,
) -> Result<()> {
    // ---- Characters ----
    if !payload.characters.is_empty() {
        let store = CharacterStore::new(projects);
        let mut all = store.list(project_id)?;
        for cand in &payload.characters {
            let name = cand.name.trim();
            if name.is_empty() {
                continue;
            }
            let key = name.to_lowercase();
            // Name (or alias) already known → enrich instead of skip.
            // A campaign evolves: new session notes carry new facts about
            // existing people. See `enrich_character` for the rules.
            if let Some(existing) = all
                .iter_mut()
                .find(|c| c.match_terms().any(|t| t.to_lowercase() == key))
            {
                if enrich_character(existing, cand, doc_id) {
                    report.characters_enriched += 1;
                }
                continue;
            }
            let mut c = Character::fresh(project_id, name);
            c.aliases = cand
                .aliases
                .iter()
                .map(|a| a.trim().to_string())
                .filter(|a| !a.is_empty())
                .collect();
            c.role = parse_role(&cand.role_guess);
            c.motivation = cand.motivation.trim().to_string();
            c.voice_notes = cand.voice_notes.trim().to_string();
            c.arc_one_liner = cand.arc_one_liner.trim().to_string();
            c.ai_suggested = true;
            c.source_doc_id = Some(doc_id.to_string());
            all.push(c);
            report.characters_added += 1;
        }
        if report.characters_added > 0 || report.characters_enriched > 0 {
            store.save(project_id, &all)?;
        }
    }

    // ---- Locations / factions + lore concepts → World Bible ----
    let total_world_to_consider = payload.locations_factions.len() + payload.lore_concepts.len();
    if total_world_to_consider > 0 {
        let store = WorldStore::new(projects);
        let mut all = store.list(project_id)?;

        // Collect (name, description, kind) candidates from both arrays.
        let mut candidates: Vec<(&str, &str, WorldKind)> = Vec::new();
        for cand in &payload.locations_factions {
            let kind = if cand.kind.trim().eq_ignore_ascii_case("faction") {
                WorldKind::Faction
            } else {
                WorldKind::Location
            };
            candidates.push((cand.name.trim(), cand.description.trim(), kind));
        }
        for cand in &payload.lore_concepts {
            candidates.push((cand.name.trim(), cand.description.trim(), WorldKind::Lore));
        }

        for (name, descr, kind) in candidates {
            if name.is_empty() {
                continue;
            }
            let key = name.to_lowercase();
            // Match by name/alias across ALL kinds — if the user
            // recategorized "Circle of Dawn" from faction to lore, a
            // re-extraction must enrich that entry, not duplicate it.
            if let Some(existing) = all.iter_mut().find(|w| {
                w.name.trim().to_lowercase() == key
                    || w.aliases.iter().any(|a| a.trim().to_lowercase() == key)
            }) {
                if enrich_world_entry(existing, descr, kind, doc_id) {
                    report.world_enriched += 1;
                }
                continue;
            }
            let mut w = WorldEntry::fresh(project_id, name, kind);
            w.description = descr.to_string();
            w.ai_suggested = true;
            w.source_doc_id = Some(doc_id.to_string());
            all.push(w);
            report.world_added += 1;
        }

        if report.world_added > 0 || report.world_enriched > 0 {
            store.save(project_id, &all)?;
        }
    }

    // ---- Threads ----
    if !payload.threads.is_empty() {
        let store = ThreadStore::new(projects);
        let existing = store.list(project_id)?;
        let mut existing_titles: std::collections::HashSet<String> =
            existing.iter().map(|t| t.title.to_lowercase()).collect();
        let mut all = existing;
        for cand in &payload.threads {
            let title = cand.title.trim();
            if title.is_empty() {
                continue;
            }
            let key = title.to_lowercase();
            if existing_titles.contains(&key) {
                continue;
            }
            existing_titles.insert(key);
            let mut t = Thread::fresh(project_id, title);
            t.description = cand.description.trim().to_string();
            t.ai_suggested = true;
            t.source_doc_id = Some(doc_id.to_string());
            all.push(t);
            report.threads_added += 1;
        }
        if report.threads_added > 0 {
            store.save(project_id, &all)?;
        }
    }

    Ok(())
}

/// Enrich an existing character with newly-extracted facts. Returns true
/// if anything changed.
///
/// Rules (user edits are sacred):
/// - New aliases are always merged (case-insensitive dedup).
/// - Empty fields are filled from the candidate.
/// - If the entry is AI-suggested, has never been hand-edited
///   (`updated_at == created_at` — user edits go through `update()` which
///   bumps the timestamp; extraction writes never do), and came from this
///   same doc, the doc is the entry's source of truth: refresh fields
///   wholesale so an updated note keeps its entry current.
fn enrich_character(existing: &mut Character, cand: &CandidateCharacter, doc_id: &str) -> bool {
    let mut changed = merge_aliases(&mut existing.aliases, &existing.name, &cand.aliases);

    let refresh = existing.ai_suggested
        && existing.updated_at == existing.created_at
        && existing.source_doc_id.as_deref() == Some(doc_id);

    let mut apply = |field: &mut String, new_val: &str| {
        let new_val = new_val.trim();
        if new_val.is_empty() {
            return;
        }
        if (refresh && *field != new_val) || field.is_empty() {
            *field = new_val.to_string();
            changed = true;
        }
    };
    apply(&mut existing.motivation, &cand.motivation);
    apply(&mut existing.voice_notes, &cand.voice_notes);
    apply(&mut existing.arc_one_liner, &cand.arc_one_liner);

    if refresh && !cand.role_guess.trim().is_empty() {
        let role = parse_role(&cand.role_guess);
        if existing.role != role {
            existing.role = role;
            changed = true;
        }
    }
    changed
}

/// World-entry counterpart of `enrich_character`. Same refresh/fill rules;
/// `kind` is only refreshed for untouched same-doc entries (a user's
/// recategorization is deliberate and permanent).
fn enrich_world_entry(
    existing: &mut WorldEntry,
    descr: &str,
    kind: WorldKind,
    doc_id: &str,
) -> bool {
    let mut changed = false;
    let refresh = existing.ai_suggested
        && existing.updated_at == existing.created_at
        && existing.source_doc_id.as_deref() == Some(doc_id);

    let descr = descr.trim();
    if !descr.is_empty()
        && ((refresh && existing.description != descr) || existing.description.is_empty())
    {
        existing.description = descr.to_string();
        changed = true;
    }
    if refresh && existing.kind != kind {
        existing.kind = kind;
        changed = true;
    }
    changed
}

/// Merge `new` aliases into `existing`, skipping empties, duplicates
/// (case-insensitive), and the primary name itself. Returns true if any
/// alias was added.
fn merge_aliases(existing: &mut Vec<String>, primary: &str, new: &[String]) -> bool {
    let primary_l = primary.trim().to_lowercase();
    let mut known: std::collections::HashSet<String> =
        existing.iter().map(|a| a.trim().to_lowercase()).collect();
    known.insert(primary_l);
    let mut added = false;
    for a in new {
        let a = a.trim();
        if a.is_empty() {
            continue;
        }
        let key = a.to_lowercase();
        if known.insert(key) {
            existing.push(a.to_string());
            added = true;
        }
    }
    added
}

fn parse_role(s: &str) -> CharacterRole {
    match s.trim().to_lowercase().as_str() {
        "protagonist" => CharacterRole::Protagonist,
        "antagonist" => CharacterRole::Antagonist,
        "mentor" => CharacterRole::Mentor,
        "ally" => CharacterRole::Ally,
        "love-interest" | "love_interest" | "loveinterest" => CharacterRole::LoveInterest,
        "family" => CharacterRole::Family,
        "foil" => CharacterRole::Foil,
        "minor" => CharacterRole::Minor,
        _ => CharacterRole::Supporting,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::llm::{ChatProvider, ChatResponse};
    use crate::services::storage::ProjectStore;
    use async_trait::async_trait;
    use std::sync::Mutex;

    /// A canned-response chat provider; lets tests assert end-to-end
    /// merge behavior without a network call.
    struct CannedChat {
        reply: String,
        called: Mutex<u32>,
    }

    impl CannedChat {
        fn new(reply: impl Into<String>) -> Self {
            Self {
                reply: reply.into(),
                called: Mutex::new(0),
            }
        }
    }

    #[async_trait]
    impl ChatProvider for CannedChat {
        fn provider_id(&self) -> &str {
            "canned"
        }
        fn model_id(&self) -> &str {
            "canned-v0"
        }
        async fn chat(&self, _req: &ChatRequest) -> Result<ChatResponse> {
            *self.called.lock().unwrap() += 1;
            Ok(ChatResponse {
                content: self.reply.clone(),
                tokens_in: 0,
                tokens_out: 0,
                model: "canned-v0".to_string(),
            })
        }
    }

    fn make_chunk(doc_id: &str, idx: u32, text: &str, sensitivity: ChunkSensitivity) -> CanonChunk {
        CanonChunk {
            id: format!("{doc_id}:{idx}"),
            doc_id: doc_id.to_string(),
            project_id: "p1".to_string(),
            index: idx,
            offset: 0,
            text: text.to_string(),
            headings: vec![],
            word_count: text.split_whitespace().count() as u32,
            sensitivity,
            source_path: "/tmp/x.md".to_string(),
            kind: crate::models::CanonKind::Lore,
            embedding_model: String::new(),
        }
    }

    #[tokio::test]
    async fn merges_new_entities_with_ai_flag() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let p = projects.create("Demo").unwrap();

        let chat = CannedChat::new(
            r#"{
  "characters": [
    {"name": "Kaelan", "aliases": ["Kael"], "role_guess": "protagonist", "motivation": "find his brother", "voice_notes": "clipped, sardonic"}
  ],
  "locations_factions": [
    {"name": "The Hollow Wastes", "kind": "location", "description": "Scorched expanse east of Tarn."}
  ],
  "lore_concepts": [
    {"name": "The Tarn Pact", "description": "Old agreement between dragons and humans."}
  ],
  "threads": [
    {"title": "Kaelan's blood debt", "description": "Owes life to the dragon who saved him."}
  ]
}"#,
        );

        let chunks = vec![make_chunk(
            "doc_a",
            0,
            "irrelevant",
            ChunkSensitivity::Public,
        )];
        let report = extract_and_merge(&p.id, "doc_a", &chunks, &chat, &projects)
            .await
            .unwrap();

        assert_eq!(report.characters_added, 1);
        assert_eq!(report.world_added, 2);
        assert_eq!(report.threads_added, 1);

        let chars = CharacterStore::new(&projects).list(&p.id).unwrap();
        assert_eq!(chars.len(), 1);
        assert_eq!(chars[0].name, "Kaelan");
        assert!(chars[0].ai_suggested);
        assert_eq!(chars[0].source_doc_id.as_deref(), Some("doc_a"));
        assert_eq!(chars[0].aliases, vec!["Kael".to_string()]);
        assert_eq!(chars[0].role, CharacterRole::Protagonist);

        let world = WorldStore::new(&projects).list(&p.id).unwrap();
        assert_eq!(world.len(), 2);
        assert!(world.iter().all(|w| w.ai_suggested));
        assert!(world
            .iter()
            .any(|w| w.kind == WorldKind::Location && w.name.contains("Hollow Wastes")));
        assert!(world
            .iter()
            .any(|w| w.kind == WorldKind::Lore && w.name.contains("Tarn Pact")));

        let threads = ThreadStore::new(&projects).list(&p.id).unwrap();
        assert_eq!(threads.len(), 1);
        assert!(threads[0].ai_suggested);
        assert_eq!(threads[0].source_doc_id.as_deref(), Some("doc_a"));

        // last_extracted_at got stamped
        let meta = DocMetaStore::new(&projects).get(&p.id, "doc_a").unwrap();
        assert!(meta.last_extracted_at.is_some());
    }

    #[tokio::test]
    async fn dedupes_against_existing_entries() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let p = projects.create("Demo").unwrap();

        // Pre-seed: user already added "Kaelan" by hand.
        let cs = CharacterStore::new(&projects);
        cs.create(&p.id, "Kaelan").unwrap();

        let chat = CannedChat::new(
            r#"{
  "characters": [
    {"name": "kaelan", "aliases": [], "role_guess": "protagonist"}
  ],
  "locations_factions": [],
  "lore_concepts": [],
  "threads": []
}"#,
        );
        let chunks = vec![make_chunk(
            "doc_a",
            0,
            "irrelevant",
            ChunkSensitivity::Public,
        )];
        let report = extract_and_merge(&p.id, "doc_a", &chunks, &chat, &projects)
            .await
            .unwrap();

        assert_eq!(
            report.characters_added, 0,
            "case-insensitive match should dedupe"
        );
        let chars = cs.list(&p.id).unwrap();
        assert_eq!(chars.len(), 1);
        // The user's hand-created entry must NOT have been flipped to ai_suggested.
        assert!(!chars[0].ai_suggested);
    }

    #[tokio::test]
    async fn enriches_existing_character_fill_and_aliases() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let p = projects.create("Demo").unwrap();

        // User created Kaelan by hand — minimal entry, no motivation.
        let store = CharacterStore::new(&projects);
        store.create(&p.id, "Kaelan").unwrap();

        let chat = CannedChat::new(
            r#"{
  "characters": [
    {"name": "Kaelan", "aliases": ["Kael", "kaelan"], "role_guess": "protagonist", "motivation": "Find his brother"}
  ],
  "locations_factions": [], "lore_concepts": [], "threads": []
}"#,
        );
        let chunks = vec![make_chunk("doc_a", 0, "x", ChunkSensitivity::Public)];
        let report = extract_and_merge(&p.id, "doc_a", &chunks, &chat, &projects)
            .await
            .unwrap();

        assert_eq!(report.characters_added, 0);
        assert_eq!(report.characters_enriched, 1);
        let chars = store.list(&p.id).unwrap();
        assert_eq!(chars.len(), 1);
        assert_eq!(
            chars[0].motivation, "Find his brother",
            "empty field filled"
        );
        assert_eq!(
            chars[0].aliases,
            vec!["Kael".to_string()],
            "alias merged, dup skipped"
        );
        assert!(
            !chars[0].ai_suggested,
            "hand-created entry keeps its provenance"
        );
        assert_eq!(
            chars[0].role,
            CharacterRole::Supporting,
            "role untouched outside refresh mode"
        );
    }

    #[tokio::test]
    async fn refreshes_untouched_ai_entry_but_respects_user_edits() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let p = projects.create("Demo").unwrap();
        let chunks = vec![make_chunk("doc_a", 0, "x", ChunkSensitivity::Public)];

        // Pass 1: extraction creates Morn + Cinterra.
        let chat1 = CannedChat::new(
            r#"{
  "characters": [{"name": "Morn", "motivation": "Old motivation"}],
  "locations_factions": [{"name": "Cinterra", "kind": "location", "description": "Old description"}],
  "lore_concepts": [], "threads": []
}"#,
        );
        extract_and_merge(&p.id, "doc_a", &chunks, &chat1, &projects)
            .await
            .unwrap();

        // User hand-edits Morn's motivation (bumps updated_at). Cinterra untouched.
        let cstore = CharacterStore::new(&projects);
        let morn_id = cstore.list(&p.id).unwrap()[0].id.clone();
        cstore
            .update(
                &p.id,
                &morn_id,
                crate::models::brain::CharacterPatch {
                    motivation: Some("My hand-written motivation".into()),
                    ..Default::default()
                },
            )
            .unwrap();

        // Pass 2: same doc, updated facts.
        let chat2 = CannedChat::new(
            r#"{
  "characters": [{"name": "Morn", "motivation": "New extracted motivation"}],
  "locations_factions": [{"name": "Cinterra", "kind": "location", "description": "New description"}],
  "lore_concepts": [], "threads": []
}"#,
        );
        let report = extract_and_merge(&p.id, "doc_a", &chunks, &chat2, &projects)
            .await
            .unwrap();

        let morn = &cstore.list(&p.id).unwrap()[0];
        assert_eq!(
            morn.motivation, "My hand-written motivation",
            "user-edited entry must never be overwritten"
        );
        let world = WorldStore::new(&projects).list(&p.id).unwrap();
        assert_eq!(
            world[0].description, "New description",
            "untouched same-doc AI entry refreshes with the doc"
        );
        assert_eq!(report.world_enriched, 1);
        assert_eq!(report.world_added, 0);
    }

    #[tokio::test]
    async fn skips_when_every_chunk_is_do_not_send() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let p = projects.create("Demo").unwrap();

        let chat = CannedChat::new("(should never be called)");
        let chunks = vec![
            make_chunk("doc_a", 0, "secret", ChunkSensitivity::DoNotSend),
            make_chunk("doc_a", 1, "also secret", ChunkSensitivity::DoNotSend),
        ];
        let report = extract_and_merge(&p.id, "doc_a", &chunks, &chat, &projects)
            .await
            .unwrap();

        assert!(report.skipped_do_not_send);
        assert_eq!(report.characters_added, 0);
        assert_eq!(*chat.called.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn tolerates_null_string_fields() {
        // Regression: Gemini Flash emits `null` for optional string fields
        // it has no value for. A single null must not discard the batch.
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let p = projects.create("Demo").unwrap();

        let chat = CannedChat::new(
            r#"{
  "characters": [
    {
      "name": "Morn the Darkness",
      "aliases": ["Morn"],
      "role_guess": "antagonist",
      "motivation": "Spread death and rebirth",
      "voice_notes": null,
      "arc_one_liner": null
    }
  ],
  "locations_factions": [
    {"name": "Cinterra", "kind": "location", "description": null}
  ],
  "lore_concepts": [],
  "threads": [
    {"title": "Will Morn's seal hold?", "description": null}
  ]
}"#,
        );
        let chunks = vec![make_chunk("doc_a", 0, "x", ChunkSensitivity::Public)];
        let report = extract_and_merge(&p.id, "doc_a", &chunks, &chat, &projects)
            .await
            .unwrap();
        assert_eq!(
            report.characters_added, 1,
            "null fields must not abort parse"
        );
        assert_eq!(report.world_added, 1);
        assert_eq!(report.threads_added, 1);

        let chars = CharacterStore::new(&projects).list(&p.id).unwrap();
        assert_eq!(chars[0].name, "Morn the Darkness");
        assert_eq!(chars[0].voice_notes, "", "null → empty string");
    }

    #[tokio::test]
    async fn skips_entity_with_null_name_keeps_rest() {
        // A null/empty name on one candidate shouldn't nuke the others.
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let p = projects.create("Demo").unwrap();

        let chat = CannedChat::new(
            r#"{
  "characters": [
    {"name": null, "role_guess": "minor"},
    {"name": "Luther Kaine", "role_guess": "antagonist"}
  ],
  "locations_factions": [],
  "lore_concepts": [],
  "threads": []
}"#,
        );
        let chunks = vec![make_chunk("doc_a", 0, "x", ChunkSensitivity::Public)];
        let report = extract_and_merge(&p.id, "doc_a", &chunks, &chat, &projects)
            .await
            .unwrap();
        assert_eq!(
            report.characters_added, 1,
            "null-name entry dropped, valid one kept"
        );
        let chars = CharacterStore::new(&projects).list(&p.id).unwrap();
        assert_eq!(chars[0].name, "Luther Kaine");
    }

    #[test]
    fn classifies_transient_errors() {
        assert!(is_transient(&QuillError::Storage(
            "gemini chat HTTP 503 Service Unavailable: high demand".into()
        )));
        assert!(is_transient(&QuillError::Storage(
            "gemini chat HTTP 429 Too Many Requests".into()
        )));
        // A genuine bad-request / not-found should NOT be retried.
        assert!(!is_transient(&QuillError::Storage(
            "gemini chat HTTP 404 Not Found: model missing".into()
        )));
        assert!(!is_transient(&QuillError::Internal(
            "extraction: unparseable JSON".into()
        )));
    }

    /// Chat provider that fails with a transient error N times, then
    /// succeeds — verifies the retry loop recovers.
    struct FlakyChat {
        fail_times: Mutex<u32>,
        reply: String,
    }

    #[async_trait]
    impl ChatProvider for FlakyChat {
        fn provider_id(&self) -> &str {
            "flaky"
        }
        fn model_id(&self) -> &str {
            "flaky-v0"
        }
        async fn chat(&self, _req: &ChatRequest) -> Result<ChatResponse> {
            let mut n = self.fail_times.lock().unwrap();
            if *n > 0 {
                *n -= 1;
                return Err(QuillError::Storage(
                    "gemini chat HTTP 503 Service Unavailable: high demand".into(),
                ));
            }
            Ok(ChatResponse {
                content: self.reply.clone(),
                tokens_in: 0,
                tokens_out: 0,
                model: "flaky-v0".into(),
            })
        }
    }

    #[tokio::test(start_paused = true)]
    async fn retries_transient_then_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let p = projects.create("Demo").unwrap();
        let chat = FlakyChat {
            fail_times: Mutex::new(2), // fail twice, succeed on the third call
            reply: r#"{"characters":[{"name":"Juno Verne"}],"locations_factions":[],"lore_concepts":[],"threads":[]}"#.into(),
        };
        let chunks = vec![make_chunk("doc_a", 0, "x", ChunkSensitivity::Public)];
        // start_paused auto-advances the tokio clock past the backoff sleeps.
        let report = extract_and_merge(&p.id, "doc_a", &chunks, &chat, &projects)
            .await
            .unwrap();
        assert_eq!(
            report.characters_added, 1,
            "retry should eventually succeed"
        );
    }

    #[test]
    fn salvages_truncated_json() {
        // Simulates a response cut off mid-array (token cap). The first
        // complete character object must survive.
        let truncated = r#"{
  "characters": [
    {"name": "Morn the Darkness", "role_guess": "antagonist", "motivation": "death and rebirth"},
    {"name": "Kaelar Vostik", "role_guess": "supp"#;
        let payload = parse_json_payload(truncated).expect("salvage should parse");
        assert!(
            !payload.characters.is_empty(),
            "at least the first complete object survives"
        );
        assert_eq!(payload.characters[0].name, "Morn the Darkness");
    }

    #[tokio::test]
    async fn tolerates_code_fenced_json() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let p = projects.create("Demo").unwrap();

        let chat = CannedChat::new(
            "```json\n{ \"characters\": [{\"name\": \"Lyra\"}], \"locations_factions\": [], \"lore_concepts\": [], \"threads\": [] }\n```",
        );
        let chunks = vec![make_chunk("doc_a", 0, "x", ChunkSensitivity::Public)];
        let report = extract_and_merge(&p.id, "doc_a", &chunks, &chat, &projects)
            .await
            .unwrap();
        assert_eq!(report.characters_added, 1);
    }
}
