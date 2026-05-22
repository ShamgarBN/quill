# Development setup

## Prerequisites

| Tool                     | Version | Notes                                                                   |
| ------------------------ | ------- | ----------------------------------------------------------------------- |
| macOS                    | 14+     | Apple Silicon                                                           |
| Xcode Command Line Tools | latest  | `xcode-select --install`                                                |
| Rust                     | 1.78+   | install via [rustup](https://rustup.rs)                                 |
| Node.js                  | 20+     | install via [fnm](https://github.com/Schniz/fnm) or `brew install node` |
| pnpm                     | 9+      | `npm install -g pnpm`                                                   |
| Tauri CLI                | 2.x     | installed automatically via `cargo` on first build                      |

## First-time setup

```bash
git clone https://github.com/ShamgarBN/writing-assistant.git
cd writing-assistant
./scripts/bootstrap.sh
```

The bootstrap script:

1. Verifies all toolchain dependencies
2. Installs frontend dependencies via `pnpm install`
3. Pre-fetches Rust crates so the first `cargo tauri dev` is fast
4. Copies `.env.example` to `.env` if `.env` doesn't exist

## Running locally

```bash
cd apps/desktop
pnpm tauri dev
```

This launches the dev build of the macOS app with hot-reload on the React side.

## Project layout

```
writing-assistant/
├─ apps/desktop/          # Tauri app (the only deliverable)
│  ├─ src/                # React/TS UI
│  ├─ src-tauri/          # Rust core
│  ├─ package.json
│  └─ vite.config.ts
├─ docs/                  # PRD, architecture, prompts, privacy
├─ scripts/               # bootstrap, signing, icons
├─ tests/                 # Rust unit + e2e
└─ .github/workflows/     # CI
```

## Common commands

```bash
# Frontend dev (rare — usually you want full Tauri)
pnpm --filter desktop dev

# Full Tauri dev build (UI + Rust, with hot-reload)
pnpm --filter desktop tauri dev

# Rust gates
cd apps/desktop/src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test

# Frontend gates
cd apps/desktop
pnpm typecheck
pnpm lint
pnpm format          # write
pnpm format:check    # verify

# Build a release .app bundle (unsigned)
pnpm --filter desktop tauri build
```

## Where user data lives during dev

By default Tauri uses the production app-support path. To keep dev runs isolated:

```bash
QUILL_DATA_DIR=$PWD/.dev-userdata pnpm --filter desktop tauri dev
```

`.dev-userdata/` is gitignored.

## Code style

- Rust: `cargo fmt` (rustfmt defaults), `cargo clippy --all-targets -- -D warnings`
- TypeScript: Prettier (default config — `.prettierignore` excludes
  `src-tauri/`, build outputs, lockfiles), ESLint 9 flat config
  (recommended-ts + react-hooks)
- Both are enforced in CI; do not disable warnings without justification.

## Commit hygiene

- Conventional Commits: `feat: ...`, `fix: ...`, `chore: ...`, `docs: ...`
- One logical change per commit
- All Phase-boundary commits should be tagged: `git tag phase-0-complete`
