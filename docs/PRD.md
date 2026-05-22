# PRD — Quill: A Domain-Tuned Writing Companion for YA Fantasy

**Version:** 0.1 — planning lock (approved 2026-05-22)
**Owner:** Shamgar (single user)
**Repository:** `github.com/ShamgarBN/writing-assistant`
**Target platform:** macOS 14+ on Apple Silicon (MacBook Pro M4, 16 GB RAM)
**Distribution:** code-signed + notarized .dmg with Developer ID; Tauri auto-updater

---

## 1. Vision

A persistent, opinionated writing partner that learns the user's voice, holds worldbuilding canon as ground truth, drives drafting through a Save-the-Cat structural skeleton, and produces an 85,000-word standalone YA epic fantasy at professional quality. Not a ChatGPT wrapper — a domain-tuned system in which the LLM is one component among five (canon retrieval, voice fingerprinting, beat tracking, structural editing, prose generation).

## 2. Non-goals (v1)

- Multi-user / collaboration
- Cross-platform (Windows, Linux, iOS — explicitly out)
- Final production formatting (DOCX/EPUB/KDP-ready PDF) — deferred to v1.x
- Audio/dictation input
- Image generation (covers, character art)
- Native plugin ecosystem
- Mobile companion

## 3. Target reader profile of the book being written

| Attribute | Value |
|---|---|
| Sub-genre | Epic fantasy |
| Reader age | 12–16, accessible down to 11 |
| Length (Book 1) | ~85,000 words, standalone with series potential |
| POV / tense | 3rd person limited, past — recommended default. (1st person past available.) |
| Romance | Mild interest, aloof flirting only — no on-page kiss in Book 1 unless plot-critical |
| Content rating | Tween-safe: clean violence, no sexual content, no profanity beyond mild fantasy oaths |

## 4. Voice blend

Reference shelf: Eragon, Percy Jackson, Harry Potter, Wingfeather Saga.

| Weight | Cluster | Anchors | What we extract |
|---|---|---|---|
| 35% | Wry Conversational with whimsy | Rick Riordan, Andrew Peterson | Banter rhythm, narrator warmth, comic timing |
| 35% | Architectural Worldbuilder, accessible | Christopher Paolini, J.K. Rowling | Epic scope, clean exposition, magic-as-wonder, character-led worldbuilding |
| 30% | Spare Cinematic, light palette | Leigh Bardugo (Shadow & Bone trilogy only) | Propulsive scene-craft, subtext dialogue, tight pacing |

The drift dial moves over time as the system learns the user's voice from corrections.

## 5. Architecture overview

```
┌─────────────────────────────────────────────────────────────┐
│  Tauri 2.x desktop app (Rust core + React/TS UI)            │
├─────────────────────────────────────────────────────────────┤
│  UI layer    : React + TypeScript + Tailwind + shadcn       │
│  Editor      : Lexical — extensible, track-changes          │
│  IPC         : Tauri commands (Rust ↔ TS)                   │
├─────────────────────────────────────────────────────────────┤
│  Core (Rust)                                                │
│  ├─ Canon Service     : PDF + Markdown ingest, chunk, embed │
│  ├─ Vector Store      : LanceDB (embedded, Rust-native)     │
│  ├─ LLM Provider      : pluggable (Gemini → Claude later)   │
│  ├─ Voice Engine      : feature pipeline + few-shot builder │
│  ├─ Structure Engine  : Save-the-Cat beats + Story Grid     │
│  ├─ Storage           : SQLite (metadata) + MD files (text) │
│  ├─ Crypto            : Argon2id + AES-256-GCM at rest      │
│  └─ Git Auto-Commit   : libgit2 / gitoxide on every save    │
├─────────────────────────────────────────────────────────────┤
│  External (cloud, hobby phase)                              │
│  ├─ Google Gemini 2.5 Pro (drafting + critique, free tier)  │
│  ├─ Google Gemini Embedding API                             │
│  └─ Groq Llama 3.3 70B (rate-limit fallback)                │
└─────────────────────────────────────────────────────────────┘
```

## 6. Core capabilities (v1)

### 6.1 Canon ingestion (Phase 1)

- Watch dedicated novel-vault Obsidian folder
- Ingest PDFs (text extraction + OCR fallback via Tesseract for scanned content)
- Chunk semantically (400–800 tokens, overlap 80) with structural boundaries respected
- Embed via Gemini Embedding API; store vectors in LanceDB; metadata in SQLite
- Tag canon entries with `kind`, `spoiler-tier`, `do-not-send`

### 6.2 Structural engine — Save-the-Cat + Story Grid (Phase 3)

First-class data model, not prompts:

- **Beat** — one of 15 Save-the-Cat slots; tracks target word count, target chapter range, status (empty / drafted / locked)
- **Chapter** — POV, target word count, beat assignment(s)
- **Scene** — Story Grid value-shift template: opening value, closing value, conflict driver, turning point, decision
- **Thread** — recurring plot/character arc that must close by Book 1's end

The app constantly answers: "Where am I in the structure? Which beat am I underwriting? What thread is dangling?"

### 6.3 Drafting — three modes, hot-switchable (Phase 5)

| Mode | Scope | Trigger | Use case |
|---|---|---|---|
| **Scene draft** | Full scene, 800–2500 words | `⌘⇧S` | When you have an outlined scene and want a full pass |
| **Paragraph cowrite** | Next paragraph from cursor | `⌘⇧P` | When you're steering beat by beat |
| **Sentence completion** | Next sentence | `Tab` | Tight co-writing, you stay in the seat |

All three condition on: current beat, scene card, character POV, voice fingerprint, retrieved canon (top-k=5), recent 3 paragraphs.

### 6.4 Voice modeling — three layered correction channels (Phases 4 & 6)

1. **Inline edits** — your rewrites are silently diffed; deltas feed the voice fingerprint
2. **Explicit "better" callout** — highlight + corrected version + optional reason tag (Too purple / Wrong rhythm / Off-character / Too modern / Not whimsical enough)
3. **Reference pins** — paste 3–5 paragraphs from books you love; pinned exemplars become live few-shot examples

**Fail-loud drift detection:** voice fingerprint computed on every generation; if generated text falls outside a tunable distance threshold from the established voice, the UI surfaces a "voice drift detected — regenerate?" badge before the user sees the text in flow.

### 6.5 Revision loop (Phase 6)

- Inline track-changes editor (default), with side-by-side and replace-in-place toggleable per session
- Every accepted/rejected change is a training signal
- Per-scene "revision passes": Structural → Voice → Line → Polish

### 6.6 Second brain (Phase 7)

| Tab | Behavior |
|---|---|
| Manuscript | The book itself, chapter/scene tree |
| Character Bible | Auto-extracted on every save |
| Idea Park | Stray ideas not yet placed |
| Research Notes | Pasted exemplars, craft notes |

### 6.7 Privacy controls

- Free-tier disclosure banner shown on first run; recorded in user_settings with timestamp of acknowledgment
- Per-document `do-not-send` flag honored across all retrieval and generation
- "What gets sent" preview before any cloud call (toggleable, off by default after first use)
- Local audit log of every cloud request: timestamp, provider, token count, content category

## 7. Data model (high level)

```
Project
  ├─ Manuscript
  │   └─ Chapters[]
  │       └─ Scenes[]
  │           └─ ProseBlocks[]   (markdown, version-tracked)
  ├─ BeatSheet (15 beats, locked/fluid per beat)
  ├─ Threads[]
  ├─ Characters[]                 (auto-derived + user-edited)
  ├─ Locations[]
  ├─ LoreEntries[]
  ├─ CanonDocuments[]             (PDFs + Markdown)
  ├─ ReferencePins[]              (passages from reference shelf)
  ├─ VoiceFingerprint             (rolling, versioned)
  ├─ Corrections[]                (training signal log)
  └─ AuditLog[]                   (cloud calls)
```

## 8. UI direction

- Minimal, two-pane primary layout: collapsible left tree (Manuscript / Bible / Ideas / Research / Beats / Settings), right pane is the editor
- Light + dark themes, system-aware, manual toggle
- Typography: Inter for UI, Charter for the writing pane (JetBrains Mono optional)
- Accent color: warm amber `#C8924A` (light) / `#E0B470` (dark) — single accent, fantasy-evocative without being cheesy
- Focus mode: ⌘. collapses everything except the current scene
- Distraction hierarchy: structural alerts (beat drift, voice drift, continuity error) appear as quiet badges, never modal popups

## 9. Success criteria — when v1 is "done"

| Criterion | Bar |
|---|---|
| Functional | All 9 phases ship and pass acceptance |
| Quality | One full Save-the-Cat beat sheet + at least 5,000 words of generated/edited prose at acceptable voice match |
| Stability | No data loss across 30 days of use; auto-save survives forced quits and OS crashes |
| Performance | <2s for paragraph cowrite, <8s for scene draft, on free-tier Gemini Pro with cold cache |
| Distribution | Notarized .dmg installs cleanly on a fresh macOS 14+ system; auto-updater verified |

## 10. Phases

| # | Phase | Est. duration (hobby pace) |
|---|---|---|
| 0 | Foundation | 1–2 weeks |
| 1 | Canon ingestion | 1–2 weeks |
| 2 | LLM provider layer | 1 week |
| 3 | Structural engine | 1–2 weeks |
| 4 | Voice fingerprint | 2–3 weeks |
| 5 | Drafting modes | 2–3 weeks |
| 6 | Revision loop | 2 weeks |
| 7 | Second brain | 1–2 weeks |
| 8 | Distribution | 1 week |
| 9 | Local embeddings (optional) | 1–2 weeks |

Total: ~12–18 weeks at hobby pace (3–4 months).

## 11. Phase 0 — Foundation: acceptance criteria

- [ ] App launches cleanly on macOS 14+ Apple Silicon
- [ ] Light/dark theme toggles correctly
- [ ] User can create a new project (Project = a single book)
- [ ] Settings page exists and persists changes between launches
- [ ] Sensitive settings encrypted at rest (Argon2id KDF + AES-GCM)
- [ ] Every save commits to a local Git repository (no remote required)
- [ ] CI pipeline builds the app on PR
- [ ] Phase 0 docs published (this PRD, ARCHITECTURE.md, DEVELOPMENT.md, PRIVACY.md)
