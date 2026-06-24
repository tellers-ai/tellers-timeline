mod common;
use common::*;

#[test]
fn insert_on_video_in_cluster_preserves_cluster_and_pads_bound_tracks() {
    let mut stack = Stack::default();

    for id in ["A1", "A2", "A3"] {
        let mut audio = Track::new(TrackKind::Audio, Some(id.to_string()));
        audio.items.push(synced_clip_item_with_rate(
            62.88,
            0.0,
            &format!("{id}-g3"),
            3,
            25.0,
        ));
        stack.children.push(audio);
    }

    let mut video = Track::new(TrackKind::Video, Some("video".to_string()));
    video.items.push(synced_clip_item_with_rate(62.88, 0.0, "v-g3", 3, 25.0));
    video.items.push(Item::Gap(Gap::make_gap(5.0)));
    video
        .items
        .push(synced_clip_item_with_rate(62.88, 67.88, "v-g2", 2, 25.0));
    stack.children.push(video);

    let video_track_index = stack.children.len() - 1;
    let audio_track_indices: Vec<_> = (0..video_track_index).collect();

    let groups_before = stack.sync_track_info();
    assert_eq!(groups_before.len(), 1);
    assert_eq!(groups_before[0].track_indices, vec![0, 1, 2, 3]);

    let result = stack.insert_item_at_time(
        video_track_index,
        2.0,
        Item::Clip(clip(1.5, Some("inserted-at-2"))),
        OverlapPolicy::Override,
        InsertPolicy::SplitAndInsert,
        None,
    None,
    );
    assert!(matches!(result, Some(InsertItemAtTimeResult::ItemId(_))));

    let groups_after = stack.sync_track_info();
    assert_eq!(groups_after.len(), 1);
    assert_eq!(groups_after[0].track_indices, vec![0, 1, 2, 3]);

    let (insert_track_index, insert_item_index, insert_item) =
        stack.get_item("inserted-at-2").expect("inserted clip should exist");
    assert_eq!(insert_track_index, video_track_index);
    assert_eq!(
        stack.children[video_track_index].start_time_of_item(insert_item_index),
        2.0
    );
    assert_eq!(insert_item.duration(), 1.5);

    let video_track = &stack.children[video_track_index];
    let head_index = video_track.get_item_at_time(0.0).unwrap();
    assert_eq!(video_track.items[head_index].duration(), 2.0);
    let tail_index = video_track.get_item_at_time(3.5).unwrap();
    assert_eq!(video_track.items[tail_index].duration(), 62.88 - 3.5);

    for &audio_track_index in &audio_track_indices {
        let audio_track = &stack.children[audio_track_index];
        let spacer_index = audio_track.get_item_at_time(2.0).unwrap();
        assert!(matches!(audio_track.items[spacer_index], Item::Gap(_)));
        assert_eq!(audio_track.items[spacer_index].duration(), 1.5);
        let head_index = audio_track.get_item_at_time(0.0).unwrap();
        assert_eq!(audio_track.items[head_index].duration(), 2.0);
    }
}

#[test]
fn add_track_at_allows_insertion_inside_boundary_groups() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("linked-v".to_string())));
    insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(4.0, Some("linked-video")),
        vec![audio_clip(4.0, "file:///linked-audio.wav", None)],
    )
    .expect("linked insert should succeed");

    let mut unsynced_audio = Track::new(TrackKind::Audio, Some("unlinked-a".to_string()));
    unsynced_audio
        .items
        .push(audio_clip(4.0, "file:///unlinked-audio.wav", None));
    stack.children.push(unsynced_audio);
    let mut unsynced_video = Track::new(TrackKind::Video, Some("unlinked-v".to_string()));
    unsynced_video
        .items
        .push(Item::Clip(clip(4.0, Some("unlinked-video"))));
    stack.children.push(unsynced_video);

    // New layout after synced insert: audio ("A1") below the video at index 0, video
    // ("linked-v") on top at index 1. Adding inside the group at index 1 lands between
    // them.
    assert!(stack.add_track_at(
        Track::new(TrackKind::Audio, Some("inside-linked-group".to_string())),
        1,
    ));
    assert_eq!(stack.children.len(), 5);
    assert_eq!(stack.children[0].get_id().as_deref(), Some("A1"));
    assert_eq!(
        stack.children[1].get_id().as_deref(),
        Some("inside-linked-group")
    );
    assert_eq!(stack.children[2].get_id().as_deref(), Some("linked-v"));

    assert!(stack.add_track_at(
        Track::new(TrackKind::Audio, Some("between-groups".to_string())),
        3,
    ));
    assert_eq!(stack.children.len(), 6);
    assert_eq!(
        stack.children[3].get_id().as_deref(),
        Some("between-groups")
    );
}

#[test]
fn insert_audio_primary_with_fewer_audio_links_fills_remaining_audio_track_with_gap() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Audio, Some("primary-audio".to_string())));

    let mut first_primary = audio_clip(2.0, "file:///first-primary.wav", None);
    first_primary.set_id(Some("first-primary".to_string()));
    let first = match stack.insert_item_at_time(
        0,
        0.0,
        first_primary,
        OverlapPolicy::Push,
        InsertPolicy::InsertBeforeOrAfter,
        Some(vec![
            audio_clip(2.0, "file:///first-a1.wav", None),
            audio_clip(2.0, "file:///first-a2.wav", None),
            audio_clip(2.0, "file:///first-a3.wav", None),
        ]),
    None,
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        _ => panic!("first audio-primary linked insert should succeed"),
    };
    let primary_track_index = stack.get_item("first-primary").unwrap().0;

    let mut second_primary = audio_clip(3.0, "file:///second-primary.wav", None);
    second_primary.set_id(Some("second-primary".to_string()));
    let second = match stack.insert_item_at_time(
        primary_track_index,
        2.0,
        second_primary,
        OverlapPolicy::Push,
        InsertPolicy::InsertBeforeOrAfter,
        Some(vec![
            audio_clip(3.0, "file:///second-a1.wav", None),
            audio_clip(3.0, "file:///second-a2.wav", None),
        ]),
    None,
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        _ => panic!("second audio-primary linked insert should succeed"),
    };

    assert_eq!(
        stack
            .children
            .iter()
            .filter(|track| track.kind == TrackKind::Audio)
            .count(),
        4
    );
    assert_eq!(second.audio_clips.len(), 2);
    assert_eq!(stack.get_item("second-primary").unwrap().0, primary_track_index);
    assert_eq!(first.audio_clips[0].1, second.audio_clips[0].1);
    assert_eq!(first.audio_clips[1].1, second.audio_clips[1].1);
    // The second group has fewer audio links than the cluster has audio tracks, so the
    // extra (third) sync track is padded with a gap spacer. The spacer lands at the end
    // of that track and is trimmed during sanitize, so the track keeps just its original
    // first-group clip and is not given a second-group clip.
    let extra_track_index = first.audio_clips[2].1;
    assert!(!second
        .audio_clips
        .iter()
        .any(|(_, track_index)| *track_index == extra_track_index));
    let extra_track = &stack.children[extra_track_index];
    assert_eq!(extra_track.items.len(), 1);
    assert!(matches!(extra_track.items[0], Item::Clip(_)));
    assert_eq!(extra_track.total_duration(), 2.0);
    // The synced audio tracks sit below the audio primary, i.e. at lower indices.
    assert!(
        first
            .audio_clips
            .iter()
            .all(|(_, track_index)| *track_index < primary_track_index)
    );
}

#[test]
fn insert_audio_primary_with_more_audio_links_creates_additional_audio_track() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Audio, Some("primary-audio".to_string())));

    let mut first_primary = audio_clip(2.0, "file:///first-primary.wav", None);
    first_primary.set_id(Some("first-primary".to_string()));
    let first = match stack.insert_item_at_time(
        0,
        0.0,
        first_primary,
        OverlapPolicy::Push,
        InsertPolicy::InsertBeforeOrAfter,
        Some(vec![
            audio_clip(2.0, "file:///first-a1.wav", None),
            audio_clip(2.0, "file:///first-a2.wav", None),
        ]),
    None,
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        _ => panic!("first audio-primary linked insert should succeed"),
    };
    let primary_track_index = stack.get_item("first-primary").unwrap().0;

    let mut second_primary = audio_clip(3.0, "file:///second-primary.wav", None);
    second_primary.set_id(Some("second-primary".to_string()));
    let second = match stack.insert_item_at_time(
        primary_track_index,
        2.0,
        second_primary,
        OverlapPolicy::Push,
        InsertPolicy::InsertBeforeOrAfter,
        Some(vec![
            audio_clip(3.0, "file:///second-a1.wav", None),
            audio_clip(3.0, "file:///second-a2.wav", None),
            audio_clip(3.0, "file:///second-a3.wav", None),
        ]),
    None,
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        _ => panic!("second audio-primary linked insert should succeed"),
    };

    assert_eq!(
        stack
            .children
            .iter()
            .filter(|track| track.kind == TrackKind::Audio)
            .count(),
        4
    );
    assert_eq!(second.audio_clips.len(), 3);
    // Both primaries share the same track. The new audio track was created below the
    // primary, pushing it up by one, so its index is now primary_track_index + 1.
    assert_eq!(
        stack.get_item("first-primary").unwrap().0,
        stack.get_item("second-primary").unwrap().0
    );
    assert_eq!(stack.get_item("second-primary").unwrap().0, primary_track_index + 1);
    // The two existing cluster tracks are reused and exactly one new track is created
    // (nearest the primary, so it carries the first assigned clip).
    let first_tracks: std::collections::HashSet<usize> =
        first.audio_clips.iter().map(|(_, t)| *t).collect();
    let second_tracks: std::collections::HashSet<usize> =
        second.audio_clips.iter().map(|(_, t)| *t).collect();
    assert!(first_tracks.is_subset(&second_tracks));
    let new_tracks: Vec<usize> = second
        .audio_clips
        .iter()
        .map(|(_, t)| *t)
        .filter(|t| !first_tracks.contains(t))
        .collect();
    assert_eq!(new_tracks.len(), 1);
    assert_eq!(second.audio_clips[0].1, new_tracks[0]);
    for (audio_id, _) in &second.audio_clips {
        let (track_index, item_index, item) = stack.get_item(audio_id).unwrap();
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            2.0
        );
        assert_eq!(item.duration(), 3.0);
    }
}

#[test]
fn insert_audio_primary_with_sync_video_clip_creates_video_track() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Audio, Some("primary-audio-track".to_string())));

    let mut primary = audio_clip(3.0, "file:///primary.wav", None);
    primary.set_id(Some("primary-audio".to_string()));
    let mut synced_video = Item::Clip(clip(3.0, Some("synced-video")));
    synced_video.set_id(Some("synced-video".to_string()));

    let result = match stack.insert_item_at_time(
        0,
        1.0,
        primary,
        OverlapPolicy::Push,
        InsertPolicy::InsertBeforeOrAfter,
        Some(vec![audio_clip(3.0, "file:///linked-a1.wav", None)]),
        Some(synced_video),
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        _ => panic!("audio-primary insert with sync video clip should succeed"),
    };

    assert_eq!(result.primary_clip_id, "primary-audio");
    assert_eq!(result.synced_video_clip_id.as_deref(), Some("synced-video"));
    assert_eq!(result.audio_clips.len(), 1);

    let primary_track_index = stack.get_item("primary-audio").unwrap().0;
    let (video_track_index, video_item_index, video_item) =
        stack.get_item("synced-video").unwrap();
    assert_eq!(stack.children[video_track_index].kind, TrackKind::Video);
    assert!(
        video_track_index > primary_track_index,
        "synced video must sit above the audio-primary group"
    );

    let primary_start = stack.children[primary_track_index]
        .start_time_of_item(stack.get_item("primary-audio").unwrap().1);
    assert_eq!(
        stack.children[video_track_index].start_time_of_item(video_item_index),
        primary_start
    );
    assert_eq!(sync_clips_id(video_item), result.sync_clips_id);
    assert_eq!(
        sync_clips_id(stack.get_item("primary-audio").unwrap().2),
        result.sync_clips_id
    );
    assert_eq!(
        sync_clips_id(stack.get_item(&result.audio_clips[0].0).unwrap().2),
        result.sync_clips_id
    );
}

#[test]
fn insert_audio_primary_with_sync_video_clip_only_links_primary_and_video() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Audio, Some("primary-audio-track".to_string())));

    let mut primary = audio_clip(2.0, "file:///primary.wav", None);
    primary.set_id(Some("primary-audio".to_string()));

    let result = match stack.insert_item_at_time(
        0,
        0.0,
        primary,
        OverlapPolicy::Push,
        InsertPolicy::InsertBeforeOrAfter,
        None,
        Some(Item::Clip(clip(2.0, Some("synced-video")))),
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        _ => panic!("audio-primary insert with only sync video clip should succeed"),
    };

    assert_eq!(result.primary_clip_id, "primary-audio");
    assert_eq!(result.audio_clips, Vec::new());
    assert_eq!(result.synced_video_clip_id.as_deref(), Some("synced-video"));
    assert_eq!(
        sync_clips_id(stack.get_item("primary-audio").unwrap().2),
        result.sync_clips_id
    );
    assert_eq!(
        sync_clips_id(stack.get_item("synced-video").unwrap().2),
        result.sync_clips_id
    );
}

#[test]
fn insert_sync_video_clip_requires_audio_destination_track() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("video".to_string())));

    let result = stack.insert_item_at_time(
        0,
        0.0,
        Item::Clip(clip(2.0, Some("primary"))),
        OverlapPolicy::Push,
        InsertPolicy::InsertBeforeOrAfter,
        None,
        Some(Item::Clip(clip(2.0, Some("synced-video")))),
    );

    assert!(result.is_none());
}

#[test]
fn insert_item_at_index_with_sync_video_clip_creates_synced_video() {
    let mut stack = Stack::default();
    stack.children.push(Track::new(
        TrackKind::Audio,
        Some("primary-audio-track".to_string()),
    ));
    stack.children[0]
        .items
        .push(Item::Gap(Gap::make_gap(5.0)));

    let result = match stack.insert_item_at_index(
        "primary-audio-track",
        0,
        audio_clip(2.0, "file:///primary.wav", None),
        OverlapPolicy::Override,
        None,
        Some(Item::Clip(clip(2.0, Some("synced-video")))),
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        _ => panic!("index insert with sync video clip should succeed"),
    };

    assert_eq!(result.synced_video_clip_id.as_deref(), Some("synced-video"));
    let (video_track_index, _, video_item) = stack.get_item("synced-video").unwrap();
    assert_eq!(stack.children[video_track_index].kind, TrackKind::Video);
    assert_eq!(sync_clips_id(video_item), result.sync_clips_id);
    assert_eq!(
        stack.children[video_track_index].start_time_of_item(
            stack.get_item("synced-video").unwrap().1
        ),
        0.0
    );
}

#[test]
fn insert_on_separate_empty_audio_track_preserves_synced_cluster_and_clips() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let synced = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(4.0, Some("clip-a-v")),
        vec![audio_clip(4.0, "file:///clip-a-a.wav", None)],
    )
    .unwrap();
    // Synced layout is [A (idx 0), V (idx 1)]. Add empty A2 just below the synced audio.
    let audio_track_index = synced.audio_clips[0].1;
    let audio_clip_id = synced.audio_clips[0].0.clone();
    let video_track_index = stack.get_item("clip-a-v").unwrap().0;
    let video_clip_duration_before = stack.get_item("clip-a-v").unwrap().2.duration();
    let audio_clip_duration_before = stack.get_item(&audio_clip_id).unwrap().2.duration();
    let sync_group_id = synced.sync_clips_id.unwrap();

    let mut a2 = Track::new(TrackKind::Audio, Some("a2".to_string()));
    a2.items.push(Item::Gap(Gap::make_gap(4.0)));
    stack.children.insert(audio_track_index, a2);
    let a2_track_index = audio_track_index;
    let audio_track_index = audio_track_index + 1;
    let video_track_index = video_track_index + 1;

    let groups_before = stack.sync_track_info();
    assert_eq!(groups_before.len(), 2);
    assert_eq!(
        groups_before
            .iter()
            .find(|group| group.track_indices.contains(&video_track_index))
            .map(|group| group.track_indices.as_slice()),
        Some([audio_track_index, video_track_index].as_slice())
    );
    assert!(
        groups_before
            .iter()
            .any(|group| group.track_indices == vec![a2_track_index])
    );

    let result = stack.insert_item_at_time(
        a2_track_index,
        0.0,
        Item::Clip(clip(2.0, Some("a2-clip"))),
        OverlapPolicy::Override,
        InsertPolicy::SplitAndInsert,
        None,
    None,
    );
    assert!(matches!(result, Some(InsertItemAtTimeResult::ItemId(_))));

    let groups_after = stack.sync_track_info();
    assert_eq!(groups_after.len(), 2);
    assert_eq!(
        groups_after
            .iter()
            .find(|group| group.track_indices.contains(&video_track_index))
            .map(|group| group.track_indices.as_slice()),
        Some([audio_track_index, video_track_index].as_slice())
    );
    assert!(
        groups_after
            .iter()
            .any(|group| group.track_indices == vec![a2_track_index])
    );

    let (_, _, video_clip_after) = stack.get_item("clip-a-v").unwrap();
    let (_, _, audio_clip_after) = stack.get_item(&audio_clip_id).unwrap();
    assert_eq!(video_clip_after.duration(), video_clip_duration_before);
    assert_eq!(audio_clip_after.duration(), audio_clip_duration_before);
    assert_eq!(sync_clips_id(video_clip_after), Some(sync_group_id));
    assert_eq!(sync_clips_id(audio_clip_after), Some(sync_group_id));
    assert_eq!(stack.get_item("a2-clip").unwrap().0, a2_track_index);
}

#[test]
fn insert_without_sync_clips_pads_cluster_audio_track() {
    // A plain insert (no synced audio) still pads the destination cluster's audio tracks
    // with a gap spacer at the insertion point so the synced group stays aligned. The
    // cluster is the destination's sync group (tracks synced with the destination), so we
    // first build a real synced video + audio pair, then do a plain insert on the video.
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let first = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(4.0, Some("synced-video")),
        vec![audio_clip(4.0, "file:///synced-audio.wav", None)],
    )
    .unwrap();
    let audio_track_index = first.audio_clips[0].1;
    let primary_track_index = stack.get_item("synced-video").unwrap().0;

    let result = stack.insert_item_at_time(
        primary_track_index,
        1.0,
        Item::Clip(clip(1.0, Some("inserted"))),
        OverlapPolicy::Override,
        InsertPolicy::SplitAndInsert,
        None,
    None,
    );

    assert!(matches!(result, Some(InsertItemAtTimeResult::ItemId(_))));
    assert!(stack.get_item("inserted").is_some());
    // The synced audio clip is split by the 1.0 gap spacer the cluster padding inserted
    // at 1.0, keeping it aligned with the video.
    let audio_track = &stack.children[audio_track_index];
    let spacer_index = audio_track.get_item_at_time(1.0).unwrap();
    assert!(matches!(audio_track.items[spacer_index], Item::Gap(_)));
    assert_eq!(audio_track.items[spacer_index].duration(), 1.0);
    assert_eq!(audio_track.total_duration(), 4.0);
}

// Regression for the "OTOT" shape: a video track whose first clip is a sync set
// spanning many audio tracks. A Push insert at t=0 must push the video clip AND every
// synced audio clip in lockstep — previously the audio was left behind, desyncing the
// group (video pushed right, audio stuck at 0).
#[test]
fn push_insert_at_start_of_synced_group_pushes_all_synced_audio_tracks() {
    let group_dur = 5.0;
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(group_dur, Some("v-g")))); // 0..5 synced
    video.items.push(Item::Gap(Gap::make_gap(3.0)));
    video.items.push(Item::Clip(clip(2.0, Some("v-tail"))));

    let mut stack = Stack::default();
    let mut group_ids = vec!["v-g".to_string()];
    for i in 0..4 {
        let mut audio = Track::new(TrackKind::Audio, Some(format!("a{i}")));
        let mut a = audio_clip(group_dur, &format!("file:///a{i}.wav"), None);
        a.set_id(Some(format!("a-g{i}")));
        audio.items.push(a); // 0..5 synced
        stack.children.push(audio);
        group_ids.push(format!("a-g{i}"));
    }
    stack.children.push(video);
    stack.sync_item(&group_ids).unwrap();
    let group = sync_clips_id(stack.get_item("v-g").unwrap().2);
    let video_track_index = track_index_by_id(&stack, "v");

    // Push-insert an unsynced clip at the very start of the video track.
    let insert_dur = 4.0;
    let result = stack.insert_item_at_time(
        video_track_index,
        0.0,
        Item::Clip(clip(insert_dur, Some("inserted"))),
        OverlapPolicy::Push,
        InsertPolicy::SplitAndInsert,
        None,
    None,
    );
    assert!(result.is_some(), "insert returned None");
    assert_eq!(sync_clips_id(stack.get_item("inserted").unwrap().2), None);

    let (vt, vi, _) = stack.get_item("v-g").unwrap();
    let video_start = stack.children[vt].start_time_of_item(vi);
    assert!(
        (video_start - insert_dur).abs() < 1e-9,
        "video clip not pushed: {video_start}"
    );
    // Every synced audio clip must be pushed to the same start as the video clip.
    for i in 0..4 {
        let id = format!("a-g{i}");
        let (at, ai, item) = stack.get_item(&id).unwrap();
        let audio_start = stack.children[at].start_time_of_item(ai);
        assert!(
            (audio_start - video_start).abs() < 1e-9,
            "synced audio {id} left behind: audio_start={audio_start} video_start={video_start}"
        );
        assert_eq!(sync_clips_id(item), group, "{id} left the sync group");
    }
    assert_sync_clips_track_aligned(&stack, "push-insert-at-start-of-synced-group");
}
