//! Track groups computed from the sync clips: exclusive membership, audio
//! group owner, placement rules, swaps, track deletion, normalization,
//! pruning and manual sync.

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

/// Host layout: audio tracks first (bottom), video above, in the order given.
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

fn insert_stems(
    stack: &mut Stack,
    track_id: &str,
    time: f64,
    id: &str,
    partners: Vec<Item>,
) -> bool {
    stack
        .insert_item_at_time_by_id(
            track_id,
            time,
            named_audio(id, 2.0),
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
            Some(partners),
            None,
        )
        .is_some()
}

#[test]
fn a_track_belongs_to_one_group() {
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
    assert_eq!(stack.group_owner_of(0), 1);
    assert!(!stack.track_is_free(0));
    assert_eq!(stack.track_groups().len(), 2);
}

#[test]
fn an_audio_groups_main_track_is_the_one_with_most_clips() {
    // M2 holds two stem columns and a plain clip, M1 and M3 the stems only.
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
    // Adding clips to a partner makes it the main track: normalization then
    // puts it on top.
    for i in 0..3 {
        tied.children[0]
            .items
            .push(named_audio(&format!("fx-{i}"), 1.0));
    }
    assert_eq!(
        tied.associated_track_ids("T1").unwrap(),
        vec!["T2".to_string()]
    );
    assert!(tied.normalize_track_order());
    assert_eq!(ids(&tied), vec!["T2", "T1"]);
}

#[test]
fn stems_on_a_partner_track_join_its_group() {
    let mut stack = host_stack(&["M1", "M2", "M3"], &["V1"]);
    assert!(insert_stems(
        &mut stack,
        "M1",
        0.0,
        "stem-a",
        vec![named_audio("stem-b", 2.0)]
    ));
    // The partner took the nearest free track, M2; the group's owner is the
    // higher of the two (same clip count).
    assert_eq!(track_id_of(&stack, "stem-b"), "M2");
    assert_eq!(
        stack.associated_track_ids("M2").unwrap(),
        vec!["M1".to_string()]
    );
    // A stem column added on either member stays in the group: its partner
    // goes to the other member, not to M3.
    assert!(insert_stems(
        &mut stack,
        "M1",
        5.0,
        "stem-c",
        vec![named_audio("stem-d", 2.0)]
    ));
    assert_eq!(track_id_of(&stack, "stem-d"), "M2");
    assert!(insert_stems(
        &mut stack,
        "M2",
        10.0,
        "stem-e",
        vec![named_audio("stem-f", 2.0)]
    ));
    assert_eq!(track_id_of(&stack, "stem-f"), "M1");
    assert_eq!(stack.children.len(), 4);
}

#[test]
fn plain_audio_is_rejected_on_a_video_partner_but_fine_on_a_free_track() {
    let mut stack = host_stack(&["A1", "M1"], &["V1"]);
    assert!(insert_column(
        &mut stack,
        "V1",
        5.0,
        "vid",
        vec![named_audio("left", 2.0)]
    ));
    assert_eq!(track_id_of(&stack, "left"), "M1", "nearest free track");
    assert!(!insert_plain(
        &mut stack,
        "M1",
        0.0,
        named_audio("music", 3.0)
    ));
    // A1 holds no sync clip: it belongs to nobody and takes the clip.
    assert!(insert_plain(
        &mut stack,
        "A1",
        0.0,
        named_audio("music", 3.0)
    ));
    assert!(stack.track_is_free(track_index_by_id(&stack, "A1")));
    // A gap is always fine.
    assert!(insert_plain(
        &mut stack,
        "M1",
        0.0,
        Item::Gap(Gap::make_gap(1.0))
    ));
    assert_eq!(
        stack.associated_track_ids("V1").unwrap(),
        vec!["M1".to_string()]
    );
}

#[test]
fn audio_only_column_is_rejected_on_a_video_group_track() {
    let mut stack = host_stack(&["M1", "M2", "A1"], &["V1"]);
    assert!(insert_column(
        &mut stack,
        "V1",
        5.0,
        "vid",
        vec![named_audio("left", 2.0)]
    ));
    assert_eq!(track_id_of(&stack, "left"), "A1");
    assert!(!insert_stems(
        &mut stack,
        "A1",
        0.0,
        "stem-a",
        vec![named_audio("stem-b", 2.0)]
    ));
    assert!(insert_stems(
        &mut stack,
        "M2",
        0.0,
        "stem-a",
        vec![named_audio("stem-b", 2.0)]
    ));
    assert_eq!(track_id_of(&stack, "stem-b"), "M1");
    assert_eq!(
        stack.associated_track_ids("V1").unwrap(),
        vec!["A1".to_string()]
    );
}

#[test]
fn video_clip_is_rejected_on_an_audio_track() {
    let mut stack = host_stack(&["A1", "A2"], &["V1"]);
    assert!(insert_column(
        &mut stack,
        "V1",
        0.0,
        "vid",
        vec![named_audio("left", 2.0)]
    ));
    for policy in [InsertPolicy::SplitAndInsert, InsertPolicy::InsertBefore] {
        assert!(!stack.move_item_at_time("vid", "A1", 5.0, true, policy, OverlapPolicy::Override));
    }
    assert!(!stack.move_item_at_index("vid", "A1", 0, true, OverlapPolicy::Override));
    assert_eq!(track_id_of(&stack, "vid"), "V1");
    assert_eq!(track_id_of(&stack, "left"), "A2");
    // A video+audio column cannot be inserted on an audio track either.
    assert!(stack
        .insert_item_at_time_by_id(
            "A1",
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
    let mut stack = host_stack(&["A2", "A1"], &["V1"]);
    assert!(insert_column(
        &mut stack,
        "V1",
        0.0,
        "vid",
        vec![named_audio("left", 2.0), named_audio("right", 2.0)]
    ));
    assert_eq!(track_id_of(&stack, "left"), "A1");
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
    let other_track = track_id_of(&stack, "other");
    let before = stack.clone();
    assert!(!stack.move_item_at_time(
        "left",
        &other_track,
        5.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override
    ));
    assert_eq!(stack, before);
}

#[test]
fn partner_cannot_leave_its_video_group() {
    let mut stack = host_stack(&["A1", "A2", "B1", "B2", "F1"], &["V1", "V2"]);
    assert!(insert_column(
        &mut stack,
        "V2",
        10.0,
        "v2c",
        vec![named_audio("b-l", 2.0), named_audio("b-r", 2.0)]
    ));
    assert!(insert_column(
        &mut stack,
        "V1",
        0.0,
        "vid",
        vec![named_audio("left", 2.0), named_audio("right", 2.0)]
    ));
    let b1 = track_id_of(&stack, "b-l");
    let free = ids(&stack)
        .into_iter()
        .find(|id| stack.track_is_free(track_index_by_id(&stack, id)) && id != "V1" && id != "V2")
        .expect("one free audio track");
    let before = stack.clone();
    // Another video group's track, and a free track: both refused.
    for dest in [b1.as_str(), free.as_str()] {
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
}

#[test]
fn partner_cannot_join_an_audio_group_and_stems_cannot_join_a_video_group() {
    let mut stack = host_stack(&["M1", "M2", "A1"], &["V1"]);
    assert!(insert_column(
        &mut stack,
        "V1",
        0.0,
        "vid",
        vec![named_audio("left", 2.0)]
    ));
    assert_eq!(track_id_of(&stack, "left"), "A1");
    assert!(insert_stems(
        &mut stack,
        "M2",
        0.0,
        "stem-a",
        vec![named_audio("stem-b", 2.0)]
    ));
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
    let mut stack = host_stack(&["A2", "A1"], &["V1"]);
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
    // Nearest free first: V1 takes A1 then A2, V2 takes B1, the stems M1/M2.
    let mut stack = host_stack(&["M1", "M2", "B1", "A2", "A1"], &["V1", "V2"]);
    assert!(insert_column(
        &mut stack,
        "V1",
        0.0,
        "vid1",
        vec![named_audio("a-1", 2.0), named_audio("a-2", 2.0)]
    ));
    assert!(insert_column(
        &mut stack,
        "V2",
        0.0,
        "vid2",
        vec![named_audio("b-1", 2.0)]
    ));
    assert!(insert_stems(
        &mut stack,
        "M1",
        0.0,
        "stem-a",
        vec![named_audio("stem-b", 2.0)]
    ));
    let groups_before = stack.track_groups();
    // Scramble: owners to the bottom, partners anywhere.
    assert!(stack.reorder_track("V1", 0));
    assert!(stack.reorder_track("V2", 0));
    let order_changed = stack.normalize_track_order();
    assert!(order_changed);
    // Every group reads partners (slot order reversed) then owner, contiguous.
    for group in stack.track_groups() {
        let mut expected: Vec<usize> = group.partner_track_indices.clone();
        expected.reverse();
        expected.push(group.owner_track_index);
        let first = expected[0];
        let contiguous: Vec<usize> = (first..first + expected.len()).collect();
        assert_eq!(expected, contiguous, "{:?}", group.owner_track_id);
    }
    assert_eq!(stack.track_groups().len(), groups_before.len());
    assert!(!stack.normalize_track_order());
    // Indices are untouched by edits: only normalization moves tracks.
    let before = ids(&stack);
    assert!(insert_column(
        &mut stack,
        "V1",
        10.0,
        "vid3",
        vec![named_audio("a-3", 2.0)]
    ));
    assert_eq!(ids(&stack), before);
}

#[test]
fn a_manual_reorder_inside_a_group_is_kept_by_normalization() {
    let mut stack = host_stack(&["A2", "A1"], &["V1"]);
    assert!(insert_column(
        &mut stack,
        "V1",
        0.0,
        "vid",
        vec![named_audio("left", 2.0), named_audio("right", 2.0)]
    ));
    assert!(!stack.normalize_track_order());
    // Drag A2 right below V1: proximity is the slot order, so the group is
    // still in normal form and the next column follows the new order.
    assert!(stack.reorder_track("A2", 2));
    assert_eq!(ids(&stack), vec!["A1", "A2", "V1"]);
    assert_eq!(
        stack.associated_track_ids("V1").unwrap(),
        vec!["A2".to_string(), "A1".to_string()]
    );
    assert!(!stack.normalize_track_order());
    assert!(insert_column(
        &mut stack,
        "V1",
        5.0,
        "vid2",
        vec![named_audio("l2", 2.0), named_audio("r2", 2.0)]
    ));
    assert_eq!(track_id_of(&stack, "l2"), "A2");
    assert_eq!(track_id_of(&stack, "r2"), "A1");
}

#[test]
fn syncing_by_hand_gathers_clips_into_the_video_group() {
    let mut stack = host_stack(&["M0", "M1", "F1", "A1"], &["V1"]);
    assert!(insert_column(
        &mut stack,
        "V1",
        10.0,
        "vid0",
        vec![named_audio("a0", 2.0)]
    ));
    assert_eq!(track_id_of(&stack, "a0"), "A1");
    assert!(insert_stems(
        &mut stack,
        "M1",
        10.0,
        "stem-a",
        vec![named_audio("stem-b", 2.0)]
    ));
    assert_eq!(track_id_of(&stack, "stem-b"), "M0");
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
    let v1_partners = stack.associated_track_ids("V1").unwrap();
    for id in ["A1", "F1", music_track.as_str()] {
        assert!(
            v1_partners.contains(&id.to_string()),
            "{id} in {v1_partners:?}"
        );
    }
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

#[test]
fn track_groups_lists_every_track_once_with_its_partners() {
    let mut stack = host_stack(&["M1", "F1", "A2", "A1"], &["V1", "V2"]);
    assert!(insert_column(
        &mut stack,
        "V1",
        0.0,
        "vid",
        vec![named_audio("left", 2.0), named_audio("right", 2.0)]
    ));
    let groups = stack.track_groups();
    let summary: Vec<(Option<String>, TrackKind, Vec<Option<String>>)> = groups
        .iter()
        .map(|g| {
            (
                g.owner_track_id.clone(),
                g.kind.clone(),
                g.partner_track_ids.clone(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            (Some("M1".into()), TrackKind::Audio, vec![]),
            (Some("F1".into()), TrackKind::Audio, vec![]),
            (
                Some("V1".into()),
                TrackKind::Video,
                vec![Some("A1".into()), Some("A2".into())]
            ),
            (Some("V2".into()), TrackKind::Video, vec![]),
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
}

#[test]
fn a_video_column_never_borrows_an_audio_group_owner() {
    // M1 and M2 form a stem group; V1 has one partner A1 and needs a second
    // slot: it must create a track, not take M1 or M2.
    let mut stack = host_stack(&["M2", "M1", "A1"], &["V1"]);
    assert!(insert_column(
        &mut stack,
        "V1",
        10.0,
        "vid0",
        vec![named_audio("a0", 2.0)]
    ));
    assert_eq!(track_id_of(&stack, "a0"), "A1");
    assert!(insert_stems(
        &mut stack,
        "M1",
        10.0,
        "stem-a",
        vec![named_audio("stem-b", 2.0)]
    ));
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
    // A plain clip may go on the stem group (M2 is the lowest), never on a
    // video group's track.
    assert_eq!(stack.free_audio_track_for(0.0, 1.0).as_deref(), Some("M2"));
}

#[test]
fn prune_removes_only_empty_auto_created_audio_tracks() {
    let mut stack = host_stack(&["M1"], &["V1"]);
    // A user track named like its id is still the user's: no auto mark.
    let mut look_alike = empty_track(TrackKind::Audio, "A9", 5.0);
    look_alike.name = Some("A9".into());
    stack.children.insert(0, look_alike);
    // A four-channel column adopts M1 and A9 and creates A1 and A2.
    assert!(insert_column(
        &mut stack,
        "V1",
        0.0,
        "vid",
        vec![
            named_audio("c1", 2.0),
            named_audio("c2", 2.0),
            named_audio("c3", 2.0),
            named_audio("c4", 2.0)
        ]
    ));
    let created: Vec<String> = stack
        .children
        .iter()
        .filter(|track| track.is_auto_created())
        .filter_map(|track| track.get_id())
        .collect();
    assert_eq!(created.len(), 2);
    let m1 = track_index_by_id(&stack, "M1");
    stack.children[m1].name = Some("Voice".into());
    // Renaming a created track makes it the user's.
    let renamed = track_index_by_id(&stack, &created[1]);
    stack.children[renamed].set_name(Some("Room tone".into()));
    assert!(!stack.children[renamed].is_auto_created());
    stack
        .children
        .push(empty_track(TrackKind::Audio, "user-track", 5.0));
    // Delete the column: every partner is empty.
    assert!(!stack.delete_item("vid", true, true).is_empty());

    let removed = stack.prune_empty_partner_tracks();
    // Only the created, never renamed track goes; the look-alike, the
    // renamed track, the user track and the owner stay.
    assert_eq!(removed, vec![created[0].clone()]);
    assert_eq!(stack.children.len(), 5);
    assert!(stack.prune_empty_partner_tracks().is_empty());
}

#[test]
fn the_auto_created_mark_survives_a_json_round_trip() {
    let mut stack = host_stack(&[], &["V1"]);
    assert!(insert_column(&mut stack, "V1", 0.0, "vid", vec![named_audio("c1", 2.0)]));
    let created = stack
        .children
        .iter()
        .find(|track| track.is_auto_created())
        .expect("a created audio track");
    assert_eq!(created.metadata["tellers.ai"]["created_by"], "auto");
    let json = serde_json::to_string(&stack).unwrap();
    let back: Stack = serde_json::from_str(&json).unwrap();
    assert_eq!(back.children.iter().filter(|track| track.is_auto_created()).count(), 1);
}
