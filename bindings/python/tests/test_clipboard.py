import pytest

from tellers_timeline import Clip, MediaReference, Timeline, Track

# Translation layer only; copy/paste semantics are tested in tellers-timeline-core.


def make_timeline():
    timeline = Timeline([Track(kind="video")])
    stack = timeline.get_stack()
    for i, start in enumerate([0.0, 3.0]):
        clip = Clip(2.0, {"DEFAULT_MEDIA": MediaReference(f"file:///{i}.mov")}, id=f"c{i}")
        stack.insert_item_at_time(0, start, clip, "override", "split_and_insert")
    timeline.set_stack(stack)
    return timeline


def test_copy_then_paste_places_fresh_copies():
    timeline = make_timeline()
    clipboard = timeline.copy_items(["c0", "c1"])
    assert isinstance(clipboard, str)

    pasted = timeline.paste_items(clipboard, 10.0)

    assert len(pasted) == 2
    assert not {"c0", "c1"} & set(pasted)
    assert all(item_id in timeline.to_json() for item_id in pasted)


def test_nothing_to_copy_and_bad_clipboard():
    timeline = make_timeline()
    assert timeline.copy_items(["missing"]) is None
    with pytest.raises(ValueError):
        timeline.paste_items("not json", 0.0)
