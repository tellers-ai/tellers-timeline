//! Shared fixtures and helpers for the synced-clip integration tests.
//
// Each test binary uses only a subset of these helpers and re-exports, so allow
// the unused ones here rather than warning in every crate that includes it.
#![allow(dead_code, unused_imports)]

pub use tellers_timeline_core::{
    Clip, Gap, IdMetadataExt, InsertItemAtTimeResult, InsertPolicy, Item, SyncedInsertResult,
    MediaReference, OverlapPolicy, RationalTime, Stack, SyncTrackInfo, TimeRange, Timeline, Track,
    TrackKind,
};

pub fn range(duration: f64) -> TimeRange {
    TimeRange {
        otio_schema: "TimeRange.1".to_string(),
        start_time: RationalTime {
            otio_schema: "RationalTime.1".to_string(),
            rate: 1.0,
            value: 0.0,
        },
        duration: RationalTime {
            otio_schema: "RationalTime.1".to_string(),
            rate: 1.0,
            value: duration,
        },
    }
}

pub fn media_ref(url: &str, media_id: Option<&str>) -> MediaReference {
    let metadata = media_id
        .map(|id| {
            serde_json::json!({
                "media_id": id,
                "tellers.ai": {
                    "media_id": id
                }
            })
        })
        .unwrap_or_else(|| serde_json::json!({}));

    MediaReference::ExternalReference {
        target_url: url.to_string(),
        available_range: Some(range(100.0)),
        name: None,
        available_image_bounds: Some(serde_json::Value::Null),
        metadata,
    }
}

pub fn clip(duration: f64, id: Option<&str>) -> Clip {
    Clip::new_single_media_reference(
        range(duration),
        media_ref("file:///video.mov", Some("shared-media")),
        None,
        id.map(|s| s.to_string()),
    )
}

pub fn clip_with_references(duration: f64, active_key: Option<&str>, id: Option<&str>) -> Clip {
    let mut refs = std::collections::HashMap::new();
    refs.insert(
        "ALT".to_string(),
        media_ref("file:///replacement-alt.mov", Some("replacement-alt")),
    );
    refs.insert(
        "DEFAULT_MEDIA".to_string(),
        media_ref("file:///replacement-default.mov", Some("replacement-default")),
    );
    Clip::new(
        range(duration),
        refs,
        active_key.map(str::to_string),
        None,
        id.map(str::to_string),
    )
}

pub fn audio_clip(duration: f64, url: &str, media_id: Option<&str>) -> Item {
    Item::Clip(Clip::new_single_media_reference(
        range(duration),
        media_ref(url, media_id),
        None,
        None,
    ))
}

pub fn audio_clip_with_available_duration(duration: f64, url: &str, available_duration: f64) -> Item {
    Item::Clip(Clip::new_single_media_reference(
        range(duration),
        MediaReference::ExternalReference {
            target_url: url.to_string(),
            available_range: Some(range(available_duration)),
            name: None,
            available_image_bounds: Some(serde_json::Value::Null),
            metadata: serde_json::json!({}),
        },
        None,
        None,
    ))
}

pub fn clip_with_media_range(
    duration: f64,
    source_start: f64,
    media_start: f64,
    media_duration: f64,
) -> Clip {
    let mut c = clip(duration, None);
    c.source_range.start_time.value = source_start;
    c.media_references.insert(
        "DEFAULT_MEDIA".to_string(),
        MediaReference::ExternalReference {
            target_url: "file:///ranged.mov".to_string(),
            available_range: Some(TimeRange {
                otio_schema: "TimeRange.1".to_string(),
                start_time: RationalTime {
                    otio_schema: "RationalTime.1".to_string(),
                    rate: 1.0,
                    value: media_start,
                },
                duration: RationalTime {
                    otio_schema: "RationalTime.1".to_string(),
                    rate: 1.0,
                    value: media_duration,
                },
            }),
            name: None,
            available_image_bounds: Some(serde_json::Value::Null),
            metadata: serde_json::json!({}),
        },
    );
    c
}

pub fn sync_clips_id(item: &Item) -> Option<i64> {
    match item {
        Item::Clip(clip) => clip
            .metadata
            .get("Resolve_OTIO")
            .and_then(|v| v.get("Link Group ID"))
            .and_then(|v| v.as_i64()),
        Item::Gap(_) => None,
    }
}

pub fn source_start(item: &Item) -> f64 {
    match item {
        Item::Clip(clip) => clip.source_range.start_time.value,
        Item::Gap(gap) => gap.source_range.start_time.value,
    }
}

pub fn active_target_url(item: &Item) -> Option<&str> {
    let Item::Clip(clip) = item else {
        return None;
    };
    let key = clip
        .active_media_reference_key
        .as_deref()
        .unwrap_or("DEFAULT_MEDIA");
    clip.media_references
        .get(key)
        .and_then(MediaReference::target_url)
        .map(String::as_str)
}

pub fn range_is_gap_backed_for_test(track: &Track, start: f64, end: f64) -> bool {
    let mut pos = 0.0;
    for item in &track.items {
        let item_start = pos;
        let item_end = pos + item.duration().max(0.0);
        if item_end > start + 1e-9 && item_start < end - 1e-9 && !matches!(item, Item::Gap(_)) {
            return false;
        }
        pos = item_end;
    }
    true
}

pub fn synced_clip_item(duration: f64, id: &str, sync_clips_id: i64) -> Item {
    let mut clip = clip(duration, Some(id));
    clip.metadata["Resolve_OTIO"] = serde_json::json!({
        "Link Group ID": sync_clips_id
    });
    Item::Clip(clip)
}

pub fn synced_clip_item_with_source_start(
    duration: f64,
    source_start: f64,
    id: &str,
    sync_clips_id: i64,
) -> Item {
    let mut item = synced_clip_item(duration, id, sync_clips_id);
    if let Item::Clip(clip) = &mut item {
        clip.source_range.start_time.value = source_start;
    }
    item
}

pub fn insert_with_audio(
    stack: &mut Stack,
    dest_track_index: usize,
    dest_time: f64,
    clip: Clip,
    synced_audio_clips: Vec<Item>,
) -> Option<SyncedInsertResult> {
    match stack.insert_item_at_time(
        dest_track_index,
        dest_time,
        Item::Clip(clip),
        OverlapPolicy::Override,
        InsertPolicy::InsertBefore,
        Some(synced_audio_clips),
    None,
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => Some(result),
        _ => None,
    }
}

pub fn stack_with_synced_audio_below_video() -> Stack {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(2.0)));
    video.items.push(Item::Clip(clip(2.0, Some("linked-video"))));
    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio.items.push(Item::Gap(Gap::make_gap(2.0)));
    audio
        .items
        .push(audio_clip(2.0, "file:///linked-audio.wav", None));
    audio.items[1].set_id(Some("linked-audio".to_string()));

    let mut stack = Stack::default();
    stack.children.push(audio);
    stack.children.push(video);
    stack
        .sync_item(&["linked-video".to_string(), "linked-audio".to_string()])
        .unwrap();
    stack
}

pub fn resolve_metadata_clip_item(
    duration: f64,
    id: &str,
    link_group_id: i64,
    use_resolve_key: bool,
) -> Item {
    let mut clip = clip(duration, Some(id));
    let key = if use_resolve_key {
        "resolve"
    } else {
        "Resolve_OTIO"
    };
    clip.metadata[key] = serde_json::json!({
        "Link Group ID": link_group_id
    });
    Item::Clip(clip)
}

pub fn stack_with_offset_linked_clips(use_resolve_key: bool) -> Stack {
    let mut stack = Stack::default();
    let mut video = Track::new(TrackKind::Video, Some("video-track".to_string()));
    video.items.push(resolve_metadata_clip_item(
        5.0,
        "video-1",
        1,
        use_resolve_key,
    ));
    stack.children.push(video);

    let mut audio = Track::new(TrackKind::Audio, Some("audio-track".to_string()));
    audio.items.push(Item::Gap(Gap::make_gap(2.0)));
    audio.items.push(resolve_metadata_clip_item(
        5.0,
        "audio-1",
        1,
        use_resolve_key,
    ));
    stack.children.push(audio);
    stack
}

pub fn assert_sync_track_info_unchanged(before: &[SyncTrackInfo], after: &[SyncTrackInfo]) {
    assert_eq!(
        after, before,
        "sync_track_info cluster grouping should not change",
    );
}

pub fn synced_clip_item_with_rate(
    duration: f64,
    source_start: f64,
    id: &str,
    sync_clips_id: i64,
    rate: f64,
) -> Item {
    let mut item = synced_clip_item_with_source_start(duration, source_start, id, sync_clips_id);
    if let Item::Clip(clip) = &mut item {
        clip.source_range.duration.rate = rate;
        clip.source_range.start_time.rate = rate;
        clip.source_range.duration.value = duration * rate;
        clip.source_range.start_time.value = source_start * rate;
    }
    item
}

pub fn stack_v1_clip_a_clip_b_a1() -> Stack {
    let mut video = Track::new(TrackKind::Video, Some("v1".to_string()));
    video.items.push(Item::Clip(clip(10.0, Some("clip-a"))));
    video.items.push(synced_clip_item(10.0, "clip-b-video", 1));
    let mut audio = Track::new(TrackKind::Audio, Some("a1".to_string()));
    audio.items.push(Item::Gap(Gap::make_gap(10.0)));
    audio.items.push(synced_clip_item(10.0, "clip-b-audio", 1));
    let mut stack = Stack::default();
    stack.children.push(audio);
    stack.children.push(video);
    stack
}

pub fn track_index_by_id(stack: &Stack, id: &str) -> usize {
    stack
        .children
        .iter()
        .position(|track| track.get_id().as_deref() == Some(id))
        .unwrap_or_else(|| panic!("track {id:?} not found"))
}

pub fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

pub fn assert_item_span(track: &Track, item_index: usize, expected_start: f64, expected_duration: f64) {
    let start = track.start_time_of_item(item_index);
    let duration = track.items[item_index].duration();
    assert!(
        (start - expected_start).abs() < 1e-9,
        "item {item_index} start: got {start}, expected {expected_start}"
    );
    assert!(
        (duration - expected_duration).abs() < 1e-9,
        "item {item_index} duration: got {duration}, expected {expected_duration}"
    );
}

pub fn stack_v1_c1_c2_a1() -> Stack {
    let mut video = Track::new(TrackKind::Video, Some("v1".to_string()));
    video.items.push(synced_clip_item(3.0, "c1", 1));
    video.items.push(Item::Clip(clip(4.0, Some("c2"))));
    let mut audio = Track::new(TrackKind::Audio, Some("a1".to_string()));
    audio.items.push(synced_clip_item(3.0, "c1a", 1));
    let mut stack = Stack::default();
    stack.children.push(audio);
    stack.children.push(video);
    stack
}

pub fn two_synced_clips() -> Stack {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(4.0, Some("vA")))); // 0..4
    video.items.push(Item::Clip(clip(4.0, Some("vB")))); // 4..8
    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio.items.push(audio_clip(4.0, "file:///aA.wav", None));
    audio.items[0].set_id(Some("aA".to_string()));
    audio.items.push(audio_clip(4.0, "file:///aB.wav", None));
    audio.items[1].set_id(Some("aB".to_string()));
    let mut stack = Stack::default();
    stack.children.push(audio);
    stack.children.push(video);
    stack.sync_item(&["vA".to_string(), "aA".to_string()]).unwrap();
    stack.sync_item(&["vB".to_string(), "aB".to_string()]).unwrap();
    stack
}

// For every sync clips, the sorted list of member durations must be identical on
// each track the group occupies (a group split into segments must split the same
// way on every track).
pub fn assert_sync_clips_track_aligned(stack: &Stack, label: &str) {
    use std::collections::HashMap;
    let mut map: HashMap<i64, HashMap<usize, Vec<f64>>> = HashMap::new();
    for (track_index, track) in stack.children.iter().enumerate() {
        for item in &track.items {
            if let Some(group) = sync_clips_id(item) {
                map.entry(group)
                    .or_default()
                    .entry(track_index)
                    .or_default()
                    .push(item.duration().max(0.0));
            }
        }
    }
    for (group, per_track) in &map {
        let mut sorted: Vec<Vec<f64>> = per_track
            .values()
            .map(|v| {
                let mut v = v.clone();
                v.sort_by(|a, b| a.partial_cmp(b).unwrap());
                v
            })
            .collect();
        let reference = sorted.pop().unwrap();
        for durs in &sorted {
            assert_eq!(
                durs.len(),
                reference.len(),
                "{label}: sync clips {group} segment count differs across tracks: {durs:?} vs {reference:?}"
            );
            for (x, y) in durs.iter().zip(reference.iter()) {
                assert!(
                    (x - y).abs() < 1e-9,
                    "{label}: sync clips {group} duration footprint differs across tracks: {durs:?} vs {reference:?}"
                );
            }
        }
    }
}

// Build one sync set (1 video + `audio_count` audios), all at time 0, duration `dur`.
// Tracks are named "{prefix}-v" / "{prefix}-a{i}", clips "{prefix}-vid" / "{prefix}-aud{i}".
pub fn push_sync_set(stack: &mut Stack, prefix: &str, dur: f64, audio_count: usize) {
    let mut video = Track::new(TrackKind::Video, Some(format!("{prefix}-v")));
    video
        .items
        .push(Item::Clip(clip(dur, Some(&format!("{prefix}-vid")))));
    stack.children.push(video);
    let mut ids = vec![format!("{prefix}-vid")];
    for i in 0..audio_count {
        let mut audio = Track::new(TrackKind::Audio, Some(format!("{prefix}-a{i}")));
        let mut a = audio_clip(dur, &format!("file:///{prefix}-a{i}.wav"), None);
        a.set_id(Some(format!("{prefix}-aud{i}")));
        audio.items.push(a);
        stack.children.push(audio);
        ids.push(format!("{prefix}-aud{i}"));
    }
    stack.sync_item(&ids.iter().map(String::as_str).map(str::to_string).collect::<Vec<_>>())
        .unwrap();
}

// Build an empty destination: 1 video track + `audio_count` audio tracks, all gaps.
pub fn push_empty_dest(stack: &mut Stack, prefix: &str, audio_count: usize, len: f64) {
    let mut video = Track::new(TrackKind::Video, Some(format!("{prefix}-v")));
    video.items.push(Item::Gap(Gap::make_gap(len)));
    stack.children.push(video);
    for i in 0..audio_count {
        let mut audio = Track::new(TrackKind::Audio, Some(format!("{prefix}-a{i}")));
        audio.items.push(Item::Gap(Gap::make_gap(len)));
        stack.children.push(audio);
    }
}

pub fn ids_for_set(prefix: &str, audio_count: usize) -> Vec<String> {
    let mut ids = vec![format!("{prefix}-vid")];
    for i in 0..audio_count {
        ids.push(format!("{prefix}-aud{i}"));
    }
    ids
}
