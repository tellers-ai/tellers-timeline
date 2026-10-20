import pytest

from tellers_timeline import Clip, Item, MediaReference, Stack, Timeline, Track

# Editing inside a composite clip: edit_composite lends its tracks out as a
# working Stack, so item ops apply to nested items. A composite without
# source_range follows its content and the outer track ripples.


def clip(id, duration):
    return Item.from_clip(
        Clip(duration, {"DEFAULT_MEDIA": MediaReference(f"file:///{id}.mov")}, id=id)
    )


def make_stack():
    """V1: [a 2s][comp: inner [x 3s]][b 2s]"""
    comp = Item.from_composite([Track(kind="video", id="inner", children=[clip("x", 3.0)])], name="Comp", id="comp")
    return Stack([Track(kind="video", id="v1", children=[clip("a", 2.0), comp, clip("b", 2.0)])])


def start_of(stack, id):
    ti, ii, _ = stack.get_item(id)
    return stack.tracks()[ti].start_time_of_item(ii)


def test_nested_lookups():
    stack = make_stack()
    assert stack.get_item("x") is None
    assert stack.get_nested_item("x").get_id() == "x"
    assert stack.composite_path("x") == ["comp"]
    assert stack.composite_path("comp") == []
    assert stack.composite_path("nope") is None
    inner = stack.composite_stack("comp")
    assert inner.get_name() == "Comp"
    assert [t.get_id() for t in inner.tracks()] == ["inner"]
    assert stack.composite_stack("a") is None


def test_edit_composite_returns_the_callable_result_and_ripples():
    stack = make_stack()

    def edit(inner):
        assert inner.get_name() == "Comp"
        return inner.insert_item_at_time(0, 3.0, clip("y", 2.0), "override", "split_and_insert")

    assert stack.edit_composite("comp", edit) == "y"
    assert stack.get_nested_item("comp").duration() == 5.0
    assert start_of(stack, "b") == 7.0

    assert stack.edit_composite("comp", lambda inner: inner.delete_item("y", False))
    assert stack.get_nested_item("comp").duration() == 3.0
    assert start_of(stack, "b") == 5.0


def test_split_inside_composite():
    stack = make_stack()
    assert stack.edit_composite("comp", lambda inner: inner.split_item_at_time("x", 1.0))
    children = stack.get_nested_item("comp").get_children()
    assert [item.duration() for item in children[0].get_items()] == [1.0, 2.0]
    assert start_of(stack, "b") == 5.0


def test_edit_composite_errors():
    stack = make_stack()
    with pytest.raises(KeyError):
        stack.edit_composite("a", lambda inner: None)

    def boom(inner):
        inner.delete_item("x", False)
        raise RuntimeError("boom")

    before = Timeline(stack).to_json()
    with pytest.raises(RuntimeError):
        stack.edit_composite("comp", boom)
    assert Timeline(stack).to_json() == before


def test_set_composite_tracks():
    stack = make_stack()
    assert stack.set_composite_tracks("comp", [Track(kind="video", children=[clip("p", 4.0)])])
    assert stack.get_nested_item("comp").duration() == 4.0
    assert start_of(stack, "b") == 6.0
    assert not stack.set_composite_tracks("a", [])


def test_nested_composites():
    inner = Item.from_composite([Track(kind="video", children=[clip("x", 3.0)])], id="inner")
    outer = Item.from_composite([Track(kind="video", children=[inner, clip("z", 1.0)])], id="outer")
    stack = Stack([Track(kind="video", children=[outer, clip("b", 2.0)])])
    assert stack.composite_path("x") == ["outer", "inner"]
    stack.edit_composite(
        "inner", lambda s: s.insert_item_at_time(0, 3.0, clip("y", 2.0), "override", "split_and_insert")
    )
    assert stack.get_nested_item("outer").duration() == 6.0
    assert start_of(stack, "b") == 6.0


def test_item_tellers_effects_reach_nested_items():
    timeline = Timeline(make_stack())
    assert timeline.push_item_tellers_effect("x", {"effect_name": "Blur"})
    assert [e["effect_name"] for e in timeline.get_item_tellers_effects("x")] == ["Blur"]
