mod common;
use common::*;

#[test]
fn move_offset_linked_clips_preserves_relative_offsets() {
    for use_resolve_key in [false, true] {
        let mut stack = stack_with_offset_linked_clips(use_resolve_key);
        assert!(stack.move_item_at_time(
            "video-1",
            "video-track",
            10.0,
            true,
            InsertPolicy::SplitAndInsert,
            OverlapPolicy::Override,
        ));

        let (video_track, video_index, _) = stack.get_item("video-1").unwrap();
        let (audio_track, audio_index, _) = stack.get_item("audio-1").unwrap();
        assert_eq!(
            stack.children[video_track].start_time_of_item(video_index),
            10.0
        );
        assert_eq!(
            stack.children[audio_track].start_time_of_item(audio_index),
            12.0
        );
    }
}

#[test]
fn move_synced_set_on_same_track_in_cluster_preserves_track_and_cluster_count() {
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

    let sync_group = sync_clips_id(stack.get_item("v-g3").unwrap().2);
    let track_count = stack.children.len();
    let clusters_before = stack.sync_track_info();

    assert!(stack.move_item_at_time(
        "v-g3",
        "video",
        4.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    assert_eq!(stack.children.len(), track_count);
    assert_sync_track_info_unchanged(&clusters_before, &stack.sync_track_info());

    let (video_track, video_index, video_item) = stack.get_item("v-g3").unwrap();
    assert_eq!(
        stack.children[video_track].start_time_of_item(video_index),
        4.0
    );
    assert_eq!(sync_clips_id(video_item), sync_group);

    for id in ["A1-g3", "A2-g3", "A3-g3"] {
        let (audio_track, audio_index, audio_item) = stack.get_item(id).unwrap();
        assert_eq!(
            stack.children[audio_track].start_time_of_item(audio_index),
            4.0
        );
        assert_eq!(sync_clips_id(audio_item), sync_group);
    }
}

#[test]
fn move_synced_audio_creates_video_track_below_audio_group_in_resolve_layout() {
    let mut stack = Stack::default();
    const DUR: f64 = 5.0;

    let mut a15 = Track::new(TrackKind::Audio, Some("A15".to_string()));
    a15.items.push(Item::Gap(Gap::make_gap(100.0)));
    let mut aud = audio_clip(DUR, "file:///moving-aud.wav", None);
    aud.set_id(Some("moving-aud".to_string()));
    a15.items.push(aud);

    let mut v1 = Track::new(TrackKind::Video, Some("V1".to_string()));
    v1.items.push(Item::Gap(Gap::make_gap(100.0)));
    v1.items
        .push(Item::Clip(clip(DUR, Some("moving-vid"))));

    stack.children.push(a15);
    stack.children.push(v1);
    stack
        .sync_item(&["moving-vid".to_string(), "moving-aud".to_string()])
        .unwrap();

    let mut separator = Track::new(TrackKind::Video, Some("separator-v".to_string()));
    separator
        .items
        .push(Item::Clip(clip(100.0, Some("separator-vid"))));
    stack.children.push(separator);

    let mut dest_a = Track::new(TrackKind::Audio, Some("dest-a".to_string()));
    dest_a.items.push(Item::Gap(Gap::make_gap(100.0)));
    stack.children.push(dest_a);

    let dest_a_index = stack.get_track_by_id("dest-a").unwrap().0;
    let v1_index = stack.get_track_by_id("V1").unwrap().0;
    let track_count_before = stack.children.len();

    assert!(stack.move_item_at_time(
        "moving-aud",
        "dest-a",
        50.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    assert_eq!(stack.children.len(), track_count_before + 1);
    let (video_track, video_index, _) = stack.get_item("moving-vid").unwrap();
    assert_eq!(stack.children[video_track].kind, TrackKind::Video);
    assert_eq!(
        video_track, dest_a_index + 1,
        "new video track must sit directly below dest-a, not at stack top or next to V1"
    );
    assert_eq!(stack.get_track_by_id("V1").unwrap().0, v1_index);
    assert_eq!(
        stack.children[video_track].start_time_of_item(video_index),
        50.0
    );
}

#[test]
fn move_item_at_time_moves_synced_clips() {
    let mut stack = Stack::default();
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(8.0)));
    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio.items.push(Item::Gap(Gap::make_gap(8.0)));
    stack.children.push(video);
    stack.children.push(audio);

    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(3.0, Some("primary")),
        vec![audio_clip(3.0, "file:///a1.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();

    assert!(stack.move_item_at_time(
        &audio_id,
        "a",
        5.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    let (video_track, video_index, video_item) = stack.get_item("primary").unwrap();
    let (audio_track, audio_index, audio_item) = stack.get_item(&audio_id).unwrap();
    assert_eq!(
        stack.children[video_track].start_time_of_item(video_index),
        stack.children[audio_track].start_time_of_item(audio_index)
    );
    assert_eq!(sync_clips_id(video_item), result.sync_clips_id);
    assert_eq!(sync_clips_id(audio_item), result.sync_clips_id);
}

#[test]
fn move_synced_video_to_new_boundary_creates_audio_track_without_retiming() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("source-v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip_with_media_range(3.0, 2.0, 0.0, 10.0),
        vec![audio_clip(3.0, "file:///source-audio.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();
    let (source_audio_track_index, source_audio_item_index, _) = stack.get_item(&audio_id).unwrap();
    let source_audio_track_id = stack.children[source_audio_track_index].get_id().unwrap();
    if let Item::Clip(clip) =
        &mut stack.children[source_audio_track_index].items[source_audio_item_index]
    {
        clip.source_range.start_time.value = 1.5;
    }

    let mut dest_video = Track::new(TrackKind::Video, Some("dest-v".to_string()));
    dest_video.items.push(Item::Gap(Gap::make_gap(10.0)));
    stack.children.push(dest_video);

    assert!(stack.move_item_at_time(
        &result.primary_clip_id,
        "dest-v",
        4.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    let (video_track_index, video_item_index, video_item) =
        stack.get_item(&result.primary_clip_id).unwrap();
    let (audio_track_index, audio_item_index, audio_item) = stack.get_item(&audio_id).unwrap();
    assert_eq!(
        stack.children[video_track_index].get_id().as_deref(),
        Some("dest-v")
    );
    // The synced audio reuses its original partner track (A1) when the synced set is
    // moved via the insert path. Timing and the media source offset are preserved.
    assert_eq!(
        stack.children[audio_track_index].get_id().as_deref(),
        Some(source_audio_track_id.as_str())
    );
    assert_eq!(video_track_index, 2);
    assert_eq!(
        stack.children[video_track_index].start_time_of_item(video_item_index),
        4.0
    );
    assert_eq!(
        stack.children[audio_track_index].start_time_of_item(audio_item_index),
        4.0
    );
    assert_eq!(video_item.duration(), 3.0);
    assert_eq!(audio_item.duration(), 3.0);
    assert_eq!(source_start(video_item), 2.0);
    assert_eq!(source_start(audio_item), 1.5);
    assert_eq!(active_target_url(audio_item), Some("file:///source-audio.wav"));
    assert_eq!(sync_clips_id(video_item), result.sync_clips_id);
    assert_eq!(sync_clips_id(audio_item), result.sync_clips_id);
}

#[test]
fn move_synced_clip_between_boundaries_override_splits_destination_link_groups() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("source-v".to_string())));
    let moving_result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(1.0, Some("moving-video")),
        vec![audio_clip(1.0, "file:///moving-audio.wav", None)],
    )
    .unwrap();
    let moving_audio_id = moving_result.audio_clips[0].0.clone();

    let mut dest_audio = Track::new(TrackKind::Audio, Some("dest-a".to_string()));
    dest_audio.items.push(Item::Gap(Gap::make_gap(10.0)));
    let mut dest_video = Track::new(TrackKind::Video, Some("dest-v".to_string()));
    dest_video.items.push(Item::Gap(Gap::make_gap(10.0)));
    stack.children.push(dest_audio);
    stack.children.push(dest_video);
    let dest_video_index = stack.get_track_by_id("dest-v").unwrap().0;

    let dest_result = insert_with_audio(
        &mut stack,
        dest_video_index,
        0.0,
        clip(4.0, Some("existing-video")),
        vec![audio_clip(4.0, "file:///existing-audio.wav", None)],
    )
    .unwrap();
    let dest_group = dest_result.sync_clips_id.unwrap();
    let dest_audio_index = dest_result.audio_clips[0].1;

    assert!(stack.move_item_at_time(
        "moving-video",
        "dest-v",
        1.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    let (moving_v_t, moving_v_i, moving_v) = stack.get_item("moving-video").unwrap();
    assert_eq!(stack.children[moving_v_t].get_id().as_deref(), Some("dest-v"));
    assert_eq!(
        stack.children[moving_v_t].start_time_of_item(moving_v_i),
        1.0
    );
    assert_eq!(moving_v.duration(), 1.0);
    let moving_group = sync_clips_id(moving_v);
    assert_ne!(moving_group, Some(dest_group));

    let (moving_a_t, moving_a_i, moving_a) = stack.get_item(&moving_audio_id).unwrap();
    assert_eq!(
        stack.children[moving_a_t].start_time_of_item(moving_a_i),
        1.0
    );
    assert_eq!(sync_clips_id(moving_a), moving_group);

    // The destination boundary clip was split by the override insert: left keeps the
    // original link group, right gets a new one.
    let video_track = &stack.children[dest_video_index];
    let left_index = video_track.get_item_at_time(0.5).unwrap();
    let right_index = video_track.get_item_at_time(2.5).unwrap();
    assert_eq!(video_track.items[left_index].duration(), 1.0);
    assert_eq!(video_track.items[right_index].duration(), 2.0);
    let left_group = sync_clips_id(&video_track.items[left_index]);
    let right_group = sync_clips_id(&video_track.items[right_index]);
    assert_eq!(left_group, Some(dest_group));
    assert_ne!(left_group, right_group);
    assert_ne!(right_group, moving_group);
    assert_eq!(right_group, Some(dest_group + 1).max(moving_group.map(|id| id + 1)));

    let audio_track = &stack.children[dest_audio_index];
    let audio_left_index = audio_track.get_item_at_time(0.5).unwrap();
    let audio_right_index = audio_track.get_item_at_time(2.5).unwrap();
    assert_eq!(sync_clips_id(&audio_track.items[audio_left_index]), left_group);
    assert_eq!(
        sync_clips_id(&audio_track.items[audio_right_index]),
        right_group
    );
    assert_sync_clips_track_aligned(&stack, "move-between-boundaries");
}

#[test]
fn move_synced_video_creates_only_missing_destination_audio_tracks() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("source-v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(3.0, Some("primary")),
        vec![
            audio_clip(3.0, "file:///a1.wav", None),
            audio_clip(3.0, "file:///a2.wav", None),
        ],
    )
    .unwrap();
    let first_audio_id = result.audio_clips[0].0.clone();
    let second_audio_id = result.audio_clips[1].0.clone();
    let source_audio_track_ids: Vec<_> = [&first_audio_id, &second_audio_id]
        .iter()
        .map(|id| {
            let (track_index, _, _) = stack.get_item(id).unwrap();
            stack.children[track_index].get_id().unwrap()
        })
        .collect();

    let mut dest_audio = Track::new(TrackKind::Audio, Some("dest-a".to_string()));
    dest_audio.items.push(Item::Gap(Gap::make_gap(10.0)));
    let mut dest_video = Track::new(TrackKind::Video, Some("dest-v".to_string()));
    dest_video.items.push(Item::Gap(Gap::make_gap(10.0)));
    stack.children.push(dest_audio);
    stack.children.push(dest_video);
    let track_count = stack.children.len();

    assert!(stack.move_item_at_time(
        "primary",
        "dest-v",
        4.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    // Destination-cluster audio tracks are preferred over the original source tracks.
    assert_eq!(stack.children.len(), track_count + 1);
    let (first_audio_track, _, _) = stack.get_item(&first_audio_id).unwrap();
    let (second_audio_track, _, second_audio_item) = stack.get_item(&second_audio_id).unwrap();
    assert_eq!(
        stack.children[first_audio_track].get_id().as_deref(),
        Some("dest-a")
    );
    assert_ne!(
        stack.children[second_audio_track].get_id().as_deref(),
        Some("dest-a")
    );
    assert!(
        !source_audio_track_ids.contains(&stack.children[second_audio_track].get_id().unwrap()),
        "second audio should not remain on a source-cluster track when dest-a is available"
    );
    assert_eq!(second_audio_item.duration(), 3.0);
    assert_eq!(
        stack.children[stack.get_item("primary").unwrap().0]
            .get_id()
            .as_deref(),
        Some("dest-v")
    );
    // The whole synced group stays aligned at the destination time.
    for item_id in ["primary", &first_audio_id, &second_audio_id] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(stack.children[track_index].start_time_of_item(item_index), 4.0);
        assert_eq!(sync_clips_id(item), result.sync_clips_id);
    }
}

#[test]
fn move_synced_set_creates_audio_track_when_preferred_track_has_content() {
    let mut stack = Stack::default();
    let mut video = Track::new(TrackKind::Video, Some("d-v".to_string()));
    video
        .items
        .push(Item::Gap(Gap::make_gap(5.0)));
    video
        .items
        .push(Item::Clip(clip(3.0, Some("d-vid"))));
    let mut audio0 = Track::new(TrackKind::Audio, Some("d-a0".to_string()));
    audio0.items.push(Item::Gap(Gap::make_gap(5.0)));
    let mut a0 = audio_clip(3.0, "file:///d-a0.wav", None);
    a0.set_id(Some("d-aud0".to_string()));
    audio0.items.push(a0);
    let mut audio1 = Track::new(TrackKind::Audio, Some("d-a1".to_string()));
    audio1
        .items
        .push(Item::Clip(clip(2.0, Some("occupant"))));
    audio1.items.push(Item::Gap(Gap::make_gap(3.0)));
    let mut a1 = audio_clip(3.0, "file:///d-a1.wav", None);
    a1.set_id(Some("d-aud1".to_string()));
    audio1.items.push(a1);
    stack.children.push(video);
    stack.children.push(audio0);
    stack.children.push(audio1);
    stack
        .sync_item(&[
            "d-vid".to_string(),
            "d-aud0".to_string(),
            "d-aud1".to_string(),
        ])
        .unwrap();
    let source_second_audio_track = stack.get_item("d-aud1").unwrap().0;
    let track_count_before = stack.children.len();

    assert!(stack.move_item_at_time(
        "d-vid",
        "d-v",
        0.0,
        true,
        InsertPolicy::InsertBefore,
        OverlapPolicy::Push,
    ));

    assert!(stack.children.len() > track_count_before);
    let (_, second_audio_track, _) = stack.get_item("d-aud1").unwrap();
    assert_ne!(
        second_audio_track, source_second_audio_track,
        "second audio must not land on a preferred track that already has clips at the move time"
    );
    assert_sync_clips_track_aligned(&stack, "move-busy-preferred-audio");
}

#[test]
fn move_synced_video_to_upper_cluster_uses_destination_audio_tracks() {
    const AUDIO_COUNT: usize = 8;

    let mut stack = Stack::default();
    push_empty_dest(&mut stack, "upper", AUDIO_COUNT, 20.0);
    push_sync_set(&mut stack, "lower", 3.0, AUDIO_COUNT);

    assert!(stack.move_item_at_time(
        "lower-vid",
        "upper-v",
        0.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    let (video_track, _, _) = stack.get_item("lower-vid").unwrap();
    assert_eq!(
        stack.children[video_track].get_id().as_deref(),
        Some("upper-v"),
        "video must land on the destination cluster"
    );

    for i in 0..AUDIO_COUNT {
        let audio_id = format!("lower-aud{i}");
        let (audio_track, _, _) = stack.get_item(&audio_id).unwrap();
        let track_id = stack.children[audio_track]
            .get_id()
            .clone()
            .unwrap_or_default();
        assert!(
            track_id.starts_with("upper-a"),
            "audio partner {audio_id} should be in the destination cluster, got {track_id}"
        );
        assert!(
            !track_id.starts_with("lower-a"),
            "audio partner {audio_id} must not remain on the source cluster"
        );
    }
    assert_sync_clips_track_aligned(&stack, "move-video-to-upper-cluster");
}

#[test]
fn move_synced_video_at_index_uses_destination_boundary_without_retiming() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("source-v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip_with_media_range(3.0, 2.0, 0.0, 10.0),
        vec![audio_clip(3.0, "file:///a1.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();

    let mut dest_audio = Track::new(TrackKind::Audio, Some("dest-a".to_string()));
    dest_audio.items.push(Item::Gap(Gap::make_gap(10.0)));
    let mut dest_video = Track::new(TrackKind::Video, Some("dest-v".to_string()));
    dest_video.items.push(Item::Gap(Gap::make_gap(2.0)));
    dest_video.items.push(Item::Clip(clip(2.0, Some("dest-v-later"))));
    stack.children.push(dest_audio);
    stack.children.push(dest_video);

    assert!(stack.move_item_at_index(
        &result.primary_clip_id,
        "dest-v",
        1,
        true,
        OverlapPolicy::Push,
    ));

    let (video_track_index, video_item_index, video_item) =
        stack.get_item(&result.primary_clip_id).unwrap();
    let (audio_track_index, audio_item_index, audio_item) = stack.get_item(&audio_id).unwrap();
    assert_eq!(
        stack.children[video_track_index].get_id().as_deref(),
        Some("dest-v")
    );
    assert_eq!(
        stack.children[audio_track_index].get_id().as_deref(),
        Some("dest-a")
    );
    assert_eq!(stack.children[video_track_index].start_time_of_item(video_item_index), 2.0);
    assert_eq!(stack.children[audio_track_index].start_time_of_item(audio_item_index), 2.0);
    assert_eq!(video_item.duration(), 3.0);
    assert_eq!(audio_item.duration(), 3.0);
    assert_eq!(source_start(video_item), 2.0);
    assert_eq!(sync_clips_id(video_item), result.sync_clips_id);
    assert_eq!(sync_clips_id(audio_item), result.sync_clips_id);
}

#[test]
fn move_item_at_index_moves_synced_clips() {
    let mut stack = Stack::default();
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(8.0)));
    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio.items.push(Item::Gap(Gap::make_gap(8.0)));
    stack.children.push(video);
    stack.children.push(audio);

    let result = insert_with_audio(
        &mut stack,
        0,
        2.0,
        clip(3.0, Some("primary")),
        vec![audio_clip(3.0, "file:///a1.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();

    assert!(stack.move_item_at_index("primary", "v", 0, true, OverlapPolicy::Override,));

    let (video_track, video_index, video_item) = stack.get_item("primary").unwrap();
    let (audio_track, audio_index, audio_item) = stack.get_item(&audio_id).unwrap();
    assert_eq!(
        stack.children[video_track].start_time_of_item(video_index),
        0.0
    );
    assert_eq!(
        stack.children[audio_track].start_time_of_item(audio_index),
        0.0
    );
    assert_eq!(sync_clips_id(video_item), result.sync_clips_id);
    assert_eq!(sync_clips_id(audio_item), result.sync_clips_id);
}

#[test]
fn move_synced_item_preserves_duration_mismatch() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(3.0, Some("primary")),
        vec![audio_clip(3.0, "file:///a1.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();
    let (audio_track, audio_index, _) = stack.get_item(&audio_id).unwrap();
    if let Item::Clip(clip) = &mut stack.children[audio_track].items[audio_index] {
        clip.source_range.duration.value = 4.0;
    }

    assert!(stack.move_item_at_time(
        "primary",
        "v",
        1.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    let (_, _, primary_item) = stack.get_item("primary").unwrap();
    let (_, _, audio_item) = stack.get_item(&audio_id).unwrap();
    assert_eq!(primary_item.duration(), 3.0);
    assert_eq!(audio_item.duration(), 4.0);
    assert_eq!(sync_clips_id(primary_item), sync_clips_id(audio_item));
}

#[test]
fn move_synced_audio_to_lower_cluster_reuses_cluster_video_not_upper_neighbor() {
    const GROUP8: i64 = 8;
    const GROUP5: i64 = 5;
    const GROUP7: i64 = 7;
    const LEAD: f64 = 9.18;
    const SYNC_DUR: f64 = 1.4;
    const FAR_DUR: f64 = 6.68;

    let mut stack = Stack::default();

    let mut a1 = Track::new(TrackKind::Audio, Some("A1".to_string()));
    a1.items.push(Item::Gap(Gap::make_gap(LEAD)));
    a1.items.push(synced_clip_item(SYNC_DUR, "hh10-a1", GROUP8));
    stack.children.push(a1);

    let mut a2 = Track::new(TrackKind::Audio, Some("A2".to_string()));
    a2.items.push(Item::Gap(Gap::make_gap(LEAD)));
    a2.items.push(synced_clip_item(SYNC_DUR, "hh10-a2", GROUP8));
    stack.children.push(a2);

    let mut main_video = Track::new(TrackKind::Video, Some("Main Video".to_string()));
    main_video.items.push(Item::Gap(Gap::make_gap(LEAD)));
    main_video
        .items
        .push(synced_clip_item(SYNC_DUR, "hh10-v", GROUP8));
    stack.children.push(main_video);

    for id in ["A9", "A10", "A11"] {
        let mut audio = Track::new(TrackKind::Audio, Some(id.to_string()));
        audio.items.push(synced_clip_item(FAR_DUR, &format!("{id}-g5"), GROUP5));
        audio
            .items
            .push(synced_clip_item(FAR_DUR - LEAD, &format!("{id}-g7"), GROUP7));
        stack.children.push(audio);
    }
    let mut video2 = Track::new(TrackKind::Video, Some("Video 2".to_string()));
    video2.items.push(synced_clip_item(FAR_DUR, "v2-g5", GROUP5));
    video2
        .items
        .push(synced_clip_item(FAR_DUR - LEAD, "v2-g7", GROUP7));
    stack.children.push(video2);

    assert!(stack.move_item_at_time(
        "hh10-a1",
        "A9",
        LEAD,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    let (video_track, _, _) = stack.get_item("hh10-v").unwrap();
    assert_eq!(
        stack.children[video_track].get_id().as_deref(),
        Some("Video 2"),
        "synced video must land on the destination cluster video track, not Main Video above A9"
    );
    assert!(
        stack.get_item("hh10-v").is_some(),
        "video partner must survive the audio-primary move"
    );
}

#[test]
fn move_synced_audio_cut_through_destination_cluster_reuses_video2_not_v1() {
    let json =
        std::fs::read_to_string(fixture_path("new_project_move_cut.otio")).expect("fixture");
    let mut tl: Timeline = serde_json::from_str(&json).expect("parse");
    tl.sanitize();

    // Cut into link group 7 on A9 (~26 frames into the 63-frame tail clip).
    const DEST_TIME: f64 = 7.716;
    assert!(tl.tracks.move_item_at_time(
        "68dd84161a27",
        "A9",
        DEST_TIME,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    let (video_track, _, _) = tl.tracks.get_item("0399904de764").unwrap();
    assert_eq!(
        tl.tracks.children[video_track].get_id().as_deref(),
        Some("4c7edef5-ed41-4699-8b5f-b4d91d7918c4"),
        "synced video must stay on the destination cluster video track when the insert cuts through existing clips"
    );
    assert!(
        tl.tracks.get_track_by_id("V1").is_none(),
        "must not spawn V1 when Video 2 can host the override split insert"
    );

    let a9 = track_index_by_id(&tl.tracks, "A9");
    let a9_track = &tl.tracks.children[a9];
    let cut_index = a9_track.get_item_at_time(DEST_TIME).unwrap();
    let cut_start = a9_track.start_time_of_item(cut_index);
    assert!(
        (cut_start - DEST_TIME).abs() < 0.05,
        "moved audio should land at the cut point, got {cut_start}"
    );
}

#[test]
fn move_sync_set_to_different_track_matrix() {
    for &primary_is_video in &[true, false] {
        for &audio_count in &[1usize, 2] {
            for &dest_has_sync in &[false, true] {
                for &op in &[OverlapPolicy::Override, OverlapPolicy::Push] {
                    for &by_index in &[false, true] {
                        for &replace_with_gap in &[true, false] {
                            let label = format!(
                                "video={primary_is_video} n_audio={audio_count} dest_sync={dest_has_sync} op={op:?} by_index={by_index} gap={replace_with_gap}"
                            );
                            let mut stack = Stack::default();
                            push_sync_set(&mut stack, "s", 3.0, audio_count);
                            if dest_has_sync {
                                push_sync_set(&mut stack, "d", 2.0, audio_count);
                            } else {
                                push_empty_dest(&mut stack, "d", audio_count, 20.0);
                            }

                            let primary_id = if primary_is_video {
                                "s-vid".to_string()
                            } else {
                                "s-aud0".to_string()
                            };
                            let dest_track_id = if primary_is_video {
                                "d-v".to_string()
                            } else {
                                "d-a0".to_string()
                            };

                            let moved = if by_index {
                                stack.move_item_at_index(
                                    &primary_id,
                                    &dest_track_id,
                                    0,
                                    replace_with_gap,
                                    op,
                                )
                            } else {
                                stack.move_item_at_time(
                                    &primary_id,
                                    &dest_track_id,
                                    0.0,
                                    replace_with_gap,
                                    InsertPolicy::InsertBefore,
                                    op,
                                )
                            };
                            assert!(moved, "{label}: move returned false");

                            // The whole source set survived as one sync set, one shared
                            // duration (3.0), and the primary landed on the destination track.
                            let group = sync_clips_id(stack.get_item(&primary_id).unwrap().2);
                            assert!(group.is_some(), "{label}: moved primary lost its sync id");
                            for id in ids_for_set("s", audio_count) {
                                let (track_index, item_index, item) = stack
                                    .get_item(&id)
                                    .unwrap_or_else(|| panic!("{label}: {id} missing after move"));
                                assert_eq!(
                                    sync_clips_id(item),
                                    group,
                                    "{label}: {id} left the sync set"
                                );
                                assert_eq!(
                                    item.duration(),
                                    3.0,
                                    "{label}: {id} duration changed"
                                );
                                assert_eq!(
                                    stack.children[track_index].start_time_of_item(item_index),
                                    0.0,
                                    "{label}: {id} not at moved time"
                                );
                            }
                            let (primary_track, _, _) = stack.get_item(&primary_id).unwrap();
                            assert_eq!(
                                stack.children[primary_track].get_id().as_deref(),
                                Some(dest_track_id.as_str()),
                                "{label}: primary not on destination track"
                            );

                            // Whole timeline stays coherent (every sync set aligned).
                            assert_sync_clips_track_aligned(&stack, &label);
                        }
                    }
                }
            }
        }
    }
}
