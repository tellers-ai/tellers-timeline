import json

from tellers_timeline import Clip, Gap, Item, MediaReference, Stack, Timeline, Track

# These tests cover ONLY the Python <-> Rust translation layer: argument
# marshalling, result shapes, metadata round-tripping, and error mapping.
# Timeline behavior (track placement, reuse, sync propagation, and the
# semantics of insert/move/resize/split/delete) is covered by the Rust test
# suite in tellers-timeline-core and must not be re-tested here.


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


def test_track_timeline_ids_returns_child_item_ids_in_order():
    track = Track(
        kind="video",
        id="track",
        children=[
            Item.from_clip(
                Clip(1.0, {"DEFAULT_MEDIA": MediaReference("file:///one.mov")}, id="clip-1")
            ),
            Item.from_gap(Gap(1.0, id="gap-1")),
            Item.from_clip(
                Clip(1.0, {"DEFAULT_MEDIA": MediaReference("file:///two.mov")}, id="clip-2")
            ),
        ],
    )

    assert track.timeline_ids() == ["clip-1", "gap-1", "clip-2"]


def test_insert_with_linked_audio_translates_to_synced_result_dict():
    stack = Stack([Track(kind="video")])
    primary = Clip(2.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="primary")
    audio_ref = MediaReference(
        "file:///audio.wav",
        metadata_json=json.dumps({"media_id": "asset-1", "tellers.ai": {"media_id": "asset-1"}}),
    )
    audio = Clip(2.0, {"DEFAULT_MEDIA": audio_ref}, id="audio")

    result = stack.insert_item_at_time(
        0, 0.0, primary, "override", "split_and_insert", [audio]
    )

    # The Rust Synced result enum is translated into a dict with these keys.
    assert isinstance(result, dict)
    assert set(result) == {
        "primary_clip_id",
        "audio_clips",
        "linked_video_clip_id",
        "link_group_id",
        "created_track_indices",
    }
    assert result["primary_clip_id"] == "primary"
    assert isinstance(result["audio_clips"], list)
    assert len(result["audio_clips"]) == 1
    assert result["linked_video_clip_id"] is None
    assert result["link_group_id"] is not None
    assert isinstance(result["created_track_indices"], list)

    # The generated sync id is written into the clip metadata, and media
    # reference metadata round-trips through the binding unchanged.
    primary_item = stack.get_item("primary")[2]
    assert maybe_link_group_id(primary_item) == result["link_group_id"]
    audio_item = stack.get_item("audio")[2]
    media = audio_item.get_media_references()["DEFAULT_MEDIA"]
    media_metadata = json.loads(media.get_metadata_json())
    assert media_metadata["media_id"] == "asset-1"
    assert media_metadata["tellers.ai"]["media_id"] == "asset-1"


def test_insert_without_linked_audio_translates_to_item_id_str():
    stack = Stack([Track(kind="video")])

    result = stack.insert_item_at_time(
        0,
        0.0,
        Clip(2.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="primary"),
        "override",
        "split_and_insert",
    )

    assert result == "primary"


def test_insert_item_at_index_translates_to_synced_result_dict():
    stack = Stack([Track(kind="video", id="v", children=[Item.from_gap(Gap(5.0))])])

    result = stack.insert_item_at_index(
        "v",
        0,
        Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="primary"),
        "override",
        [Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")}, id="audio")],
    )

    assert isinstance(result, dict)
    assert result["primary_clip_id"] == "primary"
    assert len(result["audio_clips"]) == 1


def test_insert_with_linked_video_translates_to_synced_result_dict():
    stack = Stack(
        [
            Track(kind="audio", id="a", children=[Item.from_gap(Gap(10.0))]),
            Track(kind="video", id="v", children=[Item.from_gap(Gap(10.0))]),
        ]
    )
    audio = Clip(2.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")}, id="audio")
    video = Clip(2.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="video")

    result = stack.insert_item_at_time(
        0,
        0.0,
        audio,
        "override",
        "split_and_insert",
        linked_video_clip=video,
    )

    assert isinstance(result, dict)
    assert result["primary_clip_id"] == "audio"
    assert result["linked_video_clip_id"] == "video"
    assert result["audio_clips"] == []
    assert result["link_group_id"] is not None
    assert stack.get_item("video") is not None


def test_linked_audio_clips_require_clip_item():
    stack = Stack([Track(kind="audio", children=[Item.from_gap(Gap(3.0, id="gap"))])])

    assert_type_error_message(
        lambda: stack.insert_item_at_time(
            0,
            0.0,
            Gap(3.0),
            "override",
            "split_and_insert",
            [Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")})],
        ),
        "linked_audio_clips and linked_video_clip can only be used when item is a Clip",
    )

    assert_type_error_message(
        lambda: stack.replace_item(
            "gap",
            Gap(3.0),
            [Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")})],
        ),
        "linked_audio_clips can only be used when item is a Clip",
    )


def test_link_and_unlink_item_translate_return_types():
    stack = Stack(
        [
            Track(
                kind="video",
                children=[
                    Item.from_clip(
                        Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="primary")
                    )
                ],
            ),
            Track(
                kind="audio",
                children=[
                    Item.from_clip(
                        Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")}, id="audio")
                    )
                ],
            ),
        ]
    )

    group = stack.link_item(["primary", "audio"])
    assert isinstance(group, int)

    removed = stack.unlink_item(["primary", "audio"])
    assert isinstance(removed, int)


def test_group_and_ungroup_item_translate_return_types():
    stack = Stack(
        [
            Track(
                kind="video",
                children=[
                    Item.from_clip(
                        Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///a.mov")}, id="a")
                    )
                ],
            ),
            Track(
                kind="audio",
                children=[
                    Item.from_clip(
                        Clip(3.0, {"DEFAULT_MEDIA": MediaReference("file:///b.wav")}, id="b")
                    )
                ],
            ),
        ]
    )

    group_id = stack.group_item(["a", "b"])
    assert isinstance(group_id, int)

    # The Tellers group id round-trips through the clip's tellers.ai metadata.
    metadata = json.loads(stack.get_item("a")[2].get_metadata_json())
    assert metadata.get("tellers.ai", {}).get("Tellers Group ID") == group_id

    removed = stack.ungroup_item(["a"])
    assert isinstance(removed, int)


def test_sync_track_info_translates_to_list_of_dicts():
    stack = Stack([Track(kind="video", id="v")])
    stack.insert_item_at_time(
        0,
        0.0,
        Clip(4.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="vid"),
        "override",
        "split_and_insert",
        [Clip(4.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")})],
    )

    groups = stack.sync_track_info()
    assert isinstance(groups, list)
    assert groups
    group = groups[0]
    assert set(group) == {
        "track_indices",
        "track_ids",
    }
    assert isinstance(group["track_indices"], list)
    assert isinstance(group["track_ids"], list)


def test_stack_edit_methods_translate_return_types():
    stack = Stack([Track(kind="video", id="v"), Track(kind="audio", id="a")])
    stack.insert_item_at_time(
        0,
        0.0,
        Clip(4.0, {"DEFAULT_MEDIA": MediaReference("file:///video.mov")}, id="primary"),
        "override",
        "split_and_insert",
        [Clip(4.0, {"DEFAULT_MEDIA": MediaReference("file:///audio.wav")}, id="audio")],
    )

    # Each edit method is callable through the binding and its result maps to the
    # documented Python type. Behavior is asserted in the Rust suite.
    assert isinstance(stack.resize_item("primary", 0.0, 2.0, "override", True), bool)
    assert isinstance(stack.split_item_at_time("primary", 1.0), bool)
    assert isinstance(
        stack.replace_item(
            "primary",
            Clip(2.0, {"DEFAULT_MEDIA": MediaReference("file:///replacement.mov")}),
        ),
        bool,
    )
    assert isinstance(
        stack.move_item_at_time("primary", "a", 0.0, True, "override", "split_and_insert"),
        bool,
    )
    assert isinstance(stack.delete_item("primary", True), list)


def test_timeline_delegates_to_stack():
    timeline = Timeline(Stack([Track(kind="video", id="v")]))

    # Timeline exposes the same translated surface and delegates to its Stack.
    assert isinstance(timeline.get_stack(), Stack)
    assert isinstance(timeline.sync_track_info(), list)
