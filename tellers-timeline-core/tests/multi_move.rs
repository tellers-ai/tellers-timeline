// `Stack::move_items_at_time`: moving a multi-selection by dragging one of
// its clips. The dragged clip goes to the drop point; every other selected
// clip shifts by the same delta on its own track.

mod common;

use common::*;

fn video_track(id: &str, items: Vec<Item>) -> Track {
    let mut track = Track::new(TrackKind::Video, Some(id.to_string()));
    track.items = items;
    track
}

fn clip_item(duration: f64, id: &str) -> Item {
    Item::Clip(clip(duration, Some(id)))
}

fn gap(duration: f64) -> Item {
    Item::Gap(Gap::make_gap(duration))
}

fn start_of(stack: &Stack, item_id: &str) -> f64 {
    let (track_index, item_index, _) = stack.get_item(item_id).unwrap();
    stack.children[track_index].start_time_of_item(item_index)
}

fn track_id_of(stack: &Stack, item_id: &str) -> String {
    let (track_index, _, _) = stack.get_item(item_id).unwrap();
    stack.children[track_index].get_id().unwrap()
}

fn ids(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn move_items(stack: &mut Stack, anchor: &str, others: &[&str], track: &str, time: f64) -> bool {
    stack.move_items_at_time(
        anchor,
        &ids(others),
        track,
        time,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    )
}

/// v1: A at 0..2, v2: B at 4..6, v3: empty.
fn two_tracks() -> Stack {
    let mut stack = Stack::default();
    stack.children.push(video_track("v1", vec![clip_item(2.0, "A")]));
    stack
        .children
        .push(video_track("v2", vec![gap(4.0), clip_item(2.0, "B")]));
    stack.children.push(video_track("v3", vec![]));
    stack
}

#[test]
fn other_selected_clips_shift_by_the_anchor_delta() {
    let mut stack = two_tracks();
    assert!(move_items(&mut stack, "A", &["B"], "v1", 3.0));
    assert_eq!(start_of(&stack, "A"), 3.0);
    assert_eq!(start_of(&stack, "B"), 7.0);
    assert_eq!(track_id_of(&stack, "B"), "v2");
}

#[test]
fn only_the_anchor_changes_track() {
    let mut stack = two_tracks();
    assert!(move_items(&mut stack, "A", &["B"], "v3", 1.0));
    assert_eq!(track_id_of(&stack, "A"), "v3");
    assert_eq!(start_of(&stack, "A"), 1.0);
    assert_eq!(track_id_of(&stack, "B"), "v2");
    assert_eq!(start_of(&stack, "B"), 5.0);
}

#[test]
fn clips_on_one_track_keep_their_spacing_both_ways() {
    for (time, a, b, c) in [(2.0, 2.0, 5.0, 8.0), (0.0, 0.0, 3.0, 6.0)] {
        let mut stack = Stack::default();
        stack.children.push(video_track(
            "v1",
            vec![
                gap(1.0),
                clip_item(2.0, "A"), // 1..3
                gap(1.0),
                clip_item(2.0, "B"), // 4..6
                gap(1.0),
                clip_item(2.0, "C"), // 7..9
            ],
        ));
        assert!(move_items(&mut stack, "A", &["B", "C"], "v1", time));
        assert_eq!(
            (start_of(&stack, "A"), start_of(&stack, "B"), start_of(&stack, "C")),
            (a, b, c),
            "drop at {time}"
        );
    }
}

#[test]
fn a_delta_past_zero_is_clamped_to_the_earliest_clip() {
    let mut stack = two_tracks();
    // Dragging B to 1.0 is a delta of -3: A (at 0) would land at -3, so the
    // whole selection stops where A reaches 0.
    assert!(move_items(&mut stack, "B", &["A"], "v2", 1.0));
    assert_eq!(start_of(&stack, "A"), 0.0);
    assert_eq!(start_of(&stack, "B"), 4.0);
}

#[test]
fn a_selected_group_member_brings_its_whole_group() {
    let mut stack = two_tracks();
    stack
        .children
        .push(video_track("v4", vec![gap(1.0), clip_item(2.0, "G")]));
    stack.group_item(&ids(&["B", "G"])).unwrap();
    assert!(move_items(&mut stack, "A", &["B"], "v1", 2.0));
    assert_eq!(start_of(&stack, "A"), 2.0);
    assert_eq!(start_of(&stack, "B"), 6.0);
    assert_eq!(start_of(&stack, "G"), 3.0);
}

#[test]
fn a_selected_clip_brings_its_sync_partners() {
    let mut stack = stack_with_synced_audio_below_video();
    // linked-video / linked-audio at 2..4 on "v" / "a"; add a free clip X.
    stack
        .children
        .push(video_track("v2", vec![clip_item(1.0, "X")]));
    assert!(move_items(&mut stack, "X", &["linked-video"], "v2", 3.0));
    assert_eq!(start_of(&stack, "X"), 3.0);
    assert_eq!(start_of(&stack, "linked-video"), 5.0);
    assert_eq!(start_of(&stack, "linked-audio"), 5.0);
}

#[test]
fn ignored_others_fall_back_to_a_single_move() {
    let mut single = two_tracks();
    let mut multi = single.clone();
    assert!(single.move_item_at_time(
        "A",
        "v3",
        2.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));
    assert!(move_items(&mut multi, "A", &["A", "missing"], "v3", 2.0));
    for stack in [&single, &multi] {
        assert_eq!(track_id_of(stack, "A"), "v3");
        assert_eq!(start_of(stack, "A"), 2.0);
        assert_eq!(start_of(stack, "B"), 4.0);
    }
}

#[test]
fn a_refused_move_changes_nothing() {
    let mut stack = two_tracks();
    stack.children.push(Track::new(TrackKind::Audio, Some("a1".to_string())));
    let before = stack.clone();
    // A video clip cannot land on an audio track.
    assert!(!move_items(&mut stack, "A", &["B"], "a1", 1.0));
    assert_eq!(stack, before);
}

fn transition(id: &str) -> Item {
    Item::Transition(tellers_timeline_core::Transition::new(
        0.5,
        0.5,
        None,
        Some(id.to_string()),
    ))
}

fn track_ids(stack: &Stack, track: usize) -> Vec<String> {
    stack.children[track]
        .items
        .iter()
        .map(|it| match it {
            Item::Gap(_) => "gap".to_string(),
            _ => it.get_id().unwrap_or_default(),
        })
        .collect()
}

#[test]
fn transitions_move_with_the_selected_clips() {
    let mut stack = Stack::default();
    stack.children.push(video_track(
        "v1",
        vec![gap(2.0), clip_item(2.0, "A"), transition("tA"), clip_item(2.0, "A2")],
    ));
    stack
        .children
        .push(video_track("v2", vec![gap(2.0), clip_item(2.0, "B")]));
    assert!(move_items(&mut stack, "A", &["A2", "B"], "v1", 4.0));
    assert_eq!(track_ids(&stack, 0), vec!["gap", "A", "tA", "A2"]);
    assert_eq!(start_of(&stack, "A"), 4.0);
    assert_eq!(start_of(&stack, "A2"), 6.0);
    assert_eq!(start_of(&stack, "B"), 4.0);
}
