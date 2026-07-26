#!/usr/bin/env -S just --justfile

set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]
set shell := ["bash", "-cu"]

_default:
  @just --list -u

# ==================== ALIASES ====================
alias r := ready
alias f := fix

# ==================== CORE DEVELOPMENT ====================

# Run all CI checks
ci:
  typos
  cargo ck
  cargo test --workspace --all-features
  cargo lint -- -D warnings
  git diff --exit-code

# Run pre-commit checks (strict)
ready:
  git diff --exit-code --quiet
  typos
  cargo fmt
  cargo ck
  cargo test --all-features
  cargo lint -- -D warnings
  RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items
  git status

# Run cargo check
check *args:
  cargo ck {{args}}

# Run tests
test *args:
  cargo test --workspace --all-features {{args}}

# Lint with clippy
[unix]
lint *args:
  cargo lint -- -D warnings {{args}}

[windows]
lint *args:
  $Env:CARGO_BUILD_WARNINGS='deny'; cargo lint -- -D warnings {{args}}

# Format all Rust files
fmt:
  cargo fmt

[unix]
doc *args:
  RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --document-private-items {{args}}

[windows]
doc *args:
  $Env:RUSTDOCFLAGS='-D warnings'; cargo doc --no-deps --document-private-items {{args}}

# Fix auto-fixable issues
fix:
  cargo clippy --fix --allow-staged --no-deps
  just fmt
  typos -w
  git status

# Install git pre-commit hook
install-hook:
  echo -e "#!/bin/sh\njust ready" > .git/hooks/pre-commit
  chmod +x .git/hooks/pre-commit

# ==================== DATA GENERATION ====================

# Generate block/state data from the Minecraft jar
data-gen:
  cargo run -r -p data_gen

# Bump target Minecraft version to the latest release
[unix]
bump-version:
  #!/usr/bin/env bash
  set -euo pipefail
  LATEST=$(curl -s https://piston-meta.mojang.com/mc/game/version_manifest_v2.json | jq -r '.versions.[0].id')
  echo "$LATEST" > mc-version
  if ! git diff --quiet -- mc-version; then
    cargo r -r -p data_gen
    rm -rf crates/mcre_world/src/data
    cargo r -r -p world_data_gen
    cargo fmt
  else
    echo "Already at latest version ($LATEST)"
  fi
