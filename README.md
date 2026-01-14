# <div align="center">MCRE</div>

<div align="center">

[![MIT licensed][license-badge]][license-url]
[![Build Status][ci-badge]][ci-url]

</div>

**Minecraft Rust Edition** is an experiment to build a modular, 1:1 Minecraft **client** in Rust.

## 🎯 Goals

These describe what the project is aiming for, not guarantees or a roadmap.

* **Drop-In Client Replacement**
  MCRE is intended to behave like the Minecraft Java Edition client, without requiring changes to existing servers, resource packs, or player workflows.

* **Native Resource Pack Support**
  Standard Minecraft texture packs and resource packs should load and work as-is, with no conversion steps or special tooling.

* **Shader Compatibility via Transpilation**
  The client will attempt to support existing Minecraft shader packs by transpiling them to WGSL at runtime.

  * This is experimental and explicitly best-effort.
  * Some shaders may render incorrectly or not at all, depending on complexity and hardware limits.

* **Vanilla Parity**
  The goal is functional equivalence with Minecraft Java Edition: mechanics, rendering behavior, and gameplay semantics should match as closely as practical.

  <sub>Reproducing every historical bug is very intentionally out of scope.</sub>

* **Protocol Compatibility**
  The client should be able to speak the vanilla Minecraft network protocol without extensions or custom patches.

## 🏗️ Design Principles

MCRE is early and experimental. These are constraints on *how* the code is written, not promises about what already exists.

They exist to keep the project technically honest while things are still flexible.

### Architecture & Portability

* **`no_std` where it makes sense**
  Core crates should avoid OS dependencies unless they are genuinely required. If `std` isn’t pulling its weight, it shouldn’t be there.

* **Platform-agnostic by default**
  Core game logic should not care whether it’s running on desktop, web, or something more exotic.
  Platform-specific concerns (filesystem, networking backends, windowing) live behind traits.

  *Example*: a storage abstraction that maps to [`std::fs`](https://doc.rust-lang.org/stable/std/fs/index.html) on desktop and [OPFS](https://developer.mozilla.org/en-US/docs/Web/API/File_System_API/Origin_private_file_system) on the web.

* **Modular, not monolithic**
  The project is split into small, focused crates. You should be able to use the protocol layer or core game logic without pulling in rendering, audio, or windowing.

### Performance & Ergonomics

* **Performance matters early**
  Rust is a deliberate choice. The code should avoid accidental overhead and prefer simple, predictable data layouts.
  Clarity still beats micro-optimizations until profiling proves otherwise.

* **Readable code beats clever code**
  APIs should be boring, explicit, and easy to reason about. If something is hard to understand, that’s a design problem.
  Documentation and comments are treated as part of the architecture.

* **Extensibility isn’t an afterthought**
  The original Minecraft client is famously rigid. MCRE tries to avoid baking in that inflexibility from the start.
  Clean boundaries, hooks, and replaceable systems are preferred, even if they add some upfront complexity.

## 📖 License

MCRE is open-source software released under the [MIT License](./LICENSE).

[license-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[license-url]: https://github.com/mcre-project/mcre/blob/main/LICENSE
[ci-badge]: https://github.com/mcre-project/mcre/actions/workflows/ci.yml/badge.svg?event=push&branch=main
[ci-url]: https://github.com/mcre-project/mcre/actions/workflows/ci.yml?query=event%3Apush+branch%3Amain
