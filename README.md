# Quill — A Domain-Tuned YA Fantasy Writing Companion

> A persistent, opinionated writing partner that learns your voice, holds your worldbuilding canon as ground truth, drives drafting through a Save-the-Cat structural skeleton, and is the workspace where you finish a standalone YA epic fantasy.

**This is not a ChatGPT wrapper.** The LLM is one component among five: canon retrieval, voice fingerprinting, beat tracking, structural editing, and prose generation.

---

## Status

Currently at **v1.0.0**. Quill is a complete writing environment. You can create a project, ingest your worldbuilding canon (with a live Obsidian-vault watcher that auto-reingests on save), manage a 15-beat Save-the-Cat sheet, pin reference passages for voice modeling, and draft scenes in a real editor with debounced autosave, Git auto-commit, a live voice-drift gauge, a today's-words counter, drag-to-reorder scenes, a per-scene metadata strip (POV / setting / status / beat / plot threads), full-manuscript search, and one-click compile-to-Markdown.

The AI drafting loop is wired end-to-end. A side-by-side panel assembles a context-bounded prompt from the scene, its beat, the POV character's Bible entry, setting-matched canon, tagged Idea Park notes, active plot threads, the top canon chunks, and your voice anchors — then enforces the voice-drift gate before any call. Suggestions come back into an **inline track-changes review** (sentence-level green/red diff with per-chunk accept/reject) so nothing reaches your manuscript without your say-so. The Character Bible, Idea Park, and Plot Threads "second brain" are live with cross-link search, and per-file privacy rules + a corpus inspector keep sensitive material out of cloud calls.

See the [**user manual**](docs/MANUAL.md) ([HTML](docs/MANUAL.html)) for a complete walkthrough, [`docs/PRD.md`](docs/PRD.md) for the original plan, and [`HANDOFF.md`](HANDOFF.md) for the engineering history.

| Phase                                | Status                                                                          |
| ------------------------------------ | ------------------------------------------------------------------------------- |
| 0. Foundation                        | ✅ complete                                                                     |
| 1. Canon ingestion                   | ✅ complete                                                                     |
| 2. LLM provider layer                | ✅ complete                                                                     |
| 3. Structural engine                 | ✅ complete                                                                     |
| 4. Voice fingerprint                 | ✅ complete                                                                     |
| 5. Drafting + writing UX             | ✅ complete (editor, autosave, drift gauge, search, compile, scene metadata)    |
| 5.x. Obsidian vault watcher          | ✅ complete (live watcher + privacy rules + corpus inspector)                   |
| 6. Revision loop                     | ✅ complete (draft panel + drift gate + inline track-changes review)            |
| 7. Second brain                      | ✅ complete (Character Bible + Idea Park + Plot Threads + cross-links)          |
| 8. Distribution (signing/notarizing) | ⚪ not planned — builds are ad-hoc signed (see Gatekeeper note under Install)   |
| 9. Local embeddings (optional)       | ⏳ pending                                                                      |

---

## Install

### Option A — Use the prebuilt `.dmg` (recommended)

1. Go to the [Releases page](https://github.com/ShamgarBN/quill/releases).
2. Download `Quill_1.0.0_aarch64.dmg` (Apple Silicon only).
3. Double-click the `.dmg`, drag **Quill.app** into your **Applications** folder.
4. **First-launch Gatekeeper bypass.** The bundle is **ad-hoc signed** but does **not** have an Apple Developer ID yet (that's Phase 8). Because the DMG is downloaded via the browser, macOS sets the `com.apple.quarantine` extended attribute on the app, and modern macOS (Sonoma 14+ / Sequoia 15+) will refuse to open it with one of two dialogs:
   - **"Quill.app is damaged and can't be opened"** — Gatekeeper rejected it outright. The most reliable fix:

     ```bash
     xattr -cr /Applications/Quill.app
     ```

     Then double-click the app. It opens, and macOS remembers the decision.
   - **"Quill.app cannot be opened because the developer cannot be verified"** — the older dialog. Right-click (or Control-click) **Quill.app** in Applications, pick **Open**, then click **Open** in the new dialog.

   If neither works, the universal escape hatch is the `xattr -cr` command above. It's safe — all it does is strip the quarantine flag macOS added when you downloaded the file.

5. The app creates `~/Library/Application Support/Quill/` on first run. All your projects, canon, manuscripts, and the encrypted secret store live there.

### Option B — Build from source

You'd do this if you don't trust the prebuilt artifact, want to develop, or you're on Intel macOS (which I haven't tested but should work).

```bash
# Prerequisites: Xcode CLT, Rust ≥ 1.78, Node ≥ 20, pnpm ≥ 9
git clone https://github.com/ShamgarBN/quill.git
cd quill
./scripts/bootstrap.sh

# Produce a release .app + .dmg in apps/desktop/src-tauri/target/release/bundle/
cd apps/desktop
pnpm tauri build
```

The bundle ends up at:

```
apps/desktop/src-tauri/target/release/bundle/dmg/Quill_<version>_aarch64.dmg
apps/desktop/src-tauri/target/release/bundle/macos/Quill.app
```

For a deeper setup walkthrough (toolchain verification, GitHub auth, dev-data isolation), see [`HANDOFF.md`](HANDOFF.md).

---

## First-time setup inside the app

After Quill opens for the first time:

### 1. Create your project

Use the project picker (top-left). Pick a memorable name — it becomes the directory under `~/Library/Application Support/Quill/projects/<id>/`.

### 2. (Optional but recommended) Pin reference passages

Open the **Research** tab. Paste 3–10 short passages (300–800 words each) from books whose voice you want Quill to learn. The defaults you've named are good anchors:

- _Eragon_ (Christopher Paolini)
- _Percy Jackson and the Olympians_ (Rick Riordan)
- _Harry Potter_ (early books — voice is closer to your target tween-safe range)
- _The Wingfeather Saga_ (Andrew Peterson)

Don't blend genres — only YA fantasy that matches the tone you want.

The fingerprint is the word-count-weighted centroid of all enabled pins. The Manuscript view's drift gauge lights up green/amber/red as you write, scoring how close your prose is to that centroid. You need at least one pin and ~30 words in a scene before the gauge engages.

### 3. Wire up your D&D campaign canon (Obsidian)

See **[Connecting Your Obsidian Vault](#connecting-your-obsidian-vault)** below — full walkthrough.

### 4. (Optional) Configure cloud LLM credentials

Open **Settings → Privacy & Cloud LLMs**.

1. Acknowledge the privacy disclosure (one-time; sets `privacy_acknowledged_at`).
2. Paste your **Gemini API key** (free tier — get one at [https://aistudio.google.com/apikey](https://aistudio.google.com/apikey)). Format: `AIza...`. Encrypted at rest under `~/Library/Application Support/Quill/secrets/`.
3. Paste your **Groq API key** (free tier — get one at [https://console.groq.com/keys](https://console.groq.com/keys)). Format: `gsk_...`.
4. Set **Chat provider** to `gemini` (recommended) or `groq`.
5. Set **Embedding provider** to `gemini` (Groq has no embeddings endpoint).
6. Hit the **Ping** button next to each provider — it sends a tiny verification payload and confirms credentials work.

Leave both providers as `mock` if you want to develop or write fully offline. Ingestion + drift detection both work without any cloud calls.

### 5. Set up your beat sheet

Open the **Beat Sheet** tab. You'll see all 15 Save the Cat beats with target word percentages. Either:

- Edit each beat's summary inline (slow but precise), or
- Click **Import outline** and paste your existing acts/chapters/beats — the heuristic matcher will assign each line to the closest beat and show you what didn't match for manual tagging.

Set the **Target word count** at the top (default 85,000 — appropriate for standalone YA). Toggle **Freeze sheet** when you're committed to the structure; this prevents accidental edits during drafting.

### 6. Write

Open the **Manuscript** tab. Create a scene from the left rail, type into the editor. Autosave runs 800ms after you stop typing. Every save:

- Writes the scene as `<NNNN>-<scene-id>.md` under the project's `manuscript/` directory.
- Mirrors the word count back to the beat sheet so progress bars stay accurate.
- Creates a Git commit (best-effort — invisible on success, logged on failure).
- Recomputes the voice-drift score against your fingerprint.

### 7. Draft with AI (Phase 6)

With a scene open, click **Draft** in the Manuscript header. A side panel appears with three operations:

- **Continue** — pick up the scene from where it ends and push toward the active beat.
- **Rewrite** — replace the selected passage with a tighter version in your voice.
- **Critique** — return craft notes (voice, pacing, continuity) for the selection — no prose to insert.

Each call assembles its prompt from:

- the scene text already on disk (the orchestrator reads from disk, not the editor buffer, so what's saved is what's sent), plus the scene card (POV / setting / status),
- the linked beat (label + canonical description),
- the POV character's Character Bible entry, when the scene's POV names a known character,
- setting-matched canon (Location / Cosmology chunks for the scene's setting field),
- tagged Idea Park notes (`beat:`, `pov:`, `scene:` tags) and active (Open / Advancing) plot threads,
- the top-K canon chunks ranked by cosine similarity (chunks tagged `do_not_send` are excluded automatically),
- your top-N reference voice anchors (most-recently-pinned, weighted by length).

Before you commit to the call, hit **Preview** to see exactly what categories of context will be sent and a token-budget estimate. The voice-drift gate runs against the current scene: if the drift score is `≥ 0.7`, the call is held back behind an explicit "Override the drift gate this once" toggle and the worst feature deltas are surfaced (e.g. "sentences are 2.3× longer than your voice"). Every call is recorded in `audit.jsonl` with operation, provider, model, included content categories, and token counts — never the content itself.

For **Continue** and **Rewrite**, click **Review changes** to open the inline track-changes view: the suggestion is diffed against your scene sentence by sentence, additions in green and deletions in red, with per-chunk accept / reject (plus accept-all / reject-all / reset). **Apply** writes only your accepted result back into the scene; anything left pending is treated as rejected. **Critique** is read-only — you integrate the notes by hand.

### 8. Capture characters, ideas, and plot threads (Phase 7)

The **Character Bible** tab is one card per character: name, aliases, role (protagonist / antagonist / mentor / ally / love-interest / family / foil / supporting / minor), arc one-liner, motivation, voice notes, and a `secrets` field that is tagged `do_not_send` by default so it never crosses the network. Each card shows cross-links — every scene and canon chunk that mentions the character by name or alias — so you can audit consistency at a glance. When a scene's POV names a Bible character, that entry is auto-injected into the drafting prompt.

The **Idea Park** tab is a tag-able capture buffer for fragments, beats, or "what if" thoughts. Each idea has its own `do_not_send` flag, and special tags (`beat:catalyst`, `pov:kaelan`, `scene:<id>`) surface the idea automatically in the matching AI draft. Filter by tag chips at the top.

The **Plot Threads** tab tracks recurring arcs that must close by the book's end. Open / Advancing threads are injected into every draft (scene-linked ones marked `[linked]`); Resolved / Abandoned threads are kept for reference but excluded from AI context. Link threads to scenes from the Manuscript metadata strip.

All three stores live as plain JSON under `<project>/bible/`, `<project>/ideas/`, and `<project>/threads/`, and are picked up by the Git auto-commit on save.

---

## Connecting Your Obsidian Vault

Quill is designed to coexist with Obsidian as your "second brain." Point Quill at your vault directory and turn on the **live watcher** — files you save in Obsidian are re-ingested automatically (debounced). Manual one-click ingest still works for one-off files. Per-folder **privacy rules**, YAML-frontmatter overrides, and a **corpus inspector** gate what's allowed to reach a cloud LLM (see below).

### Recommended directory layout

Put your D&D worldbuilding vault somewhere stable, e.g.:

```
~/Documents/ObsidianVaults/Aerthos-Campaign/
├─ Characters/
│  ├─ Kaelan.md
│  ├─ Lirien.md
│  └─ ...
├─ Locations/
│  ├─ Stormhold.md
│  └─ ...
├─ Factions/
├─ Magic/
├─ History/
├─ Cosmology/
├─ Timeline/
├─ Plot/                ← high-level plot notes (often `do_not_send` if spoilery)
└─ DM-Notes/            ← session prep, NPC stat blocks (always `do_not_send`)
```

The directory layout doesn't have to match this — Quill ingests files individually and you tag each one's `CanonKind` and `ChunkSensitivity` at ingest time. But mirroring your folders to `CanonKind` makes batch ingest faster.

### Sensitivity tags (read this carefully)

Every chunk you ingest gets a sensitivity flag:

| Tag           | Meaning                                                                                   | Sent to cloud LLM? |
| ------------- | ----------------------------------------------------------------------------------------- | ------------------ |
| `public`      | Background lore, public-facing world details. Default.                                    | Yes                |
| `spoiler`     | Plot reveals you're okay sharing with the LLM but want flagged in retrieval.              | Yes (with warning) |
| `do_not_send` | Anything you never want crossing the network — DM notes, twist reveals, private journals. | **Never**          |

The `do_not_send` flag is enforced at the retrieval layer: chunks with that tag are excluded from any search whose results would feed an LLM call. **Always tag DM-only material as `do_not_send`** — once it's in the cloud LLM's free-tier training set, you can't pull it back.

### Ingest workflow

1. Open the **Canon** tab.
2. Click **Ingest file**.
3. Pick a file from your Obsidian vault directory (`.md`, `.txt`, or `.pdf`).
4. Set the `CanonKind` (character, location, faction, magic, history, cosmology, timeline, lore, plot_notes, dm_notes, other).
5. Set the sensitivity (default `public`).
6. Click **Ingest**.

Quill extracts the text, breaks it into 400–800-word chunks (Markdown headings respected as natural breakpoints), embeds each chunk, and stores them in the per-project vector index.

You can re-ingest the same file at any time — Quill keys on the absolute file path, so the second ingest replaces the first cleanly. Use this when you've updated a character note in Obsidian and want Quill's index to catch up.

### Searching what you ingested

The Canon tab has a search box: paste any query ("the dragon's true name," "what happened at the Battle of Three Crowns") and Quill returns the top-K most-similar chunks ranked by cosine similarity. This is the same retrieval that feeds the AI drafting prompts.

### Live vault watcher

In the Canon tab, **Pick vault…** to choose your vault directory, then **Start watching**. Files you save in Obsidian are re-ingested automatically; new files are picked up too. The status line shows events received, files re-ingested, and the last change. Deletions are intentionally ignored (Obsidian saves atomically, so acting on "removed" events would risk data loss) — use the corpus inspector to prune chunks for files you've deleted. If a project has a saved vault path with auto-watch on, Quill resumes the watcher when you open it.

### Privacy rules & corpus inspector

- **Folder rules** map a folder name or path prefix to a sensitivity tier (e.g. `DM-Notes` → `do_not_send`). Saving a rule **retroactively re-tags** every matching chunk already in the index.
- A note's **YAML frontmatter** wins over folder rules: add `quill-sensitivity: do_not_send` at the top of any Markdown file to lock it down regardless of location. Anything unmatched falls back to your project default.
- The **corpus inspector** ("Indexed documents") lists every document with its sensitivity, lets you filter, bulk-retag, reveal sources in Finder, delete a document, or **prune missing** (chunks for files no longer on disk).
- When a vault is connected, a cloud provider is selected, and no rules exist with the default still `public`, Quill shows a banner across the Manuscript header so you don't auto-sync private notes by accident.

---

## Daily writing workflow

A typical session looks like this:

1. **Open Quill.** Your project loads automatically if there's only one.
2. **Skim the Beat Sheet.** Whichever beats are unsatisfied are your "TODO" list. Pick the next unsatisfied beat — that's the scene you're writing today.
3. **Search the Canon for context.** In the Canon tab, type a query like "what does Kaelan know about the Hollow King." Quill surfaces the top relevant chunks. Read them so you don't contradict your own worldbuilding.
4. **Open the scene in Manuscript.** Either pick an existing scene from the rail or create a new one.
5. **Write.** Watch the bottom-right drift indicator. If it goes amber or red, your prose is wandering off-voice — pause and re-read a couple of pinned passages from Research.
6. **Stop when you stop.** Autosave + Git commit happen on every flush. There's nothing to "save" or "publish" — just close the window.

### Working in 30-minute bursts vs. multi-hour sessions

Quill is designed for both. The autosave + Git commit cadence means no work is ever lost, and the beat sheet lets you orient quickly: "where was I" is one glance. For multi-hour sessions, the focus mode toggle (`⌘.`) hides the sidebar and chrome.

### Where your work lives on disk

Everything is a plain file you can grep, diff, or open in any editor:

```
~/Library/Application Support/Quill/
├─ projects/
│  └─ <project_id>/
│     ├─ project.json              # metadata + vault path / rules / default sensitivity
│     ├─ manuscript/               # one .md per scene, prefixed with order
│     │  ├─ 0000-front-matter.md
│     │  ├─ 0001-scn_xxx.md
│     │  └─ ...
│     ├─ structure/
│     │  ├─ beat_sheet.json
│     │  └─ scenes.json            # scene metadata (title, POV, setting, status, beat, threads)
│     ├─ canon/                    # ingested originals (kept for re-ingest)
│     ├─ voice/
│     │  └─ pins.json              # reference passages
│     ├─ bible/
│     │  └─ characters.json        # Character Bible
│     ├─ ideas/
│     │  └─ ideas.json             # Idea Park
│     ├─ threads/                  # Plot Threads
│     └─ .git/                     # auto-commit history
├─ vectors.json                    # embedded canon chunks (per-project keyed)
├─ secrets/                        # Argon2id+AES-GCM-encrypted API keys
├─ audit.jsonl                     # log of every cloud LLM call
└─ settings.json
```

Backup strategy: zip `~/Library/Application Support/Quill/` periodically. Or, since each project is its own Git repo, `git push` to a private GitHub remote (the auto-commit history will go with it).

### Recovering an earlier draft

Every save is a Git commit. To roll back:

```bash
cd ~/Library/Application\ Support/Quill/projects/<project_id>
git log --oneline                    # find the commit you want
git checkout <commit> -- manuscript/<scene-file>
```

Quill will pick up the change on next reload.

### Switching providers mid-project

Settings → Cloud LLMs → change the provider dropdown. The change takes effect on the next call (no restart needed). The previous provider's audit-log entries stay; new entries record the new provider.

---

## Repo layout

```
quill/
├─ apps/desktop/          # Tauri app (Rust core + React UI)
├─ docs/                  # PRD, architecture, privacy
├─ scripts/               # bootstrap, signing helpers, icons
├─ tests/                 # Rust + e2e
└─ .github/workflows/     # CI
```

See [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) for development setup and [`HANDOFF.md`](HANDOFF.md) for the full per-phase migration guide.

---

## Tech stack

| Layer                  | Choice                                                       | Why                                                                                            |
| ---------------------- | ------------------------------------------------------------ | ---------------------------------------------------------------------------------------------- |
| Desktop shell          | Tauri 2.x (Rust + WebView)                                   | Small binary, native feel, no Electron tax                                                     |
| UI                     | React 18 + TypeScript + Tailwind 3                           | Mature, hobby-friendly                                                                         |
| Editor                 | Plain `<textarea>` + sentence-diff track-changes overlay     | Simple is shippable; review mode renders an inline diff without a heavyweight rich-text engine  |
| State                  | Zustand                                                      | Minimal, escape-hatch friendly                                                                 |
| LLM (hobby phase)      | Google Gemini 2.5 Pro free tier; Groq Llama 3.3 70B fallback | Best free quality 2026; pluggable for paid Claude/GPT later                                    |
| Embeddings (v1)        | Gemini Embedding API                                         | Phase 9 swaps to local `bge-m3` via `candle-transformers`                                      |
| Voice fingerprint      | Custom Rust pipeline                                         | Sentence rhythm, function words, dialogue ratios, punctuation cadence                          |
| Storage                | Plain Markdown (manuscript) + JSON (everything else)         | You're never trapped in a proprietary format                                                   |
| Encryption at rest     | Argon2id KDF + AES-256-GCM                                   | Modern, vetted                                                                                 |
| Vector store (current) | JSON-backed embedded store                                   | Lightweight, brute-force cosine. LanceDB swap is a Phase 9 task                                |
| Versioning             | Local Git auto-commit (system `git`)                         | Full history, one-command rollback. Plain shell-out is more stable than gitoxide at this scale |

---

## Privacy

- Canon, manuscript, voice fingerprint, and corrections all live locally in `~/Library/Application Support/Quill/`.
- During the hobby phase, drafted text is sent to Google Gemini's free tier (which trains on free-tier inputs). See [`docs/PRIVACY.md`](docs/PRIVACY.md) for the full disclosure and switch-to-paid plan.
- Per-document `do_not_send` flag is honored across all retrieval and generation.
- Every cloud call is logged to `~/Library/Application Support/Quill/audit.jsonl`. The log records categories of content sent (system prompt, user prompt, canon chunks, voice anchors), token counts, and errors — never the content itself.

---

## License

TBD (private project for now).
