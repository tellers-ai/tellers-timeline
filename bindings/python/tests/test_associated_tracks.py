import json

from tellers_timeline import Clip, Gap, Item, MediaReference, Stack, Timeline, Track


def _gap_track(kind: str, track_id: str, length: float = 20.0) -> Track:
    return Track(kind=kind, id=track_id, children=[Item.from_gap(Gap(length))])


def _clip(clip_id: str, duration: float, url: str) -> Clip:
    return Clip(duration, {"DEFAULT_MEDIA": MediaReference(url)}, id=clip_id)


def _audio(url: str, duration: float) -> Item:
    return Item.from_clip(Clip(duration, {"DEFAULT_MEDIA": MediaReference(url)}))


def _track_of(stack: Stack, item_id: str) -> str:
    track_index, _, _ = stack.get_item(item_id)
    return stack.tracks()[track_index].get_id()


def _insert_column(stack: Stack, track_index: int, time: float, clip_id: str, channels: int):
    return stack.insert_item_at_time(
        track_index,
        time,
        _clip(clip_id, 2.0, f"file:///{clip_id}.mov"),
        "override",
        "split_and_insert",
        linked_audio_clips=[_audio(f"file:///{clip_id}-{i}.wav", 2.0) for i in range(channels)],
    )


def test_association_is_computed_from_sync_clips_and_never_stored():
    # Bottom to top: A1, A2, V1. The nearest free track below V1 is A2.
    stack = Stack([_gap_track("audio", "A1"), _gap_track("audio", "A2"), _gap_track("video", "V1")])
    assert stack.associated_track_ids("V1") == []
    result = _insert_column(stack, 2, 0.0, "c", 2)
    first_id, _ = result["audio_clips"][0]
    second_id, _ = result["audio_clips"][1]
    assert _track_of(stack, first_id) == "A2"
    assert _track_of(stack, second_id) == "A1"
    assert stack.associated_track_ids("V1") == ["A2", "A1"]
    assert len(stack.tracks()) == 3
    # Nothing was written in the document for it.
    for track in stack.tracks():
        assert "associated_track_ids" not in json.loads(track.get_metadata_json()).get("tellers.ai", {})

    # A1 and A2 are V1's: a plain audio clip is steered to another track.
    assert stack.free_audio_track_for(0.0, 1.0) is None
    stack.add_track(_gap_track("audio", "A3"), 0)
    assert stack.free_audio_track_for(0.0, 1.0) == "A3"

    timeline = Timeline()
    timeline.set_stack(stack)
    assert timeline.associated_track_ids("V1") == ["A2", "A1"]
    assert timeline.free_audio_track_for(0.0, 1.0) == "A3"

    # Delete the column: the tracks hold no sync clip, so they are free again.
    stack.delete_item("c", True)
    assert stack.associated_track_ids("V1") == []
    assert stack.free_audio_track_for(0.0, 1.0) == "A3"  # lowest index wins


def test_track_groups_report_each_track_once():
    stack = Stack(
        [
            _gap_track("audio", "M1"),
            _gap_track("audio", "A2"),
            _gap_track("audio", "A1"),
            _gap_track("video", "V1"),
        ]
    )
    # Nearest first: A1 (index 2) then A2 (index 1); M1 stays free.
    assert _insert_column(stack, 3, 0.0, "c", 2) is not None
    groups = stack.track_groups()
    assert [(g["owner_track_id"], g["kind"], g["partner_track_ids"]) for g in groups] == [
        ("M1", "audio", []),
        ("V1", "video", ["A1", "A2"]),
    ]
    assert groups[1]["partner_track_indices"] == [2, 1]
    timeline = Timeline()
    timeline.set_stack(stack)
    assert [g["owner_track_id"] for g in timeline.track_groups()] == ["M1", "V1"]


def test_prune_empty_partner_tracks_removes_auto_named_tracks_only():
    voice = _gap_track("audio", "Voice")
    voice.set_name("Voice over")  # a user name: never pruned
    stack = Stack([voice, _gap_track("video", "V1")])
    assert _insert_column(stack, 1, 0.0, "c", 2) is not None
    partners = stack.associated_track_ids("V1")
    assert len(partners) == 2
    created = [p for p in partners if p != "Voice"]
    stack.delete_item("c", True)
    assert stack.prune_empty_partner_tracks() == created
    assert [t.get_id() for t in stack.tracks()] == ["Voice", "V1"]


def test_normalize_puts_each_group_together_owner_on_top():
    stack = Stack(
        [
            _gap_track("audio", "A1"),
            _gap_track("video", "V1"),
            _gap_track("audio", "A2"),
        ]
    )
    assert _insert_column(stack, 1, 0.0, "c", 2) is not None
    assert stack.normalize_track_order() is True
    order = [t.get_id() for t in stack.tracks()]
    assert order.index("V1") == 2, order
    assert set(order[:2]) == {"A1", "A2"}
    assert stack.normalize_track_order() is False
