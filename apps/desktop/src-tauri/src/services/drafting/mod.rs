//! Drafting service — Phase 6.
//!
//! Composes the LLM call that powers the manuscript editor's drafting panel.
//! This is the *only* place in the codebase that combines:
//!
//!   - the active beat (macro structure)
//!   - the active scene (micro structure + prior prose)
//!   - the top-K canon excerpts (filtered for `do_not_send`)
//!   - the user's pinned reference style passages (voice anchors)
//!   - the user's natural-language instruction
//!
//! into a single prompt and dispatches it to the configured chat provider.
//!
//! Discipline:
//! - Pure prompt assembly lives in `prompt.rs` and has no IO. It can be
//!   unit-tested with synthetic inputs.
//! - The orchestrator in `orchestrator.rs` does the side-effectful work:
//!   loads scene + beat + canon hits, optionally enforces the drift gate,
//!   and writes an audit-log entry.
//! - The drift gate is enforced server-side. The frontend can preview the
//!   request and read the gate state, but cannot bypass it without setting
//!   `override_drift_gate = true` on the request itself — which is itself
//!   logged in the audit entry.

mod orchestrator;
mod prompt;

pub use orchestrator::{DraftPreview, DraftRequest, DraftSuggestion, DraftingService};
// `DraftOperation` and the prompt-assembly helpers are part of the
// drafting module's public surface for tests and the command layer; the
// `pub use` re-exports keep call-sites tidy even when not all items are
// consumed externally yet.
#[allow(unused_imports)]
pub use orchestrator::DraftOperation;
#[allow(unused_imports)]
pub use prompt::{assemble_messages, PromptInputs};
