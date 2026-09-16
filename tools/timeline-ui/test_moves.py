"""Regression checks through the same Python adapter used by visual dragging.

Run after rebuilding bindings: bindings/python/.venv/bin/python -m unittest
discover -s tools/timeline-ui -p 'test_*.py'
"""
import unittest

import server as ui
from tellers_timeline import Clip, Gap, Item, MediaReference, Stack, Track


LONG_CLIP_DURATION = 90
LONG_CLIPS_PER_TRACK = 360  # Nine hours per track.


def fixture(grouped=True):
    state = ui.State()
    tracks = []
    for name, kind, start in [("a1", "audio", 0), ("v1", "video", 0),
                              ("a2", "audio", 4), ("v2", "video", 4)]:
        clip = Item.from_clip(Clip(2, {"DEFAULT_MEDIA": MediaReference("file:///test")},
                                   id=name + "c", name=name))
        items = ([Item.from_gap(Gap(start))] if start else []) + [clip]
        tracks.append(Track(kind=kind, id=name, children=items))
    state.stack = Stack(tracks)
    state.stack.link_item(["a1c", "v1c"])
    state.stack.link_item(["a2c", "v2c"])
    if grouped:
        state.stack.group_item(["v1c", "v2c"])
    return state


def long_form_fixture():
    """Ten tracks / 3,600 clips, exercised through the UI operation adapter."""
    state = ui.State()
    tracks = []
    for track_number in range(10):
        kind = "video" if track_number % 2 == 0 else "audio"
        clips = [Item.from_clip(Clip(
            LONG_CLIP_DURATION,
            {"DEFAULT_MEDIA": MediaReference("file:///long-form-placeholder")},
            id=f"{track_number}-{clip_number}",
        )) for clip_number in range(LONG_CLIPS_PER_TRACK)]
        tracks.append(Track(kind=kind, id=f"track-{track_number}", children=clips))
    state.stack = Stack(tracks)
    state.stack.link_item(["0-100", "1-100"])
    state.stack.link_item(["2-120", "3-120"])
    state.stack.group_item(["0-100", "2-120"])
    return state


class MoveRegressionTests(unittest.TestCase):
    def test_replace_operation_accepts_optional_linked_audio(self):
        state = fixture(grouped=False)
        ok, _ = ui.op_replace_item(state, {
            "id": "v1c",
            "item": {"kind": "clip", "name": "Replacement", "duration": 3,
                     "url": "file:///replacement.mov"},
            "linked_audio": [{"kind": "clip", "name": "Replacement audio", "duration": 3,
                              "url": "file:///replacement.wav"}],
        })
        self.assertTrue(ok)
        view = state.view()
        video = next(item for track in view["tracks"] if track["id"] == "v1"
                     for item in track["items"] if item["kind"] == "clip")
        self.assertEqual((video["id"], video["name"], video["duration"]),
                         ("v1c", "Replacement", 3))
        audio = next(item for track in view["tracks"] if track["id"] == "a1"
                     for item in track["items"] if item["name"] == "Replacement audio")
        self.assertEqual(audio["duration"], 3)

    def test_repeated_grouped_sync_moves(self):
        for selected in ["a1c", "v1c", "a2c", "v2c"]:
            for leave_gap in [True, False]:
                for overlap in ["override", "push"]:
                    with self.subTest(selected=selected, leave_gap=leave_gap, overlap=overlap):
                        state = fixture()
                        for start in [2, 8, 1, 5]:
                            ok, _ = ui.op_move_item(state, {
                                "id": selected, "track_id": selected[:-1],
                                "time": start + (4 if "2" in selected else 0),
                                "replace_with_gap": leave_gap, "mode": "time",
                                "insert_policy": "split_and_insert", "overlap_policy": overlap,
                            })
                            self.assertTrue(ok)
                            view = state.view()
                            self.assertEqual([t["id"] for t in view["tracks"]], ["a1", "v1", "a2", "v2"])
                            for track in view["tracks"]:
                                clips = [i for i in track["items"] if i["kind"] == "clip"]
                                self.assertEqual(len(clips), 1)
                                clip = clips[0]
                                self.assertEqual(clip["id"], track["id"] + "c")
                                self.assertEqual(clip["duration"], 2)
                                self.assertEqual(clip["start"], start + (4 if "2" in track["id"] else 0))
                                self.assertIsNotNone(clip["group_id"])

    def test_negative_group_move_rolls_back(self):
        state = fixture()
        before = state.snapshot()
        ok, _ = ui.op_move_item(state, {"id": "v2c", "track_id": "v2", "time": 1,
                                      "insert_policy": "split_and_insert"})
        self.assertFalse(ok)
        self.assertEqual(state.snapshot(), before)

    def test_long_form_ten_track_drag_modes(self):
        end = LONG_CLIP_DURATION * LONG_CLIPS_PER_TRACK

        # The normal UI time-drop path: grouped + synced members move together.
        state = long_form_fixture()
        ok, _ = ui.op_move_item(state, {
            "id": "0-100", "track_id": "track-0", "time": end,
            "replace_with_gap": True, "mode": "time",
            "insert_policy": "split_and_insert", "overlap_policy": "override",
        })
        self.assertTrue(ok)
        view = state.view()
        self.assertEqual(len(view["tracks"]), 10)
        expected = {"0-100": ("track-0", end), "1-100": ("track-1", end),
                    "2-120": ("track-2", end + 1800), "3-120": ("track-3", end + 1800)}
        found = {item["id"]: (track["id"], item["start"], item["duration"])
                 for track in view["tracks"] for item in track["items"] if item["kind"] == "clip"}
        for item_id, (track_id, start) in expected.items():
            self.assertEqual(found[item_id], (track_id, start, LONG_CLIP_DURATION))

        # Index-drop and Timeline.move_item are the other drag entry points.
        for mode, payload in [
            ("index", {"index": LONG_CLIPS_PER_TRACK}),
            ("timeline", {"time": end}),
        ]:
            state = long_form_fixture()
            ok, _ = ui.op_move_item(state, {
                "id": "0-100", "track_id": "track-0", "mode": mode,
                "replace_with_gap": False, "overlap_policy": "override",
                "insert_policy": "split_and_insert", **payload,
            })
            self.assertTrue(ok)
            self.assertEqual(len(state.view()["tracks"]), 10)
            for item_id, track_id in [("0-100", "track-0"), ("1-100", "track-1"),
                                      ("2-120", "track-2"), ("3-120", "track-3")]:
                self.assertEqual(next(track["id"] for track in state.view()["tracks"]
                                      if any(item["id"] == item_id for item in track["items"])), track_id)

        # An ordinary clip remains an independent move on a different video track.
        state = long_form_fixture()
        ok, _ = ui.op_move_item(state, {
            "id": "4-200", "track_id": "track-4", "time": end,
            "replace_with_gap": False, "mode": "time",
            "insert_policy": "split_and_insert", "overlap_policy": "push",
        })
        self.assertTrue(ok)
        item = next(item for track in state.view()["tracks"] if track["id"] == "track-4"
                    for item in track["items"] if item["id"] == "4-200")
        self.assertEqual((item["start"], item["duration"]), (end, LONG_CLIP_DURATION))


if __name__ == "__main__":
    unittest.main()
