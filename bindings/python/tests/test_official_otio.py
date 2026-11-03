"""The .otio files this library writes must load in the official OpenTimelineIO.

Each timeline is written with ``Timeline.to_json`` and read back with
``opentimelineio``'s ``otio_json`` adapter. The test is skipped when
``opentimelineio`` is not installed, unless ``REQUIRE_OFFICIAL_OTIO=1`` is set
(as CI does), in which case a missing package is a failure.
"""

import os
from pathlib import Path

import pytest

from tellers_timeline import (
    Clip,
    Gap,
    Item,
    MediaReference,
    MediaReferenceCrop,
    MediaReferencePosition,
    Timeline,
    Track,
    Transition,
)

if os.environ.get("REQUIRE_OFFICIAL_OTIO") == "1":
    import opentimelineio as otio
else:
    otio = pytest.importorskip("opentimelineio")

REPO = Path(__file__).resolve().parents[3]
FILES = sorted(
    [*(REPO / "spec" / "examples").glob("*.json"), *(REPO / "tellers-timeline-core" / "tests" / "fixtures").glob("*.otio")]
)


def _objects(tl):
    """Every serializable object under ``tl``: items plus their effects, markers and media references."""
    items = [tl, tl.tracks, *tl.tracks.find_children()]
    for item in items:
        yield item
        yield from getattr(item, "effects", [])
        yield from getattr(item, "markers", [])
        if isinstance(item, otio.schema.Clip):
            yield from item.media_references().values()


def _official(out: str):
    """Parse ``out`` with the official library and check every object is a known schema."""
    tl = otio.adapters.read_from_string(out, "otio_json")
    assert isinstance(tl, otio.schema.Timeline)
    unknown = [o for o in _objects(tl) if getattr(o, "is_unknown_schema", False)]
    assert not unknown, f"official OTIO did not recognise: {[o.data for o in unknown]}"
    # Exercises time math on every item (durations, ranges, transitions).
    tl.duration()
    for track in tl.tracks:
        track.range_of_all_children()
    # The official library can write it back out again.
    otio.adapters.write_to_string(tl, "otio_json")
    return tl


def _clip(duration: float, item_id: str, url: str = "file:///media/a.mov") -> Clip:
    return Clip(duration, {"DEFAULT_MEDIA": MediaReference(url)}, id=item_id)


def _rich_timeline() -> Timeline:
    """A timeline using most features the library writes: gaps, transitions,
    text and shape generators, speed, position/crop, effects, composites,
    markers, fonts, disabled items and clip groups."""
    tl = Timeline()

    a = _clip(3.0, "a")
    a.set_position(MediaReferencePosition(x=0.1, y=-0.1, rotation=15.0, zoom_x=1.2, zoom_y=1.2))
    a.set_crop(MediaReferenceCrop(crop_left=0.1, crop_right=0.05, crop_top=0.0, crop_bottom=0.2))
    assert a.push_tellers_effect({"effect_name": "Blur", "parameters": {}})

    b = _clip(2.0, "b", "https://cdn.example.test/b.mp4")
    b.set_speed(2.0)

    c = _clip(2.0, "c")
    c.set_enabled(False)

    video = Track(
        kind="video",
        id="v1",
        children=[
            Item.from_clip(a),
            Item.from_transition(Transition(0.5, 0.5, id="t1")),
            Item.from_clip(b),
            Item.from_gap(Gap(1.0, id="g1")),
            Item.from_clip(c),
        ],
    )

    text = Clip.new_text(2.0, {"text": "Hello", "style": {"fontFamily": "Brand Sans", "fontSize": 64}}, name="Title", id="txt")
    shape = Clip.new_shape(2.0, {"shape": "rectangle", "fill": "#00ff00", "corner_radius": 12}, id="shape")
    overlay = Track(kind="video", id="v2", children=[Item.from_clip(text), Item.from_clip(shape)])

    inner = Track(kind="video", children=[Item.from_clip(_clip(2.0, "inner-clip"))])
    composite = Track(kind="video", id="v3", children=[Item.from_composite([inner], name="Lower third", id="comp")])

    audio = Track(kind="audio", id="a1", children=[Item.from_clip(_clip(5.0, "music", "file:///media/music.wav"))])

    for track in (audio, video, overlay, composite):
        tl.add_track(track)

    tl.add_font("Brand Sans", "https://cdn.example.test/brand-sans.woff2")
    assert tl.add_marker(1.0, name="beat", comment="drop", color="RED")
    tl.create_clip_group(["txt", "shape"])
    return tl


def test_rich_timeline_parses_in_official_otio():
    tl = _rich_timeline()
    assert not tl.validate()
    official = _official(tl.to_json())

    # We write "video"/"audio"; official OTIO's TrackKind constants are
    # "Video"/"Audio" and the kind is a free string, so compare without case.
    kinds = sorted(t.kind.lower() for t in official.tracks)
    assert kinds == ["audio", "video", "video", "video"]

    clips = list(official.find_clips())
    assert any(isinstance(c.media_reference, otio.schema.GeneratorReference) for c in clips)
    assert any(not c.enabled for c in clips)
    assert list(official.find_children(descended_from_type=otio.schema.Transition))
    assert any(isinstance(e, otio.schema.LinearTimeWarp) for c in clips for e in c.effects)
    assert official.tracks.markers or any(t.markers for t in official.tracks)


@pytest.mark.parametrize("path", FILES, ids=lambda p: p.name)
def test_rewritten_fixture_parses_in_official_otio(path: Path):
    tl = Timeline.parse_json(path.read_text())
    _official(tl.to_json())
