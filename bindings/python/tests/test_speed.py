import pytest

from tellers_timeline import Clip, MediaReference, Stack, Track

# Translation layer only; speed semantics are tested in tellers-timeline-core.


def make_clip(id="c"):
    return Clip(4.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id=id)


def test_clip_speed_round_trip():
    clip = make_clip()
    assert clip.get_speed() is None
    assert clip.get_source_duration() == 4.0

    clip.set_speed(2.0, preserve_pitch=False)
    assert clip.get_speed() == {"speed": 2.0, "preserve_pitch": False}
    assert clip.get_source_duration() == 8.0

    clip.set_speed(0.0)
    assert clip.get_speed() == {"speed": 0.0, "preserve_pitch": False}
    assert clip.clear_speed()
    assert clip.get_speed() is None

    with pytest.raises(ValueError):
        clip.set_speed(10.0)


def test_negative_speed_plays_backwards():
    clip = make_clip()
    assert not clip.is_reversed()
    clip.set_speed(-2.0)
    assert clip.get_speed() == {"speed": -2.0, "preserve_pitch": True}
    assert clip.is_reversed()
    assert clip.get_source_duration() == 8.0
    assert clip.get_source_time_at(0.0) == 8.0
    assert clip.get_source_time_at(4.0) == 0.0


def test_keyframed_time_warp_is_returned_as_keyframes():
    clip = make_clip()
    clip.push_tellers_effect(
        {"effect_name": "TimeWarp", "parameters": {"speed": {"keyframes": [{"t": 0, "v": 1}, {"t": 2, "v": 3}]}}}
    )
    speed = clip.get_speed()
    assert speed["speed"] == {"keyframes": [{"t": 0.0, "v": 1.0}, {"t": 2.0, "v": 3.0}]}
    assert speed["preserve_pitch"] is True


def test_stack_set_item_speed():
    stack = Stack([Track(kind="video")])
    stack.insert_item_at_time(0, 0.0, make_clip("v"), "override", "split_and_insert")
    assert stack.set_item_speed("v", 0.5) == 1
    assert stack.set_item_speed("missing", 0.5, with_sync_clips=False) == 0
    assert stack.set_item_speed("v", -1.0) == 1
    with pytest.raises(ValueError):
        stack.set_item_speed("v", -10.0)


def test_stack_slip_item():
    stack = Stack([Track(kind="video")])
    stack.insert_item_at_time(0, 0.0, make_clip("v"), "override", "split_and_insert")
    bounds = stack.get_item_slip_bounds("v")
    assert bounds is not None and bounds[0] == 0.0
    assert stack.slip_item("v", 1.5) == 1
    assert stack.get_item_slip_bounds("missing") is None
    assert stack.slip_item("missing", 1.0) == 0
