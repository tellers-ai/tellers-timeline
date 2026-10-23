# Developer guide

## Layout

| Path | Responsibility |
| --- | --- |
| `tellers-timeline-core/` | Rust data model, editing engine, metadata conventions, validation, and serialization. |
| `bindings/python/` | PyO3 bindings, Python type signatures, and binding tests. |
| `tellers-timeline-schema/` | Internal CLIs that generate `spec/otio.schema.json` (`just regen-schema`) and `docs/otio-reference.md` (`just regen-docs`). CI's `docs` job fails when the committed reference is stale (`just check-docs` runs the same check), and its tests fail when a schema field is undocumented. |
| `docs/` | The format reference (generated) and the hand-written guide to reading a timeline; `test_docs_examples.py` runs the guide's worked example and Python snippet. |
| `spec/examples/` | Example documents used by compatibility tests. |
| `tools/timeline-ui/` | Local browser UI for trying editing operations against the Python bindings. |
| `tools/audit_associated_tracks.py` | Inspect computed sync groups in project documents. |

The workspace currently ships a Rust library and Python bindings. There is no
WASM crate in the workspace; older `Justfile` recipes still mention one.

## Local setup

Use a stable Rust toolchain and Python 3.11 to match the release builds. Python
packages declare support for Python 3.8 and newer via PyO3's stable ABI.

```sh
git clone https://github.com/tellers-ai/tellers-timeline.git
cd tellers-timeline
python3.11 -m venv .venv
source .venv/bin/activate
python -m pip install --upgrade pip
python -m pip install 'maturin>=1.4,<2' pytest
maturin develop -m bindings/python/Cargo.toml
```

On Windows, activate with `.venv\Scripts\Activate.ps1` in PowerShell.

## Checks

Run from the repository root:

```sh
cargo build -p tellers-timeline-core -p tellers-timeline-schema
cargo test -p tellers-timeline-core -p tellers-timeline-schema
maturin develop -m bindings/python/Cargo.toml
python -m pytest -q bindings/python/tests
```

For Python, rebuild the extension after changing Rust or bindings. Tests import
that local extension through `conftest.py`. The publishing workflow instead copies
tests outside the source package and tests installed wheels.

Generate the JSON Schema after changing modeled types:

```sh
cargo run -p tellers-timeline-schema --bin tellers-timeline-schema --quiet  # or: just regen-schema
```

Inspect the generated diff before committing. The schema describes the modeled
JSON shape; `validate_timeline` currently checks negative item durations and
effects naming a track no other video track matches, rather than performing
comprehensive schema validation.

## Working with the engine

### Time, identity, and serialization

Public editing calls use seconds (`f64`). `RationalTime` stores `value` and `rate`;
`to_seconds()` and `set_from_seconds()` convert without assuming every document
uses a rate of 1. Track/item/marker identity lives in
`metadata["tellers.ai"]["timeline_id"]`, not a top-level UUID field. Generated IDs
are currently 12-character hexadecimal strings; callers can supply string IDs.

`Timeline::to_json` / Python `Timeline.to_json()` serialize the document.
`to_json_with_options` in Rust and `to_json(precision=..., pretty=...)` in Python
control output precision/format. Arbitrary metadata is retained, but unknown
OTIO object types and unknown fields outside metadata are not universally
preserved. Keep claims of compatibility specific to tested inputs.

### Placement and overlaps

Use track IDs when an operation can create or reorder tracks.

| Policy | Behavior |
| --- | --- |
| `OverlapPolicy::Override` / `"override"` | Replace overlapping material, trimming or splitting existing items as needed. |
| `OverlapPolicy::Push` / `"push"` | Push material to make room. |
| `InsertPolicy::SplitAndInsert` / `"split_and_insert"` | Split a gap around the insertion; inside a clip, fall back to the nearest boundary. |
| `InsertPolicy::InsertBefore` / `"insert_before"` | Align insertion to the containing item's start. |
| `InsertPolicy::InsertAfter` / `"insert_after"` | Align insertion to the containing item's end. |
| `InsertPolicy::InsertBeforeOrAfter` / `"insert_before_or_after"` | Choose the nearest boundary. |

A split policy does not promise to split an arbitrary clip at the requested time.
Use the explicit split API when that is the intended edit. Track-level helpers
report deleted/split clips; stack-level operations propagate relevant changes to
sync partners.

### Sanitization

`Timeline.sanitize()` delegates to the stack. It clamps clips to available media,
clamps negative durations, removes zero-length items, drops a transition that
follows another one or stands alone on its track, merges adjacent gaps, removes
trailing gaps (composite clips' tracks get the same cleanup, keeping their
trailing gaps), ensures track/item IDs are nonempty and unique within the
stack, and cleans dangling sync links. These operations can change a document;
call them deliberately at the host's chosen boundary.

Some editing paths preserve all-gap tracks while operating. Track normalization
and empty auto-track pruning are separate explicit operations.

### Associated tracks and sync partners

Sync partners are computed from clips' `metadata["Resolve_OTIO"]["Link Group ID"]`;
track associations are never persisted. Each sync column has an owner: its video
track, or, for audio-only columns, the track holding the most clips (highest on a
tie). Partners are ordered nearest first. A track belongs to the owner with which
it shares the most columns; a track with no sync clips is free.

Partner placement tries, in order:

1. The partner's own track on a horizontal move or replace.
2. The nearest associated track free over the column (including the owner for
   audio-only groups).
3. The nearest free track in no group.
4. The source track when moving onto a track with no partners.
5. A new track beyond the existing partners and slots already used by the column.

A column containing video is inserted on a video track. Plain audio and audio-only
columns avoid audio tracks owned by a video group. The audio partner of a video
column stays in its video's group: it can move in time, or swap channels with a
sibling track when the destination permits it. Moving the video moves the column;
audio and video groups do not mix. Deleting a track removes only its channel.
Resizing propagates to partners, and trimming a clip's head preserves its ID.
Syncing clips by hand gathers them into one group and creates partner tracks when
needed to move clips out of other groups.

`normalize_track_order()` makes groups contiguous with the owner followed by its
partners. `prune_empty_partner_tracks()` removes empty audio tracks marked
`metadata["tellers.ai"]["created_by"] = "auto"`. Renaming a track clears that mark,
so tracks explicitly named by a user are retained.

Relevant Rust/Python APIs: `associated_track_ids`, `track_groups`,
`free_audio_track_for`, `normalize_track_order`, `prune_empty_partner_tracks`, and
`insert_item_at_time_by_id`.

### Effects and assets

LUTs (`tellers.ai.ColorLut`) and clip volume (`tellers.ai.Volume`) are ordinary
tellers effects, edited with the `*_tellers_effect*` methods like any other.
All their data is in `metadata["tellers.ai"]["parameters"]`.

An effect that loads an asset (a LUT's `.cube`) stores the persistent
`asset_id` and is played from `target_url`, named like a media reference's.
`Timeline.effect_asset_ids` lists the ids to resolve, `set_effect_target_urls`
fills the URLs on every stage before playback, and `clear_target_urls` strips
media and effect URLs before persistence (see `effect_assets.rs`).

Legacy forms are migrated by `sanitize`: LUT keys written beside `parameters`
and the `cube_url` name (`color_grading.rs`), and the Resolve clip volume
(`volume.rs`). Rendering is the consuming application's responsibility. See the
module documentation in `effects.rs`, `fonts.rs`, and `markers.rs` for conventions and compatibility behavior.

## Visual test UI

Try edits without media and inspect the exact API call, validation output, and
serialized document for every action. The UI supports moving/resizing/splitting
clips, grouping/linking, inserting clips or gaps, and adding tracks.

```sh
just ui
# Or:
tools/timeline-ui/run.sh
```

Open <http://127.0.0.1:8765/>. The launcher runs `maturin develop` first; use
`SKIP_BUILD=1` to reuse the current extension. VS Code also has a
`Timeline test UI (browser)` launch configuration.

## Releases

Rust and Python have independent package versions: the Rust library is
`tellers-timeline-core`; the Python distribution is `tellers-timeline`.
The schema generator is a repository development tool.

### Python / PyPI

The [Publish Python package workflow](.github/workflows/publish-python.yml) builds
and tests Linux x86-64, macOS Intel/Apple Silicon, and Windows x86-64 wheels, then
rebuilds the source archive and checks all metadata with Twine.

1. Update the version in both `bindings/python/pyproject.toml` and
   `bindings/python/Cargo.toml`; update `Cargo.lock` as needed.
2. Merge the release changes into `main`.
3. Run the workflow from `main`. Leave publishing unchecked for a build-only run,
   or enable it to upload to PyPI.
4. Check the workflow result and install the published version in a fresh environment.

Publishing uses the GitHub `pypi` environment and PyPI Trusted Publishing, with no
stored API token. For a new project, register a pending publisher at
<https://pypi.org/manage/account/publishing/> using owner `tellers-ai`, repository
`tellers-timeline`, workflow `publish-python.yml`, environment `pypi`. PyPI requires
a verified email and two-factor authentication. The first upload creates the
project. See the [PyPI documentation](https://docs.pypi.org/trusted-publishers/creating-a-project-through-oidc/).

Uploaded filenames cannot be reused. README/metadata changes in the repository
appear on the package index only with a new package upload.

### Rust / crates.io

1. Update `tellers-timeline-core/Cargo.toml` for a new release and refresh the lockfile.
2. Run Rust tests and inspect the packaged files:

   ```sh
   cargo test -p tellers-timeline-core -p tellers-timeline-schema
   cargo package -p tellers-timeline-core --list
   cargo publish -p tellers-timeline-core --dry-run --locked
   ```

3. Merge the release commit into `main`.
4. Authenticate with `cargo login` in your own terminal if needed, then publish:

   ```sh
   cargo publish -p tellers-timeline-core --locked
   ```

5. Check crates.io/docs.rs and verify `cargo add tellers-timeline-core` from a fresh project.

The first release requires a crates.io account with a verified email and a publish
credential. Keep tokens out of source control, logs, and chat. See the
[Cargo publishing guide](https://doc.rust-lang.org/cargo/reference/publishing.html).
