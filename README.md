### tellers-timeline

Open-source, cross-language library for reading, writing, validating, and editing a simplified subset of OTIO (OpenTimelineIO) JSON files.

- Core: Rust (`tellers-timeline-core`)
- Schema: Rust (`tellers-timeline-schema`) exporting JSON Schema with `schemars`
- Bindings: Python (package `tellers-timeline`, import `tellers_timeline`), JS/Wasm (npm package `tellers-timeline`), Flutter/Dart (native crate under `bindings/flutter/tellers_timeline`)

#### Quick start
- Build everything: `just build-all`
- Run tests: `just test-all`
- Regenerate schema: `just regen-schema` (writes to `spec/otio.schema.json`)

#### Subset implemented
- Timeline, Tracks, Clips, Gaps, MediaSource, Metadata
- Time values are seconds (`f64`)
- IDs are optional UUIDs (may be omitted/null for portability)
- Metadata is `serde_json::Value` with typed getters/setters for `clip_id`

See `spec/examples` for golden JSON examples.
