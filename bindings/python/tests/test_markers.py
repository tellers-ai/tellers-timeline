"""Tests for OTIO markers on the stack, tracks and clips.

The API takes and returns timeline seconds; clip markers are stored in the
clip's source time and follow the clip.
"""
import json

from tellers_timeline import MARKER_COLORS, Clip, Gap, Item, MediaReference, Timeline, Track


def make_clip(id):
    ref = MediaReference("file:///test.mp4")
    return Clip(10.0, {"DEFAULT_MEDIA": ref}, "DEFAULT_MEDIA", id=id)


def make_timeline():
    track = Track(
        kind="video",
        id="t1",
        children=[
            Item.from_clip(make_clip("c1")),
            Item.from_gap(Gap(1.0, id="g1")),
            Item.from_clip(make_clip("c2")),
        ],
    )
    return Timeline([track])


def test_add_and_list_markers():
    tl = make_timeline()
    assert tl.add_marker(1.0, name="intro", id="m-stack") == "m-stack"
    assert tl.add_marker(2.0, owner_id="t1", color="GREEN", id="m-track") == "m-track"
    assert (
        tl.add_marker(13.0, name="fix", comment="too dark", owner_id="c2", created_by="u1", id="m-clip")
        == "m-clip"
    )
    assert tl.add_marker(0.0, owner_id="g1") is None
    assert tl.add_marker(0.0, owner_id="missing") is None

    markers = {m["id"]: m for m in tl.get_markers()}
    assert set(markers) == {"m-stack", "m-track", "m-clip"}
    assert markers["m-stack"]["owner_kind"] == "stack"
    assert markers["m-track"]["track_id"] == "t1"
    clip_marker = markers["m-clip"]
    assert clip_marker["owner_kind"] == "clip"
    assert clip_marker["owner_id"] == "c2"
    assert clip_marker["timeline_start"] == 13.0
    assert clip_marker["comment"] == "too dark"
    assert clip_marker["created_by"] == "u1"
    assert clip_marker["color"] == "RED"
    assert clip_marker["visible"] is True


def test_update_and_delete_marker():
    tl = make_timeline()
    marker_id = tl.add_marker(12.0, name="a", owner_id="c2")
    assert tl.update_marker(marker_id, name="b", timeline_start=15.0, duration=1.5)
    marker = tl.get_marker(marker_id)
    assert marker["name"] == "b"
    assert marker["timeline_start"] == 15.0
    assert marker["duration"] == 1.5
    assert not tl.update_marker("nope", name="x")
    assert tl.delete_marker(marker_id)
    assert tl.get_marker(marker_id) is None
    assert not tl.delete_marker(marker_id)


def test_markers_round_trip_json():
    tl = make_timeline()
    tl.add_marker(3.0, owner_id="c1", id="m1")
    data = json.loads(tl.to_json())
    clip = data["tracks"]["children"][0]["children"][0]
    assert clip["markers"][0]["OTIO_SCHEMA"] == "Marker.2"
    back = Timeline.parse_json(json.dumps(data))
    assert back.get_marker("m1")["timeline_start"] == 3.0


def test_marker_colors():
    assert "RED" in MARKER_COLORS
