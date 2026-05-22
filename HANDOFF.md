# Quill — Hand-off Guide

This document is everything you need to pick the project up on the destination
MacBook Pro M4 and keep building. Read it top-to-bottom the first time, then
keep it open while you bootstrap.

---

## 1. What you have right now

A native macOS desktop app (Tauri 2.x, Rust core + React UI) with these
phases implemented end-to-end:

- **Phase 0 — Foundation.** Tauri shell, two-pane layout, light/dark themes,
  Inter / Charter / JetBrains Mono fonts, settings store, encrypted secret
  store (Argon2id + AES-256-GCM), per-project filesystem layout under
  `~/Library/Application Support/Quill/`, local Git auto-commit.
- **Phase 1 — Canon ingestion.** `.md`, `.txt`, and `.pdf` files are
  extracted, semantically chunked (~400–800 word chunks with overlap and
  Markdown-heading-aware breakpoints), embedded, and stored. There's a Canon
  view with ingest controls and semantic search. The vector backend is a
  JSON-backed in-memory cosine store; switching to LanceDB later is a
  drop-in trait swap. A vault watcher (Obsidian-friendly) is wired but not
  exposed in the UI yet.
- **Phase 2 — LLM provider layer.** Three provider implementations behind a
  single trait: deterministic mock (used for tests + offline dev), Google
  Gemini (chat + embeddings), Groq (chat only — Llama 3.3 70B). API keys
  are encrypted at rest. Settings has a privacy disclosure flow, per-provider
  key entry, and an audit-log viewer. Every cloud call is recorded
  locally — operation, provider, model, included content categories, token
  counts, errors. The audit log never contains the actual content.
- **Phase 3 — Structural engine.** All 15 Save the Cat beats with
  canonical descriptions and target word percentages, an interactive Beat
  Sheet view with editable summaries, satisfied/locked toggles, sheet-wide
  freeze, target word-count slider, and an outline-paste import flow that
  matches free-form headings to beats heuristically.
- **Phase 4 — Voice fingerprint.** A pure-Rust feature extractor capturing
  sentence rhythm, lexical density, dialogue ratios, function-word
  frequencies, and punctuation cadence. A reference-pin store lets you
  paste passages whose voice you want to emulate; the fingerprint is the
  weighted centroid of those pins. A drift detector compares any candidate
  passage against the fingerprint and reports both a 0–1 drift score and the
  top per-feature deltas. The Research view is the management surface.
- **Phase 5 — Drafting MVP.** A real Manuscript view with a left scene rail,
  a centered editor, debounced autosave (800 ms), word/char counts, an
  on-canvas voice-drift indicator, and per-save Git commits.

Everything compiles cleanly: 75 Rust unit tests, zero clippy warnings under
`-D warnings`, zero TypeScript errors, ESLint clean, Prettier clean, rustfmt
clean.

What is NOT yet implemented (do these next, in order): chat-driven
drafting workflows that actually call the LLMs, inline track-changes UX,
the Character Bible / Idea Park tabs (currently placeholder routes), and
distribution (signing + notarization + auto-update).

---

## 2. Repo orientation

```
fantasy-novel/
├─ HANDOFF.md                    ← you are here
├─ README.md
├─ apps/desktop/                 ← the only deliverable lives here
│  ├─ src/                       ← React/TS UI
│  │  ├─ components/             ← shell, layout primitives
│  │  ├─ lib/                    ← cn(), ipc.ts (typed Tauri bindings)
│  │  ├─ routes/                 ← one file per top-level view
│  │  │  ├─ Manuscript.tsx       ← Phase 5 editor
│  │  │  ├─ Beats.tsx            ← Phase 3 beat sheet
│  │  │  ├─ Canon.tsx            ← Phase 1 ingest + search
│  │  │  ├─ Research.tsx         ← Phase 4 reference pins + drift tester
│  │  │  ├─ Settings.tsx         ← Phase 2 provider keys + audit log
│  │  │  └─ Bible.tsx, Ideas.tsx ← placeholders for Phase 7
│  │  ├─ stores/app.ts           ← Zustand store
│  │  ├─ styles/globals.css      ← Tailwind + design tokens
│  │  └─ types.ts                ← mirrors Rust serde models
│  ├─ src-tauri/                 ← Rust core
│  │  └─ src/
│  │     ├─ commands/            ← thin Tauri command handlers
│  │     ├─ models/              ← serde-serialized data models
│  │     ├─ services/            ← all business logic
│  │     │  ├─ canon/            ← extract, chunker, ingest, watcher
│  │     │  ├─ crypto/           ← Argon2id + AES-GCM secret store
│  │     │  ├─ git/              ← system `git` shell-out
│  │     │  ├─ llm/              ← gemini, groq, mock + audit
│  │     │  ├─ manuscript/       ← per-scene Markdown content
│  │     │  ├─ storage/          ← atomic JSON writes, project layout
│  │     │  ├─ structure/        ← beat sheet, scenes, outline import
│  │     │  ├─ vector/           ← JSON-backed cosine store
│  │     │  └─ voice/            ← extractor, fingerprint, pin store
│  │     ├─ state.rs             ← AppState wired up at boot
│  │     ├─ telemetry.rs         ← tracing subscriber
│  │     └─ lib.rs               ← invoke_handler! registry
│  ├─ package.json
│  ├─ pnpm-workspace.yaml
│  ├─ vite.config.ts
│  ├─ tailwind.config.ts
│  ├─ eslint.config.js
│  └─ .prettierignore
├─ docs/
│  ├─ PRD.md                     ← full product plan + phasing
│  ├─ ARCHITECTURE.md            ← module boundaries
│  ├─ DEVELOPMENT.md             ← setup
│  └─ PRIVACY.md                 ← data handling + cloud disclosure
├─ scripts/bootstrap.sh
├─ tests/{rust,e2e}/             ← reserved for integration suites
└─ .github/workflows/            ← CI (already templated)
```

---

## 3. What to do on the new MacBook (in order)

### 3.1 Install prerequisites

```bash
# Apple toolchain
xcode-select --install

# Rust 1.78+
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Node 20+ (via fnm or homebrew)
brew install fnm && fnm install 20 && fnm use 20

# pnpm 9+
corepack enable && corepack prepare pnpm@9 --activate
```

Also make sure git ≥ 2.40 is installed (it ships with Xcode CLT). Optional
but useful: `brew install gh` for GitHub CLI.

### 3.2 Pull the repo

```bash
cd ~/Desktop          # or wherever
git clone https://github.com/ShamgarBN/writing-assistant.git
cd writing-assistant
./scripts/bootstrap.sh
```

If `bootstrap.sh` complains, the manual fallback is:

```bash
cd apps/desktop
pnpm install
cd src-tauri && cargo fetch
```

### 3.3 First run

```bash
# From repo root, isolating dev data so you don't pollute the
# eventual production app-support directory:
QUILL_DATA_DIR=$PWD/.dev-userdata pnpm --filter desktop tauri dev
```

The first build is ~5–10 minutes (Tauri pulls macOS frameworks and compiles
all Rust deps). Subsequent builds are seconds.

### 3.4 Sanity tests

```bash
cd apps/desktop/src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

```bash
cd apps/desktop
pnpm typecheck
pnpm lint
pnpm exec prettier --check .
```

All should be green.

---

## 4. Connecting your existing data

### 4.1 The D&D vault

The plan was always: keep the source material as Markdown in an Obsidian
vault, and let Quill watch the directory.

For now (until the watcher is exposed in the UI), copy or symlink the vault
into your project's canon directory and ingest each file via the Canon view.
The path is:

```
~/Library/Application Support/Quill/projects/<project_id>/canon/
```

Or, more practically: open the Canon view, click **Ingest file**, and pick
files directly from the dialog. Each file becomes a `CanonDocument` with
`CanonKind` (character, location, faction, magic, history, cosmology,
timeline, lore, plot_notes, dm_notes, other) and a `ChunkSensitivity` tag
(`public`, `spoiler`, or `do_not_send`). The `do_not_send` tag is the kill
switch: chunks with that tag are excluded from any retrieval that feeds a
cloud LLM.

When you're ready to surface the Obsidian vault watcher in the UI, the
plumbing is already there:
`apps/desktop/src-tauri/src/services/canon/watcher.rs`. You'll need to add
a Tauri command + a settings field for the vault path + a Canon UI control.

### 4.2 Cloud LLM keys

In Settings:

1. Acknowledge the privacy disclosure (this writes
   `privacy_acknowledged_at` so the disclosure isn't shown again).
2. Paste your Gemini API key (`AIza...`). It's encrypted at rest under
   `~/Library/Application Support/Quill/secrets/`.
3. Paste your Groq API key (`gsk_...`).
4. Set the "Chat provider" dropdown to `gemini` (recommended) or `groq`.
5. Set the "Embedding provider" dropdown to `gemini`. (Groq has no
   embeddings endpoint; mock is for testing.)
6. Hit the **Ping** button next to each provider to verify the key works.
   The ping is logged to the audit log with `operation: "ping"`.

If you switch back to mock providers, ingestion + drift still work entirely
offline. This is the recommended development mode when you're not actively
testing chat behavior.

### 4.3 Voice reference passages

In Research, paste 3–10 short passages (300–800 words each) from the books
you've named as voice anchors: _Eragon_, _Percy Jackson_, _Harry Potter_,
_Wingfeather Saga_. Don't blend genres — only YA fantasy.

The fingerprint is the weighted centroid of all enabled pins. Once you
have at least one pin, the Manuscript view will start computing drift
against your scene as you write (after ~30 words).

---

## 5. Architecture quick reference

The two rules that keep the codebase honest:

1. **Commands compose services. Services do not compose commands.**
   `apps/desktop/src-tauri/src/commands/*.rs` files are thin glue: parse
   args, call a service, return a serializable result. All real logic
   lives in `services::*`.

2. **Frontend never calls `invoke` directly from components.** Every
   command goes through `apps/desktop/src/lib/ipc.ts`, which is the single
   place that knows the IPC schema.

The data lifecycle:

```
PDF / Markdown → extract → chunk → embed → JSON vector store
                                              ↓
                              user query → cosine search → ChunkRef[]
                                              ↓
                                  (Phase 6) prompt assembly + LLM call
                                              ↓
                                  candidate prose → voice drift check
                                              ↓
                                  scene editor → on-disk Markdown
                                              ↓
                                  per-save git commit
```

Three abstractions to know:

- `services::vector::VectorStore` — current impl is `JsonVectorStore`. To
  swap to LanceDB, implement the trait and rebind in `state.rs`.
- `services::llm::ChatProvider` / `EmbeddingsProvider` — providers are
  resolved at call time from `ProviderRegistry`, so changing the configured
  provider in Settings takes effect on the next call without a restart.
- `services::voice::ReferencePinStore` — the source of truth for the
  fingerprint. The fingerprint is recomputed on every drift call rather
  than cached, which keeps things simple and is fast enough at this scale.

---

## 6. Scripts and commands cheat sheet

```bash
# Dev loop
QUILL_DATA_DIR=$PWD/.dev-userdata pnpm --filter desktop tauri dev

# Rust gates
cd apps/desktop/src-tauri
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test

# Frontend gates
cd apps/desktop
pnpm typecheck
pnpm lint
pnpm format        # write
pnpm format:check  # verify

# Release build (unsigned, no notarization yet)
pnpm --filter desktop tauri build
```

Useful environment variables:

- `QUILL_DATA_DIR` — override the app-support directory. Use a per-checkout
  path so `cargo test` and the live app don't collide.
- `RUST_LOG=quill_desktop=debug` — verbose tracing.

---

## 7. What I would do next, in priority order

1. **Phase 6.1: minimal "Draft this scene" button.** In Manuscript view,
   add a button that takes the current scene's outline (title + Story Grid
   five commandments + linked beat) plus the top-K canon chunks (excluding
   `do_not_send`), assembles a prompt, calls the configured chat provider,
   and pastes the result above the user's existing prose. Show the
   "what will be sent" preview before the call (the audit-log infrastructure
   already records the categories — the preview is just rendering them).
2. **Phase 6.2: drift gate.** Block the draft action if the candidate's
   drift score is `> 0.7` and surface the top three deltas so the user
   knows _why_ it's off (e.g., "sentences are 2.3× longer than your voice").
3. **Phase 6.3: track-changes diff.** Use a JS diff library (e.g., `diff` or
   `fast-diff`) to render inserts/deletes inline. The user accepts/rejects
   per chunk. This pairs naturally with the existing autosave loop.
4. **Phase 7: Character Bible + Idea Park.** Both are placeholder routes.
   The model is straightforward: a JSON file per character / idea card,
   stored under `<project>/bible/` and `<project>/ideas/`. Reuse the same
   `atomic_write_json` helper.
5. **Phase 8: Distribution.** Code-sign with your Apple Developer ID,
   notarize via `notarytool`, ship a `.dmg` via the Tauri updater. Don't
   start this until the app is feature-complete enough that you actually
   want to install it as the production binary.

The thing I would NOT do next is swap the vector store for LanceDB. The
JSON store handles tens of thousands of chunks fine and the LanceDB swap is
a contained refactor (one trait impl) — defer it until ingestion volume is
actually a bottleneck.

---

## 8. Known sharp edges

- The `pdf-extract` crate is heuristic. Tables, multi-column layouts, and
  scanned PDFs all degrade. Prefer Markdown sources whenever possible.
- The voice extractor's English-language assumptions (function-word list,
  sentence boundaries, dialogue tags) are baked in. Multi-language
  manuscripts would need a redesign.
- The autosave debounce is global per-scene. Switching scenes with unsaved
  changes still flushes (because the effect that changes scenes also
  unmounts the timer, and the next render's effect for the new scene
  does not see the old text), but if you ever add multi-window editing
  this needs a per-scene queue.
- The drift score uses an `L2`-into-`tanh` mapping; it's good enough for a
  visual indicator but not calibrated. Don't write product copy that
  promises "score = X means Y" without re-checking against your own pins.
- Git auto-commit is best-effort. If `git` isn't on `PATH`, saves succeed
  but a tracing warning is logged. Surface this in the UI before shipping
  to a non-developer audience.

---

## 9. If something on the new machine breaks

The likeliest failure modes, in order:

1. **`pnpm install` fails on a `node-gyp` step.** Make sure Xcode CLT is
   installed (`xcode-select -p` should print a path). On Apple Silicon you
   may also need `softwareupdate --install-rosetta --agree-to-license` if
   any dependency still ships an x86_64 prebuilt.
2. **`cargo build` fails compiling `pdf-extract` or `aes-gcm`.** Both
   require recent Rust; run `rustup update` if you're below 1.78.
3. **`tauri dev` opens a blank white window.** The Vite dev server probably
   isn't reachable on the configured port. Check `apps/desktop/vite.config.ts`
   and the matching `tauri.conf.json` — they must agree on the dev URL.
4. **Saves silently fail.** Likely a permissions issue on
   `~/Library/Application Support/Quill/`. Check ownership; if you ran the
   app once as root by mistake, `chown -R $(whoami)` the directory.

---

## 10. Closing note from the past

The hardest part of this kind of project is staying disciplined about what
the LLM does and doesn't do. Quill is built on the premise that the LLM is
a _renderer_, not an author: the canon retrieval, voice fingerprint, and
beat sheet are the author. Resist the temptation, when you wire up Phase 6,
to just pipe "make this scene better" through the chat provider and trust
the result. That's the path to AI slop. Always keep the loop:

1. The user (or the structural engine) chooses a target — beat, scene,
   paragraph.
2. Quill assembles the canon evidence, the voice anchors, and the
   structural constraints.
3. The LLM produces a candidate.
4. The voice drift gauge gates whether that candidate is shown at all.
5. The user accepts, edits, or regenerates — and every accept goes back
   into the corpus that the fingerprint averages over.

That's the whole machine. Phases 6–8 are the last 30% of the build, but
all the hard architectural decisions are already made. Good luck.
