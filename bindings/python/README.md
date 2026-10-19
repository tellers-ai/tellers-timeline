# tellers-timeline

An OpenTimelineIO (OTIO) timeline engine for building editors and automating edits,
powered by Rust and PyO3. Read and write timelines, insert with explicit policies,
move and resize synced clips, sanitize documents, and work with effects, markers,
and stable IDs. Full OTIO coverage and portable effect/ID conventions are the
project's goals; see the repository's feature table for current support.

```sh
pip install tellers-timeline
```

```python
from tellers_timeline import Timeline

timeline = Timeline()
print(timeline.to_json())
```

Prebuilt wheels are built for Linux x86-64, macOS Intel and Apple Silicon, and
Windows x86-64. Building from the source distribution requires Rust and Cargo.

See the [repository](https://github.com/tellers-ai/tellers-timeline) for the
supported timeline operations and development instructions.
