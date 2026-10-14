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
Which tracks a clip's sync partners go to is computed from the sync clips
(the Resolve `Link Group ID` on clips), never stored: the saved document is
plain OTIO. Every sync column has an owner, its video track, or for an
audio-only column the member track holding the most clips (highest on a tie).
The owner's *associated tracks* are the tracks holding the other clips of its
columns, nearest first; a track belongs to one owner (the one it shares the
most columns with), and a track holding no sync clip is *free*. A *video
group* is a video track with its audio tracks; an *audio group* is audio
tracks only.

Placement of a column's partners, in order: on a horizontal move or replace
the partner's own track; the nearest associated track that is free over the
column (the owner itself counts for audio groups); the nearest free track in
no group; on a move onto a track with no partners, the source track; otherwise
a new track created past the existing partners (and the slots already used
by the column), so slot order is also stack order once normalized.

Rules: a column with a video clip is inserted on a video track (its primary is
the video); a plain audio clip or an audio-only column never lands on a video
group's audio track (a track with no sync clip is free and always accepts
them); the audio partner of a video column never leaves its video's group: it
moves in time, or onto a sibling track where it swaps with the channel there
(refused when the slot holds anything else), and a drop on any other track is
refused; the video clip moves the whole column to another video track; audio
groups and video groups never mix. Deleting a track drops only its channel.
Resizes mirror on the partners what happened on the primary track, and a
clip whose head is trimmed keeps its id. Syncing clips by hand gathers them
into one group, moving clips off other groups onto new partner tracks.

`normalize_track_order` is the only call that reorders tracks: each group
becomes contiguous, owner on top with its partners right below in slot order.
`prune_empty_partner_tracks` removes the auto-numbered audio tracks the
library created once they hold no clip. Both are separate steps the host runs
when it wants, e.g. once per request or on save.

API: `Stack.associated_track_ids(track_id)`, `Stack.track_groups()` (every
group as one object: owner, kind, partners in slot order; tracks in no group
are groups of one), `Stack.free_audio_track_for(start, duration)`,
`Stack.normalize_track_order()`, `Stack.prune_empty_partner_tracks()`,
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
