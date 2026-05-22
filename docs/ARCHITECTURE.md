# Architecture

## Process model

Quill is a single-process Tauri 2.x app. There is no daemon, no companion service, no Python sidecar.

```
┌───────────────────────────────────────────────────────┐
│                     macOS process                     │
│ ┌─────────────────────┐   ┌─────────────────────────┐ │
│ │  WebView (WKWebKit) │◀─▶│  Rust core (tokio rt)   │ │
│ │   React + TS UI     │   │  services / commands    │ │
│ └─────────────────────┘   └─────────────────────────┘ │
│            │                        │                 │
│            └─── Tauri IPC commands ─┘                 │
└───────────────────────────────────────────────────────┘
                         │
                         ▼
              ~/Library/Application Support/Quill/
                ├─ projects/<id>/...
                ├─ settings.json
                ├─ secrets.enc        (AES-GCM, Argon2id KDF)
                └─ audit.log
```

## Module boundaries (Rust core)

| Module | Responsibility | Phase introduced |
|---|---|---|
| `commands` | Tauri command handlers (the IPC surface). Thin glue, zero business logic. | 0 |
| `services::storage` | Project filesystem layout, JSON metadata, Markdown manuscript files. | 0 |
| `services::crypto` | Argon2id KDF, AES-256-GCM seal/open for sensitive blobs. | 0 |
| `services::git` | Local Git auto-commit using `gix` (gitoxide). | 0 |
| `services::canon` | PDF + Markdown ingest, semantic chunking, Obsidian vault watcher. | 1 |
| `services::vector` | LanceDB integration for embeddings retrieval. | 1 |
| `services::llm` | Provider trait + Gemini, Groq implementations. Token accounting. Audit log. | 2 |
| `services::structure` | Save-the-Cat 15-beat model, Story Grid scene cards, beat health scoring. | 3 |
| `services::voice` | Voice feature extractor, fingerprint store, drift detector. | 4 |
| `prompts` | Versioned prompt templates with explicit input/output schemas. | 2 |

**Cross-cutting:**
- `models/` — serde data structures shared across modules
- `error.rs` — single `QuillError` enum; commands return `Result<T, QuillError>` which serializes to typed TS errors
- `config.rs` — runtime configuration (paths, feature flags)
- `telemetry.rs` — local-only audit log (no external telemetry, ever)

## IPC surface (Tauri commands)

The frontend talks to Rust exclusively through typed Tauri commands. No raw `fetch`, no shelling out, no plugin escapes.

Command naming: `<domain>_<verb>` — e.g., `project_create`, `settings_get`, `theme_set`.

Phase 0 commands:

```
project_create(name: string) -> Project
project_list() -> Project[]
project_open(id: string) -> Project
settings_get() -> Settings
settings_update(patch: Partial<Settings>) -> Settings
theme_set(theme: 'light' | 'dark' | 'system') -> void
secret_set(key: string, value: string) -> void   // sealed via AES-GCM
secret_get(key: string) -> string | null
git_commit(project_id: string, message?: string) -> CommitInfo
```

## Storage layout

```
~/Library/Application Support/Quill/
├─ settings.json                  # non-sensitive app settings
├─ secrets.enc                    # AES-GCM sealed; Argon2id-derived key
├─ audit.log                      # JSON-lines, append-only
└─ projects/
   └─ <project_id>/               # one folder per book
      ├─ project.json             # metadata: name, created, last opened
      ├─ manuscript/              # Markdown files, the book itself
      │   ├─ 00-front-matter.md
      │   ├─ 01-chapter-01.md
      │   └─ ...
      ├─ canon/                   # ingested PDFs + Markdown copies
      ├─ structure/
      │   ├─ beats.json           # Save-the-Cat 15 beats
      │   └─ scenes.json          # Story Grid scene cards
      ├─ bible/
      │   ├─ characters.json
      │   ├─ locations.json
      │   └─ lore.json
      ├─ voice/
      │   ├─ fingerprint.json
      │   ├─ pins.json            # reference passages
      │   └─ corrections.jsonl
      ├─ vectors/                 # LanceDB tables (Phase 1+)
      └─ .git/                    # local-only Git history
```

**Why plain Markdown for manuscript:** the user is never trapped. They can quit Quill forever and open every chapter in any text editor. This is a non-negotiable principle.

## Encryption boundaries

| Data | Encryption | Key derivation |
|---|---|---|
| Manuscript Markdown | None at rest (Apple FileVault assumed) | n/a |
| Project metadata | None at rest | n/a |
| Canon documents | None at rest | n/a |
| Vector embeddings | None at rest | n/a |
| **Secrets** (API keys, optional passphrase content) | **AES-256-GCM** | **Argon2id**, m=64 MiB, t=3, p=1; salt per-secret |

Rationale: bulk content relies on FileVault (macOS-default disk encryption) for at-rest protection. Only API keys and explicitly sensitive blobs go through the AES-GCM seal. This keeps the system fast and the manuscript files portable.

## Threading model

- Tauri command handlers are `async` and run on the tokio runtime.
- Long operations (PDF ingest, embedding calls, LLM calls) emit progress via Tauri events to the UI.
- The UI never blocks on a command — every long op has a cancel path.
