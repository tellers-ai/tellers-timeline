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


def test_clip_constructor_prefers_default_media_when_active_key_missing():
    clip = Clip(
        2.0,
        {
            "ALT": MediaReference("file:///alt.mov"),
            "DEFAULT_MEDIA": MediaReference("file:///default.mov"),
        },
    )

    assert clip.get_active_media_reference_key() == "DEFAULT_MEDIA"


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
    # Audio tracks are created below the video (lower index), which stays on top.
    assert result["audio_clips"][0][1] == 0
    assert result["linked_video_clip_id"] is None
    assert result["created_track_indices"] == [0]

    tracks = stack.tracks()
    assert tracks[0].get_id() == "A1"
    assert tracks[0].get_name() == "A1"
    primary_item = stack.get_item("primary")[2]
    audio_item = next(item for item in tracks[result["audio_clips"][0][1]].items() if item.is_clip())

    assert link_group_id(primary_item) == result["link_group_id"]
    assert link_group_id(audio_item) == result["link_group_id"]

    media = audio_item.get_media_references()["DEFAULT_MEDIA"]
    media_metadata = json.loads(media.get_metadata_json())
    assert media_metadata["media_id"] == "asset-1"
    assert media_metadata["tellers.ai"]["media_id"] == "asset-1"


def test_insert_master_clip_with_multiple_linked_audio_clips():
    stack = Stack([Track(kind="video", id="v")])
    primary = Clip(
        4.0,
        {"DEFAULT_MEDIA": MediaReference("file:///master-video.mov")},
        id="master-video",
    )
    audio_one = Clip(
        4.0,
        {"DEFAULT_MEDIA": MediaReference("file:///master-audio-1.wav")},
        id="audio-one",
    )
    audio_two = Clip(
        4.0,
        {"DEFAULT_MEDIA": MediaReference("file:///master-audio-2.wav")},
        id="audio-two",
    )
    audio_three = Clip(
        4.0,
        {"DEFAULT_MEDIA": MediaReference("file:///master-audio-3.wav")},
        id="audio-three",
    )

    result = stack.insert_item_at_time(
        0,
        2.0,
        primary,
        "override",
        "split_and_insert",
        [audio_one, audio_two, audio_three],
    )

    assert result is not None
    assert result["primary_clip_id"] == "master-video"
    assert len(result["audio_clips"]) == 3
    assert result["linked_video_clip_id"] is None
    # Audio tracks are created below the video (lower indices); the video ends
    # up on top, and the first audio clip sits directly below it.
    assert result["created_track_indices"] == [0, 1, 2]

    tracks = stack.tracks()
    assert [tracks[index].get_id() for _, index in result["audio_clips"]] == [
        "A3",
        "A2",
        "A1",
    ]
    primary_track, primary_index, primary_item = stack.get_item("master-video")
    assert tracks[primary_track].start_time_of_item(primary_index) == 2.0
    assert primary_item.duration() == 4.0
    assert maybe_link_group_id(primary_item) == result["link_group_id"]

    expected_urls = [
        "file:///master-audio-1.wav",
        "file:///master-audio-2.wav",
        "file:///master-audio-3.wav",
    ]
    for (audio_id, track_index), expected_url in zip(result["audio_clips"], expected_urls):
        actual_track, item_index, audio_item = stack.get_item(audio_id)
        assert actual_track == track_index
        assert tracks[track_index].start_time_of_item(item_index) == 2.0
        assert audio_item.duration() == 4.0
        assert maybe_link_group_id(audio_item) == result["link_group_id"]
        assert (
            audio_item.get_media_references()["DEFAULT_MEDIA"].get_url()
            == expected_url
        )


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
    # Audio track sits below the video (lower index) so the sync insert reuses it.
    stack = Stack([Track(kind="audio", id="a"), Track(kind="video", id="v")])
    result = stack.insert_item_at_time(
        1,
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
    # The audio sync track reuses the existing audio track "a" instead of spawning a
    # new one, so deleting the video leaves only that single audio track behind.
    assert len(stack.tracks()) == 1
    assert any(track.get_id() == "a" for track in stack.tracks())
    assert stack.get_item(audio_id) is None
    assert any(
        item.is_gap() and item.duration() == 3.0
        for track in stack.tracks()
        for item in track.items()
    )


def test_timeline_delete_track_removes_linked_assets_left_behind():
    timeline = Timeline(Stack([Track(kind="audio", id="a"), Track(kind="video", id="v")]))
    stack = timeline.get_stack()
    result = stack.insert_item_at_time(
        1,
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


def test_sync_track_info_reports_primary_and_bound_tracks():
    stack = Stack([Track(kind="video", id="linked-v")])
    stack.insert_item_at_time(
        0,
        0.0,
        Clip(4.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="linked-video"),
        "override",
        "split_and_insert",
        [Clip(4.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")})],
    )
    stack.set_tracks(
        stack.tracks()
        + [
            Track(
                kind="audio",
                id="unlinked-a",
                children=[
                    Item.from_clip(
                        Clip(
                            4.0,
                            {"DEFAULT_MEDIA": MediaReference("file:///unlinked-a.wav")},
                        )
                    )
                ],
            ),
            Track(
                kind="video",
                id="unlinked-v",
                children=[
                    Item.from_clip(
                        Clip(
                            4.0,
                            {"DEFAULT_MEDIA": MediaReference("file:///unlinked-v.mov")},
                            id="unlinked-video",
                        )
                    )
                ],
            ),
        ]
    )

    groups = stack.sync_track_info()

    assert len(groups) == 3
    assert groups[0]["start_index"] == 0
    assert groups[0]["end_index"] == 2
    assert groups[0]["track_indices"] == [0, 1]
    # The audio is created below the video (lower index); the video stays on top.
    assert groups[0]["track_ids"] == ["A1", "linked-v"]
    assert groups[0]["primary_track_index"] == 1
    assert groups[0]["primary_track_id"] == "linked-v"
    assert groups[0]["bound_track_indices"] == [0]
    assert groups[0]["bound_track_ids"] == ["A1"]

    assert groups[1]["track_indices"] == [2]
    assert groups[1]["primary_track_id"] == "unlinked-a"
    assert groups[1]["bound_track_indices"] == []

    assert groups[2]["track_indices"] == [3]
    assert groups[2]["primary_track_id"] == "unlinked-v"
    assert groups[2]["bound_track_indices"] == []


def test_timeline_sync_track_info_reports_primary_and_bound_tracks():
    timeline = Timeline(Stack([Track(kind="video", id="linked-v")]))
    stack = timeline.get_stack()
    stack.insert_item_at_time(
        0,
        0.0,
        Clip(4.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="linked-video"),
        "override",
        "split_and_insert",
        [Clip(4.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")})],
    )
    timeline.set_stack(stack)

    groups = timeline.sync_track_info()

    assert groups[0]["primary_track_id"] == "linked-v"
    assert groups[0]["bound_track_ids"] == ["A1"]


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


def test_replace_item_keeps_same_content_audio():
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
    assert maybe_link_group_id(stack.tracks()[audio_track].items()[0]) == result["link_group_id"]
    right_link_group_id = result["link_group_id"] + 1
    assert maybe_link_group_id(stack.tracks()[video_track].items()[1]) == right_link_group_id
    assert maybe_link_group_id(stack.tracks()[audio_track].items()[1]) == right_link_group_id
    assert (
        stack.tracks()[video_track].items()[0].get_id()
        != stack.tracks()[video_track].items()[1].get_id()
    )
    assert (
        stack.tracks()[audio_track].items()[0].get_id()
        != stack.tracks()[audio_track].items()[1].get_id()
    )


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

    # No linked audio clips: a plain insert returns the inserted item id (str).
    assert result is not None
    assert isinstance(result, str)
    item = next(item for item in stack.tracks()[0].items() if item.is_clip())
    assert maybe_link_group_id(item) is None
    assert item.duration() == 5.0


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
    # Audio track is created below the video (lower index), which stays on top.
    assert result["created_track_indices"] == [0]
    assert maybe_link_group_id(stack.get_item("primary")[2]) == result["link_group_id"]
    assert maybe_link_group_id(stack.get_item("audio")[2]) == result["link_group_id"]


def test_insert_item_at_time_keeps_same_content_audio():
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


def test_move_unlinked_item_without_gap_pulls_later_linked_assets():
    stack = Stack(
        [
            Track(
                kind="video",
                id="v",
                children=[
                    Item.from_clip(
                        Clip(
                            1.0,
                            {"DEFAULT_MEDIA": MediaReference("file:///unlinked.mov")},
                            id="unlinked",
                        )
                    )
                ],
            ),
            Track(kind="audio", id="a"),
        ]
    )
    result = stack.insert_item_at_time(
        0,
        1.0,
        Clip(2.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="linked-video"),
        "override",
        "split_and_insert",
        [Clip(2.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")})],
    )
    audio_id = result["audio_clips"][0][0]

    assert stack.move_item_at_time(
        "unlinked",
        "v",
        2.0,
        False,
        "push",
        "insert_before_or_after",
    )

    video_track, video_index, _video_item = stack.get_item("linked-video")
    audio_track, audio_index, _audio_item = stack.get_item(audio_id)
    assert stack.tracks()[video_track].start_time_of_item(
        video_index
    ) == stack.tracks()[audio_track].start_time_of_item(audio_index)
    assert [item.get_id() for item in stack.tracks()[video_track].items()] == [
        "linked-video",
        "unlinked",
    ]
    assert all(item.is_clip() for item in stack.tracks()[video_track].items())
    audio_items = stack.tracks()[audio_track].items()
    # The linked group is pulled flush to the start, leaving just the audio clip
    # aligned with the video (no stray leading gap).
    assert len(audio_items) == 1
    assert audio_items[audio_index].is_clip()


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


def test_resize_video_updates_linked_audio_with_different_initial_boundary():
    audio = Clip(
        49.88,
        {"DEFAULT_MEDIA": MediaReference("file:///shared.mov", media_duration=49.88)},
        id="audio",
    )
    audio.set_metadata_json(
        json.dumps({"tellers.ai": {"timeline_id": "audio"}, "Resolve_OTIO": {"Link Group ID": 1}})
    )
    video = Clip(
        49.11530679434423,
        {"DEFAULT_MEDIA": MediaReference("file:///shared.mov", media_duration=49.88)},
        id="video",
    )
    video.set_metadata_json(
        json.dumps({"tellers.ai": {"timeline_id": "video"}, "Resolve_OTIO": {"Link Group ID": 1}})
    )
    stack = Stack(
        [
            Track(
                kind="audio",
                id="A1",
                children=[
                    Item.from_gap(Gap(0.3879401240763142)),
                    Item.from_clip(audio),
                ],
            ),
            Track(
                kind="video",
                id="V1",
                children=[
                    Item.from_gap(Gap(0.3879401240763142)),
                    Item.from_gap(Gap(0.7646932056557713)),
                    Item.from_clip(video),
                ],
            ),
        ]
    )
    old_video_start = 0.3879401240763142 + 0.7646932056557713
    new_video_start = 2.0
    new_duration = 10.0

    assert stack.resize_item("video", new_video_start, new_duration, "override", False)

    audio_track, audio_index, audio_item = stack.get_item("audio")
    video_track, video_index, video_item = stack.get_item("video")
    delta = new_video_start - old_video_start
    assert stack.tracks()[video_track].start_time_of_item(video_index) == new_video_start
    assert stack.tracks()[audio_track].start_time_of_item(audio_index) == 0.3879401240763142 + delta
    assert video_item.duration() == new_duration
    assert audio_item.duration() == new_duration


def test_resize_video_updates_linked_audio_with_same_initial_boundary():
    audio = Clip(
        49.88,
        {"DEFAULT_MEDIA": MediaReference("file:///shared.mov", media_duration=49.88)},
        id="audio",
    )
    audio.set_metadata_json(
        json.dumps({"tellers.ai": {"timeline_id": "audio"}, "Resolve_OTIO": {"Link Group ID": 1}})
    )
    video = Clip(
        49.88,
        {"DEFAULT_MEDIA": MediaReference("file:///shared.mov", media_duration=49.88)},
        id="video",
    )
    video.set_metadata_json(
        json.dumps({"tellers.ai": {"timeline_id": "video"}, "Resolve_OTIO": {"Link Group ID": 1}})
    )
    stack = Stack(
        [
            Track(kind="audio", id="A1", children=[Item.from_gap(Gap(0.5)), Item.from_clip(audio)]),
            Track(kind="video", id="V1", children=[Item.from_gap(Gap(0.5)), Item.from_clip(video)]),
        ]
    )
    new_start = 2.0
    new_duration = 10.0

    assert stack.resize_item("video", new_start, new_duration, "override", False)

    audio_track, audio_index, audio_item = stack.get_item("audio")
    video_track, video_index, video_item = stack.get_item("video")
    assert stack.tracks()[video_track].start_time_of_item(video_index) == new_start
    assert stack.tracks()[audio_track].start_time_of_item(audio_index) == new_start
    assert video_item.duration() == new_duration
    assert audio_item.duration() == new_duration


def test_resize_item_moves_selected_split_linked_group_by_selected_delta():
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
    assert stack.split_item_at_time("primary", 2.0)
    primary_track = stack.get_item("primary")[0]
    audio_track = stack.get_item(audio_id)[0]
    right_video_id = stack.tracks()[primary_track].items()[1].get_id()
    right_audio_id = stack.tracks()[audio_track].items()[1].get_id()
    right_link_group_id = maybe_link_group_id(stack.get_item(right_video_id)[2])
    assert right_link_group_id == maybe_link_group_id(stack.get_item(right_audio_id)[2])
    assert right_link_group_id != result["link_group_id"]

    assert stack.resize_item("primary", 1.0, 1.0, "override", False)

    for item_id in ["primary", audio_id]:
        track_index, item_index, item = stack.get_item(item_id)
        assert stack.tracks()[track_index].start_time_of_item(item_index) == 1.0
        assert item.duration() == 1.0
    for item_id in [right_video_id, right_audio_id]:
        _, _, item = stack.get_item(item_id)
        assert item.duration() == 1.0
        assert maybe_link_group_id(item) == right_link_group_id
