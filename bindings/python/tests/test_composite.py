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
