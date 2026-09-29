// Tests for the Tellers group feature (group / ungroup, and group-aware move,
// delete, split). These are additive: existing behavior is covered elsewhere
// and must remain unchanged.

use tellers_timeline_core::{
    Clip, Gap, IdMetadataExt, InsertPolicy, Item, MediaReference, OverlapPolicy, RationalTime,
    Stack, TimeRange, Track, TrackKind,
};

fn range(duration: f64) -> TimeRange {
    TimeRange {
        otio_schema: "TimeRange.1".to_string(),
        start_time: RationalTime {
            otio_schema: "RationalTime.1".to_string(),
            rate: 1.0,
            value: 0.0,
        },
        duration: RationalTime {
            otio_schema: "RationalTime.1".to_string(),
            rate: 1.0,
            value: duration,
        },
    }
}

fn media_ref(url: &str) -> MediaReference {
    MediaReference::ExternalReference {
        target_url: url.to_string(),
        available_range: Some(range(100.0)),
        name: None,
        available_image_bounds: Some(serde_json::Value::Null),
        metadata: serde_json::json!({}),
    }
}

fn clip_item(duration: f64, id: &str) -> Item {
    Item::Clip(Clip::new_single_media_reference(
        range(duration),
        media_ref("file:///video.mov"),
        None,
        Some(id.to_string()),
    ))
}

fn audio_track(id: &str, items: Vec<Item>) -> Track {
    let mut track = Track::new(TrackKind::Audio, Some(id.to_string()));
    track.items = items;
    track
}

fn group_id(stack: &Stack, item_id: &str) -> Option<i64> {
    let (_, _, item) = stack.get_item(item_id)?;
    match item {
        Item::Clip(clip) => clip
            .metadata
            .get("tellers.ai")
            .and_then(|v| v.get("Tellers Group ID"))
            .and_then(|v| v.as_i64()),
        Item::Gap(_) => None,
    }
}

fn start_of(stack: &Stack, item_id: &str) -> f64 {
    let (track_index, item_index, _) = stack.get_item(item_id).unwrap();
    stack.children[track_index].start_time_of_item(item_index)
}

fn track_id_of(stack: &Stack, item_id: &str) -> String {
    let (track_index, _, _) = stack.get_item(item_id).unwrap();
    stack.children[track_index].get_id().unwrap()
}

// ----- group_item / ungroup_item -----

#[test]
fn group_pulls_in_sync_partners() {
    let mut stack = Stack::default();
    stack
        .children
        .push(audio_track("t1", vec![clip_item(2.0, "A")]));
    stack
        .children
        .push(audio_track("t2", vec![clip_item(2.0, "A_audio")]));
    stack
        .children
        .push(audio_track("t3", vec![clip_item(2.0, "B")]));
    stack
        .sync_item(&["A".to_string(), "A_audio".to_string()])
        .unwrap();

    let id = stack
        .group_item(&["A".to_string(), "B".to_string()])
        .expect("group should succeed");

    // A's sync partner A_audio is pulled into the group automatically.
    assert_eq!(group_id(&stack, "A"), Some(id));
    assert_eq!(group_id(&stack, "A_audio"), Some(id));
    assert_eq!(group_id(&stack, "B"), Some(id));
}

#[test]
fn group_returns_none_for_fewer_than_two_members() {
    let mut stack = Stack::default();
    stack
        .children
        .push(audio_track("t1", vec![clip_item(2.0, "A")]));

    assert_eq!(stack.group_item(&["A".to_string()]), None);
    assert_eq!(group_id(&stack, "A"), None);
}

#[test]
fn group_reassigns_existing_membership() {
    let mut stack = Stack::default();
    stack
        .children
        .push(audio_track("t1", vec![clip_item(2.0, "A")]));
    stack
        .children
        .push(audio_track("t2", vec![clip_item(2.0, "B")]));
    stack
        .children
        .push(audio_track("t3", vec![clip_item(2.0, "C")]));

    let g1 = stack
        .group_item(&["A".to_string(), "B".to_string()])
        .unwrap();
    let g2 = stack
        .group_item(&["B".to_string(), "C".to_string()])
        .unwrap();

    assert_ne!(g1, g2);
    assert_eq!(group_id(&stack, "A"), Some(g1));
    assert_eq!(group_id(&stack, "B"), Some(g2));
    assert_eq!(group_id(&stack, "C"), Some(g2));
}

#[test]
fn ungroup_clears_whole_group() {
    let mut stack = Stack::default();
    stack
        .children
        .push(audio_track("t1", vec![clip_item(2.0, "A")]));
    stack
        .children
        .push(audio_track("t2", vec![clip_item(2.0, "A_audio")]));
    stack
        .children
        .push(audio_track("t3", vec![clip_item(2.0, "B")]));
    stack
        .sync_item(&["A".to_string(), "A_audio".to_string()])
        .unwrap();
    stack
        .group_item(&["A".to_string(), "B".to_string()])
        .unwrap();

    // Ungrouping from any single member clears the whole group.
    let removed = stack.ungroup_item(&["B".to_string()]);
    assert_eq!(removed, 3);
    assert_eq!(group_id(&stack, "A"), None);
    assert_eq!(group_id(&stack, "A_audio"), None);
    assert_eq!(group_id(&stack, "B"), None);
}

// ----- group-aware move -----

#[test]
fn group_move_rejects_negative_member_time_atomically() {
    let mut stack = spaced_group();
    let before = stack.clone();
    assert!(!stack.move_item_at_time(
        "B",
        "t1",
        1.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override
    ));
    assert_eq!(stack, before);
}

fn spaced_group() -> Stack {
    let mut stack = Stack::default();
    stack.children.push(audio_track(
        "t1",
        vec![
            clip_item(2.0, "A"),
            Item::Gap(Gap::make_gap(2.0)),
            clip_item(2.0, "B"),
        ],
    ));
    stack.children.push(audio_track("t2", vec![]));
    stack.group_item(&["A".into(), "B".into()]).unwrap();
    stack
}

#[test]
fn ripple_group_move_preserves_spacing() {
    for time in [1.0, 3.0, 7.0] {
        let mut stack = spaced_group();
        assert!(stack.move_item_at_time(
            "A",
            "t1",
            time,
            false,
            InsertPolicy::SplitAndInsert,
            OverlapPolicy::Override
        ));
        assert_eq!(start_of(&stack, "A"), time);
        assert_eq!(start_of(&stack, "B"), time + 4.0);
    }
}

#[test]
fn index_move_honors_group() {
    let mut stack = spaced_group();
    stack.children[1].items.push(Item::Gap(Gap::make_gap(2.0)));
    assert!(stack.move_item_at_index("A", "t2", 1, true, OverlapPolicy::Override));
    assert_eq!(start_of(&stack, "A"), 2.0);
    assert_eq!(start_of(&stack, "B"), 6.0);
}

#[test]
fn horizontal_sync_moves_preserve_all_track_assignments() {
    for selected in ["left", "right", "video"] {
        let mut stack = Stack::default();
        for (id, kind) in [
            ("left", TrackKind::Audio),
            ("right", TrackKind::Audio),
            ("video", TrackKind::Video),
        ] {
            let mut track = Track::new(kind, Some(format!("{id}-track")));
            track.items.push(clip_item(2.0, id));
            stack.children.push(track);
        }
        stack
            .sync_item(&["left".into(), "right".into(), "video".into()])
            .unwrap();
        for time in [3.0, 6.0, 1.0] {
            assert!(stack.move_item_at_time(
                selected,
                &format!("{selected}-track"),
                time,
                true,
                InsertPolicy::SplitAndInsert,
                OverlapPolicy::Override
            ));
            assert_eq!(stack.children.len(), 3);
            for id in ["left", "right", "video"] {
                assert_eq!(track_id_of(&stack, id), format!("{id}-track"));
                assert_eq!(start_of(&stack, id), time);
            }
        }
    }
}

#[test]
fn synced_insert_reuses_adjacent_empty_audio_tracks() {
    let mut stack = Stack::default();
    for (id, kind) in [
        ("a1", TrackKind::Audio),
        ("a2", TrackKind::Audio),
        ("v1", TrackKind::Video),
        ("v2", TrackKind::Video),
    ] {
        stack.children.push(Track::new(kind, Some(id.into())));
    }
    assert!(stack
        .insert_item_at_time(
            2,
            0.0,
            clip_item(2.0, "v"),
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
            Some(vec![clip_item(2.0, "a"), clip_item(2.0, "b")]),
            None
        )
        .is_some());
    assert_eq!(stack.children.len(), 4);
    assert_eq!(track_id_of(&stack, "a"), "a2");
    assert_eq!(track_id_of(&stack, "b"), "a1");
}

#[test]
fn grouped_sync_columns_keep_tracks_through_repeated_moves() {
    for selected in ["a1c", "v1c", "a2c", "v2c"] {
        for leave_gap in [true, false] {
            let mut stack = Stack::default();
            for (id, kind, start, clip) in [
                ("a1", TrackKind::Audio, 0.0, "a1c"),
                ("v1", TrackKind::Video, 0.0, "v1c"),
                ("a2", TrackKind::Audio, 4.0, "a2c"),
                ("v2", TrackKind::Video, 4.0, "v2c"),
            ] {
                let mut track = Track::new(kind, Some(id.into()));
                if start > 0.0 {
                    track.items.push(Item::Gap(Gap::make_gap(start)));
                }
                track.items.push(clip_item(2.0, clip));
                stack.children.push(track);
            }
            stack.sync_item(&["a1c".into(), "v1c".into()]).unwrap();
            stack.sync_item(&["a2c".into(), "v2c".into()]).unwrap();
            stack.group_item(&["v1c".into(), "v2c".into()]).unwrap();
            let selected_track = track_id_of(&stack, selected);
            for first_start in [2.0, 8.0, 1.0, 5.0] {
                let selected_time = first_start + if selected.contains('2') { 4.0 } else { 0.0 };
                assert!(stack.move_item_at_time(
                    selected,
                    &selected_track,
                    selected_time,
                    leave_gap,
                    InsertPolicy::SplitAndInsert,
                    OverlapPolicy::Override
                ));
                assert_eq!(stack.children.len(), 4);
                for (clip, track, offset) in [
                    ("a1c", "a1", 0.0),
                    ("v1c", "v1", 0.0),
                    ("a2c", "a2", 4.0),
                    ("v2c", "v2", 4.0),
                ] {
                    assert_eq!(track_id_of(&stack, clip), track);
                    assert_eq!(start_of(&stack, clip), first_start + offset);
                    assert!(group_id(&stack, clip).is_some());
                }
            }
        }
    }
}

#[test]
fn moving_many_audio_partners_creates_only_missing_tracks() {
    let mut stack = Stack::default();
    for (id, kind) in [
        ("dest-a", TrackKind::Audio),
        ("dest-v", TrackKind::Video),
        ("a1", TrackKind::Audio),
        ("a2", TrackKind::Audio),
        ("a3", TrackKind::Audio),
        ("src-v", TrackKind::Video),
    ] {
        let mut track = Track::new(kind, Some(id.into()));
        if !id.starts_with("dest") {
            track.items.push(clip_item(2.0, &format!("{id}c")));
        }
        stack.children.push(track);
    }
    stack
        .sync_item(&["a1c".into(), "a2c".into(), "a3c".into(), "src-vc".into()])
        .unwrap();
    assert!(stack.move_item_at_time(
        "src-vc",
        "dest-v",
        3.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override
    ));
    assert_eq!(stack.children.len(), 8);
    let mut tracks = std::collections::HashSet::new();
    for id in ["a1c", "a2c", "a3c"] {
        let track = track_id_of(&stack, id);
        assert!(!["a1", "a2", "a3"].contains(&track.as_str()));
        assert!(tracks.insert(track));
        assert_eq!(start_of(&stack, id), 3.0);
    }
}

#[test]
fn grouped_move_includes_offset_link_partners() {
    let mut stack = Stack::default();
    let mut video = Track::new(TrackKind::Video, Some("video".into()));
    video.items = vec![clip_item(5.0, "v1"), clip_item(3.0, "v2")];
    let mut audio = audio_track(
        "audio",
        vec![Item::Gap(Gap::make_gap(2.0)), clip_item(5.0, "a1")],
    );
    // Model imported OTIO: only the video clips carry the Tellers group, while
    // v1 and its offset audio partner share Resolve link metadata.
    for item in &mut video.items {
        if let Item::Clip(clip) = item {
            clip.metadata["tellers.ai"]["Tellers Group ID"] = serde_json::json!(1);
        }
    }
    for item in [&mut video.items[0], &mut audio.items[1]] {
        if let Item::Clip(clip) = item {
            clip.metadata["Resolve_OTIO"] = serde_json::json!({"Link Group ID": 1});
        }
    }
    stack.children = vec![video, audio];
    assert!(stack.move_item_at_time(
        "v1",
        "video",
        10.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override
    ));
    assert_eq!(start_of(&stack, "v1"), 10.0);
    assert_eq!(start_of(&stack, "a1"), 12.0);
    assert_eq!(start_of(&stack, "v2"), 15.0);
    assert_eq!(stack.children.len(), 2);
    for id in ["v1", "a1"] {
        assert_eq!(
            tellers_timeline_core::item_link_group_id(stack.get_item(id).unwrap().2),
            Some(1)
        );
    }
}

#[test]
fn move_group_shifts_all_members_by_same_delta() {
    let mut stack = Stack::default();
    stack
        .children
        .push(audio_track("t1", vec![clip_item(2.0, "A")]));
    stack
        .children
        .push(audio_track("t2", vec![clip_item(2.0, "B")]));
    stack
        .group_item(&["A".to_string(), "B".to_string()])
        .unwrap();

    assert!(stack.move_item_at_time(
        "A",
        "t1",
        5.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    assert_eq!(start_of(&stack, "A"), 5.0);
    assert_eq!(start_of(&stack, "B"), 5.0);
    // Neither clip changed track.
    assert_eq!(track_id_of(&stack, "A"), "t1");
    assert_eq!(track_id_of(&stack, "B"), "t2");
}

#[test]
fn move_group_changes_only_selected_track() {
    let mut stack = Stack::default();
    stack
        .children
        .push(audio_track("t1", vec![clip_item(2.0, "A")]));
    stack
        .children
        .push(audio_track("t2", vec![clip_item(2.0, "B")]));
    stack
        .children
        .push(audio_track("t3", vec![Item::Gap(Gap::make_gap(1.0))]));

    stack
        .group_item(&["A".to_string(), "B".to_string()])
        .unwrap();

    assert!(stack.move_item_at_time(
        "A",
        "t3",
        3.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    // Only the selected clip A switches tracks; B stays on its own track and
    // shifts by the same delta (0 -> 3).
    assert_eq!(track_id_of(&stack, "A"), "t3");
    assert_eq!(start_of(&stack, "A"), 3.0);
    assert_eq!(track_id_of(&stack, "B"), "t2");
    assert_eq!(start_of(&stack, "B"), 3.0);
}

#[test]
fn move_group_moves_whole_sync_column_of_selected() {
    // Realistic sync column: a video clip above its paired audio clip. The
    // grouped standalone clip B lives on a separate video track so it is not in
    // the audio sync cluster that the column move reorganizes.
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(clip_item(2.0, "A"));
    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio.items.push(clip_item(2.0, "A_audio"));
    let mut video_b = Track::new(TrackKind::Video, Some("v2".to_string()));
    video_b.items.push(clip_item(2.0, "B"));

    let mut stack = Stack::default();
    stack.children.push(audio);
    stack.children.push(video);
    stack.children.push(video_b);
    stack
        .sync_item(&["A".to_string(), "A_audio".to_string()])
        .unwrap();
    stack
        .group_item(&["A".to_string(), "B".to_string()])
        .unwrap();

    assert!(stack.move_item_at_time(
        "A",
        "v",
        4.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    assert_eq!(start_of(&stack, "A"), 4.0);
    assert_eq!(start_of(&stack, "A_audio"), 4.0);
    assert_eq!(start_of(&stack, "B"), 4.0);
}

#[test]
fn move_group_orders_forward_moves_biggest_start_first() {
    // Two grouped clips on the same track. Shifting both forward would collide if
    // applied left-to-right, so forward moves are applied biggest-start first.
    let mut stack = Stack::default();
    stack.children.push(audio_track(
        "t1",
        vec![
            clip_item(2.0, "A"),           // 0..2
            Item::Gap(Gap::make_gap(2.0)), // 2..4
            clip_item(2.0, "B"),           // 4..6
        ],
    ));
    stack
        .group_item(&["A".to_string(), "B".to_string()])
        .unwrap();

    // delta = +4: A 0->4, B 4->8.
    assert!(stack.move_item_at_time(
        "A",
        "t1",
        4.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    assert_eq!(start_of(&stack, "A"), 4.0);
    assert_eq!(start_of(&stack, "B"), 8.0);
}

#[test]
fn move_group_orders_backward_moves_smallest_start_first() {
    // Two grouped clips on the same track. Shifting both backward would collide
    // if applied right-to-left, so backward moves are applied smallest-start
    // first.
    let mut stack = Stack::default();
    stack.children.push(audio_track(
        "t1",
        vec![
            Item::Gap(Gap::make_gap(4.0)), // 0..4
            clip_item(2.0, "B"),           // 4..6
            Item::Gap(Gap::make_gap(2.0)), // 6..8
            clip_item(2.0, "A"),           // 8..10
        ],
    ));
    stack
        .group_item(&["A".to_string(), "B".to_string()])
        .unwrap();

    // delta = -4: A 8->4, B 4->0.
    assert!(stack.move_item_at_time(
        "A",
        "t1",
        4.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    assert_eq!(start_of(&stack, "A"), 4.0);
    assert_eq!(start_of(&stack, "B"), 0.0);
}

#[test]
fn move_ungrouped_clip_is_unaffected() {
    let mut stack = Stack::default();
    stack
        .children
        .push(audio_track("t1", vec![clip_item(2.0, "A")]));
    stack
        .children
        .push(audio_track("t2", vec![clip_item(2.0, "B")]));

    assert!(stack.move_item_at_time(
        "A",
        "t1",
        5.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    assert_eq!(start_of(&stack, "A"), 5.0);
    // B is not grouped, so it does not move.
    assert_eq!(start_of(&stack, "B"), 0.0);
}

// ----- group-aware delete -----

#[test]
fn delete_group_removes_all_members() {
    let mut stack = Stack::default();
    stack
        .children
        .push(audio_track("t1", vec![clip_item(2.0, "A")]));
    stack
        .children
        .push(audio_track("t2", vec![clip_item(2.0, "A_audio")]));
    stack
        .children
        .push(audio_track("t3", vec![clip_item(2.0, "B")]));
    stack
        .sync_item(&["A".to_string(), "A_audio".to_string()])
        .unwrap();
    stack
        .group_item(&["A".to_string(), "B".to_string()])
        .unwrap();

    let removed = stack.delete_item("A", true);
    assert!(removed.len() >= 3);
    assert!(stack.get_item("A").is_none());
    assert!(stack.get_item("A_audio").is_none());
    assert!(stack.get_item("B").is_none());
}

#[test]
fn delete_group_collapse_removes_all_members() {
    let mut stack = Stack::default();
    stack
        .children
        .push(audio_track("t1", vec![clip_item(2.0, "A")]));
    stack
        .children
        .push(audio_track("t2", vec![clip_item(2.0, "B")]));
    stack
        .group_item(&["A".to_string(), "B".to_string()])
        .unwrap();

    let removed = stack.delete_item("A", false);
    assert!(removed.len() >= 2);
    assert!(stack.get_item("A").is_none());
    assert!(stack.get_item("B").is_none());
}

// ----- group-aware split -----

#[test]
fn split_keeps_group_on_both_halves() {
    let mut stack = Stack::default();
    stack
        .children
        .push(audio_track("t1", vec![clip_item(4.0, "A")]));
    stack
        .children
        .push(audio_track("t2", vec![clip_item(4.0, "B")]));
    let g = stack
        .group_item(&["A".to_string(), "B".to_string()])
        .unwrap();

    assert!(stack.split_item_at_time("A", 2.0));

    // Left half keeps the id "A"; the right half is a new clip on the same
    // track. Both retain the group id.
    let (track_index, item_index, _) = stack.get_item("A").unwrap();
    assert_eq!(group_id(&stack, "A"), Some(g));
    let right = &stack.children[track_index].items[item_index + 1];
    let right_group = match right {
        Item::Clip(clip) => clip
            .metadata
            .get("tellers.ai")
            .and_then(|v| v.get("Tellers Group ID"))
            .and_then(|v| v.as_i64()),
        Item::Gap(_) => None,
    };
    assert_eq!(right_group, Some(g));
}
