import json

from tellers_timeline import Clip, Gap, Item, MediaReference, Stack, Timeline, Track


def link_group_id(item):
    metadata = json.loads(item.get_metadata_json())
    return metadata["Resolve_OTIO"]["Link Group ID"]


def maybe_link_group_id(item):
    metadata = json.loads(item.get_metadata_json())
    return metadata.get("Resolve_OTIO", {}).get("Link Group ID")


def assert_type_error_message(fn, message):
    try:
        fn()
    except TypeError as exc:
        assert message in str(exc)
    else:
        raise AssertionError("expected TypeError")


def test_insert_item_at_time_returns_linked_ids_and_preserves_media_id():
    stack = Stack([Track(kind="video")])
    primary = Clip(2.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="primary")
    audio_ref = MediaReference(
        "file:///audio.wav",
        metadata_json=json.dumps(
            {"media_id": "asset-1", "tellers.ai": {"media_id": "asset-1"}}
        ),
    )
    audio = Clip(2.0, {"DEFAULT_MEDIA": audio_ref}, id="audio")

    result = stack.insert_item_at_time(
        0, 1.0, primary, "override", "split_and_insert", [audio]
    )

    assert result is not None
    assert result["primary_clip_id"] == "primary"
    assert len(result["audio_clips"]) == 1
    assert result["audio_clips"][0][1] == 0
    assert result["linked_video_clip_id"] is None
    assert result["created_track_indices"] == [0]

    tracks = stack.tracks()
    primary_item = stack.get_item("primary")[2]
    audio_item = next(item for item in tracks[result["audio_clips"][0][1]].items() if item.is_clip())

    assert link_group_id(primary_item) == result["link_group_id"]
    assert link_group_id(audio_item) == result["link_group_id"]

    media = audio_item.get_media_references()["DEFAULT_MEDIA"]
    media_metadata = json.loads(media.get_metadata_json())
    assert media_metadata["media_id"] == "asset-1"
    assert media_metadata["tellers.ai"]["media_id"] == "asset-1"


def test_insert_without_link_group_only_changes_destination_track():
    stack = Stack(
        [
            Track(kind="video", id="v", children=[Item.from_gap(Gap(4.0))]),
            Track(
                kind="audio",
                id="a",
                children=[
                    Item.from_clip(
                        Clip(
                            4.0,
                            {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")},
                            id="unlinked-audio",
                        )
                    )
                ],
            ),
        ]
    )

    result = stack.insert_item_at_time(
        0,
        1.0,
        Clip(1.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="inserted"),
        "override",
        "split_and_insert",
    )

    assert result == "inserted"
    assert stack.get_item("inserted") is not None
    assert stack.get_item("unlinked-audio")[0] == 1


def test_delete_item_can_delete_linked_clips():
    stack = Stack([Track(kind="video")])
    result = stack.insert_item_at_time(
        0,
        0.0,
        Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="primary"),
        "override",
        "split_and_insert",
        [Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")})],
    )

    removed = stack.delete_item("primary", True)

    assert len(removed) == 2
    for track in stack.tracks():
        assert all(item.is_gap() for item in track.items())


def test_delete_unlinked_item_only_removes_selected_item():
    stack = Stack(
        [
            Track(
                kind="video",
                children=[
                    Item.from_clip(
                        Clip(
                            3.0,
                            {"DEFAULT_MEDIA": MediaReference("file:///video.mov")},
                            id="primary",
                        )
                    )
                ],
            ),
            Track(
                kind="audio",
                children=[
                    Item.from_clip(
                        Clip(
                            3.0,
                            {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")},
                            id="unlinked-audio",
                        )
                    )
                ],
            ),
        ]
    )

    removed = stack.delete_item("primary", True)

    assert len(removed) == 1
    assert stack.get_item("primary") is None
    assert stack.get_item("unlinked-audio") is not None


def test_delete_item_removes_video_asset_linked_to_audio_primary():
    stack = Stack([Track(kind="audio")])
    result = stack.insert_item_at_time(
        0,
        0.0,
        Clip(4.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")}, id="audio"),
        "override",
        "split_and_insert",
        None,
        Clip(4.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="video"),
    )

    removed = stack.delete_item("audio", True)

    assert len(removed) == 2
    assert stack.get_item("audio") is None
    assert stack.get_item("video") is None
    for track in stack.tracks():
        assert all(item.is_gap() for item in track.items())


def test_delete_item_keeps_empty_linked_tracks():
    stack = Stack([Track(kind="video")])
    stack.insert_item_at_time(
        0,
        0.0,
        Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="primary"),
        "override",
        "split_and_insert",
        [Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")})],
    )

    removed = stack.delete_item("primary", True)

    assert len(removed) == 2
    assert len(stack.tracks()) == 2
    for track in stack.tracks():
        assert all(item.is_gap() for item in track.items())


def test_delete_track_removes_linked_assets_left_behind():
    stack = Stack([Track(kind="video", id="v"), Track(kind="audio", id="a")])
    result = stack.insert_item_at_time(
        0,
        0.0,
        Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="primary"),
        "override",
        "split_and_insert",
        [Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")})],
    )
    audio_id = result["audio_clips"][0][0]

    removed = stack.delete_track("v")

    assert removed is not None
    assert removed.get_id() == "v"
    assert len(stack.tracks()) == 2
    assert any(track.get_id() == "a" for track in stack.tracks())
    assert stack.get_item(audio_id) is None
    assert any(
        item.is_gap() and item.duration() == 3.0
        for track in stack.tracks()
        for item in track.items()
    )


def test_timeline_delete_track_removes_linked_assets_left_behind():
    timeline = Timeline(Stack([Track(kind="video", id="v"), Track(kind="audio", id="a")]))
    stack = timeline.get_stack()
    result = stack.insert_item_at_time(
        0,
        0.0,
        Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="primary"),
        "override",
        "split_and_insert",
        [Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")})],
    )
    timeline.set_stack(stack)
    audio_id = result["audio_clips"][0][0]

    removed = timeline.delete_track("v")

    assert removed is not None
    stack = timeline.get_stack()
    assert stack.get_item(audio_id) is None
    assert any(
        item.is_gap() and item.duration() == 3.0
        for track in stack.tracks()
        for item in track.items()
    )


def test_track_timeline_ids_returns_child_item_ids_in_order():
    track = Track(
        kind="video",
        id="track",
        children=[
            Item.from_clip(
                Clip(
                    1.0,
                    {"DEFAULT_MEDIA": MediaReference("file:///one.mov")},
                    id="clip-1",
                )
            ),
            Item.from_gap(Gap(1.0, id="gap-1")),
            Item.from_clip(
                Clip(
                    1.0,
                    {"DEFAULT_MEDIA": MediaReference("file:///two.mov")},
                    id="clip-2",
                )
            ),
        ],
    )

    assert track.timeline_ids() == ["clip-1", "gap-1", "clip-2"]


def test_unlink_item_accepts_multiple_ids_and_cleans_singletons():
    stack = Stack([Track(kind="video"), Track(kind="audio"), Track(kind="audio")])
    first = stack.insert_item_at_time(
        0,
        1.0,
        Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="primary"),
        "override",
        "split_and_insert",
        [Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")})],
    )
    video_track_index = stack.get_item("primary")[0]
    second = stack.insert_item_at_time(
        video_track_index,
        5.0,
        Clip(2.0, {"DEFAULT_MEDIA": MediaReference("file:///video-2.mov")}, id="primary-2"),
        "override",
        "split_and_insert",
        [Clip(2.0, {"DEFAULT_MEDIA": MediaReference("file:///audio-2.wav")})],
    )

    assert stack.unlink_item(["primary", "primary-2"]) == 4
    assert maybe_link_group_id(stack.get_item("primary")[2]) is None
    assert maybe_link_group_id(stack.get_item(first["audio_clips"][0][0])[2]) is None
    assert maybe_link_group_id(stack.get_item("primary-2")[2]) is None
    assert maybe_link_group_id(stack.get_item(second["audio_clips"][0][0])[2]) is None


def test_replace_item_updates_linked_group_duration_and_preserves_identity():
    stack = Stack([Track(kind="video"), Track(kind="audio")])
    result = stack.insert_item_at_time(
        0,
        0.0,
        Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="primary"),
        "override",
        "split_and_insert",
        [Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")})],
    )
    audio_id = result["audio_clips"][0][0]

    assert stack.replace_item(
        "primary",
        Clip(
            5.0,
            {"DEFAULT_MEDIA": MediaReference("file:///replacement.mov")},
            id="replacement",
        ),
    )

    primary = stack.get_item("primary")[2]
    audio = stack.get_item(audio_id)[2]
    assert primary.get_id() == "primary"
    assert primary.duration() == 5.0
    assert audio.duration() == 5.0
    assert maybe_link_group_id(primary) == result["link_group_id"]
    assert maybe_link_group_id(audio) == result["link_group_id"]
    assert stack.get_item("replacement") is None


def test_replace_unlinked_item_only_replaces_selected_item():
    stack = Stack(
        [
            Track(
                kind="video",
                children=[
                    Item.from_clip(
                        Clip(
                            3.0,
                            {"DEFAULT_MEDIA": MediaReference("file:///video.mov")},
                            id="primary",
                        )
                    )
                ],
            ),
            Track(
                kind="audio",
                children=[
                    Item.from_clip(
                        Clip(
                            3.0,
                            {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")},
                            id="unlinked-audio",
                        )
                    )
                ],
            ),
        ]
    )

    assert stack.replace_item(
        "primary",
        Clip(2.0, {"DEFAULT_MEDIA": MediaReference("file:///replacement.mov")}, id="replacement"),
    )

    assert stack.get_item("primary")[2].duration() == 2.0
    assert stack.get_item("replacement") is None
    assert stack.get_item("unlinked-audio") is not None


def test_replace_item_can_add_linked_audio_clip():
    stack = Stack(
        [
            Track(
                kind="video",
                children=[
                    Item.from_clip(
                        Clip(
                            3.0,
                            {"DEFAULT_MEDIA": MediaReference("file:///video.mov")},
                            id="primary",
                        )
                    )
                ],
            )
        ]
    )

    assert stack.replace_item(
        "primary",
        Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///replacement.mov")}),
        [Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")}, id="audio")],
    )

    assert len(stack.tracks()) == 2
    assert stack.tracks()[0].kind == "audio"
    primary = stack.get_item("primary")[2]
    audio = stack.get_item("audio")[2]
    assert maybe_link_group_id(primary) == maybe_link_group_id(audio)
    assert maybe_link_group_id(primary) is not None


def test_replace_item_keeps_same_content_audio_and_removes_same_content_video_input():
    stack = Stack(
        [
            Track(
                kind="video",
                children=[
                    Item.from_clip(
                        Clip(
                            3.0,
                            {"DEFAULT_MEDIA": MediaReference("file:///video.mov")},
                            id="primary",
                        )
                    )
                ],
            )
        ]
    )

    assert stack.replace_item(
        "primary",
        Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///replacement.mov")}, id="replacement"),
        [
            Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")}, id="audio"),
            Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///replacement.mov")}, id="other"),
        ],
    )

    assert stack.get_item("replacement") is None
    assert stack.get_item("audio") is not None
    audio_items = [
        item
        for track in stack.tracks()
        if track.kind == "audio"
        for item in track.items()
        if item.is_clip()
    ]
    assert len(audio_items) == 2

    stack = Stack(
        [
            Track(
                kind="video",
                children=[
                    Item.from_clip(
                        Clip(
                            3.0,
                            {"DEFAULT_MEDIA": MediaReference("file:///video.mov")},
                            id="primary",
                        )
                    )
                ],
            )
        ]
    )

    assert stack.replace_item(
        "primary",
        Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///replacement.mov")}, id="replacement"),
        None,
        Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///replacement.mov")}, id="other"),
    )
    assert len(stack.tracks()) == 1
    assert stack.get_item("primary") is not None
    assert stack.get_item("replacement") is None


def test_split_item_at_time_splits_linked_group():
    stack = Stack([Track(kind="video"), Track(kind="audio")])
    result = stack.insert_item_at_time(
        0,
        0.0,
        Clip(4.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="primary"),
        "override",
        "split_and_insert",
        [Clip(4.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")})],
    )
    audio_id = result["audio_clips"][0][0]

    assert stack.split_item_at_time("primary", 2.0)

    video_track, _, _ = stack.get_item("primary")
    audio_track, _, _ = stack.get_item(audio_id)
    assert len([item for item in stack.tracks()[video_track].items() if item.is_clip()]) == 2
    assert len([item for item in stack.tracks()[audio_track].items() if item.is_clip()]) == 2
    assert stack.tracks()[video_track].items()[0].duration() == 2.0
    assert stack.tracks()[video_track].items()[1].duration() == 2.0
    assert stack.tracks()[audio_track].items()[0].duration() == 2.0
    assert stack.tracks()[audio_track].items()[1].duration() == 2.0
    assert maybe_link_group_id(stack.tracks()[video_track].items()[0]) == result["link_group_id"]
    assert maybe_link_group_id(stack.tracks()[video_track].items()[1]) == result["link_group_id"]
    assert maybe_link_group_id(stack.tracks()[audio_track].items()[0]) == result["link_group_id"]
    assert maybe_link_group_id(stack.tracks()[audio_track].items()[1]) == result["link_group_id"]


def test_split_unlinked_item_only_splits_selected_track():
    stack = Stack(
        [
            Track(
                kind="video",
                children=[
                    Item.from_clip(
                        Clip(
                            4.0,
                            {"DEFAULT_MEDIA": MediaReference("file:///video.mov")},
                            id="primary",
                        )
                    )
                ],
            ),
            Track(
                kind="audio",
                children=[
                    Item.from_clip(
                        Clip(
                            4.0,
                            {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")},
                            id="unlinked-audio",
                        )
                    )
                ],
            ),
        ]
    )

    assert stack.split_item_at_time("primary", 2.0)

    assert len([item for item in stack.tracks()[0].items() if item.is_clip()]) == 2
    assert len([item for item in stack.tracks()[1].items() if item.is_clip()]) == 1
    assert stack.get_item("unlinked-audio") is not None


def test_replace_item_rejects_linked_audio_with_different_duration():
    stack = Stack(
        [
            Track(
                kind="video",
                children=[
                    Item.from_clip(
                        Clip(
                            3.0,
                            {"DEFAULT_MEDIA": MediaReference("file:///video.mov")},
                            id="primary",
                        )
                    )
                ],
            )
        ]
    )

    assert not stack.replace_item(
        "primary",
        Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///replacement.mov")}),
        [Clip(2.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")}, id="audio")],
    )
    assert len(stack.tracks()) == 1
    assert stack.get_item("primary")[2].duration() == 3.0
    assert stack.get_item("audio") is None


def test_replace_item_rejects_linked_video_with_different_duration():
    stack = Stack(
        [
            Track(
                kind="audio",
                children=[
                    Item.from_clip(
                        Clip(
                            4.0,
                            {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")},
                            id="audio",
                        )
                    )
                ],
            )
        ]
    )

    assert not stack.replace_item(
        "audio",
        Clip(4.0, {"DEFAULT_MEDIA": MediaReference("file:///replacement.wav")}),
        None,
        Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="video"),
    )
    assert len(stack.tracks()) == 1
    assert stack.tracks()[0].kind == "audio"
    assert stack.get_item("audio")[2].duration() == 4.0
    assert stack.get_item("video") is None


def test_replace_item_can_add_linked_video_clip_for_audio():
    stack = Stack(
        [
            Track(
                kind="audio",
                children=[
                    Item.from_clip(
                        Clip(
                            4.0,
                            {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")},
                            id="audio",
                        )
                    )
                ],
            )
        ]
    )

    assert stack.replace_item(
        "audio",
        Clip(4.0, {"DEFAULT_MEDIA": MediaReference("file:///replacement.wav")}),
        None,
        Clip(4.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="video"),
    )

    assert stack.tracks()[0].kind == "video"
    assert stack.tracks()[1].kind == "audio"
    audio = stack.get_item("audio")[2]
    video = stack.get_item("video")[2]
    assert maybe_link_group_id(audio) == maybe_link_group_id(video)
    assert maybe_link_group_id(audio) is not None


def test_link_item_links_arbitrary_existing_clips_with_new_group():
    primary = Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="primary")
    audio = Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")}, id="audio")
    stack = Stack(
        [
            Track(kind="video", children=[Item.from_clip(primary)]),
            Track(kind="audio", children=[Item.from_clip(audio)]),
        ]
    )

    group = stack.link_item(["primary", "audio"])

    assert group is not None
    assert maybe_link_group_id(stack.get_item("primary")[2]) == group
    assert maybe_link_group_id(stack.get_item("audio")[2]) == group


def test_link_item_rejects_items_with_different_boundaries():
    primary = Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="primary")
    audio = Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")}, id="audio")
    stack = Stack(
        [
            Track(kind="video", children=[Item.from_clip(primary)]),
            Track(kind="audio", children=[Item.from_gap(Gap(1.0)), Item.from_clip(audio)]),
        ]
    )

    assert stack.link_item(["primary", "audio"]) is None
    assert maybe_link_group_id(stack.get_item("primary")[2]) is None
    assert maybe_link_group_id(stack.get_item("audio")[2]) is None


def test_linked_insert_uses_normal_primary_insert_on_video_conflict():
    existing = Clip(5.0, {"DEFAULT_MEDIA": MediaReference("file:///existing.mov")}, id="existing")
    stack = Stack([Track(kind="video", children=[Item.from_clip(existing)])])

    result = stack.insert_item_at_time(
        0,
        1.0,
        Clip(2.0, {"DEFAULT_MEDIA": MediaReference("file:///new.mov")}),
        "override",
        "split_and_insert",
        [],
    )

    assert result is not None
    items = stack.tracks()[0].items()
    assert len(items) == 3
    assert [item.duration() for item in items] == [1.0, 2.0, 2.0]
    assert items[2].get_id() is not None


def test_insert_linked_clip_clamps_against_available_range():
    stack = Stack([Track(kind="video")])
    primary_ref = MediaReference("file:///video.mov", media_start=0.0, media_duration=5.0)
    primary = Clip(10.0, {"DEFAULT_MEDIA": primary_ref})

    result = stack.insert_item_at_time(0, 0.0, primary, "override", "split_and_insert", [])

    assert result is not None
    assert result["link_group_id"] is None
    item = next(item for item in stack.tracks()[0].items() if item.is_clip())
    assert maybe_link_group_id(item) is None
    assert item.duration() == 5.0


def test_insert_item_at_time_can_link_video_clip_for_audio_primary():
    stack = Stack([Track(kind="audio")])

    result = stack.insert_item_at_time(
        0,
        0.0,
        Clip(4.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")}, id="audio"),
        "override",
        "split_and_insert",
        None,
        Clip(4.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="video"),
    )

    assert result is not None
    assert result["primary_clip_id"] == "audio"
    assert result["linked_video_clip_id"] == "video"
    assert result["created_track_indices"] == [0]
    assert stack.tracks()[0].kind == "video"
    assert stack.tracks()[1].kind == "audio"
    assert maybe_link_group_id(stack.get_item("audio")[2]) == result["link_group_id"]
    assert maybe_link_group_id(stack.get_item("video")[2]) == result["link_group_id"]


def test_insert_item_at_index_returns_linked_ids():
    stack = Stack([Track(kind="video", id="v", children=[Item.from_gap(Gap(5.0))])])

    result = stack.insert_item_at_index(
        "v",
        0,
        Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="primary"),
        "override",
        [Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")}, id="audio")],
    )

    assert result is not None
    assert result["primary_clip_id"] == "primary"
    assert len(result["audio_clips"]) == 1
    assert result["created_track_indices"] == [0]
    assert maybe_link_group_id(stack.get_item("primary")[2]) == result["link_group_id"]
    assert maybe_link_group_id(stack.get_item("audio")[2]) == result["link_group_id"]


def test_insert_item_at_time_keeps_same_content_audio_and_removes_same_content_video_input():
    stack = Stack([Track(kind="video")])

    result = stack.insert_item_at_time(
        0,
        0.0,
        Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="primary"),
        "override",
        "split_and_insert",
        [Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="other")],
    )

    assert result is not None
    assert len(result["audio_clips"]) == 1
    assert result["link_group_id"] is not None
    assert len(stack.tracks()) == 2
    assert stack.get_item("primary") is not None

    stack = Stack([Track(kind="video")])
    result = stack.insert_item_at_time(
        0,
        0.0,
        Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="primary"),
        "override",
        "split_and_insert",
        None,
        Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="other"),
    )

    assert result is not None
    assert result["linked_video_clip_id"] is None
    assert result["link_group_id"] is None
    assert len(stack.tracks()) == 1
    assert stack.get_item("primary") is not None


def test_insert_item_at_time_rejects_linked_audio_with_different_duration():
    stack = Stack([Track(kind="video")])

    result = stack.insert_item_at_time(
        0,
        0.0,
        Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="primary"),
        "override",
        "split_and_insert",
        [Clip(2.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")}, id="audio")],
    )

    assert result is None
    assert len(stack.tracks()) == 1
    assert stack.get_item("primary") is None
    assert stack.get_item("audio") is None


def test_insert_item_at_time_rejects_linked_video_with_different_duration():
    stack = Stack([Track(kind="audio")])

    result = stack.insert_item_at_time(
        0,
        0.0,
        Clip(4.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")}, id="audio"),
        "override",
        "split_and_insert",
        None,
        Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="video"),
    )

    assert result is None
    assert len(stack.tracks()) == 1
    assert stack.tracks()[0].kind == "audio"
    assert stack.get_item("audio") is None
    assert stack.get_item("video") is None


def test_linked_video_clip_requires_clip_item():
    stack = Stack([Track(kind="audio", children=[Item.from_gap(Gap(3.0, id="gap"))])])

    assert_type_error_message(
        lambda: stack.insert_item_at_time(
            0,
            0.0,
            Gap(3.0),
            "override",
            "split_and_insert",
            None,
            Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}),
        ),
        "linked_video_clip can only be used when item is a Clip",
    )

    assert_type_error_message(
        lambda: stack.replace_item(
            "gap",
            Gap(3.0),
            None,
            Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}),
        ),
        "linked_video_clip can only be used when item is a Clip",
    )


def test_move_item_at_time_moves_linked_group():
    stack = Stack(
        [
            Track(kind="video", id="v", children=[Item.from_gap(Gap(8.0))]),
            Track(kind="audio", id="a", children=[Item.from_gap(Gap(8.0))]),
        ]
    )
    result = stack.insert_item_at_time(
        0,
        0.0,
        Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="primary"),
        "override",
        "split_and_insert",
        [Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")})],
    )
    audio_id = result["audio_clips"][0][0]

    assert stack.move_item_at_time(
        audio_id,
        "a",
        5.0,
        True,
        "override",
        "split_and_insert",
    )

    video_track, video_index, video_item = stack.get_item("primary")
    audio_track, audio_index, audio_item = stack.get_item(audio_id)
    assert stack.tracks()[video_track].start_time_of_item(
        video_index
    ) == stack.tracks()[audio_track].start_time_of_item(audio_index)
    assert maybe_link_group_id(video_item) == result["link_group_id"]
    assert maybe_link_group_id(audio_item) == result["link_group_id"]


def test_move_unlinked_item_only_moves_selected_item():
    stack = Stack(
        [
            Track(
                kind="video",
                id="v",
                children=[
                    Item.from_clip(
                        Clip(
                            3.0,
                            {"DEFAULT_MEDIA": MediaReference("file:///video.mov")},
                            id="primary",
                        )
                    )
                ],
            ),
            Track(
                kind="audio",
                id="a",
                children=[
                    Item.from_clip(
                        Clip(
                            3.0,
                            {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")},
                            id="unlinked-audio",
                        )
                    )
                ],
            ),
        ]
    )

    assert stack.move_item_at_time(
        "primary",
        "a",
        3.0,
        True,
        "override",
        "insert_after",
    )

    assert stack.get_item("primary")[0] == 1
    assert stack.get_item("unlinked-audio") is not None


def test_move_item_at_index_moves_linked_group():
    stack = Stack(
        [
            Track(kind="video", id="v", children=[Item.from_gap(Gap(8.0))]),
            Track(kind="audio", id="a", children=[Item.from_gap(Gap(8.0))]),
        ]
    )
    result = stack.insert_item_at_time(
        0,
        2.0,
        Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="primary"),
        "override",
        "split_and_insert",
        [Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")})],
    )
    audio_id = result["audio_clips"][0][0]

    assert stack.move_item_at_index("primary", "v", 0, True, "override")

    video_track, video_index, video_item = stack.get_item("primary")
    audio_track, audio_index, audio_item = stack.get_item(audio_id)
    assert stack.tracks()[video_track].start_time_of_item(video_index) == 0.0
    assert stack.tracks()[audio_track].start_time_of_item(audio_index) == 0.0
    assert maybe_link_group_id(video_item) == result["link_group_id"]
    assert maybe_link_group_id(audio_item) == result["link_group_id"]


def test_resize_item_updates_linked_group():
    stack = Stack([Track(kind="video", id="v")])
    result = stack.insert_item_at_time(
        0,
        0.0,
        Clip(4.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="primary"),
        "override",
        "split_and_insert",
        [Clip(4.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")})],
    )
    audio_id = result["audio_clips"][0][0]

    assert stack.resize_item(audio_id, 1.0, 2.0, "override", False)

    video_track, video_index, video_item = stack.get_item("primary")
    audio_track, audio_index, audio_item = stack.get_item(audio_id)
    assert stack.tracks()[video_track].start_time_of_item(video_index) == 1.0
    assert stack.tracks()[audio_track].start_time_of_item(audio_index) == 1.0
    assert video_item.duration() == 2.0
    assert audio_item.duration() == 2.0
