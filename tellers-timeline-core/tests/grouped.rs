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
    stack.children.push(audio_track("t1", vec![clip_item(2.0, "A")]));
    stack
        .children
        .push(audio_track("t2", vec![clip_item(2.0, "A_audio")]));
    stack.children.push(audio_track("t3", vec![clip_item(2.0, "B")]));
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
    stack.children.push(audio_track("t1", vec![clip_item(2.0, "A")]));

    assert_eq!(stack.group_item(&["A".to_string()]), None);
    assert_eq!(group_id(&stack, "A"), None);
}

#[test]
fn group_reassigns_existing_membership() {
    let mut stack = Stack::default();
    stack.children.push(audio_track("t1", vec![clip_item(2.0, "A")]));
    stack.children.push(audio_track("t2", vec![clip_item(2.0, "B")]));
    stack.children.push(audio_track("t3", vec![clip_item(2.0, "C")]));

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
    stack.children.push(audio_track("t1", vec![clip_item(2.0, "A")]));
    stack
        .children
        .push(audio_track("t2", vec![clip_item(2.0, "A_audio")]));
    stack.children.push(audio_track("t3", vec![clip_item(2.0, "B")]));
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
fn move_group_shifts_all_members_by_same_delta() {
    let mut stack = Stack::default();
    stack.children.push(audio_track("t1", vec![clip_item(2.0, "A")]));
    stack.children.push(audio_track("t2", vec![clip_item(2.0, "B")]));
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
    stack.children.push(audio_track("t1", vec![clip_item(2.0, "A")]));
    stack.children.push(audio_track("t2", vec![clip_item(2.0, "B")]));
    stack.children.push(audio_track("t3", vec![Item::Gap(Gap::make_gap(1.0))]));

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
    stack.children.push(audio_track("t1", vec![clip_item(2.0, "A")]));
    stack.children.push(audio_track("t2", vec![clip_item(2.0, "B")]));

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
    stack.children.push(audio_track("t1", vec![clip_item(2.0, "A")]));
    stack
        .children
        .push(audio_track("t2", vec![clip_item(2.0, "A_audio")]));
    stack.children.push(audio_track("t3", vec![clip_item(2.0, "B")]));
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
    stack.children.push(audio_track("t1", vec![clip_item(2.0, "A")]));
    stack.children.push(audio_track("t2", vec![clip_item(2.0, "B")]));
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
    stack.children.push(audio_track("t1", vec![clip_item(4.0, "A")]));
    stack.children.push(audio_track("t2", vec![clip_item(4.0, "B")]));
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
