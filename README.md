### tellers-timeline

Open-source, cross-language library for reading, writing, validating, and editing a simplified subset of OTIO (OpenTimelineIO) JSON files.

- Core: Rust (`tellers-timeline-core`)
- Schema: Rust (`tellers-timeline-schema`) exporting JSON Schema with `schemars`
- Bindings: Python (package `tellers-timeline`, import `tellers_timeline`), JS/Wasm (npm package `tellers-timeline`), Flutter/Dart (native crate under `bindings/flutter/tellers_timeline`)
  - Flutter via flutter_rust_bridge (unified native+web): crate `bindings/flutter_bridge`

#### Quick start
- Build everything: `just build-all`
- Run tests: `just test-all`
- Regenerate schema: `just regen-schema` (writes to `spec/otio.schema.json`)

#### Flutter (native + web) via flutter_rust_bridge
- Rust crate: `bindings/flutter_bridge`
- Exposed FRB functions (JSON-in/out for MVP):
  - `tt_parse_json(json: String) -> Result<String, String>`
  - `tt_validate_json(json: String) -> Vec<String>`
  - `tt_sanitize_json(json: String) -> Result<String, String>`
  - `tt_insert_clip(json_timeline: String, args_json: String) -> Result<String, String>`

Setup outline (from your Flutter app):
- `dart pub add flutter_rust_bridge`
- Codegen: `dart run flutter_rust_bridge_codegen --rust-input <repo>/bindings/flutter_bridge/src/lib.rs --dart-output <flutter_app>/lib/bridge_generated.dart --wasm`
- Build native lib: `cargo build -p tellers-timeline-flutter-bridge`
- Build web wasm: `rustup target add wasm32-unknown-unknown` then `cargo build -p tellers-timeline-flutter-bridge --target wasm32-unknown-unknown`

#### Subset implemented
- Timeline, Tracks, Clips, Gaps, MediaSource, Metadata
- Time values are seconds (`f64`)
- IDs are optional UUIDs (may be omitted/null for portability)
- Metadata is `serde_json::Value` with typed getters/setters for `clip_id`

See `spec/examples` for golden JSON examples.
