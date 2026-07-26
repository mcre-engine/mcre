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

# Decompile Minecraft client jar to Java source code (output in target/mc-src/)
[unix]
src version=`cat mc-version`:
  #!/usr/bin/env bash
  set -euo pipefail

  VERSION="{{version}}"
  echo "==> Decompiling Minecraft $VERSION"

  # Resolve client jar URL from Mojang version manifest
  echo "==> Fetching version manifest..."
  MANIFEST_URL=$(curl -sSf https://piston-meta.mojang.com/mc/game/version_manifest_v2.json | jq -r --arg v "$VERSION" '.versions[] | select(.id == $v) | .url // empty')
  if [ -z "$MANIFEST_URL" ]; then
    echo "Error: Version '$VERSION' not found in Mojang manifest" >&2
    exit 1
  fi

  CLIENT_URL=$(curl -sSf "$MANIFEST_URL" | jq -r '.downloads.client.url // empty')
  if [ -z "$CLIENT_URL" ]; then
    echo "Error: No client download for version '$VERSION'" >&2
    exit 1
  fi

  # Cache paths (target/ is gitignored)
  CLIENT_JAR="target/mc-client/$VERSION.jar"
  VINEFLOWER_JAR="target/tools/vineflower-1.12.0.jar"
  OUTPUT_DIR="target/mc-src/$VERSION"

  # Download client jar
  if [ ! -f "$CLIENT_JAR" ]; then
    echo "==> Downloading client jar..."
    mkdir -p "$(dirname "$CLIENT_JAR")"
    curl -# -o "$CLIENT_JAR" "$CLIENT_URL"
  else
    echo "==> Client jar already cached at $CLIENT_JAR"
  fi

  # Download Vineflower CLI (v1.12.0, requires Java 17+)
  if [ ! -f "$VINEFLOWER_JAR" ]; then
    echo "==> Downloading Vineflower 1.12.0..."
    mkdir -p "$(dirname "$VINEFLOWER_JAR")"
    curl -# -Lo "$VINEFLOWER_JAR" "https://repo1.maven.org/maven2/org/vineflower/vineflower/1.12.0/vineflower-1.12.0.jar"
  else
    echo "==> Vineflower already cached"
  fi

  # Decompile
  echo "==> Decompiling to $OUTPUT_DIR ..."
  mkdir -p "$OUTPUT_DIR"
  java -jar "$VINEFLOWER_JAR" "$CLIENT_JAR" "$OUTPUT_DIR"

  FILE_COUNT=$(find "$OUTPUT_DIR" -name '*.java' | wc -l | tr -d ' ')
  echo ""
  echo "Done — $FILE_COUNT Java files decompiled to $OUTPUT_DIR"

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
