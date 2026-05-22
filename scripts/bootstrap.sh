#!/usr/bin/env bash
# ============================================================
# Quill — first-time developer bootstrap
# ============================================================
# Verifies toolchain, installs dependencies, copies env template.
# Idempotent — safe to re-run.
# ============================================================
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

bold() { printf "\033[1m%s\033[0m\n" "$*"; }
ok()   { printf "  \033[32m✓\033[0m %s\n" "$*"; }
warn() { printf "  \033[33m!\033[0m %s\n" "$*"; }
fail() { printf "  \033[31m✗\033[0m %s\n" "$*"; exit 1; }

bold "Quill bootstrap"

# --- macOS ---
if [[ "$(uname -s)" != "Darwin" ]]; then
  fail "Quill targets macOS only (detected $(uname -s))."
fi
ok "macOS"

# --- Xcode CLT ---
if ! xcode-select -p >/dev/null 2>&1; then
  fail "Xcode Command Line Tools missing. Run: xcode-select --install"
fi
ok "Xcode Command Line Tools"

# --- Rust ---
if ! command -v rustc >/dev/null 2>&1; then
  fail "Rust missing. Install: https://rustup.rs"
fi
ok "Rust $(rustc --version | awk '{print $2}')"

# --- Node ---
if ! command -v node >/dev/null 2>&1; then
  fail "Node missing. Install Node 20+: https://nodejs.org"
fi
node_major=$(node --version | sed 's/v//' | cut -d. -f1)
if (( node_major < 20 )); then
  fail "Node $node_major detected; need 20+."
fi
ok "Node $(node --version)"

# --- pnpm ---
if ! command -v pnpm >/dev/null 2>&1; then
  warn "pnpm missing; installing via corepack"
  corepack enable
  corepack prepare pnpm@latest --activate
fi
ok "pnpm $(pnpm --version)"

# --- .env ---
if [[ ! -f .env ]]; then
  cp .env.example .env
  ok ".env created from template (edit as needed)"
else
  ok ".env exists"
fi

# --- frontend deps ---
bold "Installing frontend dependencies"
( cd apps/desktop && pnpm install )
ok "frontend deps installed"

# --- pre-fetch Rust crates (best effort, doesn't fail the script) ---
bold "Pre-fetching Rust crates"
( cd apps/desktop/src-tauri && cargo fetch ) || warn "cargo fetch failed (non-fatal)"

bold "Done."
echo
echo "Next steps:"
echo "  cd apps/desktop"
echo "  pnpm tauri dev"
