"""Regression checks through the same Python adapter used by visual dragging.

Run after rebuilding bindings: bindings/python/.venv/bin/python -m unittest
discover -s tools/timeline-ui -p 'test_*.py'
"""
import unittest

import server as ui
from tellers_timeline import Clip, Gap, Item, MediaReference, Stack, Track


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


class MoveRegressionTests(unittest.TestCase):
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


if __name__ == "__main__":
    unittest.main()
