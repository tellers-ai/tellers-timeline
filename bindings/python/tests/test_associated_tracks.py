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


def test_track_accessors_round_trip_through_metadata():
    track = Track(kind="video", id="V1")
    assert track.get_associated_track_ids() == []
    track.set_associated_track_ids(["A1", "A2", "A1", "V1"])
    assert track.get_associated_track_ids() == ["A1", "A2"]
    assert json.loads(track.get_metadata_json())["tellers.ai"]["associated_track_ids"] == [
        "A1",
        "A2",
    ]
    assert track.clear_associated_track_ids() is True
    assert track.get_associated_track_ids() == []


def test_insert_follows_and_records_the_association():
    stack = Stack(
        [
            _gap_track("audio", "A1"),
            _gap_track("audio", "A2"),
            _gap_track("video", "V1"),
        ]
    )
    assert stack.associate_tracks("V1", ["A1", "A2"])
    assert stack.associated_track_ids("V1") == ["A1", "A2"]

    result = stack.insert_item_at_time(
        2,
        0.0,
        _clip("c", 2.0, "file:///c.mov"),
        "override",
        "split_and_insert",
        linked_audio_clips=[_audio("file:///l.wav", 2.0), _audio("file:///r.wav", 2.0)],
    )
    left_id, _ = result["audio_clips"][0]
    right_id, _ = result["audio_clips"][1]
    assert _track_of(stack, left_id) == "A1"
    assert _track_of(stack, right_id) == "A2"
    assert len(stack.tracks()) == 3

    # A1 and A2 are V1's: a plain audio clip is steered to another track.
    assert stack.free_audio_track_for(0.0, 1.0) is None
    stack.add_track(_gap_track("audio", "A3"), 0)
    assert stack.free_audio_track_for(0.0, 1.0) == "A3"

    timeline = Timeline()
    timeline.set_stack(stack)
    assert timeline.associated_track_ids("V1") == ["A1", "A2"]
    assert timeline.free_audio_track_for(0.0, 1.0) == "A3"


def test_track_groups_report_each_track_once():
    stack = Stack(
        [
            _gap_track("audio", "A2"),
            _gap_track("audio", "A1"),
            _gap_track("audio", "M1"),
            _gap_track("video", "V1"),
        ]
    )
    assert stack.associate_tracks("V1", ["A1", "A2"])
    groups = stack.track_groups()
    assert [(g["owner_track_id"], g["kind"], g["partner_track_ids"], g["stored"]) for g in groups] == [
        ("M1", "audio", [], False),
        ("V1", "video", ["A1", "A2"], True),
    ]
    assert groups[1]["partner_track_indices"] == [1, 0]
    timeline = Timeline()
    timeline.set_stack(stack)
    assert [g["owner_track_id"] for g in timeline.track_groups()] == ["M1", "V1"]


def test_prune_empty_partner_tracks_removes_auto_named_partners_only():
    voice = _gap_track("audio", "Voice")
    voice.set_name("Voice over")  # a user name: never pruned
    stack = Stack([voice, _gap_track("video", "V1")])
    result = stack.insert_item_at_time(
        1,
        0.0,
        _clip("c", 2.0, "file:///c.mov"),
        "override",
        "split_and_insert",
        linked_audio_clips=[_audio("file:///l.wav", 2.0), _audio("file:///r.wav", 2.0)],
    )
    assert result is not None
    partners = stack.associated_track_ids("V1")
    assert len(partners) == 2
    stack.delete_item("c", True)
    removed = stack.prune_empty_partner_tracks()
    # "Voice" is a user name: it stays; the created track goes.
    assert removed == [p for p in partners if p != "Voice"]
    assert stack.associated_track_ids("V1") == ["Voice"]
