# tellers-timeline

**The timeline engine for tools that make the cut.**

[![PyPI](https://img.shields.io/pypi/v/tellers-timeline?logo=pypi&logoColor=white)](https://pypi.org/project/tellers-timeline/)
[![crates.io](https://img.shields.io/crates/v/tellers-timeline-core?logo=rust&logoColor=white)](https://crates.io/crates/tellers-timeline-core)
[![Rust docs](https://docs.rs/tellers-timeline-core/badge.svg)](https://docs.rs/tellers-timeline-core)
[![CI](https://github.com/tellers-ai/tellers-timeline/actions/workflows/ci.yml/badge.svg)](https://github.com/tellers-ai/tellers-timeline/actions/workflows/ci.yml)

Read timelines. Make edits. Keep everything in sync.

[Install](#install) · [Try it](#make-your-first-edit) · [Features](#what-works-today) · [Standardization](#building-a-shared-language-for-editing) · [Contribute](https://github.com/tellers-ai/tellers-timeline/blob/main/CONTRIBUTING.md)

`tellers-timeline` is an open-source timeline engine, written in Rust and available
in Python, built around [OpenTimelineIO](https://opentimelineio.readthedocs.io/).
Use it to build editors, automate cuts, and exchange timelines between tools.

Our ambition is **full OTIO coverage**, with an editing API that handles the work
around the format: insert and overlap policies, linked audio and video, stable
identity, media bounds, and document cleanup. We're also developing shared
conventions for effects and IDs so more of an edit can travel between applications.

## Install

**Python** — prebuilt wheels for Linux x86-64, macOS Intel and Apple Silicon, and Windows x86-64:

```sh
pip install tellers-timeline
```

**Rust** — add the library to your application:

```sh
cargo add tellers-timeline-core
```

Python imports `tellers_timeline`; Rust imports `tellers_timeline_core`.
Building the Python package from source requires Rust and Cargo.

## Make your first edit

Create a video track, insert a clip at two seconds, and serialize the timeline:

```python
from tellers_timeline import Clip, MediaReference, Stack, Timeline, Track

stack = Stack([Track(kind="video", id="main-video")])
clip = Clip(
    5.0,
    {"DEFAULT_MEDIA": MediaReference("file:///media/shot.mov")},
    id="opening-shot",
)

stack.insert_item_at_time_by_id(
    "main-video",
    2.0,
    clip,
    "override",          # Replace material overlapping the new clip.
    "split_and_insert",  # Split a gap when inserting inside one.
)

timeline = Timeline(stack)
timeline.sanitize()
assert not timeline.validate()
print(timeline.to_json())
```

Editing APIs take time in seconds; OTIO serialization carries `RationalTime`
values and rates. Load an existing document with `Timeline.parse_json(text)`.

In Rust, work directly with the core types and shared effect conventions:

```rust
use tellers_timeline_core::{TellersEffect, Timeline, Track, TrackKind};

let mut timeline = Timeline::default();
timeline.add_track(Track::new(TrackKind::Video, Some("main-video".into())));
timeline.push_tellers_effect(
    TellersEffect::new("ColorLut")
        .with_name("Film look")
        .with_parameter("asset_id", "film-look"),
);
timeline.sanitize();

println!("{}", timeline.to_json().unwrap());
```

## What works today

Full coverage is the goal. This table tracks the current implementation and the
remaining work; it is not a claim of complete OTIO conformance.

| Area | Status | Available today / remaining work |
| --- | --- | --- |
| Timeline structure | 🟡 Partial | Timelines, stacks of audio/video/other tracks, clips, gaps, and enabled state on tracks and items. The root stack's `enabled` is dropped on load; `metadata.tellers_aspect_ratio` is kept as plain metadata, with no typed API. |
| Time and ranges | 🟡 Partial | `RationalTime`, `TimeRange`, rate conversion, and editing in seconds. Broader OTIO time APIs remain. |
| Media references | 🟡 Partial | External and generator references, multiple references per clip, and available ranges. Missing references load as empty external references and are written back as `ExternalReference.1`; image sequences remain. |
| Metadata and serialization | ✅ Supported | Arbitrary JSON metadata, OTIO JSON reading/writing, configurable numeric precision, and JSON Schema generation. |
| Markers | ✅ Supported | Point/range markers, names, comments, colors, IDs, and source-to-timeline time conversion. |
| Effect data | 🟡 Partial | Effect lists on clips, tracks, and stacks; typed tellers effects (`tellers.ai.<Effect>`: filters, transforms, masks, audio, text animations); Resolve parameter parsing; transform and crop helpers; `target_url` resolution from an effect's `asset_id`. Clip groups (`metadata["tellers.ai"].groups` and each member's `group_id`) are typed: read, created, edited and evaluated. Portable effect schemas remain a standardization target. |
| Insert and overlap policies | ✅ Supported | Push or override overlaps; insert before, after, nearest boundary, or split a gap to insert. |
| Timeline editing | ✅ Supported | Move, replace, split, resize, delete, reorder tracks, and find snap targets. |
| Audio/video sync and groups | ✅ Supported | Linked editing, sync partner placement, associated tracks, group discovery, and optional track normalization. |
| Sanitization | ✅ Supported | Clamp clips to available media, repair missing/duplicate track and item IDs, merge gaps, remove zero-length items/trailing gaps, and clean dangling sync links. |
| Validation | 🟡 Partial | Negative item-duration checks. Comprehensive structural and schema validation remains. |
| Rich text and styling | 🟡 Partial | Resolve rich-text generators, typed Text and Shape generator clips, timeline font declarations, and LUT asset/URL helpers. Rich-text HTML cannot be edited after the clip is created. |
| Nested compositions | 🟡 Partial | Composite clips (a `Stack.1` on a track) load, round-trip, and move, split, resize, and delete like clips; their effects are editable. Deeper editing inside nested tracks remains. |
| Transitions and time effects | 🟡 Partial | `Transition.1` items load, round-trip, and are edited (typed kind, direction, colour, offsets, and audio fade curve: `tellers.ai.audio-constant-power` / `tellers.ai.audio-constant-gain`). `TimeWarp`, `LinearTimeWarp.1`, and `FreezeFrame.1` are kept as effect data; speed-aware editing remains. |
| Full OTIO schema coverage | ⬜ Missing | Remaining object types, schema-version handling, and comprehensive interoperability fixtures. |
| Format adapters | ⬜ Missing | Native AAF, EDL, FCPXML, and other format adapters. Use the OTIO ecosystem to convert to/from `.otio`. |
| Language bindings | 🟡 Partial | Rust and typed Python APIs today; additional bindings are future work. |

The library manipulates edit data. Media playback, effect rendering, and export
encoding belong to the application using it.

## Building a shared language for editing

OTIO is the foundation. Alongside broader format support, we want applications to
agree on **what an effect means**, **which object an edit addresses**, and **how
related clips stay together**. These are project conventions and proposals for
wider interoperability; they are not ratified OTIO standards.

| Convention | Implemented today | Standardization direction |
| --- | --- | --- |
| Object identity | `metadata["tellers.ai"]["timeline_id"]`; IDs on tracks, items, and markers; missing/duplicate track and item IDs repaired during sanitization. | Stable identity and clear rules for moves, copies, splits, and imports across tools. IDs are strings, not a UUID requirement. |
| Effect identity and parameters | LUTs use `Effect.1` with `effect_name: "tellers.ai.ColorLut"` and parameters under `metadata["tellers.ai"]["parameters"]`. | A documented, versioned vocabulary for portable effects, parameter units, defaults, and evaluation order. |
| Transform and crop | Read/write helpers currently use Resolve representations (volume is the `tellers.ai.Volume` effect). | Shared effect schemas and mappings between application-specific encodings. |
| Color grading assets | Ordered LUTs on clips, video tracks, and the stack; persistent `parameters.asset_id`, resolvable `parameters.target_url` (named like a media reference's). | Predictable grading stages and a common distinction between asset identity and temporary delivery URLs. |
| Sync and edit groups | Sync links use `Resolve_OTIO["Link Group ID"]`; edit groups use `tellers.ai["Tellers Group ID"]`. Associated tracks are computed. | Portable sync/group semantics without persisting redundant track associations. |
| Text and fonts | `tellers.ai.availableFonts` (fonts linked by URL or uploaded as an asset, with `target_url` resolution); rich-text generator helpers. | Portable typography and text generator conventions. |
| Provenance | `tellers.ai.created_by` on markers and automatically created tracks. | Documented creator/provenance semantics, with room to separate user identity from automatic creation. |

Bring real timelines, editor integrations, and competing representations. The
standardization work should be shaped by interoperability tests and adoption.

## Explore and build

- **[Reading a timeline file](https://github.com/tellers-ai/tellers-timeline/blob/main/docs/reading-otio.md)** — the tree, time, layers, media, effects and `tellers.ai` metadata, with a worked example.
- **[Format reference](https://github.com/tellers-ai/tellers-timeline/blob/main/docs/otio-reference.md)** — every object, field and tellers effect (generated from the code).
- **[Developer guide](https://github.com/tellers-ai/tellers-timeline/blob/main/DEVELOPMENT.md)** — architecture, local setup, editing semantics, visual test UI, and releases.
- **[Contributing](https://github.com/tellers-ai/tellers-timeline/blob/main/CONTRIBUTING.md)** — bug reports, feature proposals, compatibility fixtures, and pull requests.
- **[Rust API](https://docs.rs/tellers-timeline-core)** · **[Python type signatures](https://github.com/tellers-ai/tellers-timeline/blob/main/bindings/python/python/tellers_timeline/__init__.pyi)** · **[Example timelines](https://github.com/tellers-ai/tellers-timeline/tree/main/spec/examples)** · **[JSON Schema](https://github.com/tellers-ai/tellers-timeline/blob/main/spec/otio.schema.json)**

Built by [Tellers](https://tellers.ai) for timelines that can move between tools.
