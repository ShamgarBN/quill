# Quill — User Manual

_Version 0.2.1 · macOS (Apple Silicon)_

This is the working manual for Quill — the writing companion you built to draft
your YA fantasy novel. It's written for you, the one person who uses it, and it
describes the app exactly as it ships today. Keep it open while you write.

If you only read one thing, read this: **Quill treats the AI as a renderer, not
an author.** Your canon, your voice fingerprint, your beat sheet, and your plot
threads are the author. The model produces candidates; you decide what survives.
Everything below is in service of keeping that loop honest.

---

## Contents

1. [What Quill is](#1-what-quill-is)
2. [Launching the app](#2-launching-the-app)
3. [The workspace](#3-the-workspace)
4. [Projects](#4-projects)
5. [Settings & cloud LLMs](#5-settings--cloud-llms)
6. [Research — your voice fingerprint](#6-research--your-voice-fingerprint)
7. [Canon — worldbuilding & your Obsidian vault](#7-canon--worldbuilding--your-obsidian-vault)
8. [Privacy rules & sensitivity](#8-privacy-rules--sensitivity)
9. [Beat Sheet](#9-beat-sheet)
10. [Character Bible](#10-character-bible)
11. [Plot Threads](#11-plot-threads)
12. [Idea Park](#12-idea-park)
13. [Manuscript — writing scenes](#13-manuscript--writing-scenes)
14. [Drafting with AI](#14-drafting-with-ai)
15. [Reviewing changes (track-changes)](#15-reviewing-changes-track-changes)
16. [Compile & export](#16-compile--export)
17. [Where your work lives](#17-where-your-work-lives)
18. [Backups & version history](#18-backups--version-history)
19. [Keyboard shortcuts](#19-keyboard-shortcuts)
20. [Troubleshooting](#20-troubleshooting)

---

## 1. What Quill is

Quill is a native macOS desktop app (not a website, not a ChatGPT wrapper). It is
built around five components that work together:

- **Canon retrieval** — your worldbuilding, ingested and searchable, so the AI
  draws from your truth instead of inventing.
- **Voice fingerprint** — a statistical model of the prose voice you want,
  learned from reference passages you pin.
- **Beat tracking** — a 15-beat Save the Cat skeleton that tells you what scene
  to write next and how long it should run.
- **Structural editing** — scenes carry POV, setting, status, beat links, and
  plot-thread links that feed the drafting context.
- **Prose generation** — the LLM, gated by the voice-drift detector, used only
  to render candidates you accept by hand.

Your manuscript is plain Markdown. Everything else is plain JSON. You are never
trapped in a proprietary format — you can grep it, diff it, or open it in any
editor.

---

## 2. Launching the app

### From the installed app

Double-click **Quill.app** in Applications. If macOS refuses with _"Quill.app is
damaged"_ or _"the developer cannot be verified"_, that's Gatekeeper reacting to
the app not having an Apple Developer ID yet — the bundle is fine, just
quarantined. Strip the flag once:

```bash
xattr -cr /Applications/Quill.app
open /Applications/Quill.app
```

macOS remembers the decision after that.

### From source (development)

```bash
cd apps/desktop
QUILL_DATA_DIR=/Users/benniemann/Projects/AI-Projects/Quill/.dev-userdata pnpm tauri dev
```

`QUILL_DATA_DIR` keeps development data out of the real app-support directory so
you can wipe and start over without touching live work. The first build takes a
few minutes; later launches are seconds.

### First run

On first launch Quill creates `~/Library/Application Support/Quill/` and shows
the **project picker**. Name your book (you can rename it later) and click
**Create project**. If you only ever have one project, Quill opens it
automatically on every subsequent launch.

---

## 3. The workspace

```
┌─────────────────────────────────────────────────────────┐
│  ● Quill                              Focus  | ☀ ☾ ⌖     │  ← title bar
├──────────┬──────────────────────────────────────────────┤
│ Project  │                                               │
│  ─────   │                                               │
│ Manuscript                  (the active view)            │
│ Beat Sheet                                               │
│ Canon    │                                               │
│ Character Bible                                          │
│ Plot Threads                                             │
│ Idea Park│                                               │
│ Research │                                               │
│ ───────  │                                               │
│ Settings │                                               │
└──────────┴──────────────────────────────────────────────┘
```

**Title bar (top).** App name in the center. On the right: a **Focus** toggle
and a **theme switcher** (Light / Dark / System).

**Sidebar (left).** Your project name at the top, with two buttons beside it: a
**Reveal in Finder** button (opens the project folder) and a collapse/expand
chevron. Below are the seven views plus Settings in the footer.

**Theme.** Set Light, Dark, or System from the title bar at any time. System
follows your macOS appearance.

**Focus mode.** Click **Focus** in the title bar or press **⌘.** to hide the
sidebar and chrome for distraction-free writing. Press it again to bring the
sidebar back.

---

## 4. Projects

A project is one book. It owns its own canon index, beat sheet, scenes, voice
pins, character bible, ideas, plot threads, and Git history.

- **Create / open** — done from the project picker on launch. A single project
  opens automatically.
- **Reveal in Finder** — the folder button next to the project name in the
  sidebar opens the project directory in Finder. Handy for backups or poking at
  the raw Markdown.
- **Where it lives** — `~/Library/Application Support/Quill/projects/<id>/`. See
  [§17](#17-where-your-work-lives) for the full layout.

---

## 5. Settings & cloud LLMs

Open **Settings** from the sidebar footer.

### Appearance

- **Prose font** — the font used in the writing pane: **Charter** (serif) or
  **JetBrains Mono**.

### Drafting

- **Default generation mode** — Scene / Paragraph / Sentence. A stored preference
  for the scope you tend to work at.

### LLM providers

Quill ships three providers behind one interface:

| Provider | Use | Embeddings? | Notes |
| --- | --- | --- | --- |
| **Mock** | offline, deterministic | yes (term-bag) | Echoes prompts, hash-based vectors. No real generation. Default and safe. |
| **Gemini** | drafting + embeddings | yes (real semantic) | Free tier trains on your inputs. Get a key at aistudio.google.com. |
| **Groq** (Llama 3.3 70B) | drafting fallback | no | No embeddings endpoint — pair with Gemini or Mock for embeddings. |

To go live:

1. Set **Chat provider** (Mock / Gemini / Groq) and **Embeddings provider**
   (Mock / Gemini).
2. For each cloud provider, click **Set key**, paste your API key
   (`AIza…` for Gemini, `gsk_…` for Groq), and Save. Keys are encrypted at rest
   (Argon2id + AES-256-GCM) under `~/Library/Application Support/Quill/secrets/`.
3. Click **Ping** to verify the key actually works. The reply shows inline; the
   ping is recorded in the audit log.

Switching providers takes effect on the **next call** — no restart. Leave
everything on **Mock** to write and ingest entirely offline; canon retrieval and
voice drift both work without any cloud call.

### Privacy

- **Show "what gets sent" preview** — when on (the default), every AI call first
  shows you the assembled payload and waits for your confirmation. When off,
  calls send immediately.
- **Free-tier disclosure** — a one-time acknowledgement that the Gemini free tier
  trains on your inputs. Click **I understand the free-tier tradeoffs** to clear
  it.

### Audit log

An append-only record of every cloud call: operation, provider, model, token
counts, success/error, and the **categories** of content sent — never the content
itself. Choose how many recent entries to show, refresh, and **Reveal** the
`audit.jsonl` file in Finder.

### About

App version, current phase, and your data directory path.

---

## 6. Research — your voice fingerprint

The **Research** view is where you teach Quill the voice you're aiming for.

### Reference pins

Click **New pin**, give it a memorable label (e.g. _"Eragon ch1 — Saphira
hatching"_), and paste a 200–800 word passage. Build 3–10 pins from books whose
voice matches your target — _Eragon_, _Percy Jackson_, early _Harry Potter_,
_The Wingfeather Saga_. **Don't blend genres**; stay in YA fantasy.

Each pin card lets you:

- **Weight** it (0–10) — heavier pins pull the fingerprint harder.
- **Enable / disable** it (the power icon) — disabled pins don't count.
- **Delete** it.
- Expand to read the full passage.

The **fingerprint** is the word-count-weighted centroid of all enabled pins.
It's recomputed live on every drift check, so editing a pin updates everything
downstream immediately.

### Voice drift tester

Once you have at least one pin, a tester appears at the bottom. Paste any
candidate passage and click **Compare to fingerprint**. You get:

- **Overall drift** — a 0–100% score plus the cosine similarity. Under ~15% feels
  on-voice; over ~35% is a clear shift worth investigating.
- **Top deltas** — the specific features that diverged most (sentence length,
  dialogue ratio, punctuation cadence, etc.), as z-scores against the fingerprint.

This same machinery powers the live drift gauge in the editor and the drift gate
on AI drafts.

---

## 7. Canon — worldbuilding & your Obsidian vault

The **Canon** view is your worldbuilding index — everything the AI is allowed to
pull from when drafting. The header shows how many chunks are indexed.

### Connecting your Obsidian vault (recommended)

Under **Obsidian vault**:

1. Click **Pick vault…** and choose your vault directory (or any folder of
   Markdown).
2. Click **Start watching**. From then on, files you save in that directory are
   re-ingested automatically. New files are picked up too.
3. The status line shows it live: events received, files re-ingested, and the
   last change with a relative timestamp.

**Deletions are intentionally ignored.** Obsidian saves atomically by writing a
temporary file and renaming it, so acting on "removed" events would risk
destroying data. To clean up after files you've genuinely deleted, use the
**corpus inspector's** prune control (below).

**Auto-resume.** If a project has a vault path and watching was on, Quill
silently restarts the watcher when you open the project. If the folder moved or
permissions changed, it fails quietly — just re-pick the path in this view.

### Manual ingest

Set the **New ingest defaults** (Kind + Sensitivity), then click **Ingest file**
in the header and pick a `.md`, `.markdown`, `.txt`, or `.pdf`. Quill extracts the
text, splits it into ~400–800-word chunks (respecting Markdown headings as
breakpoints), embeds each chunk, and stores it.

- **Kind** — character, location, faction, magic, history, cosmology, timeline,
  lore (default), plot notes, DM session notes, other. Location and Cosmology
  kinds are what the drafting engine pulls for **setting** matches (see
  [§14](#14-drafting-with-ai)).
- Re-ingesting the same file path **replaces** the old chunks cleanly — use this
  to refresh a note you edited.

### Searching canon

Under **Search canon**, type a natural-language query ("the dragon's lair",
"House Vell rivalries"), choose how many results (`k`), and search. Results are
ranked by cosine similarity and show their heading trail, score, word count, and
sensitivity badge. The **Respect do-not-send** checkbox (on by default) excludes
protected chunks the same way the AI does.

> **PDF note:** the PDF extractor is heuristic. Tables, multi-column layouts, and
> scanned PDFs degrade. Prefer Markdown sources where you can.

> **Mock embeddings note:** with the embeddings provider on Mock, recall is
> term-bag-shaped, not semantic. A banner reminds you. Switch to Gemini
> embeddings for real semantic retrieval.

---

## 8. Privacy rules & sensitivity

This is the safety net between your unstructured vault and a cloud LLM. Read it
carefully — once something hits a free-tier model's training set, you can't pull
it back.

### The three tiers

| Tier | Meaning | Sent to cloud? |
| --- | --- | --- |
| **Public** | Background lore, public world details. | Yes |
| **Spoiler** | Reveals you'll share with the LLM but want flagged. | Yes (flagged) |
| **Do-not-send** | DM notes, twists, private journals. | **Never** |

`do_not_send` is enforced at the **retrieval layer**: protected chunks are
excluded from any search whose results would feed an LLM call.

### Folder rules (Canon → Privacy rules)

Map a folder name or path prefix to a tier. Example: a rule with pattern
`DM-Notes` → `do_not_send` protects every file inside any folder named
`DM-Notes`, anywhere in your vault. Add as many rules as you need, set the
**default for unmatched files**, and click **Save rules**.

Saving **retroactively re-tags** every existing chunk whose source path matches —
no preview, no confirm dialog. The status line tells you how many chunks changed.

### Frontmatter override

A note's own YAML frontmatter wins over folder rules. Put this at the top of any
Markdown file to lock it down regardless of where it lives:

```yaml
---
quill-sensitivity: do_not_send
---
```

### Priority order

1. The note's frontmatter (`quill-sensitivity:`).
2. The first matching folder rule.
3. The project default for unmatched files.

### The cloud-enable banner

If your vault is connected, a cloud provider is selected, **and** you have no
rules with the default still set to Public, Quill shows an amber banner across the
Manuscript header: _"Your vault is auto-syncing as Public."_ Click **Configure
rules** to jump straight to the rules editor. The banner disappears once you add a
rule or change the default off Public.

### Corpus inspector (Canon → Indexed documents)

Audit and clean up everything in your index:

- See every document with its word count, chunk count, and sensitivity. A
  **mixed** badge means a document's chunks have different tags.
- **Filter** by sensitivity, or show **missing only** (documents whose source
  file no longer exists on disk).
- **Bulk re-tag** — select documents, pick a target tier, **Apply**. Great for
  locking down a batch before turning on a cloud provider.
- **Prune missing** — delete index chunks for files you've removed from your vault
  (this is how you reconcile the watcher's ignore-deletions behavior).
- **Delete** a single document's chunks, or **Reveal** its source in Finder.

---

## 9. Beat Sheet

The **Beat Sheet** view holds the 15 Save the Cat beats. The header shows how
many are satisfied and your target word count.

- **Target manuscript length** — sets the % position of every beat. Default is
  appropriate for standalone YA (80–100k). Edit it and click out to save.
- **Each beat** shows its canonical label, target percentage, approximate target
  word position, and a description. Edit the **summary** inline (click out of the
  box to save), and toggle:
  - **Mark done** — you've hit this beat (turns the card green).
  - **Lock** — freezes this beat's content during generation.
- **Freeze** (header) — locks the whole sheet so you can't accidentally edit it
  mid-draft. Unfreeze to make changes again.
- **Import outline** — paste an existing outline; Quill detects Save the Cat beat
  labels and routes the surrounding text into the right slots. **Preview** dry-runs
  the match (showing matched beats and leftovers) before you **Apply**.

The beat you assign to a scene becomes part of that scene's drafting context.

---

## 10. Character Bible

The **Character Bible** view is one card per character, with a cross-link panel on
the right.

Each character carries:

- **Name** and **Aliases** (comma-separated) — both are matched when finding
  cross-links.
- **Role** — Protagonist, Antagonist, Mentor, Ally, Love Interest, Family, Foil,
  Supporting, Minor.
- **Arc one-liner**, **Motivation**, **Voice notes**.
- **Secrets** — with its own `do_not_send` toggle (**on by default**). Keep it on
  so plot twists never cross the network.

**Cross-links.** Select a character and the right panel lists every scene and
canon chunk that mentions them by name or alias (case-insensitive). This is your
consistency audit — see every appearance at a glance. Add more aliases if a
character is under-matched.

**POV auto-injection.** When a scene's **POV** field names a character that exists
in the Bible, that character's entry is automatically added to the drafting
context (see [§14](#14-drafting-with-ai)).

---

## 11. Plot Threads

The **Plot Threads** view tracks recurring arcs that must close by the book's end
— a buried grudge, a magic-system implication, a promise the narrator made.

Each thread has a **title**, a **description** (what's at stake, when it was
introduced, when it must close), and a **status**:

- **Open** / **Advancing** — active. These are injected into **every** AI draft so
  the model knows what's in motion.
- **Resolved** / **Abandoned** — kept for reference but excluded from AI context.

Threads are linked to individual scenes from the Manuscript scene-metadata strip
(see [§13](#13-manuscript--writing-scenes)). In the drafting prompt, threads tagged
on the active scene are marked `[linked]` so the model knows which are immediately
relevant.

---

## 12. Idea Park

The **Idea Park** is a capture-fast scratchpad for stray thoughts. Type into the
box at the top, optionally add comma-separated tags, and click **Add** (or press
**⌘↩**). Newest ideas sort first; tag chips along the top filter the list.

Each idea card lets you edit the text and tags inline (auto-saved), toggle
**do-not-send** (the eye icon — keeps the idea local, never sent to AI), **Copy**
the text to the clipboard, or delete it.

### Special tags that feed drafting

Tag an idea with one of these to have it auto-injected into the relevant AI draft:

| Tag | Surfaces the idea when… |
| --- | --- |
| `beat:catalyst` | drafting a scene linked to that beat |
| `pov:kaelan` | drafting a scene whose POV matches |
| `scene:<id>` | drafting that specific scene |

This is how a "what if" note you jot today shows up exactly when you're writing
the scene it belongs to — without you having to remember it.

---

## 13. Manuscript — writing scenes

The **Manuscript** view is where you actually write. It has a scene rail on the
left, the editor in the center, and (when open) the drafting panel on the right.

### Scene rail

- **Progress card** (top) — total words vs. target with a progress bar, and how
  many of the 15 beats your scenes touch.
- **Status filter** — chips for Outlined / Drafting / Drafted / Revised / Locked.
  Toggle to hide/show scenes by status. (Clearing all re-enables everything.)
- **Scene list** — each row shows order number, title, and word count. Click to
  open. **Drag to reorder** (the new order saves automatically). Hover to reveal
  the delete button. **New** creates a scene (you'll be prompted for a title).

### Scene metadata strip

A compact row above the editor for the fields you flip often, without leaving your
flow:

- **POV** — who's narrating (e.g. _"Kaelan, 3rd-limited"_). Commits on blur. When
  it matches a Bible character, that character feeds the draft context.
- **Setting** — where/when (e.g. _"The Hollow Wastes, dusk"_). Commits on blur.
  Used to pull Location/Cosmology canon into drafts.
- **Status** — Outlined / Drafting / Drafted / Revised / Locked (commits
  immediately).
- **Beat** — link the scene to one of the 15 beats (commits immediately).
- **Threads** — chips below the row. Click a linked thread to unlink it, or use
  **+ link…** to attach an Open/Advancing thread.
- **Reveal in Finder** — the folder icon opens the scene's `.md` file.

### The editor

A clean writing surface in your chosen prose font. The footer shows live **word**
and **character** counts and the **voice-drift indicator**:

- _"Pin reference passages in Research to enable voice drift."_ — no fingerprint
  yet.
- _"Drift available after 30 words."_ — keep writing.
- **Voice: on voice / drifting / off voice** with a 0–100 score. Green below 45,
  amber 45–69, red 70+. Hover for the cosine similarity.

**Autosave** fires 800 ms after you stop typing. Every save writes the Markdown
file, mirrors the word count back to the beat sheet, makes a Git commit, and
re-checks drift. The save indicator in the header shows Editing… / Saving / Saved.

### Today's words

A **+N today** badge in the header tracks your net word change since the start of
the day. Hover to see yesterday's total. It updates after each save.

### Manuscript search

Press **⌘F** to open a search bar over the whole manuscript. Results show the
scene, line number, and a snippet; **Enter** jumps to the first match, click any
result to open that scene, **Esc** closes.

### Moving between scenes

- **⌘N** — new scene.
- **⌘[** / **⌘]** — previous / next visible scene (respects the status filter).

---

## 14. Drafting with AI

With a scene open, click **Draft** in the header to open the side panel. It mirrors
the same disciplines the backend enforces — nothing is sent without your say-so,
and nothing is written to disk by the model.

### The three operations

| Operation | What it does | Needs a selection? |
| --- | --- | --- |
| **Continue** | Picks up where the scene ends and pushes toward the active beat. | No |
| **Rewrite** | Replaces the selected passage with a tighter version in your voice. | Yes |
| **Critique** | Returns notes (voice, pacing, continuity) — no prose to insert. | Yes |

Pick an operation, type an **instruction**, and (for Rewrite/Critique) select a
passage in the editor first.

### What gets assembled

The orchestrator reads the **saved** scene text from disk (so what you saved is
what's sent) and assembles a context-bounded prompt from:

- the **scene card** (POV, setting, status) and the **linked beat**;
- the **POV character's** Bible entry, when the POV matches a character;
- **setting canon** — Location/Cosmology chunks matched to the scene's setting;
- matching **Idea Park** entries (via `beat:` / `pov:` / `scene:` tags);
- **Open/Advancing plot threads** (scene-linked ones marked `[linked]`);
- the **top canon chunks** by similarity (`do_not_send` always excluded);
- your **voice anchors** (top reference pins).

### Preview and send

If **"what gets sent"** is on (Settings → Privacy), the primary button reads
**Preview what gets sent**. The preview shows context chips (POV, setting matches,
ideas, threads), the full assembled messages (expand to read every line), and the
canon/voice counts. Then **Send**. With previews off, the button sends directly.

### The drift gate

Before sending, the preview reports the scene's current voice drift. If it's
**≥ 70**, the gate **blocks** the call: extending already-drifted prose usually
compounds the drift. The recommended move is to pause, re-read your pinned
references, fix a few lines by hand, and try again. If you really mean it, tick
**Override the drift gate this once** — the override is recorded in the audit log.

### Acting on the result

The suggestion appears with token counts and the model name. For **Continue** and
**Rewrite**, click **Review changes** to enter the track-changes view
([§15](#15-reviewing-changes-track-changes)). **Critique** is read-only — integrate
the notes by hand. You can also **Retry** or **Discard**. The model never writes to
your scene; only you do, by applying a review.

Every call is logged to `audit.jsonl` (categories + token counts, never content).

---

## 15. Reviewing changes (track-changes)

When you click **Review changes**, the editor switches to an inline diff between
your current scene and the proposed version, sentence by sentence.

- **Removed** text shows struck through in red; **added** text shows highlighted in
  green. Unchanged prose is left alone.
- Each changed chunk has inline **✓ accept** / **✗ reject** controls. Changes start
  **pending** (amber highlight) until you decide.
- Header controls: **Accept all**, **Reject all**, **Reset** (back to pending), and
  the live count of accepted vs. pending.
- **Apply** writes your accepted result into the scene (which then autosaves and
  commits). **Cancel** discards the whole suggestion.

Anything still **pending** when you hit Apply is treated as **rejected** — the safe
default, so you never silently accept something you didn't look at.

---

## 16. Compile & export

Click **Compile** in the Manuscript header to stitch every scene, in rail order,
into a single Markdown file. Choose a save location in the dialog; Quill writes the
file and reports how many scenes and words it compiled. The status line clears
after a few seconds.

Because the manuscript is already plain Markdown on disk, compile is just a
convenient single-file export — you can hand the result to any converter (e.g.
pandoc) for `.docx`, `.epub`, or PDF.

---

## 17. Where your work lives

Everything is a plain file under `~/Library/Application Support/Quill/`:

```
~/Library/Application Support/Quill/
├─ projects/
│  └─ <project_id>/
│     ├─ project.json              # metadata + vault path/rules
│     ├─ manuscript/               # one .md per scene, order-prefixed
│     │  ├─ 0000-front-matter.md
│     │  └─ 0001-<scene-id>.md
│     ├─ structure/
│     │  ├─ beat_sheet.json
│     │  └─ scenes.json            # titles, POV, setting, status, beats, threads
│     ├─ canon/                    # ingested originals (kept for re-ingest)
│     ├─ voice/
│     │  └─ pins.json              # reference passages
│     ├─ bible/
│     │  └─ characters.json
│     ├─ ideas/
│     │  └─ ideas.json
│     ├─ threads/                  # plot threads
│     └─ .git/                     # auto-commit history (per project)
├─ vectors.json                    # embedded canon chunks (keyed per project)
├─ secrets/                        # encrypted API keys (Argon2id + AES-256-GCM)
├─ audit.jsonl                     # log of every cloud LLM call (no content)
└─ settings.json
```

The **Reveal in Finder** buttons (sidebar, scene strip, corpus inspector, audit
log) open the relevant path directly.

---

## 18. Backups & version history

**Every save is a Git commit** inside the project folder, so you have full history
and one-command rollback.

Roll a scene back to an earlier version:

```bash
cd ~/Library/Application\ Support/Quill/projects/<project_id>
git log --oneline                       # find the commit you want
git checkout <commit> -- manuscript/<scene-file>
```

Quill picks up the change on next reload.

**Backups.** Either zip `~/Library/Application Support/Quill/` periodically, or —
since each project is its own Git repo — add a private remote and `git push`. The
auto-commit history goes with it.

> Git auto-commit is best-effort. If `git` isn't on your PATH, saves still
> succeed but no commit is made (a warning is logged). Install the Xcode Command
> Line Tools (`xcode-select --install`) to get `git`.

---

## 19. Keyboard shortcuts

| Shortcut | Action | Where |
| --- | --- | --- |
| **⌘.** | Toggle focus mode | Anywhere |
| **⌘N** | New scene | Manuscript |
| **⌘[** | Previous visible scene | Manuscript |
| **⌘]** | Next visible scene | Manuscript |
| **⌘F** | Search the manuscript | Manuscript |
| **Esc** | Close search | Manuscript search |
| **Enter** | Jump to first search result | Manuscript search |
| **⌘↩** | Save a captured idea | Idea Park |

In-field keys: **Enter** commits POV/Setting in the scene strip; clicking out of a
beat summary or pin weight saves it.

---

## 20. Troubleshooting

**"Quill.app is damaged" / "developer cannot be verified."**
Gatekeeper quarantine, not a broken app. Run `xattr -cr /Applications/Quill.app`
and open it again. (Goes away once the app is notarized with a Developer ID.)

**The AI panel says "Mock provider."**
You're offline-safe but not generating real prose. Set a real Chat provider in
Settings → LLM providers, add a key, and Ping it.

**My draft is blocked by the drift gate.**
The scene's voice score is ≥ 70. Re-read your pinned references, fix a few lines by
hand to pull the voice back, and retry. Override only if you mean it — it's logged.

**Canon search returns nothing useful.**
If the embeddings provider is on **Mock**, recall is term-bag-shaped. Switch to
Gemini embeddings for real semantic search, then re-ingest if needed.

**I deleted a note in Obsidian but it's still in canon.**
The watcher ignores deletions on purpose (Obsidian's atomic saves would otherwise
risk data loss). Open Canon → Indexed documents → **Missing only** → **Prune
missing**, or delete the document directly.

**The amber "auto-syncing as Public" banner won't go away.**
Your vault is connected, a cloud provider is on, you have no privacy rules, and the
default is Public. Add at least one folder rule or change the default off Public
(Canon → Privacy rules).

**Saves seem to fail.**
Likely a permissions problem on `~/Library/Application Support/Quill/`. Check
ownership; if you ever ran the app as root, `chown -R $(whoami)` the directory.

**The drift gauge never appears.**
You need at least one enabled reference pin in Research and ~30 words in the scene.

**My voice fingerprint feels off.**
Disable or re-weight pins in Research, or drop any that don't match your target
tone. The fingerprint is just the weighted centroid of enabled pins.

---

_Quill is a single-user tool you built for one job: finishing this book. When in
doubt, trust the loop — canon and voice and beats choose the target, the model
renders a candidate, the drift gate guards the gate, and you decide what stays._
