# Quill — A Domain-Tuned YA Fantasy Writing Companion

> A persistent, opinionated writing partner that learns your voice, holds your worldbuilding canon as ground truth, drives drafting through a Save-the-Cat structural skeleton, and is the workspace where you finish a standalone YA epic fantasy.

**This is not a ChatGPT wrapper.** The LLM is one component among five: canon retrieval, voice fingerprinting, beat tracking, structural editing, and prose generation.

---

## Status

Currently at **v0.2.0**. The app boots, lets you create projects, ingest canon, manage a 15-beat sheet, pin reference passages for voice modeling, and draft scenes in a real editor with autosave, Git auto-commit, and a live voice-drift indicator. The AI drafting loop is wired end-to-end — a side-by-side panel in the Manuscript view assembles a context-bounded prompt (scene + beat + canon + voice anchors), enforces the voice-drift gate, calls your configured chat provider, and lets you accept the suggestion as an append or as a replacement for a selection. The Character Bible and Idea Park are live, with cross-link search across both manuscripts and canon.

See [`docs/PRD.md`](docs/PRD.md) for the full plan and [`HANDOFF.md`](HANDOFF.md) for the deeper engineering migration guide.

| Phase                                | Status                                                                        |
| ------------------------------------ | ----------------------------------------------------------------------------- |
| 0. Foundation                        | ✅ complete                                                                   |
| 1. Canon ingestion                   | ✅ complete                                                                   |
| 2. LLM provider layer                | ✅ complete                                                                   |
| 3. Structural engine                 | ✅ complete                                                                   |
| 4. Voice fingerprint                 | ✅ complete                                                                   |
| 5. Drafting modes (MVP)              | ✅ complete (editor + autosave + drift gauge)                                 |
| 6. Revision loop                     | ✅ complete (side-by-side draft panel + drift gate; inline track-changes next) |
| 7. Second brain                      | ✅ complete (Character Bible + Idea Park + cross-links)                       |
| 8. Distribution (signing/notarizing) | ⏳ pending — current builds are unsigned                                      |
| 9. Local embeddings (optional)       | ⏳ pending                                                                    |

---

## Install

### Option A — Use the prebuilt `.dmg` (recommended)

1. Go to the [Releases page](https://github.com/ShamgarBN/writing-assistant/releases).
2. Download `Quill_0.2.0_aarch64.dmg` (Apple Silicon only).
3. Double-click the `.dmg`, drag **Quill.app** into your **Applications** folder.
4. **First-launch Gatekeeper bypass.** Because the build isn't yet code-signed with a paid Apple Developer ID, macOS will refuse to open it on first run with a "cannot be opened because it is from an unidentified developer" dialog. To bypass:
   - Open **Finder → Applications**.
   - **Right-click** (or Control-click) **Quill.app** and pick **Open**.
   - The dialog will reappear with an **Open** button. Click it.
   - Future launches just work — macOS remembers your decision.

   Alternative: **System Settings → Privacy & Security → "Open Anyway"** after the first refused launch.

5. The app creates `~/Library/Application Support/Quill/` on first run. All your projects, canon, manuscripts, and the encrypted secret store live there.

### Option B — Build from source

You'd do this if you don't trust the prebuilt artifact, want to develop, or you're on Intel macOS (which I haven't tested but should work).

```bash
# Prerequisites: Xcode CLT, Rust ≥ 1.78, Node ≥ 20, pnpm ≥ 9
git clone https://github.com/ShamgarBN/writing-assistant.git
cd writing-assistant
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

With a scene open, click **Draft** in the Manuscript header. A side panel appears with five operations:

- **Continue** — generate the next paragraph(s) of the scene from where the cursor is.
- **Rewrite** — rewrite the current selection in your voice.
- **Tighten** — shorten the selection while preserving meaning.
- **Expand** — add a sentence or two of texture to the selection.
- **Critique** — return craft notes (no prose) for the selection.

Each call assembles its prompt from:

- the scene text already on disk (the orchestrator reads from disk, not the editor buffer, so what's saved is what's sent),
- the linked beat (label + canonical description),
- the top-K canon chunks ranked by cosine similarity (chunks tagged `do_not_send` are excluded automatically),
- your top-N reference voice anchors (most-recently-pinned, weighted by length).

Before you commit to the call, hit **Preview** to see exactly what categories of context will be sent and a token-budget estimate. The voice-drift gate runs against the candidate output: if the drift score is `> 0.7`, the suggestion is held back behind an explicit "Override and accept anyway" toggle and the top-three feature deltas are surfaced (e.g. "sentences are 2.3× longer than your voice"). Every call is recorded in `audit.jsonl` with operation, provider, model, included content categories, and token counts — never the content itself.

Click **Append** to paste the candidate at the end of the scene, or **Replace** to swap the suggestion in for whatever you had selected. (Inline track-changes — green/red diff view with per-chunk accept / reject — is queued as the next priority. For now you can fall back to `git diff` between auto-commits.)

### 8. Capture characters and ideas (Phase 7)

The **Bible** tab is the Character Bible: one card per character with name, aliases, role (protagonist / antagonist / mentor / love-interest / supporting / minor), arc one-liner, motivation, voice notes, and a `secrets` field that is automatically tagged as `do_not_send` so it never crosses the network. Each character card shows cross-links — every scene and canon chunk that mentions the character by name or alias (case-insensitive substring match), so you can audit consistency across the whole project at a glance.

The **Ideas** tab is the Idea Park: a tag-able capture buffer for fragments, beats, or "what if" thoughts you can't act on yet. Each card has its own `do_not_send` flag for spoiler-sensitive ideas. Filter by tag from the sidebar.

Both stores live as plain JSON under `<project>/bible/characters.json` and `<project>/ideas/ideas.json` and are picked up by the Git auto-commit on save.

---

## Connecting Your Obsidian Vault

Quill is designed to coexist with Obsidian as your "second brain." Today (Phase 5), the integration is **manual ingest with one-click reingest**; the live filesystem watcher lands in a future phase.

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

The Canon tab has a search box: paste any query ("the dragon's true name," "what happened at the Battle of Three Crowns") and Quill returns the top-K most-similar chunks ranked by cosine similarity. This is the same retrieval that will feed the Phase-6 drafting prompts.

### Coming soon (Phase 5.x)

A live filesystem watcher that auto-reingests files as you save them in Obsidian. The Rust plumbing already exists at `apps/desktop/src-tauri/src/services/canon/watcher.rs`; the missing pieces are a settings field for the vault path, a Tauri command to start/stop the watcher, and a UI control in the Canon tab. See `HANDOFF.md` section 8 for the priority order.

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
│     ├─ project.json              # metadata
│     ├─ manuscript/               # one .md per scene, prefixed with order
│     │  ├─ 0000-front-matter.md
│     │  ├─ 0001-scn_xxx.md
│     │  └─ ...
│     ├─ structure/
│     │  ├─ beat_sheet.json
│     │  └─ scenes.json            # scene metadata (titles, status, beats)
│     ├─ canon/                    # ingested originals (kept for re-ingest)
│     ├─ voice/
│     │  └─ pins.json              # reference passages
│     ├─ bible/
│     │  └─ characters.json        # Character Bible (Phase 7)
│     ├─ ideas/
│     │  └─ ideas.json             # Idea Park (Phase 7)
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
writing-assistant/
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
| Editor                 | Plain `<textarea>` (Phase 5 MVP); Lexical planned            | Simple is shippable; rich text lands with track-changes in Phase 6                             |
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
