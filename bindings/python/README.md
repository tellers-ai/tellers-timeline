# tellers-timeline

Python bindings for reading, writing, validating, and editing a simplified subset
of OpenTimelineIO (OTIO) JSON. The implementation uses Rust and PyO3.

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
