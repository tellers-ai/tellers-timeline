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
Tracks form groups. A *video group* is a video track (the owner) with zero or
more audio tracks; an *audio group* is audio tracks only, the highest one
owning the list. The owner stores, in
`metadata["tellers.ai"]["associated_track_ids"]`, the ordered tracks its sync
partners go to; a track belongs to one group. Tracks with no stored list derive
it from their sync clips (Resolve imports), and the first column inserted stores
it. A track nobody lists and that lists nothing is *free* and can be adopted by
any group.

Placement of a column's partners, in order: on a horizontal move or replace the
partner's own track; the first listed track that is free over the column (the
owner itself counts for audio groups); the nearest free unlisted track; on a
move onto a track with no partners, the source track; otherwise a new track
created past the existing partners and appended to the list.

Rules: a column with a video clip is inserted on a video track (its primary is
the video); a plain audio clip or an audio-only column never lands on a video
group's audio track; moving an audio partner onto a sibling track swaps it with
the channel there (refused when the slot holds anything else); moving it onto
another video group's track moves the whole column; audio groups and video
groups never mix. Deleting a track drops only its channel from the columns.
Empty tracks are kept. Syncing clips by hand gathers them into one group,
adopting free tracks and moving clips off other groups onto new partner tracks.

`normalize_track_order` is the only call that reorders tracks: each group
becomes contiguous, owner on top with its partners right below in list order.
Call it once at the end of a request; a manual `reorder_track` inside a group
updates the list so the layout survives normalization.

API: `Track.get/set/clear_associated_track_ids`,
`Stack.associated_track_ids(track_id)`, `Stack.associate_tracks(track_id, ids)`,
`Stack.free_audio_track_for(start, duration)`, `Stack.normalize_track_order()`,
`Stack.insert_item_at_time_by_id(...)`, in Rust and Python.
`tools/audit_associated_tracks.py` reports what these rules see in a set of
project documents.

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
