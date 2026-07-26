# MCRE — Agent Guide

## Project

MCRE is an experimental Minecraft Java Edition client in Rust. Targets the version in `mc-version` (currently 26.3-snapshot-5).

## Toolchain

- **Rust 1.97.0** (pinned in `rust-toolchain.toml`), edition 2024, resolver `"3"`
- **`just` recipes** provide all dev commands. Nix flake provides an **optional** dev shell (JDK 25, clang, etc.).

## Workspace

```
crates/           # library crates (9)
  mcre_core/      # primitives (no_std)
  mcre_data/      # block/state data structs
  mcre_world/     # world model (no_std)
  mcre_assets/    # asset extraction (no_std except test)
  mcje/           # JVM bridge (JNI)
  mcje_macros/    # proc macros: #[mcje::main], #[mcje::test]
  mcje_downloader/# MC jar/lib downloader
  mcre_static_data_gen/ # generates Rust source from JSON (build-dep of mcre_world)
  core_io/        # std::io re-export (no_std shim)
tasks/            # binary tasks (1)
  data_gen/       # extracts block/state JSON from MC jar via JNI
```

## Commands

| Action           | Command                                                                        |
|------------------|--------------------------------------------------------------------------------|
| List all         | `just --list`                                                                  |
| Check (build)    | `cargo ck` (includes `--locked`)                                              |
| Test all         | `cargo test --workspace --all-features`                                        |
| Test one crate   | `just test -- -p <crate>`                                                     |
| Lint (clippy)    | `cargo lint -- -D warnings`                                                    |
| Format           | `cargo fmt` / `cargo fmt --check`                                              |
| Spell check      | `typos`                                                                        |
| Docs             | `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items`      |
| Full CI          | `just ci`                                                                      |
| Pre-commit check | `just ready` (alias `just r`)                                                  |
| Auto-fix         | `just fix` (alias `just f`; clippy --fix + fmt + typos -w)                    |
| Install hook     | `just install-hook`                                                            |

Aliases defined in `.cargo/config.toml`: `ck` → `check --workspace --all-features --all-targets --locked`, `lint` → `clippy --workspace --all-targets --all-features`.

## Build Requirements

- **JDK 25** required (temurin). Set `JAVA_HOME` if not using the Nix dev shell.
- **Network access** required at build time — Minecraft jar + deps downloaded from Mojang servers.
- First build is slow (large downloads).
- `cargo-shear` ignores `mcre_assets`, `mcre_static_data`, `core_io` (build-only deps).

## Data Generation Pipeline

Two-step process to regenerate game data from the MC jar:

1. `cargo run -r -p data_gen` — extracts JSON to `mcre_data/`
2. Rust source in `mcre_world` is auto-generated at compile time via `build.rs` (uses `mcre_static_data_gen` as a build dependency)

Step 1 is manual; step 2 is automatic. Run step 1 whenever `mc-version` changes.

**Never manually edit generated files** (`mcre_data/` — treat them as read-only). Update the generator crate instead.

## Version Bump

To target a new Minecraft version:

1. Update `mc-version` with the target version string
2. Generate fresh data: `cargo run -r -p data_gen`
3. Run `cargo fmt` and review the diff
4. Update generator crates (`data_gen`, `mcre_static_data_gen`) if the new version changed any data format

The build script in `mcre_world` generates Rust source from `mcre_data/` JSON automatically at compile time — no separate codegen step needed.

Or use the automated command: `just bump-version` (handles steps 1-3 automatically).

## Architecture Notes

- **`no_std` crates**: `mcre_core`, `mcre_world` (and `mcre_assets` except under `cfg(test)`). Import from `core_io` instead of `std::io` for stdio in no_std contexts.
- **JNI bridge**: `mcje` crate embeds a JVM via JNI. `#[mcje::main]` wraps `async fn main(env: &mut JNIEnv)` with JVM init + bootstrap.
- **Event-driven world model** documented in `docs/world-model.md`.

## Style Conventions

- Do NOT add comments unless asked.
- No `rustfmt.toml` — uses default rustfmt settings.
