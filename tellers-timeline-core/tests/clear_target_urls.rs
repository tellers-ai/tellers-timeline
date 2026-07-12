//! Tests for `Timeline::clear_target_urls`, which empties the `target_url` of
//! every media reference on every clip (used before serving a project or
//! applying a front-end update so ephemeral media URLs are not persisted).

mod common;

use common::{audio_clip, clip_with_references};
use tellers_timeline_core::{Gap, Item, MediaReference, Stack, Timeline, Track, TrackKind};

/// Collect every external-reference `target_url` across all clips in the timeline.
fn all_target_urls(timeline: &Timeline) -> Vec<String> {
    let mut urls = Vec::new();
    for track in &timeline.tracks.children {
        for item in &track.items {
            if let Item::Clip(clip) = item {
                for reference in clip.media_references.values() {
                    if let Some(url) = reference.target_url() {
                        urls.push(url.clone());
                    }
                }
            }
        }
    }
    urls
}

fn timeline_with_clips() -> Timeline {
    // Video clip carries two external references (ALT + DEFAULT_MEDIA).
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(2.0)));
    video.items.push(Item::Clip(clip_with_references(
        4.0,
        Some("DEFAULT_MEDIA"),
        Some("video-clip"),
    )));

    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio
        .items
        .push(audio_clip(4.0, "file:///audio.wav", Some("audio-media")));

    let mut timeline = Timeline::default();
    timeline.tracks = Stack {
        children: vec![video, audio],
        ..Stack::default()
    };
    timeline
}

#[test]
fn clears_all_target_urls_on_every_clip() {
    let mut timeline = timeline_with_clips();

    // Precondition: every reference starts with a non-empty target_url.
    let before = all_target_urls(&timeline);
    assert_eq!(before.len(), 3, "expected 3 external references before clearing");
    assert!(
        before.iter().all(|url| !url.is_empty()),
        "fixture should start with populated target_urls, got {before:?}"
    );

    timeline.clear_target_urls();

    let after = all_target_urls(&timeline);
    assert_eq!(after.len(), 3, "clearing must not drop references");
    assert!(
        after.iter().all(|url| url.is_empty()),
        "all target_urls should be empty after clearing, got {after:?}"
    );
}

#[test]
fn clear_target_url_is_noop_for_generator_references() {
    let mut generator = MediaReference::GeneratorReference {
        generator_kind: "color".to_string(),
        available_range: None,
        name: None,
        available_image_bounds: None,
        metadata: serde_json::Value::Null,
        parameters: Default::default(),
    };

    // No panic and no target_url appears.
    generator.clear_target_url();
    assert_eq!(generator.target_url(), None);
}
