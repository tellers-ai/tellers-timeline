import json
from pathlib import Path

from tellers_timeline import Gap, Item, Timeline, Track

# A `Stack.1` inside a track is a composite clip: it parses, round-trips and
# reports its duration (source_range, else its longest track).

FIXTURE = (
    Path(__file__).resolve().parents[3]
    / "tellers-timeline-core"
    / "tests"
    / "fixtures"
    / "composite_clip.json"
)


def load():
    return Timeline.parse_json(FIXTURE.read_text())


def test_composite_parses_and_round_trips():
    timeline = load()
    assert json.loads(timeline.to_json()) == json.loads(FIXTURE.read_text())

    items = timeline.get_stack().tracks()[0].get_items()
    assert [item.is_composite() for item in items] == [False, True, False, True]
    assert items[1].get_id() == "lower-third"
    assert [track.get_name() for track in items[1].get_children()] == ["bar", "accent"]
    assert items[0].get_children() == []


def test_composite_durations():
    timeline = load()
    track = timeline.get_stack().tracks()[0]
    items = track.get_items()
    # source_range set
    assert items[1].duration() == 5.0
    # no source_range: longest child track (3s clip + 1s gap)
    assert items[3].duration() == 4.0
    assert items[3].get_source_range().get_duration() == 4.0
    assert items[3].get_enabled() is False
    assert track.total_duration() == 12.0


def test_split_composite():
    timeline = load()
    stack = timeline.get_stack()
    assert stack.split_item_at_time("lower-third", 4.0)
    items = stack.tracks()[0].get_items()
    assert items[1].is_composite() and items[2].is_composite()
    assert items[1].duration() == 2.0
    assert items[2].duration() == 3.0
    assert items[2].get_source_range().get_start_time() == 2.0
    assert items[2].get_id() != "lower-third"


def test_build_composite_in_python():
    inner = Track(kind="video", id="inner", children=[Item.from_gap(Gap(2.5))])
    composite = Item.from_composite([inner], name="Built", id="built")
    assert composite.is_composite()
    assert composite.duration() == 2.5
    assert json.loads(str(composite))["OTIO_SCHEMA"] == "Stack.1"

    timeline = load()
    stack = timeline.get_stack()
    assert stack.insert_item_at_time(0, 12.0, composite, "override", "split_and_insert") == "built"
    _, _, item = stack.get_item("built")
    assert item.is_composite()
    assert stack.tracks()[0].total_duration() == 14.5


def test_marker_on_composite():
    timeline = load()
    marker_id = timeline.add_marker(3.0, name="beat", owner_id="lower-third")
    assert timeline.get_marker(marker_id)["owner_kind"] == "composite"


# The player applies a composite's own filters, Transform and Blend to its
# composited image, so the item tellers effect API covers composites too.


def test_composite_tellers_effects_by_id():
    timeline = load()
    # The fixture's `Opacity` is not a player effect: it is kept, not listed.
    assert timeline.get_item_tellers_effects("lower-third") == []
    assert timeline.get_item_tellers_effects("montage") == []
    assert timeline.push_item_tellers_effect(
        "lower-third", {"effect_name": "Transform", "parameters": {"scale": 0.5}}
    )
    assert timeline.insert_item_tellers_effect_at(
        "lower-third", 0, {"effect_name": "Blend", "parameters": {"mode": "multiply"}}
    )
    assert timeline.push_item_tellers_effect("lower-third", {"effect_name": "Transform"})
    assert timeline.set_item_tellers_effect_parameter("lower-third", 2, "rotation", 90)
    assert timeline.set_item_tellers_effect_enabled("lower-third", 1, False)
    previous = timeline.replace_item_tellers_effect_at(
        "lower-third", 1, {"effect_name": "Blur", "parameters": {"strength": 4}}
    )
    assert previous["effect_name"] == "Transform" and not previous["enabled"]

    reparsed = Timeline.parse_json(timeline.to_json())
    effects = reparsed.get_item_tellers_effects("lower-third")
    assert [e["effect_name"] for e in effects] == ["Blend", "Blur", "Transform"]
    assert effects[2]["parameters"] == {"rotation": 90}
    raw = json.loads(reparsed.to_json())["tracks"]["children"][0]["children"][1]["effects"]
    assert [e["effect_name"] for e in raw] == ["Opacity", "tellers.ai.Blend", "tellers.ai.Blur", "tellers.ai.Transform"]

    assert timeline.remove_item_tellers_effect_at("lower-third", 0)["effect_name"] == "Blend"
    assert timeline.clear_item_tellers_effects("lower-third")
    assert timeline.get_item_tellers_effects("lower-third") == []
    raw = json.loads(timeline.to_json())["tracks"]["children"][0]["children"][1]["effects"]
    assert [e["effect_name"] for e in raw] == ["Opacity"]
    assert timeline.set_item_tellers_effects("montage", [{"effect_name": "Pixelate"}])
    assert timeline.get_item_tellers_effects("montage")[0]["effect_name"] == "Pixelate"

    # Gaps hold no effects.
    assert timeline.get_item_tellers_effects("g1") is None
    assert not timeline.push_item_tellers_effect("g1", {"effect_name": "Blur"})


def test_composite_item_tellers_effects():
    composite = Item.from_composite([Track(kind="video", children=[Item.from_gap(Gap(2.0))])])
    assert composite.get_tellers_effects() == []
    assert composite.push_tellers_effect({"effect_name": "Blend", "parameters": {"mode": "screen"}})
    assert not composite.push_tellers_effect({"effect_name": "NotAnEffect"})
    assert composite.get_tellers_effects()[0]["parameters"] == {"mode": "screen"}
    assert json.loads(str(composite))["effects"][0]["effect_name"] == "tellers.ai.Blend"

    gap = Item.from_gap(Gap(1.0))
    assert gap.get_tellers_effects() == []
    assert not gap.push_tellers_effect({"effect_name": "Blur"})
    assert gap.remove_tellers_effect_at(0) is None
