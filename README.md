# Quill — A Domain-Tuned YA Fantasy Writing Companion

> A persistent, opinionated writing partner that learns your voice, holds your worldbuilding canon as ground truth, drives drafting through a Save-the-Cat structural skeleton, and produces an 85,000-word standalone YA epic fantasy at professional quality.

**This is not a ChatGPT wrapper.** The LLM is one component among five: canon retrieval, voice fingerprinting, beat tracking, structural editing, and prose generation.

---

## Status

Currently at the end of **Phase 5 MVP**. The app boots, lets you create projects,
ingest canon, manage a 15-beat sheet, pin reference passages for voice modeling,
and draft scenes in a real editor with autosave + Git auto-commit + a live voice
drift indicator. Cloud LLM calls work but are ungated by chat UX yet (see Phase 6).

See [`docs/PRD.md`](docs/PRD.md) for the full plan and [`HANDOFF.md`](HANDOFF.md)
for the migration guide to the destination machine.

| Phase                          | Status                                              |
| ------------------------------ | --------------------------------------------------- |
| 0. Foundation                  | ✅ complete                                         |
| 1. Canon ingestion             | ✅ complete                                         |
| 2. LLM provider layer          | ✅ complete                                         |
| 3. Structural engine           | ✅ complete                                         |
| 4. Voice fingerprint           | ✅ complete                                         |
| 5. Drafting modes (MVP)        | ✅ complete (basic editor + autosave + drift gauge) |
| 6. Revision loop               | ⏳ pending                                          |
| 7. Second brain                | ⏳ pending                                          |
| 8. Distribution                | ⏳ pending                                          |
| 9. Local embeddings (optional) | ⏳ pending                                          |

---

## Target platform

- **macOS 14+ on Apple Silicon** (MacBook Pro M4, 16 GB RAM)
- Distribution: code-signed + notarized `.dmg` with Apple Developer ID
- Auto-update via Tauri updater

---

## Tech stack (locked in PRD v0.1)

| Layer                  | Choice                                                       | Why                                                                                            |
| ---------------------- | ------------------------------------------------------------ | ---------------------------------------------------------------------------------------------- |
| Desktop shell          | Tauri 2.x (Rust + WebView)                                   | Small binary, native feel, no Electron tax                                                     |
| UI                     | React 19 + TypeScript + Tailwind 4 + shadcn primitives       | Mature ecosystem, hobby-friendly                                                               |
| Editor                 | Lexical (Meta)                                               | Extensible, supports track-changes natively                                                    |
| State                  | Zustand                                                      | Minimal, escape-hatch friendly                                                                 |
| Vector store           | LanceDB (embedded)                                           | Rust-native, no daemon, fast                                                                   |
| LLM (hobby phase)      | Google Gemini 2.5 Pro free tier; Groq Llama 3.3 70B fallback | Best free quality 2026; pluggable for paid Claude/GPT later                                    |
| Embeddings (v1)        | Gemini Embedding API                                         | Phase 9 swaps to local `bge-m3` via `candle-transformers`                                      |
| Voice fingerprint      | Custom Rust pipeline                                         | Sentence rhythm, POS, function-words, dialogue-tag patterns                                    |
| Storage                | SQLite (metadata) + plain Markdown (manuscript)              | You're never trapped in a proprietary format                                                   |
| Encryption at rest     | Argon2id KDF + AES-256-GCM                                   | Modern, vetted                                                                                 |
| Vector store (current) | JSON-backed embedded store                                   | Phase 1 ships with a lightweight, brute-force cosine store; LanceDB swap is a Phase 9 task     |
| Versioning             | Local Git auto-commit (system `git`)                         | Full history, one-command rollback. Plain shell-out is more stable than gitoxide at this scale |

---

## Repo layout

```
writing-assistant/
├─ apps/desktop/          # Tauri app (Rust core + React UI)
├─ docs/                  # PRD, architecture, prompts, privacy
├─ scripts/               # signing, bootstrapping, icons
├─ tests/                 # Rust + e2e
└─ .github/workflows/     # CI
```

See [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) for setup.

---

## Privacy

- Canon, manuscript, voice fingerprint, and corrections all live locally in `~/Library/Application Support/Quill/`.
- During hobby phase, drafted text is sent to Google Gemini's free tier (which trains on free-tier inputs). See [`docs/PRIVACY.md`](docs/PRIVACY.md) for the full disclosure and switch-to-paid plan.
- Per-document `do-not-send` flag is honored across all retrieval and generation.
- Every cloud call is logged to a local audit trail.

---

## License

TBD (private project for now).
