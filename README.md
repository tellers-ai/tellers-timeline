### tellers-timeline

Open-source, cross-language library for reading, writing, validating, and editing a simplified subset of OTIO (OpenTimelineIO) JSON files.

#### Quick start
- Build everything: `just build-all`
- Run tests: `just test-all`
- Regenerate schema: `just regen-schema` (writes to `spec/otio.schema.json`)

#### Subset implemented
- Timeline, Tracks, Clips, Gaps, MediaReference, Metadata
- Time values are seconds (`f64`)
- IDs are optional UUIDs (may be omitted/null for portability)

#### Visual test UI
A small browser timeline backed by the Python bindings, for trying the editing
API by hand without any media: drag clips between tracks, resize edges, split,
link/group, insert clips or gaps with the per-track `+` button, add tracks with
`+ Track`. Every action shows the exact library call and result, plus
`validate()` output and the timeline JSON.

- VS Code: press F5 and pick "Timeline test UI (browser)"
- CLI: `just ui` (or `tools/timeline-ui/run.sh`), then open http://127.0.0.1:8765/

The launcher runs `maturin develop` first so the UI always uses the current
Rust code. Use `SKIP_BUILD=1` to skip that step.
