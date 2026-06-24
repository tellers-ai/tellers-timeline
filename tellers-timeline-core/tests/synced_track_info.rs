mod common;
use common::*;

#[test]
fn sync_track_info_reports_sync_groups() {
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
    // The synced insert creates the audio track ("A1") below the video, so the layout is
    // [A1 (idx0), linked-v (idx1)]. Add a stray unlinked clip onto the synced audio track
    // to make it a mixed track.
    stack.children[0]
        .items
        .push(Item::Clip(clip(2.0, Some("mixed-unlinked-video"))));

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

    let groups = stack.sync_track_info();

    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0].track_indices, vec![0, 1]);
    // Audio ("A1") sits below the video at index 0; the video ("linked-v") is on top at
    // index 1.
    assert_eq!(
        groups[0].track_ids,
        vec![Some("A1".to_string()), Some("linked-v".to_string())]
    );

    assert_eq!(groups[1].track_indices, vec![2]);
    assert_eq!(groups[1].track_ids, vec![Some("unlinked-a".to_string())]);

    assert_eq!(groups[2].track_indices, vec![3]);
    assert_eq!(groups[2].track_ids, vec![Some("unlinked-v".to_string())]);
}

#[test]
fn sync_track_info_reports_overlapping_clusters_per_track() {
    let mut stack = Stack::default();

    let mut audio_g1 = Track::new(TrackKind::Audio, Some("audio-g1".to_string()));
    audio_g1.items.push(synced_clip_item(4.0, "a-g1", 1));
    stack.children.push(audio_g1);

    let mut audio_g2 = Track::new(TrackKind::Audio, Some("audio-g2".to_string()));
    audio_g2.items.push(synced_clip_item(4.0, "a-g2", 2));
    stack.children.push(audio_g2);

    let mut video = Track::new(TrackKind::Video, Some("video".to_string()));
    video.items.push(synced_clip_item(4.0, "v-g1", 1));
    video.items.push(synced_clip_item(4.0, "v-g2", 2));
    stack.children.push(video);

    let groups = stack.sync_track_info();

    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0].track_indices, vec![0, 1, 2]);
    assert_eq!(groups[1].track_indices, vec![0, 2]);
    assert_eq!(groups[2].track_indices, vec![1, 2]);
    assert_eq!(
        groups
            .iter()
            .filter(|group| group.track_indices.contains(&2))
            .count(),
        3
    );
}

#[test]
fn sync_track_info_merges_mixed_video_with_synced_audio_tracks() {
    let mut stack = Stack::default();

    let mut a2 = Track::new(TrackKind::Audio, Some("A2".to_string()));
    a2.items.push(Item::Gap(Gap::make_gap(6.0)));
    a2.items.push(synced_clip_item(219.0, "a2-g3", 3));
    stack.children.push(a2);

    let mut a1 = Track::new(TrackKind::Audio, Some("A1".to_string()));
    a1.items.push(synced_clip_item(2.0, "a1-g2", 2));
    a1.items.push(Item::Gap(Gap::make_gap(4.0)));
    a1.items.push(synced_clip_item(219.0, "a1-g3", 3));
    a1.items.push(Item::Gap(Gap::make_gap(10.0)));
    a1.items.push(synced_clip_item(50.0, "a1-g1", 1));
    stack.children.push(a1);

    for track_id in ["A3", "A4", "A5"] {
        let mut audio = Track::new(TrackKind::Audio, Some(track_id.to_string()));
        audio.items.push(Item::Gap(Gap::make_gap(6.0)));
        audio.items.push(synced_clip_item(219.0, &format!("{track_id}-g3"), 3));
        stack.children.push(audio);
    }

    let mut video = Track::new(TrackKind::Video, Some("Video 1".to_string()));
    video.items.push(synced_clip_item(2.0, "v-g2", 2));
    video
        .items
        .push(Item::Clip(clip(4.0, Some("unlinked-video-1"))));
    video.items.push(synced_clip_item(219.0, "v-g3", 3));
    video.items.push(Item::Gap(Gap::make_gap(5.0)));
    video
        .items
        .push(Item::Clip(clip(4.0, Some("unlinked-video-2"))));
    video.items.push(Item::Gap(Gap::make_gap(1.0)));
    video.items.push(synced_clip_item(50.0, "v-g1", 1));
    stack.children.push(video);

    let groups = stack.sync_track_info();

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].track_indices, vec![0, 1, 2, 3, 4, 5]);
}

#[test]
fn sync_track_info_groups_tracks_that_share_link_group_despite_timing() {
    let mut stack = Stack::default();

    let mut aligned_audio = Track::new(TrackKind::Audio, Some("aligned-a".to_string()));
    aligned_audio.items.push(synced_clip_item(4.0, "a-sync", 1));
    stack.children.push(aligned_audio);

    let mut video = Track::new(TrackKind::Video, Some("video".to_string()));
    video.items.push(synced_clip_item(4.0, "v-sync", 1));
    stack.children.push(video);

    let mut misaligned_audio = Track::new(TrackKind::Audio, Some("misaligned-a".to_string()));
    misaligned_audio.items.push(Item::Gap(Gap::make_gap(1.0)));
    misaligned_audio
        .items
        .push(synced_clip_item(4.0, "late-a-sync", 1));
    stack.children.push(misaligned_audio);

    let groups = stack.sync_track_info();

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].track_indices, vec![0, 1, 2]);
}

#[test]
fn sync_track_info_excludes_empty_tracks_from_link_group_cluster() {
    let mut stack = Stack::default();

    for id in ["A1", "A2", "A3"] {
        let mut audio = Track::new(TrackKind::Audio, Some(id.to_string()));
        audio.items.push(synced_clip_item(5.28, &format!("{id}-sync"), 1));
        stack.children.push(audio);
    }

    for id in ["A4", "A5", "A6", "A7", "A8", "A9"] {
        stack
            .children
            .push(Track::new(TrackKind::Audio, Some(id.to_string())));
    }

    let mut video = Track::new(TrackKind::Video, Some("video".to_string()));
    video.items.push(synced_clip_item(5.28, "v-sync", 1));
    stack.children.push(video);

    let groups = stack.sync_track_info();

    assert_eq!(groups.len(), 7);
    assert_eq!(groups[0].track_indices, vec![0, 1, 2, 9]);
    for (group, track_index) in groups[1..].iter().zip(3..9) {
        assert_eq!(group.track_indices, vec![track_index]);
    }
}

#[test]
fn sync_track_info_splits_unrelated_empty_tracks_into_separate_clusters() {
    let mut stack = Stack::default();

    for id in ["empty-a", "empty-b", "empty-c"] {
        let mut track = Track::new(TrackKind::Audio, Some(id.to_string()));
        track.items.push(Item::Gap(Gap::make_gap(4.0)));
        stack.children.push(track);
    }

    let groups = stack.sync_track_info();

    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0].track_indices, vec![0]);
    assert_eq!(groups[1].track_indices, vec![1]);
    assert_eq!(groups[2].track_indices, vec![2]);
}

#[test]
fn sync_track_info_clusters_tracks_within_one_frame() {
    let mut stack = Stack::default();

    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio.items.push(Item::Gap(Gap::make_gap(67.92)));
    audio.items.push(synced_clip_item_with_rate(
        62.92,
        67.92,
        "a-sync",
        2,
        25.0,
    ));
    stack.children.push(audio);

    let mut video = Track::new(TrackKind::Video, Some("video".to_string()));
    video.items.push(synced_clip_item_with_rate(62.88, 0.0, "v-sync-3", 3, 25.0));
    video
        .items
        .push(Item::Gap(Gap::make_gap(5.0)));
    video
        .items
        .push(synced_clip_item_with_rate(62.88, 67.88, "v-sync-2", 2, 25.0));
    stack.children.push(video);

    let groups = stack.sync_track_info();

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].track_indices, vec![0, 1]);
}
