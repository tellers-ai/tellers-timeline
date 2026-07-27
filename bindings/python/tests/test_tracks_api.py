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


def _named_stack() -> Stack:
    video = Track(kind="video", id="v")
    video.set_name("Main")
    audio = Track(kind="audio", id="a")
    audio.set_name("music")
    return Stack([video, audio])


def test_get_track_by_name_returns_index_and_track():
    stack = _named_stack()

    found = stack.get_track_by_name("music")
    assert found is not None
    index, track = found
    assert index == 1
    assert track.get_id() == "a"


def test_get_track_by_name_returns_none_for_unknown_and_wrong_case():
    stack = _named_stack()

    assert stack.get_track_by_name("missing") is None
    assert stack.get_track_by_name("MUSIC") is None


def test_get_track_by_name_returns_none_when_name_is_ambiguous():
    first = Track(kind="video", id="v1")
    first.set_name("overlay")
    second = Track(kind="video", id="v2")
    second.set_name("overlay")
    stack = Stack([first, second])

    assert stack.get_track_by_name("overlay") is None
    assert [index for index, _ in stack.find_tracks_by_name("overlay")] == [0, 1]
    assert [track.get_id() for _, track in stack.find_tracks_by_name("overlay")] == [
        "v1",
        "v2",
    ]


def test_find_tracks_by_name_returns_empty_list_when_nothing_matches():
    assert _named_stack().find_tracks_by_name("nope") == []
