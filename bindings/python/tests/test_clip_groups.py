import pytest

from tellers_timeline import Clip, MediaReference, Timeline, Track

# Translation layer only; clip-group semantics are tested in tellers-timeline-core.


def make_timeline():
    timeline = Timeline([Track(kind="video")])
    stack = timeline.get_stack()
    for i, start in enumerate([0.0, 2.0]):
        clip = Clip(2.0, {"DEFAULT_MEDIA": MediaReference(f"file:///{i}.mov")}, id=f"c{i}")
        stack.insert_item_at_time(0, start, clip, "override", "split_and_insert")
    timeline.set_stack(stack)
    return timeline


def test_clip_group_round_trip():
    timeline = make_timeline()
    group_id = timeline.create_clip_group(["c0", "c1"], {"x": {"keyframes": [{"t": 0, "v": 0}, {"t": 2, "v": 0.5}]}})
    assert timeline.clip_group_member_ids(group_id) == ["c0", "c1"]
    assert timeline.get_item_clip_group_id("c1") == group_id
    assert timeline.clip_group_start(group_id) == 0.0
    assert timeline.clip_group_transform_at(group_id, 1.0)["x"] == 0.25

    timeline.set_clip_group({"id": group_id, "parameters": {"opacity": 0.5}})
    assert timeline.get_clip_group(group_id) == {"id": group_id, "parameters": {"opacity": 0.5}}
    assert [g["id"] for g in timeline.get_clip_groups()] == [group_id]

    assert timeline.set_item_clip_group_id("c1")
    assert timeline.clip_group_member_ids(group_id) == ["c0"]
    assert timeline.remove_clip_group(group_id)
    assert timeline.get_clip_groups() == []


def test_clip_group_errors():
    timeline = make_timeline()
    with pytest.raises(ValueError):
        timeline.create_clip_group(["missing"])
    with pytest.raises(ValueError):
        timeline.set_clip_group({"id": "g", "parameters": {"blur": 1}})
    assert not timeline.set_item_clip_group_id("missing", "g")
