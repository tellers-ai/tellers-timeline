mod common;
use common::*;

#[test]
fn split_item_at_time_splits_synced_clips() {
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
        clip(4.0, Some("primary")),
        vec![audio_clip(4.0, "file:///a1.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();

    assert!(stack.split_item_at_time("primary", 2.0));

    assert_eq!(stack.children[0].items.len(), 2);
    assert_eq!(stack.children[result.audio_clips[0].1].items.len(), 2);
    assert_eq!(stack.children[0].items[0].duration(), 2.0);
    assert_eq!(stack.children[0].items[1].duration(), 2.0);
    assert_eq!(
        stack.children[result.audio_clips[0].1].items[0].duration(),
        2.0
    );
    assert_eq!(
        stack.children[result.audio_clips[0].1].items[1].duration(),
        2.0
    );
    assert_eq!(
        sync_clips_id(&stack.children[0].items[0]),
        result.sync_clips_id
    );
    assert_eq!(
        sync_clips_id(&stack.children[0].items[1]),
        Some(result.sync_clips_id.unwrap() + 1)
    );
    assert_eq!(
        sync_clips_id(&stack.children[result.audio_clips[0].1].items[0]),
        result.sync_clips_id
    );
    assert_eq!(
        sync_clips_id(&stack.children[result.audio_clips[0].1].items[1]),
        Some(result.sync_clips_id.unwrap() + 1)
    );
    assert!(stack.get_item(&audio_id).is_some());
    assert_ne!(
        stack.children[0].items[0].get_id(),
        stack.children[0].items[1].get_id()
    );
    assert_ne!(
        stack.children[result.audio_clips[0].1].items[0].get_id(),
        stack.children[result.audio_clips[0].1].items[1].get_id()
    );
}

#[test]
fn split_unsynced_item_only_splits_selected_track() {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(4.0, Some("primary"))));
    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio
        .items
        .push(Item::Clip(clip(4.0, Some("unlinked-audio"))));

    let mut stack = Stack::default();
    stack.children.push(video);
    stack.children.push(audio);

    assert!(stack.split_item_at_time("primary", 2.0));

    assert_eq!(stack.children[0].items.len(), 2);
    assert_eq!(stack.children[1].items.len(), 1);
    assert!(stack.get_item("unlinked-audio").is_some());
}
