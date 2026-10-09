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

#### Associated tracks (sync partners)
A clip inserted with sync partners (a video with its audio channels, or an
audio clip with other audio clips synced to it) lands as a column. The track
receiving the primary clip stores, in `metadata["tellers.ai"]["associated_track_ids"]`,
the ordered list of tracks its partners go to: partner 1 to the first listed
track, partner 2 to the next, and so on. A listed track that is busy over the
column is skipped; a free track nobody lists is used before any track is
created; otherwise a new track is created right past the existing partners and
appended to the list. Listed tracks are reserved for their primary: a column
from another track never borrows them, and `free_audio_track_for` steers plain
audio clips to unlisted tracks first. Tracks with no stored list derive it from
their sync clips (Resolve imports), and the first column inserted stores it.

`Track.get_associated_track_ids` / `set_associated_track_ids`,
`Stack.associated_track_ids(track_id)`, `Stack.associate_tracks(track_id, ids)`
and `Stack.free_audio_track_for(start, duration)` expose this in Rust and Python.

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
