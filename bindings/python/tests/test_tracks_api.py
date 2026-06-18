import json

from tellers_timeline import Clip, Item, MediaReference, Stack, Timeline, Track

# Binding methods used by tellers-backend (Python) that were thinly covered:
# set_tracks, insert_item_at_index (placement), to_json.


def test_set_tracks_replaces_all_tracks():
    stack = Stack([Track(kind="video", id="v")])
    stack.set_tracks([Track(kind="audio", id="a1"), Track(kind="audio", id="a2")])
    assert [track.get_id() for track in stack.tracks()] == ["a1", "a2"]


def test_insert_item_at_index_places_unlinked_item_at_index():
    stack = Stack(
        [
            Track(
                kind="video",
                id="v",
                children=[
                    Item.from_clip(
                        Clip(2.0, {"DEFAULT_MEDIA": MediaReference("file:///a.mov")}, id="A")
                    ),
                    Item.from_clip(
                        Clip(2.0, {"DEFAULT_MEDIA": MediaReference("file:///b.mov")}, id="B")
                    ),
                ],
            )
        ]
    )

    result = stack.insert_item_at_index(
        "v",
        1,
        Clip(1.0, {"DEFAULT_MEDIA": MediaReference("file:///x.mov")}, id="X"),
        "override",
    )

    # Unlinked insert returns the inserted item id.
    assert result == "X"
    items = stack.tracks()[0].items()
    assert items[1].get_id() == "X"
    assert stack.get_item("X") is not None


def test_timeline_to_json_produces_valid_parseable_json():
    timeline = Timeline(
        Stack(
            [
                Track(
                    kind="video",
                    id="v",
                    children=[
                        Item.from_clip(
                            Clip(2.0, {"DEFAULT_MEDIA": MediaReference("file:///a.mov")}, id="A")
                        )
                    ],
                )
            ]
        )
    )
    out = timeline.to_json()
    parsed = json.loads(out)
    assert isinstance(parsed, dict)
    assert "A" in out
