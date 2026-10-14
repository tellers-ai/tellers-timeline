//! Associated tracks: the partner tracks a column lands on, computed from the
//! sync clips (see `associated_tracks.rs`).

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
fn association_is_computed_from_the_sync_clips() {
    // A Resolve-style import: Link Group IDs on the clips, nothing else.
    let mut stack = Stack::default();
    let mut video = Track::new(TrackKind::Video, Some("s-v".into()));
    video.items.push(synced_clip_item(3.0, "s-vid", 7));
    stack.children.push(video);
    for i in 0..2 {
        let mut audio = Track::new(TrackKind::Audio, Some(format!("s-a{i}")));
        audio
            .items
            .push(synced_clip_item(3.0, &format!("s-aud{i}"), 7));
        stack.children.push(audio);
    }
    // The video owns the column; partners nearest first.
    assert_eq!(
        stack.associated_track_ids("s-v").unwrap(),
        vec!["s-a0".to_string(), "s-a1".to_string()]
    );
    assert!(stack.associated_track_ids("s-a0").unwrap().is_empty());
    assert_eq!(
        stack.tracks_associating(track_index_by_id(&stack, "s-a1")),
        vec![0]
    );
    assert!(!stack.track_is_free(1));
    assert!(stack.track_in_video_group(2));
    // No metadata is written for it: the document is plain OTIO.
    for track in &stack.children {
        assert!(track
            .metadata
            .get("tellers.ai")
            .is_none_or(|ai| ai.get("associated_track_ids").is_none()));
    }
}

#[test]
fn a_column_fills_the_partners_nearest_first_and_creates_past_them() {
    let mut stack = host_stack(&["A1", "A2"], &["V1"]);
    let first = insert_column(
        &mut stack,
        "V1",
        0.0,
        "c1",
        vec![named_audio("c1-l", 2.0), named_audio("c1-r", 2.0)],
    );
    assert!(first);
    // Nearest free track first: A2 sits right below V1, then A1.
    assert_eq!(track_id_of(&stack, "c1-l"), "A2");
    assert_eq!(track_id_of(&stack, "c1-r"), "A1");
    assert_eq!(
        stack.associated_track_ids("V1").unwrap(),
        vec!["A2".to_string(), "A1".to_string()]
    );
    // The next column follows the same slots.
    assert!(insert_column(
        &mut stack,
        "V1",
        5.0,
        "c2",
        vec![named_audio("c2-l", 2.0), named_audio("c2-r", 2.0)]
    ));
    assert_eq!(track_id_of(&stack, "c2-l"), "A2");
    assert_eq!(track_id_of(&stack, "c2-r"), "A1");
    assert_eq!(stack.children.len(), 3);
    // A third channel gets a track created past the existing partners, so the
    // stack already reads slot 3, slot 2, slot 1, owner.
    assert!(insert_column(
        &mut stack,
        "V1",
        10.0,
        "c3",
        vec![
            named_audio("c3-1", 2.0),
            named_audio("c3-2", 2.0),
            named_audio("c3-3", 2.0)
        ]
    ));
    let third = track_id_of(&stack, "c3-3");
    assert!(!["A1", "A2", "V1"].contains(&third.as_str()));
    assert_eq!(
        ids(&stack),
        vec![third.clone(), "A1".into(), "A2".into(), "V1".into()]
    );
    assert_eq!(
        stack.associated_track_ids("V1").unwrap(),
        vec!["A2".to_string(), "A1".to_string(), third]
    );
    assert!(!stack.normalize_track_order());
}

#[test]
fn another_groups_partners_are_never_borrowed() {
    let mut stack = host_stack(&["A1", "B1", "B2"], &["V1", "V2"]);
    assert!(insert_column(
        &mut stack,
        "V1",
        0.0,
        "v1c",
        vec![named_audio("v1-a", 2.0)]
    ));
    assert!(insert_column(
        &mut stack,
        "V2",
        0.0,
        "v2c",
        vec![named_audio("v2-a", 2.0), named_audio("v2-b", 2.0)]
    ));
    // V1 took the nearest free track (B2? no: nearest to V1 is B2 at index 2)
    // and V2 the next free ones; whatever the pick, each group owns its tracks.
    let v1_partners = stack.associated_track_ids("V1").unwrap();
    let v2_partners = stack.associated_track_ids("V2").unwrap();
    assert_eq!(v1_partners.len(), 1);
    assert_eq!(v2_partners.len(), 2);
    assert!(v1_partners.iter().all(|p| !v2_partners.contains(p)));

    // A second V1 column needing two channels at a time where its partner is
    // busy: V2's tracks are reserved, so a new track is created.
    assert!(insert_column(
        &mut stack,
        "V1",
        0.5,
        "v1d",
        vec![named_audio("v1-c", 1.0), named_audio("v1-d", 1.0)]
    ));
    for id in ["v1-c", "v1-d"] {
        assert!(
            !v2_partners.contains(&track_id_of(&stack, id)),
            "{id} on V2's track"
        );
    }
    assert_eq!(stack.children.len(), 6);
}

#[test]
fn stems_go_to_free_tracks_never_to_a_videos_partner() {
    let mut stack = host_stack(&["A1", "A2", "A3"], &["V1"]);
    assert!(insert_column(
        &mut stack,
        "V1",
        5.0,
        "vid",
        vec![named_audio("ch", 2.0)]
    ));
    let video_partner = track_id_of(&stack, "ch");
    let free: Vec<&str> = ["A1", "A2", "A3"]
        .into_iter()
        .filter(|id| *id != video_partner)
        .collect();
    let mut primary = audio_clip(3.0, "file:///stem-1.wav", None);
    primary.set_id(Some("stem-1".into()));
    assert!(stack
        .insert_item_at_time_by_id(
            free[0],
            0.0,
            primary,
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
            Some(vec![audio_clip(3.0, "file:///stem-2.wav", None)]),
            None,
        )
        .is_some());
    // The partner took the other free track; the video's partner was left alone.
    let groups = stack.track_groups();
    let stems = groups
        .iter()
        .find(|g| g.kind == TrackKind::Audio && !g.partner_track_ids.is_empty())
        .expect("a stem group");
    let members: Vec<String> = std::iter::once(stems.owner_track_id.clone().unwrap())
        .chain(stems.partner_track_ids.iter().map(|id| id.clone().unwrap()))
        .collect();
    assert!(!members.contains(&video_partner));
    assert_eq!(members.len(), 2);
    assert_eq!(stack.children.len(), 4);
    assert_eq!(
        stack.associated_track_ids("V1").unwrap(),
        vec![video_partner]
    );
}

#[test]
fn round_trip_and_track_deletion_keep_the_association_consistent() {
    let mut stack = host_stack(&["A1", "A2"], &["V1"]);
    assert!(insert_column(
        &mut stack,
        "V1",
        0.0,
        "c",
        vec![named_audio("l", 2.0), named_audio("r", 2.0)]
    ));
    let timeline = Timeline {
        tracks: stack,
        ..Default::default()
    };
    let json = serde_json::to_string(&timeline).unwrap();
    assert!(!json.contains("associated_track_ids"));
    let mut reloaded: Timeline = serde_json::from_str(&json).unwrap();
    assert_eq!(
        reloaded.tracks.associated_track_ids("V1").unwrap(),
        vec!["A2".to_string(), "A1".to_string()]
    );
    // Deleting a partner track drops that channel only.
    assert!(reloaded.tracks.delete_track("A2").is_some());
    assert_eq!(
        reloaded.tracks.associated_track_ids("V1").unwrap(),
        vec!["A1".to_string()]
    );
    assert!(reloaded.tracks.get_item("l").is_none());
    assert!(reloaded.tracks.get_item("r").is_some());
}

#[test]
fn move_to_another_video_uses_that_videos_partners() {
    let mut stack = host_stack(&["A1", "A2", "B1"], &["V1", "V2"]);
    assert!(insert_column(
        &mut stack,
        "V2",
        10.0,
        "v2c",
        vec![named_audio("v2-a", 2.0)]
    ));
    let b = track_id_of(&stack, "v2-a");
    assert!(insert_column(
        &mut stack,
        "V1",
        0.0,
        "c",
        vec![named_audio("l", 2.0), named_audio("r", 2.0)]
    ));
    let v1_partners = stack.associated_track_ids("V1").unwrap();
    assert!(!v1_partners.contains(&b));
    assert!(stack.move_item_at_time(
        "c",
        "V2",
        5.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));
    assert_eq!(track_id_of(&stack, "c"), "V2");
    // First channel on V2's partner; V1's former partners are now empty, so
    // free, and the second channel may take one of them.
    assert_eq!(track_id_of(&stack, "l"), b);
    let second = track_id_of(&stack, "r");
    assert_ne!(second, b);
    assert_eq!(stack.associated_track_ids("V2").unwrap(), vec![b, second]);
    assert!(
        stack.associated_track_ids("V1").unwrap().is_empty(),
        "V1 has no column left"
    );
    assert_sync_clips_track_aligned(&stack, "move to another video's partners");
}

#[test]
fn free_audio_track_prefers_tracks_in_no_group() {
    let mut stack = host_stack(&["A1", "A2", "A3"], &["V1"]);
    assert!(insert_column(
        &mut stack,
        "V1",
        5.0,
        "c",
        vec![named_audio("l", 2.0), named_audio("r", 2.0)]
    ));
    // A2 and A3 are V1's; A1 is free.
    assert_eq!(stack.free_audio_track_for(0.0, 2.0).as_deref(), Some("A1"));
    let a1 = track_index_by_id(&stack, "A1");
    stack.children[a1].items = vec![audio_clip(20.0, "file:///music.wav", None)];
    // Only V1's partners are free there: a plain clip is never steered to a
    // video group's track, so there is no pick.
    assert_eq!(stack.free_audio_track_for(0.0, 2.0), None);
    assert_eq!(stack.free_audio_track_for(25.0, 2.0).as_deref(), Some("A1"));
}

/// V1 with a two-channel column: left on the nearest track A1, right on A2.
fn two_channel_column() -> Stack {
    let mut stack = host_stack(&["A3", "A2", "A1"], &["V1"]);
    assert!(insert_column(
        &mut stack,
        "V1",
        0.0,
        "vid",
        vec![named_audio("left", 2.0), named_audio("right", 2.0)]
    ));
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
    }
}

#[test]
fn moving_an_audio_partner_to_a_free_track_is_refused() {
    let mut stack = two_channel_column();
    let before = stack.clone();
    assert!(!stack.move_item_at_time(
        "left",
        "A3",
        4.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));
    assert_eq!(stack, before);
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
