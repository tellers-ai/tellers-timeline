import json

from tellers_timeline import Clip, Item, MediaReference, Stack, Timeline, Track


def test_round_trip_simple():
    with open("spec/examples/simple.json", "r") as f:
        data = f.read()
    tl = Timeline.parse_json(data)
    errs = tl.validate()
    assert not errs
    out = tl.to_json()
    tl2 = Timeline.parse_json(out)
    assert tl2.to_json() == out


def test_enabled_defaults_and_round_trips():
    ref = MediaReference("file:///tmp/source.mov")
    clip = Clip(4.0, {"DEFAULT_MEDIA": ref})
    track = Track(children=[Item.from_clip(clip)])

    assert clip.get_enabled() is True
    assert track.get_enabled() is True

    clip.set_enabled(False)
    track.set_enabled(False)

    clip_json = json.loads(clip.to_json())
    track_json = json.loads(str(track))
    assert clip_json["enabled"] is False
    assert track_json["enabled"] is False

    parsed_clip = Clip.parse_json(json.dumps({"OTIO_SCHEMA": "Clip.2", **clip_json}))
    assert parsed_clip.get_enabled() is False
    parsed_clip.set_enabled(True)
    assert parsed_clip.get_enabled() is True


def test_timeline_add_track_returns_inserted_track_id():
    timeline = Timeline()
    track = Track(kind="audio", id="new-audio-track")

    inserted_id = timeline.add_track(track)

    assert inserted_id == "new-audio-track"
    assert timeline.get_stack().tracks()[0].get_id() == inserted_id


def test_stack_add_track_returns_sanitized_inserted_track_id():
    stack = Stack([Track(kind="video", id="duplicate-track")])

    inserted_id = stack.add_track(Track(kind="audio", id="duplicate-track"), 1)

    assert inserted_id != "duplicate-track"
    assert stack.tracks()[1].get_id() == inserted_id
