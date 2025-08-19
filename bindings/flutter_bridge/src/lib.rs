#![allow(clippy::needless_pass_by_value)]

use flutter_rust_bridge::frb;
use tellers_timeline_core::{validate_timeline, Timeline, Track, Item, Clip, MediaSource};

// Phase 1: JSON-based API minimalist surface to enable unified Flutter native+web via FRB

#[frb]
pub fn tt_parse_json(json: String) -> Result<String, String> {
    let tl: Timeline = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    serde_json::to_string_pretty(&tl).map_err(|e| e.to_string())
}

#[frb]
pub fn tt_validate_json(json: String) -> Vec<String> {
    match serde_json::from_str::<Timeline>(&json) {
        Ok(tl) => validate_timeline(&tl).into_iter().map(|e| e.to_string()).collect(),
        Err(e) => vec![e.to_string()],
    }
}

#[frb]
pub fn tt_sanitize_json(json: String) -> Result<String, String> {
    let mut tl: Timeline = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    tl.sanitize();
    serde_json::to_string_pretty(&tl).map_err(|e| e.to_string())
}

#[derive(serde::Deserialize)]
struct InsertClipArgs {
    start_time: f64,
    duration: f64,
    url: String,
    overlap_policy: String,
    insert_policy: String,
    name: Option<String>,
    media_start: Option<f64>,
    media_duration: Option<f64>,
    track_index: usize,
}

#[frb]
pub fn tt_insert_clip(json_timeline: String, args_json: String) -> Result<String, String> {
    use tellers_timeline_core::insert::{InsertPolicy, OverlapPolicy};
    let mut tl: Timeline = serde_json::from_str(&json_timeline).map_err(|e| e.to_string())?;
    let args: InsertClipArgs = serde_json::from_str(&args_json).map_err(|e| e.to_string())?;

    if args.track_index >= tl.tracks.len() {
        // create tracks up to index as video by default
        while tl.tracks.len() <= args.track_index {
            tl.tracks.push(Track::default());
        }
    }

    let ms = MediaSource {
        otio_schema: "ExternalReference.1".to_string(),
        url: args.url,
        media_start: args.media_start.unwrap_or(0.0),
        media_duration: args.media_duration,
        metadata: serde_json::Value::Null,
    };
    let clip = Clip { otio_schema: "Clip.2".to_string(), name: args.name, duration: args.duration, source: ms, metadata: serde_json::Value::Null };
    let item = Item::Clip(clip);

    let op = match args.overlap_policy.to_ascii_lowercase().as_str() {
        "override" => OverlapPolicy::Override,
        "keep" => OverlapPolicy::Keep,
        "push" => OverlapPolicy::Push,
        _ => OverlapPolicy::Override,
    };
    let ip = match args.insert_policy.to_ascii_lowercase().as_str() {
        "split_and_insert" | "split" => InsertPolicy::SplitAndInsert,
        "insert_before" | "before" => InsertPolicy::InsertBefore,
        "insert_after" | "after" => InsertPolicy::InsertAfter,
        _ => InsertPolicy::InsertBeforeOrAfter,
    };

    tl.tracks[args.track_index].insert_at_time_with(args.start_time, item, op, ip);

    serde_json::to_string_pretty(&tl).map_err(|e| e.to_string())
}


