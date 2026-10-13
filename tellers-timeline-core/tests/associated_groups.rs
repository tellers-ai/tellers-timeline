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
fn audio_group_stems_are_recorded_on_the_group_owner() {
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
fn plain_audio_is_rejected_on_a_video_group_track_that_holds_its_channels() {
    let mut stack = host_stack(&["A1", "M1"], &["V1"]);
    assert!(stack.associate_tracks("V1", &["A1".into()]));
    assert!(insert_column(
        &mut stack,
        "V1",
        5.0,
        "vid",
        vec![named_audio("left", 2.0)]
    ));
    assert_eq!(track_id_of(&stack, "left"), "A1");
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
    assert_eq!(
        stack.associated_track_ids("V1").unwrap(),
        vec!["A1".to_string()]
    );
}

#[test]
fn plain_audio_on_an_empty_partner_track_releases_it_from_the_group() {
    let mut stack = host_stack(&["A1", "A2"], &["V1"]);
    assert!(stack.associate_tracks("V1", &["A1".into(), "A2".into()]));
    // A2 is listed but empty: a plain clip is accepted and A2 leaves the group.
    assert!(insert_plain(
        &mut stack,
        "A2",
        0.0,
        named_audio("music", 3.0)
    ));
    assert_eq!(track_id_of(&stack, "music"), "A2");
    assert_eq!(
        stack.associated_track_ids("V1").unwrap(),
        vec!["A1".to_string()]
    );
    assert!(stack.track_is_free(track_index_by_id(&stack, "A2")));
    // Stems on an empty partner do the same, and the stem group is theirs.
    assert!(stack
        .insert_item_at_time_by_id(
            "A1",
            0.0,
            named_audio("stem-a", 2.0),
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
            Some(vec![named_audio("stem-b", 2.0)]),
            None,
        )
        .is_some());
    assert!(stack.associated_track_ids("V1").unwrap().is_empty());
    let b = track_id_of(&stack, "stem-b");
    assert!(!["A1", "A2", "V1"].contains(&b.as_str()), "got {b}");
    assert_eq!(stack.associated_track_ids("A1").unwrap(), vec![b]);
    // A failed insert does not release anything.
    let mut stack = host_stack(&["A1"], &["V1"]);
    assert!(stack.associate_tracks("V1", &["A1".into()]));
    assert!(!insert_plain(
        &mut stack,
        "A1",
        0.0,
        Item::Clip(clip(0.0, Some("zero")))
    ));
    assert_eq!(
        stack.associated_track_ids("V1").unwrap(),
        vec!["A1".to_string()]
    );
}

#[test]
fn audio_only_column_is_rejected_on_a_video_group_track() {
    let mut stack = host_stack(&["A1", "A2", "M1", "M2"], &["V1"]);
    assert!(stack.associate_tracks("V1", &["A1".into(), "A2".into()]));
    // A1 holds a channel of V1 (an empty partner would be released instead).
    assert!(insert_column(&mut stack, "V1", 5.0, "vid", vec![named_audio("left", 2.0)]));
    assert_eq!(track_id_of(&stack, "left"), "A1");
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
fn partner_cannot_leave_its_video_group() {
    let mut stack = host_stack(&["A1", "A2", "B1", "B2", "F1"], &["V1", "V2"]);
    assert!(stack.associate_tracks("V1", &["A1".into(), "A2".into()]));
    assert!(stack.associate_tracks("V2", &["B1".into(), "B2".into()]));
    assert!(insert_column(
        &mut stack,
        "V1",
        0.0,
        "vid",
        vec![named_audio("left", 2.0), named_audio("right", 2.0)]
    ));
    let before = stack.clone();
    // Another video group's track, and a free track: both refused; the
    // library answers false and nothing changes (the UI relies on this).
    for dest in ["B1", "F1"] {
        assert!(!stack.move_item_at_time(
            "left",
            dest,
            4.0,
            true,
            InsertPolicy::SplitAndInsert,
            OverlapPolicy::Override
        ));
        assert!(!stack.move_item_at_index("left", dest, 0, true, OverlapPolicy::Push));
        assert_eq!(stack, before, "{dest}");
    }
    // Within the group the column still moves in time and swaps channels.
    assert!(stack.move_item_at_time(
        "left",
        "A2",
        4.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override
    ));
    assert_eq!(track_id_of(&stack, "left"), "A2");
    assert_eq!(track_id_of(&stack, "right"), "A1");
    assert_eq!(track_id_of(&stack, "vid"), "V1");
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

type GroupSummary = (Option<String>, TrackKind, Vec<Option<String>>, bool);

#[test]
fn track_groups_lists_every_track_once_with_its_partners() {
    let mut stack = host_stack(&["A2", "A1", "M1", "F1"], &["V1", "V2"]);
    assert!(stack.associate_tracks("V1", &["A1".into(), "A2".into()]));
    // M1 holds a derived audio group: a stem column with F1... keep F1 free and
    // give M1 nothing, so M1 and F1 are groups of one.
    let groups = stack.track_groups();
    let summary: Vec<GroupSummary> = groups
        .iter()
        .map(|g| {
            (
                g.owner_track_id.clone(),
                g.kind.clone(),
                g.partner_track_ids.clone(),
                g.stored,
            )
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            (Some("M1".into()), TrackKind::Audio, vec![], false),
            (Some("F1".into()), TrackKind::Audio, vec![], false),
            (
                Some("V1".into()),
                TrackKind::Video,
                vec![Some("A1".into()), Some("A2".into())],
                true
            ),
            (Some("V2".into()), TrackKind::Video, vec![], false),
        ]
    );
    let seen: Vec<usize> = groups
        .iter()
        .flat_map(|g| std::iter::once(g.owner_track_index).chain(g.partner_track_indices.clone()))
        .collect();
    let mut sorted = seen.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), stack.children.len());

    // A derived group (Resolve import) is reported as not stored.
    let mut imported = Stack::default();
    let mut video = Track::new(TrackKind::Video, Some("s-v".into()));
    video.items.push(synced_clip_item(3.0, "s-vid", 7));
    let mut audio = Track::new(TrackKind::Audio, Some("s-a0".into()));
    audio.items.push(synced_clip_item(3.0, "s-aud0", 7));
    imported.children.push(audio);
    imported.children.push(video);
    let groups = imported.track_groups();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].owner_track_id.as_deref(), Some("s-v"));
    assert_eq!(groups[0].partner_track_ids, vec![Some("s-a0".to_string())]);
    assert!(!groups[0].stored);
}

#[test]
fn a_video_column_never_borrows_an_audio_group_owner() {
    // M1 owns a stem group (M1 + M2); V1 has one partner A1 and needs a second
    // slot: it must create a track, not take M1 (listed by nobody) or M2.
    let mut stack = host_stack(&["A1", "M2", "M1"], &["V1"]);
    assert!(stack.associate_tracks("V1", &["A1".into()]));
    assert!(stack.associate_tracks("M1", &["M2".into()]));
    assert!(insert_column(
        &mut stack,
        "V1",
        0.0,
        "vid",
        vec![named_audio("left", 2.0), named_audio("right", 2.0)]
    ));
    assert_eq!(track_id_of(&stack, "left"), "A1");
    let right_track = track_id_of(&stack, "right");
    assert!(
        !["A1", "M1", "M2"].contains(&right_track.as_str()),
        "got {right_track}"
    );
    assert_eq!(stack.children.len(), 5);
    assert_eq!(
        stack.associated_track_ids("V1").unwrap(),
        vec!["A1".to_string(), right_track]
    );
    assert_eq!(
        stack.associated_track_ids("M1").unwrap(),
        vec!["M2".to_string()]
    );
    // A plain clip may go on the stem group (M2 is the lowest), never on a
    // video group's track.
    assert_eq!(stack.free_audio_track_for(0.0, 1.0).as_deref(), Some("M2"));
}

#[test]
fn audio_group_main_track_is_the_one_with_most_clips() {
    // Derived from sync clips: M2 holds two stem columns and a plain clip, M1
    // and M3 hold the stems only. M2 owns the derived group.
    let mut stack = Stack::default();
    for id in ["M1", "M2", "M3"] {
        stack
            .children
            .push(Track::new(TrackKind::Audio, Some(id.into())));
    }
    for (i, id) in ["M1", "M2", "M3"].iter().enumerate() {
        stack.children[i]
            .items
            .push(synced_clip_item(2.0, &format!("{id}-s1"), 1));
        stack.children[i]
            .items
            .push(synced_clip_item(2.0, &format!("{id}-s2"), 2));
    }
    stack.children[1].items.push(named_audio("extra", 1.0));
    assert_eq!(
        stack.associated_track_ids("M2").unwrap(),
        vec!["M1".to_string(), "M3".to_string()]
    );
    assert!(stack.associated_track_ids("M3").unwrap().is_empty());

    // On a tie the highest track owns the group.
    let mut tied = Stack::default();
    for id in ["T1", "T2"] {
        tied.children
            .push(Track::new(TrackKind::Audio, Some(id.into())));
    }
    for i in 0..2 {
        tied.children[i]
            .items
            .push(synced_clip_item(2.0, &format!("t{i}"), 9));
    }
    assert_eq!(
        tied.associated_track_ids("T2").unwrap(),
        vec!["T1".to_string()]
    );

    // A stored group is re-homed by normalization: stems were first dropped on
    // M1 (so M1 stores the list), but M3 ends up with the most clips.
    let mut stack = host_stack(&["M1", "M2", "M3"], &["V1"]);
    assert!(stack.associate_tracks("M1", &["M2".into(), "M3".into()]));
    assert!(insert_plain(
        &mut stack,
        "M3",
        0.0,
        named_audio("fx-1", 1.0)
    ));
    assert!(insert_plain(
        &mut stack,
        "M3",
        2.0,
        named_audio("fx-2", 1.0)
    ));
    assert!(stack.normalize_track_order());
    assert_eq!(
        stack.associated_track_ids("M3").unwrap(),
        vec!["M2".to_string(), "M1".to_string()],
        "the former owner takes the main track's slot"
    );
    let m1 = track_index_by_id(&stack, "M1");
    assert_eq!(stack.children[m1].stored_associated_track_ids(), None);
    // Owner on top: M3 above M2 above M1, V1 keeps its place at the top.
    assert_eq!(ids(&stack), vec!["M1", "M2", "M3", "V1"]);
    assert!(!stack.normalize_track_order());
}

#[test]
fn prune_removes_only_empty_auto_named_partner_tracks() {
    let mut stack = host_stack(&["M1", "F1"], &["V1"]);
    let v1 = track_index_by_id(&stack, "V1");
    // A column with three channels creates A1, A2, A3 (auto-named).
    assert!(insert_column(
        &mut stack,
        "V1",
        0.0,
        "vid",
        vec![
            named_audio("c1", 2.0),
            named_audio("c2", 2.0),
            named_audio("c3", 2.0)
        ]
    ));
    let _ = v1;
    let partners = stack.associated_track_ids("V1").unwrap();
    assert_eq!(partners.len(), 3);
    // M1 was adopted (free, auto-named "M1" == id); give it a user name.
    let m1 = track_index_by_id(&stack, "M1");
    stack.children[m1].name = Some("Voice".into());
    // Delete the column: every partner is now empty.
    assert!(!stack.delete_item("vid", true).is_empty());
    for id in &partners {
        let track = &stack.children[track_index_by_id(&stack, id)];
        assert!(
            track.items.iter().all(|item| matches!(item, Item::Gap(_))),
            "{id}"
        );
    }

    let removed = stack.prune_empty_partner_tracks();
    // The named partner stays; the two auto-named partners (F1, adopted, and
    // the created A1) go; the owner V1 is untouched.
    assert_eq!(removed.len(), 2);
    assert!(removed.contains(&"F1".to_string()));
    assert!(!removed.contains(&"M1".to_string()));
    assert_eq!(
        stack.associated_track_ids("V1").unwrap(),
        vec!["M1".to_string()]
    );
    assert!(stack.get_track_by_id("V1").is_some());
    assert_eq!(stack.children.len(), 2);
    assert!(stack.prune_empty_partner_tracks().is_empty());

    // An empty owner is never pruned, even auto-named.
    let mut stems = host_stack(&["S1", "S2"], &[]);
    assert!(stems.associate_tracks("S2", &["S1".into()]));
    assert_eq!(stems.prune_empty_partner_tracks(), vec!["S1".to_string()]);
    assert_eq!(ids(&stems), vec!["S2"]);
}
