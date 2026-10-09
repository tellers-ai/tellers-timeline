//! Associated tracks: the ordered partner tracks a primary track's sync
//! columns land on by preference (see `associated_tracks.rs`).

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

#[test]
fn track_accessors_store_an_ordered_unique_list() {
    let mut track = Track::new(TrackKind::Video, Some("V1".into()));
    assert_eq!(track.stored_associated_track_ids(), None);
    assert!(track.associated_track_ids().is_empty());

    track.set_associated_track_ids(vec![
        "A1".into(),
        "A2".into(),
        "A1".into(),
        "V1".into(),
        "".into(),
    ]);
    assert_eq!(
        track.associated_track_ids(),
        vec!["A1".to_string(), "A2".to_string()]
    );
    assert!(track.push_associated_track_id("A3"));
    assert!(!track.push_associated_track_id("A3"));
    assert!(!track.push_associated_track_id("V1"));
    assert!(track.remove_associated_track_id("A2"));
    assert!(!track.remove_associated_track_id("A2"));
    assert_eq!(
        track.associated_track_ids(),
        vec!["A1".to_string(), "A3".to_string()]
    );
    assert_eq!(
        track.metadata["tellers.ai"]["associated_track_ids"],
        serde_json::json!(["A1", "A3"])
    );
    assert!(track.clear_associated_track_ids());
    assert_eq!(track.stored_associated_track_ids(), None);
}

#[test]
fn insert_records_slots_on_the_primary_and_reuses_them_in_order() {
    let mut stack = host_stack(&["A1", "A2"], &["V1"]);
    let v1 = track_index_by_id(&stack, "V1");
    let first = insert_with_audio(
        &mut stack,
        v1,
        0.0,
        clip(2.0, Some("c1")),
        vec![
            audio_clip(2.0, "file:///c1-l.wav", None),
            audio_clip(2.0, "file:///c1-r.wav", None),
        ],
    )
    .unwrap();
    // Nearest free track first: A2 sits right below V1, then A1.
    assert_eq!(track_id_of(&stack, &first.audio_clips[0].0), "A2");
    assert_eq!(track_id_of(&stack, &first.audio_clips[1].0), "A1");
    assert_eq!(
        stack.associated_track_ids("V1").unwrap(),
        vec!["A2".to_string(), "A1".to_string()]
    );

    // The list is the slot order, whatever the stack order: reverse it and the
    // next column follows the list.
    assert!(stack.associate_tracks("V1", &["A1".to_string(), "A2".to_string()]));
    let second = insert_with_audio(
        &mut stack,
        v1,
        5.0,
        clip(2.0, Some("c2")),
        vec![
            audio_clip(2.0, "file:///c2-l.wav", None),
            audio_clip(2.0, "file:///c2-r.wav", None),
        ],
    )
    .unwrap();
    assert_eq!(track_id_of(&stack, &second.audio_clips[0].0), "A1");
    assert_eq!(track_id_of(&stack, &second.audio_clips[1].0), "A2");
    assert_eq!(stack.children.len(), 3);
}

#[test]
fn busy_associated_tracks_get_a_new_track_below_them_not_a_borrowed_one() {
    // V1 owns A1; V2 owns A2. A3 is free but reserved by nobody... except we
    // make it V2's too, so V1 has no unreserved track to borrow.
    let mut stack = host_stack(&["A1", "A2", "A3"], &["V1", "V2"]);
    assert!(stack.associate_tracks("V1", &["A1".to_string()]));
    assert!(stack.associate_tracks("V2", &["A2".to_string(), "A3".to_string()]));
    let v1 = track_index_by_id(&stack, "V1");
    let first = insert_with_audio(
        &mut stack,
        v1,
        0.0,
        clip(4.0, Some("c1")),
        vec![audio_clip(4.0, "file:///c1.wav", None)],
    )
    .unwrap();
    assert_eq!(track_id_of(&stack, &first.audio_clips[0].0), "A1");

    // Same column span on V1 again with Override on a free spot of V1 but A1 busy
    // at 1..3: Override may not eat A1's unrelated content, so a new track is
    // created right below A1 and appended to V1's list.
    let second = insert_with_audio(
        &mut stack,
        v1,
        1.0,
        clip(2.0, Some("c2")),
        vec![audio_clip(2.0, "file:///c2.wav", None)],
    );
    // Override on V1 at 1..3 replaces c1 (and its partner), so A1 is reusable.
    let second = second.unwrap();
    assert_eq!(track_id_of(&stack, &second.audio_clips[0].0), "A1");
    assert_eq!(stack.children.len(), 5);

    // Now a Push insert at 0 on V1 while A1 holds unrelated audio over the
    // column: the partner cannot go to A2/A3 (reserved by V2) so a track is
    // created right below A1.
    let a1 = track_index_by_id(&stack, "A1");
    stack.children[a1]
        .items
        .insert(0, audio_clip(1.0, "file:///music.wav", None));
    let a1_music_id = stack.children[a1].items[0].get_id().unwrap();
    stack.unsync_item(&[a1_music_id]);
    let third = match stack.insert_item_at_time(
        track_index_by_id(&stack, "V1"),
        0.0,
        Item::Clip(clip(1.0, Some("c3"))),
        OverlapPolicy::Override,
        InsertPolicy::SplitAndInsert,
        Some(vec![audio_clip(1.0, "file:///c3.wav", None)]),
        None,
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        other => panic!("insert should succeed, got {other:?}"),
    };
    let created = track_id_of(&stack, &third.audio_clips[0].0);
    assert!(
        !["A1", "A2", "A3"].contains(&created.as_str()),
        "got {created}"
    );
    assert_eq!(stack.children.len(), 6);
    assert_eq!(
        stack.associated_track_ids("V1").unwrap(),
        vec!["A1".to_string(), created.clone()]
    );
    // Created right below A1 (lower index), keeping the group contiguous.
    assert_eq!(
        track_index_by_id(&stack, &created) + 1,
        track_index_by_id(&stack, "A1")
    );
    assert_eq!(
        stack.associated_track_ids("V2").unwrap(),
        vec!["A2".to_string(), "A3".to_string()]
    );
}

#[test]
fn audio_primary_lists_its_audio_partner_tracks() {
    let mut stack = host_stack(&["A1", "A2", "A3"], &["V1"]);
    assert!(stack.associate_tracks("V1", &["A1".to_string()]));
    let a2 = track_index_by_id(&stack, "A2");
    let mut primary = audio_clip(3.0, "file:///stem-1.wav", None);
    primary.set_id(Some("stem-1".into()));
    let result = match stack.insert_item_at_time(
        a2,
        0.0,
        primary,
        OverlapPolicy::Override,
        InsertPolicy::SplitAndInsert,
        Some(vec![audio_clip(3.0, "file:///stem-2.wav", None)]),
        None,
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        other => panic!("audio-only sync insert should succeed, got {other:?}"),
    };
    // A1 is V1's: the stem partner takes the unreserved A3 and A2 records it.
    assert_eq!(track_id_of(&stack, &result.audio_clips[0].0), "A3");
    assert_eq!(
        stack.associated_track_ids("A2").unwrap(),
        vec!["A3".to_string()]
    );
    assert_eq!(
        stack.associated_track_ids("V1").unwrap(),
        vec!["A1".to_string()]
    );
    assert_eq!(stack.children.len(), 4);
}

#[test]
fn derived_association_comes_from_sync_clips_until_stored() {
    let mut stack = Stack::default();
    push_sync_set(&mut stack, "s", 3.0, 2);
    // Nothing stored yet: derived from the Link Group IDs, nearest first.
    let v = track_index_by_id(&stack, "s-v");
    assert_eq!(stack.children[v].stored_associated_track_ids(), None);
    assert_eq!(
        stack.associated_track_ids("s-v").unwrap(),
        vec!["s-a0".to_string(), "s-a1".to_string()]
    );
    // The audio members are not primaries of that group.
    assert!(stack.associated_track_ids("s-a0").unwrap().is_empty());
    assert_eq!(
        stack.tracks_associating(track_index_by_id(&stack, "s-a1")),
        vec![v]
    );

    // The first column inserted on the video materializes the list.
    let result = insert_with_audio(
        &mut stack,
        v,
        3.0,
        clip(1.0, Some("next")),
        vec![
            audio_clip(1.0, "file:///n0.wav", None),
            audio_clip(1.0, "file:///n1.wav", None),
        ],
    )
    .unwrap();
    assert_eq!(track_id_of(&stack, &result.audio_clips[0].0), "s-a0");
    assert_eq!(track_id_of(&stack, &result.audio_clips[1].0), "s-a1");
    assert_eq!(
        stack.children[v].stored_associated_track_ids(),
        Some(vec!["s-a0".to_string(), "s-a1".to_string()])
    );
}

#[test]
fn association_survives_json_round_trip_and_track_deletion() {
    let mut stack = host_stack(&["A1", "A2"], &["V1"]);
    assert!(stack.associate_tracks(
        "V1",
        &["A1".to_string(), "A2".to_string(), "nope".to_string()]
    ));
    let mut timeline = Timeline::default();
    timeline.tracks = stack;
    let json = serde_json::to_string(&timeline).unwrap();
    let mut reloaded: Timeline = serde_json::from_str(&json).unwrap();
    assert_eq!(
        reloaded.tracks.associated_track_ids("V1").unwrap(),
        vec!["A1".to_string(), "A2".to_string()]
    );

    assert!(reloaded.tracks.delete_track("A1").is_some());
    assert_eq!(
        reloaded.tracks.associated_track_ids("V1").unwrap(),
        vec!["A2".to_string()]
    );
    let v1 = track_index_by_id(&reloaded.tracks, "V1");
    assert_eq!(
        reloaded.tracks.children[v1].stored_associated_track_ids(),
        Some(vec!["A2".to_string()])
    );
}

#[test]
fn move_to_another_video_uses_that_video_partners() {
    let mut stack = host_stack(&["A1", "A2", "B1"], &["V1", "V2"]);
    assert!(stack.associate_tracks("V1", &["A1".to_string(), "A2".to_string()]));
    assert!(stack.associate_tracks("V2", &["B1".to_string()]));
    let v1 = track_index_by_id(&stack, "V1");
    let result = insert_with_audio(
        &mut stack,
        v1,
        0.0,
        clip(2.0, Some("c")),
        vec![
            audio_clip(2.0, "file:///l.wav", None),
            audio_clip(2.0, "file:///r.wav", None),
        ],
    )
    .unwrap();
    assert!(stack.move_item_at_time(
        "c",
        "V2",
        5.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));
    assert_eq!(track_id_of(&stack, "c"), "V2");
    // First partner on V2's B1; V2 has no second slot and A1/A2 are V1's, so
    // a track is created and V2 adopts it.
    assert_eq!(track_id_of(&stack, &result.audio_clips[0].0), "B1");
    let second = track_id_of(&stack, &result.audio_clips[1].0);
    assert!(
        !["A1", "A2", "B1"].contains(&second.as_str()),
        "got {second}"
    );
    assert_eq!(
        stack.associated_track_ids("V2").unwrap(),
        vec!["B1".to_string(), second]
    );
    assert_eq!(
        stack.associated_track_ids("V1").unwrap(),
        vec!["A1".to_string(), "A2".to_string()]
    );
    assert_sync_clips_track_aligned(&stack, "move to another video's partners");
}

#[test]
fn free_audio_track_prefers_unlisted_tracks() {
    let mut stack = host_stack(&["A1", "A2", "A3"], &["V1"]);
    assert!(stack.associate_tracks("V1", &["A1".to_string(), "A2".to_string()]));
    assert_eq!(stack.free_audio_track_for(0.0, 2.0).as_deref(), Some("A3"));
    let a3 = track_index_by_id(&stack, "A3");
    stack.children[a3].items = vec![audio_clip(20.0, "file:///music.wav", None)];
    // Only listed tracks are free: the lowest index wins.
    assert_eq!(stack.free_audio_track_for(0.0, 2.0).as_deref(), Some("A1"));
    let a1 = track_index_by_id(&stack, "A1");
    let a2 = track_index_by_id(&stack, "A2");
    stack.children[a1].items = vec![audio_clip(20.0, "file:///a1.wav", None)];
    stack.children[a2].items = vec![audio_clip(20.0, "file:///a2.wav", None)];
    assert_eq!(stack.free_audio_track_for(0.0, 2.0), None);
    // Past every clip the tracks are free again.
    assert_eq!(stack.free_audio_track_for(25.0, 2.0).as_deref(), Some("A3"));
}

fn two_channel_column() -> Stack {
    let mut stack = host_stack(&["A1", "A2", "A3"], &["V1"]);
    assert!(stack.associate_tracks("V1", &["A1".to_string(), "A2".to_string()]));
    let v1 = track_index_by_id(&stack, "V1");
    let mut left = audio_clip(2.0, "file:///left.wav", None);
    left.set_id(Some("left".into()));
    let mut right = audio_clip(2.0, "file:///right.wav", None);
    right.set_id(Some("right".into()));
    insert_with_audio(
        &mut stack,
        v1,
        0.0,
        clip(2.0, Some("vid")),
        vec![left, right],
    )
    .unwrap();
    assert_eq!(track_id_of(&stack, "left"), "A1");
    assert_eq!(track_id_of(&stack, "right"), "A2");
    stack
}

#[test]
fn moving_an_audio_partner_onto_its_sibling_track_swaps_the_channels() {
    for policy in [OverlapPolicy::Override, OverlapPolicy::Push] {
        let mut stack = two_channel_column();
        assert!(stack.move_item_at_time(
            "left",
            "A2",
            4.0,
            true,
            InsertPolicy::SplitAndInsert,
            policy,
        ));
        // The whole column moves to 4; the right channel takes the video's
        // other slot instead of being evicted outside its partners.
        assert_eq!(track_id_of(&stack, "left"), "A2");
        assert_eq!(track_id_of(&stack, "right"), "A1");
        assert_eq!(track_id_of(&stack, "vid"), "V1");
        for id in ["left", "right", "vid"] {
            let (ti, ii, _) = stack.get_item(id).unwrap();
            assert_eq!(
                stack.children[ti].start_time_of_item(ii),
                4.0,
                "{id} {policy:?}"
            );
        }
        assert_eq!(stack.children.len(), 4);
        // The audio tracks never get a list of their own: the video owns the column.
        for id in ["A1", "A2", "A3"] {
            let index = track_index_by_id(&stack, id);
            assert_eq!(
                stack.children[index].stored_associated_track_ids(),
                None,
                "{id}"
            );
        }
        assert_eq!(
            stack.associated_track_ids("V1").unwrap(),
            vec!["A1".to_string(), "A2".to_string()]
        );
    }
}

#[test]
fn moving_an_audio_partner_outside_the_partners_makes_the_video_adopt_that_track() {
    let mut stack = two_channel_column();
    assert!(stack.move_item_at_time(
        "left",
        "A3",
        4.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));
    assert_eq!(track_id_of(&stack, "left"), "A3");
    assert_eq!(track_id_of(&stack, "right"), "A2");
    assert_eq!(track_id_of(&stack, "vid"), "V1");
    assert_eq!(
        stack.associated_track_ids("V1").unwrap(),
        vec!["A1".to_string(), "A2".to_string(), "A3".to_string()]
    );
    let a3 = track_index_by_id(&stack, "A3");
    assert_eq!(stack.children[a3].stored_associated_track_ids(), None);
}

#[test]
fn moving_an_audio_partner_in_time_keeps_every_channel_on_its_track() {
    let mut stack = two_channel_column();
    assert!(stack.move_item_at_time(
        "right",
        "A2",
        8.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));
    assert_eq!(track_id_of(&stack, "left"), "A1");
    assert_eq!(track_id_of(&stack, "right"), "A2");
    assert_eq!(track_id_of(&stack, "vid"), "V1");
    for id in ["left", "right", "vid"] {
        let (ti, ii, _) = stack.get_item(id).unwrap();
        assert_eq!(stack.children[ti].start_time_of_item(ii), 8.0, "{id}");
    }
}
