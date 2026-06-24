mod common;
use common::*;

#[test]
fn synced_delete_can_remove_entire_sync_clips() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    stack
        .children
        .push(Track::new(TrackKind::Audio, Some("a".to_string())));

    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(3.0, Some("primary")),
        vec![audio_clip(3.0, "file:///a1.wav", None)],
    )
    .unwrap();

    let removed = stack.delete_item("primary", true);
    assert_eq!(removed.len(), 2);
    assert!(stack.children[0]
        .items
        .iter()
        .all(|item| matches!(item, Item::Gap(_))));
    assert!(stack.children[result.audio_clips[0].1]
        .items
        .iter()
        .all(|item| matches!(item, Item::Gap(_))));

    let gap_ids: Vec<_> = stack
        .children
        .iter()
        .flat_map(|track| track.items.iter())
        .filter_map(|item| item.get_id())
        .collect();
    let unique: std::collections::HashSet<_> = gap_ids.iter().collect();
    assert_eq!(gap_ids.len(), unique.len());
}

#[test]
fn synced_delete_keeps_touched_tracks_without_remaining_clips() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));

    insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(3.0, Some("primary")),
        vec![audio_clip(3.0, "file:///a1.wav", None)],
    )
    .unwrap();

    let removed = stack.delete_item("primary", true);

    assert_eq!(removed.len(), 2);
    assert_eq!(stack.children.len(), 2);
    assert!(stack
        .children
        .iter()
        .all(|track| track.items.iter().all(|item| matches!(item, Item::Gap(_)))));
}

#[test]
fn synced_delete_without_gap_collapses_sync_clips() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    stack
        .children
        .push(Track::new(TrackKind::Audio, Some("a".to_string())));

    insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(3.0, Some("primary")),
        vec![audio_clip(3.0, "file:///a1.wav", None)],
    )
    .unwrap();

    let removed = stack.delete_item("primary", false);
    assert_eq!(removed.len(), 2);
    assert!(stack.children.iter().all(|track| track.items.is_empty()));
}

#[test]
fn delete_clip_a_replace_with_gap_preserves_synced_partner_spans() {
    let mut stack = stack_v1_clip_a_clip_b_a1();
    let v1 = track_index_by_id(&stack, "v1");
    let a1 = track_index_by_id(&stack, "a1");

    let removed = stack.delete_item("clip-a", true);
    assert_eq!(removed.len(), 1);
    assert!(stack.get_item("clip-a").is_none());
    assert!(stack.get_item("clip-b-video").is_some());
    assert!(stack.get_item("clip-b-audio").is_some());

    let video = &stack.children[v1];
    assert_eq!(video.items.len(), 2);
    assert!(matches!(video.items[0], Item::Gap(_)));
    assert_item_span(video, 0, 0.0, 10.0);
    assert_item_span(video, 1, 10.0, 10.0);

    let audio = &stack.children[a1];
    assert_eq!(audio.items.len(), 2);
    assert!(matches!(audio.items[0], Item::Gap(_)));
    assert_item_span(audio, 0, 0.0, 10.0);
    assert_item_span(audio, 1, 10.0, 10.0);
    assert_eq!(
        sync_clips_id(&video.items[1]),
        sync_clips_id(&audio.items[1])
    );
}

#[test]
fn delete_clip_a_collapse_pulls_synced_partner_left() {
    let mut stack = stack_v1_clip_a_clip_b_a1();
    let v1 = track_index_by_id(&stack, "v1");
    let a1 = track_index_by_id(&stack, "a1");

    let removed = stack.delete_item("clip-a", false);
    assert_eq!(removed.len(), 2);
    assert!(stack.get_item("clip-a").is_none());
    assert!(stack.get_item("clip-b-video").is_some());
    assert!(stack.get_item("clip-b-audio").is_some());

    let video = &stack.children[v1];
    assert_eq!(video.items.len(), 1);
    assert_item_span(video, 0, 0.0, 10.0);

    let audio = &stack.children[a1];
    assert_eq!(audio.items.len(), 1);
    assert_item_span(audio, 0, 0.0, 10.0);
    assert_eq!(
        sync_clips_id(&video.items[0]),
        sync_clips_id(&audio.items[0])
    );
}

#[test]
fn delete_synced_audio_collapse_removes_video_partner_and_moves_following_clip() {
    let mut stack = stack_v1_c1_c2_a1();
    let v1 = track_index_by_id(&stack, "v1");
    let a1 = track_index_by_id(&stack, "a1");

    let removed = stack.delete_item("c1a", false);
    assert_eq!(removed.len(), 2);
    assert!(stack.get_item("c1a").is_none());
    assert!(stack.get_item("c1").is_none());
    assert!(stack.get_item("c2").is_some());

    let video = &stack.children[v1];
    assert_eq!(video.items.len(), 1);
    assert_item_span(video, 0, 0.0, 4.0);
    assert_eq!(video.items[0].get_id().as_deref(), Some("c2"));

    assert!(stack.children[a1].items.is_empty());
}

#[test]
fn delete_unsynced_item_only_removes_selected_item() {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(3.0, Some("primary"))));
    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio
        .items
        .push(Item::Clip(clip(3.0, Some("unlinked-audio"))));

    let mut stack = Stack::default();
    stack.children.push(video);
    stack.children.push(audio);

    let removed = stack.delete_item("primary", true);

    assert_eq!(removed.len(), 1);
    assert!(stack.get_item("primary").is_none());
    assert!(stack.get_item("unlinked-audio").is_some());
}

#[test]
fn delete_unsynced_item_without_gap_pulls_later_synced_assets() {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(1.0, Some("unlinked"))));
    let audio = Track::new(TrackKind::Audio, Some("a".to_string()));

    let mut stack = Stack::default();
    stack.children.push(video);
    stack.children.push(audio);
    let result = insert_with_audio(
        &mut stack,
        0,
        1.0,
        clip(2.0, Some("linked-video")),
        vec![audio_clip(2.0, "file:///linked-audio.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();

    let removed = stack.delete_item("unlinked", false);

    assert_eq!(removed.len(), 2);
    let (video_track_index, video_item_index, _) = stack.get_item("linked-video").unwrap();
    let (audio_track_index, audio_item_index, _) = stack.get_item(&audio_id).unwrap();
    assert_eq!(
        stack.children[video_track_index].start_time_of_item(video_item_index),
        0.0
    );
    assert_eq!(
        stack.children[audio_track_index].start_time_of_item(audio_item_index),
        0.0
    );
}

#[test]
fn delete_track_removes_synced_assets_left_behind() {
    // "video on top" layout: the synced insert creates a fresh audio track below the
    // video; deleting the video leaves that sync audio track behind as a gap.
    let mut stack = Stack::default();
    let audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    let video = Track::new(TrackKind::Video, Some("v".to_string()));
    stack.children.push(audio);
    stack.children.push(video);

    let result = insert_with_audio(
        &mut stack,
        1,
        0.0,
        clip(3.0, Some("primary")),
        vec![audio_clip(3.0, "file:///a1.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();

    let removed = stack.delete_track("v").unwrap();

    assert_eq!(removed.get_id().as_deref(), Some("v"));
    assert_eq!(stack.children.len(), 2);
    assert!(stack
        .children
        .iter()
        .any(|track| track.get_id().as_deref() == Some("a")));
    assert!(stack.get_item(&audio_id).is_none());
    assert!(stack.children.iter().any(|track| {
        track
            .items
            .iter()
            .any(|item| matches!(item, Item::Gap(_)) && (item.duration() - 3.0).abs() <= 1e-9)
    }));
}
