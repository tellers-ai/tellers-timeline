//! Track groups: exclusive membership, audio-group owner, placement rules,
//! swaps, track deletion, normalization and manual sync.

mod common;
use common::*;

fn empty_track(kind: TrackKind, id: &str, len: f64) -> Track {
    let mut track = Track::new(kind, Some(id.to_string()));
    track.items.push(Item::Gap(Gap::make_gap(len)));
    track
}

fn track_id_of(stack: &Stack, item_id: &str) -> String {
    let (track_index, _, _) = stack.get_item(item_id).unwrap();
    stack.children[track_index].get_id().unwrap()
}

fn ids(stack: &Stack) -> Vec<String> {
    stack.children.iter().map(|t| t.get_id().unwrap()).collect()
}

fn named_audio(id: &str, duration: f64) -> Item {
    let mut item = audio_clip(duration, &format!("file:///{id}.wav"), None);
    item.set_id(Some(id.to_string()));
    item
}

/// Host layout: audio tracks first (bottom), video above.
fn host_stack(audio_ids: &[&str], video_ids: &[&str]) -> Stack {
    let mut stack = Stack::default();
    for id in audio_ids {
        stack.children.push(empty_track(TrackKind::Audio, id, 20.0));
    }
    for id in video_ids {
        stack.children.push(empty_track(TrackKind::Video, id, 20.0));
    }
    stack
}

fn insert_plain(stack: &mut Stack, track_id: &str, time: f64, item: Item) -> bool {
    stack
        .insert_item_at_time_by_id(
            track_id,
            time,
            item,
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
            None,
            None,
        )
        .is_some()
}

fn insert_column(stack: &mut Stack, track_id: &str, time: f64, id: &str, audio: Vec<Item>) -> bool {
    stack
        .insert_item_at_time_by_id(
            track_id,
            time,
            Item::Clip(clip(2.0, Some(id))),
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
            Some(audio),
            None,
        )
        .is_some()
}

#[test]
fn a_track_belongs_to_one_group() {
    let mut stack = host_stack(&["A1", "A2"], &["V1", "V2"]);
    assert!(stack.associate_tracks("V1", &["A1".into(), "A2".into()]));
    assert!(stack.associate_tracks("V2", &["A2".into()]));
    assert_eq!(
        stack.associated_track_ids("V1").unwrap(),
        vec!["A1".to_string()]
    );
    assert_eq!(
        stack.associated_track_ids("V2").unwrap(),
        vec!["A2".to_string()]
    );
    let a2 = track_index_by_id(&stack, "A2");
    assert_eq!(stack.group_owner_of(a2), track_index_by_id(&stack, "V2"));
    assert!(!stack.track_is_free(0));
}

#[test]
fn derived_membership_is_exclusive_too() {
    // A1 holds columns of both videos (two with V1, one with V2): it is V1's.
    let mut stack = Stack::default();
    let mut a1 = Track::new(TrackKind::Audio, Some("A1".into()));
    a1.items.push(synced_clip_item(1.0, "a-1", 1));
    a1.items.push(synced_clip_item(1.0, "a-2", 2));
    a1.items.push(synced_clip_item(1.0, "a-3", 3));
    let mut v1 = Track::new(TrackKind::Video, Some("V1".into()));
    v1.items.push(synced_clip_item(1.0, "v1-1", 1));
    v1.items.push(synced_clip_item(1.0, "v1-2", 2));
    let mut v2 = Track::new(TrackKind::Video, Some("V2".into()));
    v2.items.push(Item::Gap(Gap::make_gap(2.0)));
    v2.items.push(synced_clip_item(1.0, "v2-3", 3));
    stack.children.extend([a1, v1, v2]);
    assert_eq!(
        stack.associated_track_ids("V1").unwrap(),
        vec!["A1".to_string()]
    );
    assert!(stack.associated_track_ids("V2").unwrap().is_empty());
}

#[test]
fn audio_group_owner_is_the_highest_track() {
    let mut stack = host_stack(&["M1", "M2", "M3"], &["V1"]);
    // Stems on M1 with a partner: M1 is alone, so it owns the new group and
    // the partner goes to a free track.
    let m1 = track_index_by_id(&stack, "M1");
    let result = match stack.insert_item_at_time(
        m1,
        0.0,
        named_audio("stem-a", 2.0),
        OverlapPolicy::Override,
        InsertPolicy::SplitAndInsert,
        Some(vec![named_audio("stem-b", 2.0)]),
        None,
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        other => panic!("stems should insert, got {other:?}"),
    };
    assert_eq!(track_id_of(&stack, &result.audio_clips[0].0), "M2");
    assert_eq!(
        stack.associated_track_ids("M1").unwrap(),
        vec!["M2".to_string()]
    );

    // A stem column inserted on the partner M2 is recorded on the owner M1.
    assert!(stack
        .insert_item_at_time_by_id(
            "M2",
            5.0,
            named_audio("stem-c", 2.0),
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
            Some(vec![named_audio("stem-d", 2.0)]),
            None,
        )
        .is_some());
    assert_eq!(track_id_of(&stack, "stem-d"), "M1");
    let m2 = track_index_by_id(&stack, "M2");
    assert_eq!(stack.children[m2].stored_associated_track_ids(), None);
}

#[test]
fn plain_audio_is_rejected_on_a_video_group_track() {
    let mut stack = host_stack(&["A1", "M1"], &["V1"]);
    assert!(stack.associate_tracks("V1", &["A1".into()]));
    assert!(!insert_plain(
        &mut stack,
        "A1",
        0.0,
        named_audio("music", 3.0)
    ));
    assert!(insert_plain(
        &mut stack,
        "M1",
        0.0,
        named_audio("music", 3.0)
    ));
    // A gap is always fine.
    assert!(insert_plain(
        &mut stack,
        "A1",
        0.0,
        Item::Gap(Gap::make_gap(1.0))
    ));
}

#[test]
fn audio_only_column_is_rejected_on_a_video_group_track() {
    let mut stack = host_stack(&["A1", "A2", "M1", "M2"], &["V1"]);
    assert!(stack.associate_tracks("V1", &["A1".into(), "A2".into()]));
    let stems = || vec![named_audio("stem-b", 2.0)];
    assert!(stack
        .insert_item_at_time_by_id(
            "A1",
            0.0,
            named_audio("stem-a", 2.0),
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
            Some(stems()),
            None,
        )
        .is_none());
    assert!(stack
        .insert_item_at_time_by_id(
            "M2",
            0.0,
            named_audio("stem-a", 2.0),
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
            Some(stems()),
            None,
        )
        .is_some());
    assert_eq!(track_id_of(&stack, "stem-b"), "M1");
}

#[test]
fn video_clip_is_rejected_on_an_audio_track() {
    let mut stack = host_stack(&["A1", "A2"], &["V1"]);
    assert!(stack.associate_tracks("V1", &["A1".into(), "A2".into()]));
    assert!(insert_column(
        &mut stack,
        "V1",
        0.0,
        "vid",
        vec![named_audio("left", 2.0)]
    ));
    for policy in [InsertPolicy::SplitAndInsert, InsertPolicy::InsertBefore] {
        assert!(!stack.move_item_at_time("vid", "A2", 5.0, true, policy, OverlapPolicy::Override));
    }
    assert!(!stack.move_item_at_index("vid", "A2", 0, true, OverlapPolicy::Override));
    assert_eq!(track_id_of(&stack, "vid"), "V1");
    assert_eq!(track_id_of(&stack, "left"), "A1");
    // A video+audio column cannot be inserted on an audio track either.
    assert!(stack
        .insert_item_at_time_by_id(
            "A2",
            5.0,
            named_audio("x", 2.0),
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
            None,
            Some(Item::Clip(clip(2.0, Some("v2")))),
        )
        .is_none());
}

#[test]
fn partner_swaps_only_with_its_own_column() {
    let mut stack = host_stack(&["A1", "A2"], &["V1"]);
    assert!(stack.associate_tracks("V1", &["A1".into(), "A2".into()]));
    assert!(insert_column(
        &mut stack,
        "V1",
        0.0,
        "vid",
        vec![named_audio("left", 2.0), named_audio("right", 2.0)]
    ));
    // Swap at the same time: linked clip under the target.
    assert!(stack.move_item_at_time(
        "left",
        "A2",
        0.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override
    ));
    assert_eq!(track_id_of(&stack, "left"), "A2");
    assert_eq!(track_id_of(&stack, "right"), "A1");

    // Another column of the same video at 5: moving left onto it is refused.
    assert!(insert_column(
        &mut stack,
        "V1",
        5.0,
        "vid2",
        vec![named_audio("other", 2.0)]
    ));
    assert_eq!(track_id_of(&stack, "other"), "A1");
    let before = stack.clone();
    assert!(!stack.move_item_at_time(
        "left",
        "A1",
        5.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override
    ));
    assert_eq!(stack, before);
    // Index 2 on A1 is the other column's clip at 5.
    assert!(!stack.move_item_at_index("left", "A1", 2, true, OverlapPolicy::Push));
    assert_eq!(stack, before);
}

#[test]
fn partner_moved_to_another_video_group_takes_the_column_along() {
    let mut stack = host_stack(&["A1", "A2", "B1", "B2"], &["V1", "V2"]);
    assert!(stack.associate_tracks("V1", &["A1".into(), "A2".into()]));
    assert!(stack.associate_tracks("V2", &["B1".into(), "B2".into()]));
    assert!(insert_column(
        &mut stack,
        "V1",
        0.0,
        "vid",
        vec![named_audio("left", 2.0), named_audio("right", 2.0)]
    ));
    assert!(stack.move_item_at_time(
        "left",
        "B1",
        4.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override
    ));
    assert_eq!(track_id_of(&stack, "vid"), "V2");
    assert_eq!(track_id_of(&stack, "left"), "B1");
    assert_eq!(track_id_of(&stack, "right"), "B2");
    assert_eq!(stack.children.len(), 6);
    assert_sync_clips_track_aligned(&stack, "partner moved to another video group");
}

#[test]
fn partner_cannot_join_an_audio_group_and_stems_cannot_join_a_video_group() {
    let mut stack = host_stack(&["A1", "M1", "M2"], &["V1"]);
    assert!(stack.associate_tracks("V1", &["A1".into()]));
    assert!(stack.associate_tracks("M2", &["M1".into()]));
    assert!(insert_column(
        &mut stack,
        "V1",
        0.0,
        "vid",
        vec![named_audio("left", 2.0)]
    ));
    assert!(stack
        .insert_item_at_time_by_id(
            "M2",
            0.0,
            named_audio("stem-a", 2.0),
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
            Some(vec![named_audio("stem-b", 2.0)]),
            None,
        )
        .is_some());
    let before = stack.clone();
    assert!(!stack.move_item_at_time(
        "left",
        "M1",
        5.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override
    ));
    assert!(!stack.move_item_at_time(
        "stem-a",
        "A1",
        5.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override
    ));
    assert_eq!(stack, before);
}

#[test]
fn deleting_a_partner_track_drops_only_that_channel() {
    let mut stack = host_stack(&["A1", "A2"], &["V1"]);
    assert!(stack.associate_tracks("V1", &["A1".into(), "A2".into()]));
    assert!(insert_column(
        &mut stack,
        "V1",
        0.0,
        "vid",
        vec![named_audio("left", 2.0), named_audio("right", 2.0)]
    ));
    assert!(stack.delete_track("A1").is_some());
    assert!(stack.get_item("left").is_none());
    let (_, _, right) = stack.get_item("right").unwrap();
    let (_, _, vid) = stack.get_item("vid").unwrap();
    assert_eq!(sync_clips_id(right), sync_clips_id(vid));
    assert!(sync_clips_id(vid).is_some());
    assert_eq!(
        stack.associated_track_ids("V1").unwrap(),
        vec!["A2".to_string()]
    );
    assert_eq!(stack.children.len(), 2);
}

#[test]
fn normalize_track_order_makes_groups_contiguous_owner_on_top() {
    // Scrambled: V1's partners are spread around, V2's partner sits above V2.
    let mut stack = Stack::default();
    for (kind, id) in [
        (TrackKind::Audio, "A2"),
        (TrackKind::Video, "V1"),
        (TrackKind::Audio, "M1"),
        (TrackKind::Audio, "A1"),
        (TrackKind::Video, "V2"),
        (TrackKind::Audio, "B1"),
        (TrackKind::Audio, "M2"),
    ] {
        stack.children.push(empty_track(kind, id, 10.0));
    }
    assert!(stack.associate_tracks("V1", &["A1".into(), "A2".into()]));
    assert!(stack.associate_tracks("V2", &["B1".into()]));
    assert!(stack.associate_tracks("M2", &["M1".into()]));
    assert!(stack.normalize_track_order());
    // Index order (bottom to top): each group's partners in reverse list order,
    // then its owner; groups in their owners' order; M1/M2 keep their place.
    assert_eq!(ids(&stack), vec!["A2", "A1", "V1", "B1", "V2", "M1", "M2"]);
    assert!(!stack.normalize_track_order());

    // Indices are untouched by edits: only normalization moves tracks.
    let before = ids(&stack);
    assert!(insert_column(
        &mut stack,
        "V1",
        0.0,
        "vid",
        vec![named_audio("l", 2.0)]
    ));
    assert_eq!(ids(&stack), before);
}

#[test]
fn normalize_keeps_free_tracks_and_created_partners_in_place() {
    let mut stack = host_stack(&["M1"], &["V1"]);
    assert!(insert_column(
        &mut stack,
        "V1",
        0.0,
        "vid",
        vec![named_audio("l", 2.0), named_audio("r", 2.0)]
    ));
    // M1 was adopted for the first channel, a track was created for the second.
    let list = stack.associated_track_ids("V1").unwrap();
    assert_eq!(list[0], "M1");
    assert_eq!(list.len(), 2);
    assert!(
        !stack.normalize_track_order(),
        "already contiguous: {:?}",
        ids(&stack)
    );
    assert_eq!(
        ids(&stack),
        vec![list[1].clone(), "M1".to_string(), "V1".to_string()]
    );
}

#[test]
fn manual_reorder_inside_a_group_updates_the_list() {
    let mut stack = host_stack(&["A2", "A1"], &["V1"]);
    assert!(stack.associate_tracks("V1", &["A1".into(), "A2".into()]));
    assert!(!stack.normalize_track_order());
    // Drag A2 right below V1: the list follows, normalization keeps it.
    assert!(stack.reorder_track("A2", 2));
    assert_eq!(ids(&stack), vec!["A1", "A2", "V1"]);
    assert_eq!(
        stack.associated_track_ids("V1").unwrap(),
        vec!["A2".to_string(), "A1".to_string()]
    );
    assert!(!stack.normalize_track_order());
}

#[test]
fn syncing_by_hand_gathers_clips_into_the_video_group() {
    let mut stack = host_stack(&["A1", "F1", "M1"], &["V1"]);
    assert!(stack.associate_tracks("V1", &["A1".into()]));
    // M1 owns an audio group with... nothing but itself listed by nobody; give
    // it a list so it is a real audio group.
    stack
        .children
        .push(empty_track(TrackKind::Audio, "M0", 20.0));
    assert!(stack.associate_tracks("M1", &["M0".into()]));
    assert!(insert_plain(
        &mut stack,
        "V1",
        2.0,
        Item::Clip(clip(3.0, Some("vid")))
    ));
    assert!(insert_plain(
        &mut stack,
        "F1",
        2.0,
        named_audio("free-audio", 3.0)
    ));
    assert!(insert_plain(
        &mut stack,
        "M1",
        2.0,
        named_audio("music", 3.0)
    ));

    let sync_id = stack
        .sync_item(&["vid".into(), "free-audio".into(), "music".into()])
        .expect("sync succeeds");
    // The free track is adopted; the clip on the audio group moved to a new
    // partner track of V1 at the same time.
    assert_eq!(track_id_of(&stack, "free-audio"), "F1");
    let music_track = track_id_of(&stack, "music");
    assert!(
        !["M1", "M0", "A1", "F1"].contains(&music_track.as_str()),
        "got {music_track}"
    );
    let (ti, ii, music) = stack.get_item("music").unwrap();
    assert_eq!(stack.children[ti].start_time_of_item(ii), 2.0);
    assert_eq!(sync_clips_id(music), Some(sync_id));
    assert_eq!(
        stack.associated_track_ids("V1").unwrap(),
        vec!["A1".to_string(), music_track, "F1".to_string()]
    );
    assert_eq!(
        stack.associated_track_ids("M1").unwrap(),
        vec!["M0".to_string()]
    );

    // Two video clips cannot be synced.
    stack
        .children
        .push(empty_track(TrackKind::Video, "V2", 20.0));
    assert!(insert_plain(
        &mut stack,
        "V2",
        2.0,
        Item::Clip(clip(3.0, Some("vid2")))
    ));
    assert!(stack.sync_item(&["vid".into(), "vid2".into()]).is_none());
}
