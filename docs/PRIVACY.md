# Privacy

## Principles

1. **Your manuscript is yours.** It lives on your disk, in plain Markdown, and you can walk away from Quill at any time without losing access to a single word.
2. **Your canon never leaves your machine by default.** Worldbuilding, character notes, lore — local only.
3. **Cloud calls are explicit, narrow, and logged.** Every byte sent to a cloud LLM is recorded in an audit log you can inspect.
4. **No telemetry.** Quill never phones home. There is no analytics SDK, no crash reporter pinging a server, no usage metrics.

## What stays local, always

- The manuscript files in `~/Library/Application Support/Quill/projects/<id>/manuscript/`
- All canon documents (PDFs, Markdown)
- Vector embeddings and the LanceDB index
- The voice fingerprint
- The corrections log
- The structure beat sheet and scene cards
- The character bible, idea park, and research notes
- The local Git history of all of the above

## What gets sent to the cloud (hobby phase, free Gemini)

When you trigger a generation or a critique pass, the app sends to Google Gemini's API:

| Sent | Purpose |
|---|---|
| The current scene's prompt context | The scaffolding the LLM needs |
| Top-K (default 5) retrieved canon chunks | Worldbuilding grounding |
| The most recent 3 paragraphs of your prose | Voice continuity |
| 3–8 reference-pin exemplars (from your shelf passages) | Voice conditioning |
| The structural beat description for the scene | Plot direction |

What the app **never** sends automatically:
- The full manuscript
- The full canon corpus
- Any document you have flagged `do-not-send`
- Anything from the audit log itself
- API keys (other than to authenticate the request)

## Free-tier disclosure

Google's free Gemini tier **trains on your inputs.** This means scene snippets and retrieved canon chunks you send during the hobby phase may end up in future Google training data.

For an exploratory hobby project this is generally acceptable. Before any of the following, you should switch to a paid tier (Claude API, GPT-5 API, paid Gemini) which contractually does **not** train on your data:

- Querying literary agents
- Submitting to publishers
- Self-publishing on KDP or elsewhere
- Sharing chapters with beta readers in a public forum

Switching providers is a one-line config change in Settings. The app supports paid Gemini, Claude, and GPT-5 as drop-in alternatives.

## "What gets sent" preview

Before any cloud call, the app can show you exactly what's about to be transmitted. This preview is on by default for the first week and toggleable thereafter.

## Audit log

Every cloud request is appended to `~/Library/Application Support/Quill/audit.log` as JSON-lines:

```json
{"ts":"2026-05-22T16:14:03Z","provider":"gemini","model":"gemini-2.5-pro","operation":"scene_draft","tokens_in":2415,"tokens_out":1820,"included":["scene_card","canon_top5","recent_3p","ref_pins"],"project_id":"...","scene_id":"..."}
```

The log is local-only. Inspect it any time. Settings → Privacy → "Open audit log."

## Per-document `do-not-send`

Any canon entry can be flagged `do-not-send`. Retrieval skips it across all generation paths. Use this for:

- Late-book spoilers you don't want leaking into early-chapter context
- Sensitive personal material that may have crept into your DM notes
- Anything you simply prefer to keep local-only

## Local encryption

API keys and any explicitly-flagged sensitive blobs are sealed with AES-256-GCM, with the key derived from a passphrase (or system keychain on macOS) via Argon2id (m=64 MiB, t=3, p=1). Bulk manuscript and canon content rely on macOS FileVault for at-rest protection, which we assume is enabled.

## No third parties

Quill ships with exactly these external dependencies:

- Google Gemini API (configurable, optional per-call)
- Groq API (configurable, optional fallback)
- GitHub (only for app updates via Tauri auto-updater, signature-verified)

That's it. No analytics. No advertising SDKs. No social login. No newsletter signup.
