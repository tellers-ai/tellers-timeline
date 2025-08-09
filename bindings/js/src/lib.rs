#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;
use js_sys::Array;
use tellers_timeline_core::insert::{InsertPolicy, OverlapPolicy};
use tellers_timeline_core::{Clip, Gap, Item, MediaSource, Track, TrackKind};

#[wasm_bindgen]
pub struct MediaSourceWasm { inner: MediaSource }

#[wasm_bindgen]
impl MediaSourceWasm {
    #[wasm_bindgen(constructor)]
    pub fn new(url: String) -> MediaSourceWasm {
        MediaSourceWasm { inner: MediaSource { otio_schema: "ExternalReference.1".to_string(), url, media_start: 0.0, media_duration: None, metadata: serde_json::Value::Null } }
    }
    #[wasm_bindgen(getter)]
    pub fn url(&self) -> String { self.inner.url.clone() }
    #[wasm_bindgen(setter)]
    pub fn set_url(&mut self, url: String) { self.inner.url = url; }
    #[wasm_bindgen(getter, js_name = mediaStart)]
    pub fn media_start(&self) -> f64 { self.inner.media_start }
    #[wasm_bindgen(setter, js_name = mediaStart)]
    pub fn set_media_start(&mut self, v: f64) { self.inner.media_start = v; }
    #[wasm_bindgen(getter, js_name = mediaDuration)]
    pub fn media_duration(&self) -> Option<f64> { self.inner.media_duration }
    #[wasm_bindgen(setter, js_name = mediaDuration)]
    pub fn set_media_duration(&mut self, v: Option<f64>) { self.inner.media_duration = v; }
}

#[wasm_bindgen]
pub struct ClipWasm { inner: Clip }

#[wasm_bindgen]
impl ClipWasm {
    #[wasm_bindgen(constructor)]
    pub fn new(duration: f64, source: &MediaSourceWasm, name: Option<String>) -> ClipWasm {
        ClipWasm { inner: Clip { otio_schema: "Clip.2".to_string(), name, duration, source: source.inner.clone(), metadata: serde_json::Value::Null } }
    }
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> Option<String> { self.inner.name.clone() }
    #[wasm_bindgen(setter)]
    pub fn set_name(&mut self, name: Option<String>) { self.inner.name = name; }
    #[wasm_bindgen(getter)]
    pub fn duration(&self) -> f64 { self.inner.duration }
    #[wasm_bindgen(setter)]
    pub fn set_duration(&mut self, v: f64) { self.inner.duration = v; }
    #[wasm_bindgen(getter)]
    pub fn source(&self) -> MediaSourceWasm { MediaSourceWasm { inner: self.inner.source.clone() } }
    #[wasm_bindgen(setter)]
    pub fn set_source(&mut self, source: &MediaSourceWasm) { self.inner.source = source.inner.clone(); }
}

#[wasm_bindgen]
pub struct GapWasm { inner: Gap }

#[wasm_bindgen]
impl GapWasm {
    #[wasm_bindgen(constructor)]
    pub fn new(duration: f64) -> GapWasm { GapWasm { inner: Gap { otio_schema: "Gap.1".to_string(), duration, metadata: serde_json::Value::Null } } }
    #[wasm_bindgen(getter)]
    pub fn duration(&self) -> f64 { self.inner.duration }
    #[wasm_bindgen(setter)]
    pub fn set_duration(&mut self, v: f64) { self.inner.duration = v; }
}

#[wasm_bindgen]
pub struct ItemWasm { inner: Item }

#[wasm_bindgen]
impl ItemWasm {
    #[wasm_bindgen(js_name = fromClip)]
    pub fn from_clip(c: &ClipWasm) -> ItemWasm { ItemWasm { inner: Item::Clip(c.inner.clone()) } }
    #[wasm_bindgen(js_name = fromGap)]
    pub fn from_gap(g: &GapWasm) -> ItemWasm { ItemWasm { inner: Item::Gap(g.inner.clone()) } }
    #[wasm_bindgen(js_name = isClip)]
    pub fn is_clip(&self) -> bool { matches!(self.inner, Item::Clip(_)) }
    #[wasm_bindgen(js_name = isGap)]
    pub fn is_gap(&self) -> bool { matches!(self.inner, Item::Gap(_)) }
    pub fn duration(&self) -> f64 { match &self.inner { Item::Clip(c) => c.duration, Item::Gap(g) => g.duration } }
    #[wasm_bindgen(js_name = setDuration)]
    pub fn set_duration(&mut self, dur: f64) { match &mut self.inner { Item::Clip(c) => c.duration = dur, Item::Gap(g) => g.duration = dur } }
}

#[wasm_bindgen]
pub struct TrackWasm { inner: Track }

fn track_kind_from_str(s: &str) -> TrackKind {
    match s.to_ascii_lowercase().as_str() {
        "video" => TrackKind::Video,
        "audio" => TrackKind::Audio,
        other => TrackKind::Other(other.to_string()),
    }
}

fn overlap_policy_from_str(s: &str) -> OverlapPolicy {
    match s.to_ascii_lowercase().as_str() {
        "override" => OverlapPolicy::Override,
        "keep" => OverlapPolicy::Keep,
        "push" => OverlapPolicy::Push,
        _ => OverlapPolicy::Override,
    }
}

fn insert_policy_from_str(s: &str) -> InsertPolicy {
    match s.to_ascii_lowercase().as_str() {
        "split_and_insert" | "split" => InsertPolicy::SplitAndInsert,
        "insert_before" | "before" => InsertPolicy::InsertBefore,
        "insert_after" | "after" => InsertPolicy::InsertAfter,
        _ => InsertPolicy::InsertBeforeOrAfter,
    }
}

#[wasm_bindgen]
impl TrackWasm {
    #[wasm_bindgen(constructor)]
    pub fn new(kind: Option<String>) -> TrackWasm {
        let k = kind.map(|s| track_kind_from_str(&s)).unwrap_or(TrackKind::Video);
        TrackWasm { inner: Track { otio_schema: "Track.1".to_string(), kind: k, items: vec![], metadata: serde_json::Value::Null } }
    }
    #[wasm_bindgen(getter)]
    pub fn kind(&self) -> String { match &self.inner.kind { TrackKind::Video => "video".to_string(), TrackKind::Audio => "audio".to_string(), TrackKind::Other(s) => s.clone(), } }
    #[wasm_bindgen(setter)]
    pub fn set_kind(&mut self, kind: String) { self.inner.kind = track_kind_from_str(&kind); }
    pub fn items(&self) -> Array {
        self.inner
            .items
            .iter()
            .cloned()
            .map(|it| ItemWasm { inner: it })
            .map(JsValue::from)
            .collect()
    }
    #[wasm_bindgen(js_name = clearItems)]
    pub fn clear_items(&mut self) { self.inner.items.clear(); }
    pub fn append(&mut self, item: &ItemWasm) { self.inner.append(item.inner.clone()); }
    #[wasm_bindgen(js_name = insertAtIndex)]
    pub fn insert_at_index(&mut self, index: usize, item: &ItemWasm) { self.inner.insert_at_index(index, item.inner.clone()); }
    #[wasm_bindgen(js_name = insertAtTimeWith)]
    pub fn insert_at_time_with(&mut self, start_time: f64, item: &ItemWasm, overlap_policy: String, insert_policy: String) {
        let op = overlap_policy_from_str(&overlap_policy);
        let ip = insert_policy_from_str(&insert_policy);
        self.inner.insert_at_time_with(start_time, item.inner.clone(), op, ip);
    }
    #[wasm_bindgen(js_name = insertClip)]
    pub fn insert_clip(&mut self, start_time: f64, duration: f64, url: String, overlap_policy: String, insert_policy: String, name: Option<String>, media_start: Option<f64>, media_duration: Option<f64>) {
        let ms = MediaSource { otio_schema: "ExternalReference.1".to_string(), url, media_start: media_start.unwrap_or(0.0), media_duration, metadata: serde_json::Value::Null };
        let clip = Clip { otio_schema: "Clip.2".to_string(), name, duration, source: ms, metadata: serde_json::Value::Null };
        let item = Item::Clip(clip);
        let op = overlap_policy_from_str(&overlap_policy);
        let ip = insert_policy_from_str(&insert_policy);
        self.inner.insert_at_time_with(start_time, item, op, ip);
    }
    #[wasm_bindgen(js_name = totalDuration)]
    pub fn total_duration(&self) -> f64 { self.inner.total_duration() }
    #[wasm_bindgen(js_name = startTimeOfItem)]
    pub fn start_time_of_item(&self, index: usize) -> f64 { self.inner.start_time_of_item(index) }
}

#[wasm_bindgen]
pub struct TimelineWasm { inner: tellers_timeline_core::Timeline }

#[wasm_bindgen]
impl TimelineWasm {
    #[wasm_bindgen(constructor)]
    pub fn new() -> TimelineWasm { TimelineWasm { inner: tellers_timeline_core::Timeline::default() } }

    #[wasm_bindgen(js_name = parseJson)]
    pub fn parse_json(s: &str) -> Result<TimelineWasm, JsValue> {
        let tl: tellers_timeline_core::Timeline = serde_json::from_str(s).map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(TimelineWasm { inner: tl })
    }

    #[wasm_bindgen(js_name = toJson)]
    pub fn to_json(&self) -> Result<String, JsValue> {
        serde_json::to_string_pretty(&self.inner).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    #[wasm_bindgen]
    pub fn validate(&self) -> js_sys::Array {
        tellers_timeline_core::validate_timeline(&self.inner)
            .into_iter()
            .map(|e| JsValue::from_str(&e.to_string()))
            .collect()
    }
    #[wasm_bindgen]
    pub fn sanitize(&mut self) { self.inner.sanitize(); }
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> Option<String> { self.inner.name.clone() }
    #[wasm_bindgen(setter)]
    pub fn set_name(&mut self, name: Option<String>) { self.inner.name = name; }
    pub fn tracks(&self) -> Array {
        self.inner
            .tracks
            .iter()
            .cloned()
            .map(|t| TrackWasm { inner: t })
            .map(JsValue::from)
            .collect()
    }
    #[wasm_bindgen(js_name = addTrack)]
    pub fn add_track(&mut self, track: &TrackWasm) { self.inner.tracks.push(track.inner.clone()); }
}
