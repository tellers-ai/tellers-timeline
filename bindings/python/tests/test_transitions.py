import json
from pathlib import Path

import pytest

from tellers_timeline import Clip, Item, MediaReference, Stack, Timeline, Track, Transition

FIXTURE = Path(__file__).resolve().parents[3] / "tellers-timeline-core" / "tests" / "fixtures" / "resolve_transitions.otio"


def _clip(duration: float, item_id: str) -> Clip:
    return Clip(duration, {"DEFAULT_MEDIA": MediaReference("file:///media.mov")}, id=item_id)


def test_resolve_export_with_transitions_parses_and_round_trips():
    data = FIXTURE.read_text()
    tl = Timeline.parse_json(data)
    assert not tl.validate()

    video2 = tl.get_stack().tracks()[1]
    items = video2.items()
    assert [i.is_clip() for i in items] == [True, False, True]
    assert items[1].is_transition()
    assert items[1].duration() == 0.0

    transition = items[1].as_transition()
    assert transition is not None
    assert transition.get_name() == "Slide Up"
    assert transition.get_transition_type() == "Custom_Transition"
    assert transition.get_in_offset() == 0.0
    assert transition.get_out_offset() == 1.0
    meta = json.loads(transition.get_metadata_json())
    assert meta["Resolve_OTIO"]["Transition Type"] == "Fusion Transition"

    # The transition takes no track time: 51 + 75 frames at 30 fps.
    assert abs(video2.total_duration() - 126 / 30) < 1e-9

    out = tl.to_json()
    again = Timeline.parse_json(out)
    assert again.to_json() == out
    assert json.loads(out)["tracks"]["children"][1]["children"][1]["OTIO_SCHEMA"] == "Transition.1"


def test_transition_constructor_and_json():
    t = Transition(0.5, 0.25, id="t-1")
    assert t.get_transition_type() == "SMPTE_Dissolve"
    assert t.get_in_offset() == 0.5
    assert t.get_out_offset() == 0.25
    assert t.get_id() == "t-1"
    t.set_name("Dissolve")
    t.set_out_offset(0.75)

    payload = json.loads(t.to_json())
    assert payload["OTIO_SCHEMA"] == "Transition.1"
    assert payload["name"] == "Dissolve"
    assert payload["out_offset"]["value"] == 0.75
    assert payload["metadata"]["tellers.ai"]["timeline_id"] == "t-1"

    parsed = Transition.parse_json(t.to_json())
    assert parsed.get_out_offset() == 0.75

    item = Item.from_transition(t)
    assert item.is_transition()
    assert not item.is_gap()
    assert item.get_id() == "t-1"


def test_deleting_a_clip_removes_attached_transitions():
    stack = Stack()
    track = Track(
        children=[
            Item.from_clip(_clip(2.0, "a")),
            Item.from_transition(Transition(0.5, 0.5, id="t1")),
            Item.from_clip(_clip(2.0, "b")),
            Item.from_transition(Transition(0.5, 0.5, id="t2")),
            Item.from_clip(_clip(2.0, "c")),
        ]
    )
    stack.add_track(track)
    stack.delete_item("b", False)
    ids = [i.get_id() for i in stack.tracks()[0].items()]
    assert ids == ["a", "c"]


def test_a_transition_can_be_deleted_by_id():
    stack = Stack()
    track = Track(
        children=[
            Item.from_clip(_clip(2.0, "a")),
            Item.from_transition(Transition(0.5, 0.5, id="t1")),
            Item.from_clip(_clip(2.0, "b")),
        ]
    )
    stack.add_track(track)
    removed = stack.delete_item("t1", False)
    assert len(removed) == 1
    ids = [i.get_id() for i in stack.tracks()[0].items()]
    assert ids == ["a", "b"]


def test_typed_kind_direction_and_color():
    t = Transition(0.5, 0.5, "Custom_Transition", "t-1")
    assert t.get_kind() == "Dissolve"
    assert t.get_direction() == "left"
    assert t.get_color() == "#000000"

    t.set_kind("push")
    assert t.get_transition_type() == "Push"
    t.set_direction("up")
    assert t.get_direction() == "up"

    t.set_kind("DipToWhite")
    assert t.get_kind() == "DipToColor"
    assert t.get_color() == "#ffffff"
    t.set_color("#ff0000")
    assert t.get_transition_type() == "DipToColor"
    assert t.get_color() == "#ff0000"

    params = json.loads(t.get_metadata_json())["tellers.ai"]["parameters"]
    assert params == {"direction": "up", "color": "#ff0000"}

    t.set_direction(None)
    t.set_color(None)
    assert json.loads(t.get_metadata_json())["tellers.ai"]["parameters"] == {}

    t.set_kind("Dissolve")
    assert t.get_transition_type() == "SMPTE_Dissolve"

    with pytest.raises(ValueError):
        t.set_kind("Spin")
    with pytest.raises(ValueError):
        t.set_direction("Up")


def test_audio_kind():
    import tellers_timeline

    assert tellers_timeline.AUDIO_TRANSITION_TYPES == ["tellers.ai.audio-constant-power", "tellers.ai.audio-constant-gain"]
    t = Transition(0.5, 0.5)
    assert t.get_audio_kind() is None
    t.set_audio_kind("constant-gain")
    assert t.get_transition_type() == "tellers.ai.audio-constant-gain"
    assert t.get_audio_kind() == "constant-gain"
    assert t.get_kind() == "Dissolve"
    t.set_audio_kind("tellers.ai.audio-constant-power")
    assert t.get_audio_kind() == "constant-power"
    with pytest.raises(ValueError):
        t.set_audio_kind("exponential")
