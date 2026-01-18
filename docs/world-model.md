# World Model Architecture

This document describes the world model used by MCRE. It focuses only on how the world is represented, mutated, and observed. Rendering, input, networking, and gameplay logic are intentionally excluded.

The goal of this model is to support:

* static worlds loaded from files
* server-driven worlds
* explicit chunk loading and unloading
* deterministic behavior suitable for testing

All examples are written in Rust-like code. Types and APIs are illustrative and may evolve.

## Design goals

The world model is built around a few constraints:

* The world has a single source of truth
* All mutations happen through explicit events
* Systems that use the world do not own it
* Chunk unloading is explicit, not implicit
* The model must work identically for file-backed and server-backed worlds

This leads naturally to an event-driven model with clear separation of responsibilities.

## Core concepts

At a high level there are four concepts:

1. WorldEvent: a description of a change to the world
2. World: the authoritative world state
3. WorldView: read-only access to the world
4. WorldSource / WorldConsumer: producers and observers of world events

Only the World owns mutable state.

## WorldEvent

A WorldEvent represents a fact about the world changing. Events are applied in order and are the only way the world mutates.

```rust
pub enum WorldEvent {
    ChunkLoaded {
        pos: ChunkPos,
        chunk: ChunkData,
    },
    ChunkUnloaded {
        pos: ChunkPos,
    },
    BlockSet {
        pos: BlockPos,
        state: BlockState,
    },
}
```

Important properties:

* Events are explicit and self-contained
* Absence of data is never implied
* Unloading is represented directly

There is no hidden behavior like distance-based eviction inside the world itself.

## World (authoritative state)

The World struct owns all mutable world state.

```rust
pub struct World {
    chunks: HashMap<ChunkPos, Chunk>,
}
```

The World exposes a single mutation entry point:

```rust
impl World {
    pub fn apply(&mut self, event: WorldEvent) {
        match event {
            WorldEvent::ChunkLoaded { pos, chunk } => {
                self.chunks.insert(pos, Chunk::from(chunk));
            }
            WorldEvent::ChunkUnloaded { pos } => {
                self.chunks.remove(&pos);
            }
            WorldEvent::BlockSet { pos, state } => {
                if let Some(chunk) = self.chunks.get_mut(&pos.chunk()) {
                    chunk.set_block(pos.local(), state);
                }
            }
        }
    }
}
```

The World does not:

* load data
* unload data on its own
* notify observers
* know about rendering

It only enforces invariants and applies events.

## WorldView

Consumers often need to inspect world state. Direct mutable access is not allowed.

WorldView is a read-only facade over World.

```rust
pub struct WorldView<'a> {
    world: &'a World,
}

impl<'a> WorldView<'a> {
    pub fn get_chunk(&self, pos: ChunkPos) -> Option<&Chunk> {
        self.world.chunks.get(&pos)
    }

    pub fn get_block(&self, pos: BlockPos) -> Option<BlockState> {
        self.world
            .chunks
            .get(&pos.chunk())
            .map(|c| c.get_block(pos.local()))
    }
}
```

WorldView:

* is cheap to create
* enforces read-only access at the type level
* allows consumers to gather context when reacting to events

---

## WorldSource

A WorldSource produces WorldEvents. It does not mutate the world directly.

```rust
pub trait WorldSource {
    fn poll_event(&mut self) -> Option<WorldEvent>;
}
```

Examples of sources:

* a region file reader
* a test fixture
* a network protocol reader

The source decides *what* events exist and *when* unloading happens.

## WorldConsumer

A WorldConsumer reacts to world events. It never mutates the world.

```rust
pub trait WorldConsumer {
    fn on_event(&mut self, event: &WorldEvent, view: &WorldView);
}
```

Examples of consumers:

* renderer
* bot perception system
* debug logger

Consumers may cache data locally but must treat the world as external truth.

## Driving the world

A typical update loop looks like this:

```rust
fn update(
    world: &mut World,
    source: &mut impl WorldSource,
    consumers: &mut [Box<dyn WorldConsumer>],
) {
    while let Some(event) = source.poll_event() {
        world.apply(event.clone());

        let view = WorldView { world };
        for consumer in consumers.iter_mut() {
            consumer.on_event(&event, &view);
        }
    }
}
```

This keeps ordering explicit and deterministic.

## Chunk loading and unloading

Chunk unloading is handled entirely through events:

```rust
WorldEvent::ChunkUnloaded { pos }
```

There is no concept of a chunk disappearing implicitly. If a consumer still references a chunk after this event, that is a bug in the consumer.

This mirrors server-driven behavior and allows:

* precise memory management
* deterministic replay
* clean separation of policy and state

## Why event-driven

This model mirrors how the Minecraft protocol works conceptually:

* the server sends facts
* the client applies them

By using the same structure internally, file-backed and server-backed worlds share the same code paths.

It also avoids common pitfalls:

* renderers owning world data
* implicit lifetime assumptions
* hidden side effects during iteration

## What this model does not do

Intentionally omitted:

* player input
* prediction
* entities
* lighting
* ticking or simulation

Those systems sit on top of the world model and interact with it through events.

## Summary

* World is a concrete struct and the single authority
* All mutations go through WorldEvent
* WorldSource produces events
* WorldConsumer reacts to events
* WorldView provides read-only access
* Chunk loading and unloading are explicit

This model is simple, restrictive, and boring by design. That is what allows it to scale without rewrites later.
