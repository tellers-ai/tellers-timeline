use tellers_timeline_core::{insert::{InsertPolicy, OverlapPolicy}, Item, Timeline, Track, Clip, MediaSource, Gap};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

#[no_mangle]
pub extern "C" fn otio_parse_json_c(json_ptr: *const c_char) -> *mut Timeline {
    if json_ptr.is_null() { return std::ptr::null_mut(); }
    let cstr = unsafe { CStr::from_ptr(json_ptr) };
    let s = cstr.to_string_lossy();
    match serde_json::from_str::<Timeline>(&s) {
        Ok(tl) => Box::into_raw(Box::new(tl)),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn otio_timeline_to_json_c(tl_ptr: *const Timeline) -> *mut c_char {
    if tl_ptr.is_null() { return std::ptr::null_mut(); }
    let tl = unsafe { &*tl_ptr };
    match serde_json::to_string_pretty(tl) {
        Ok(s) => CString::new(s).unwrap().into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn otio_free_timeline(tl_ptr: *mut Timeline) {
    if tl_ptr.is_null() { return; }
    unsafe { drop(Box::from_raw(tl_ptr)); }
}

#[no_mangle]
pub extern "C" fn otio_free_cstring(s: *mut c_char) {
    if s.is_null() { return; }
    unsafe { drop(CString::from_raw(s)); }
}

// Minimal model/ops to reach parity with Python API via FFI

#[no_mangle]
pub extern "C" fn otio_timeline_new() -> *mut Timeline {
    Box::into_raw(Box::new(Timeline::default()))
}

#[no_mangle]
pub extern "C" fn otio_timeline_add_track(tl_ptr: *mut Timeline, track_ptr: *mut Track) {
    if tl_ptr.is_null() || track_ptr.is_null() { return; }
    let tl = unsafe { &mut *tl_ptr };
    let track = unsafe { Box::from_raw(track_ptr) };
    tl.tracks.push(*track);
}

#[no_mangle]
pub extern "C" fn otio_track_new(kind: *const c_char) -> *mut Track {
    let kind_str = if kind.is_null() { None } else { Some(unsafe { CStr::from_ptr(kind) }.to_string_lossy().into_owned()) };
    let tk = match kind_str.as_deref().map(|s| s.to_ascii_lowercase()) {
        Some(s) if s == "audio" => tellers_timeline_core::TrackKind::Audio,
        Some(s) if s == "video" => tellers_timeline_core::TrackKind::Video,
        Some(other) => tellers_timeline_core::TrackKind::Other(other.to_string()),
        None => tellers_timeline_core::TrackKind::Video,
    };
    Box::into_raw(Box::new(Track { otio_schema: "Track.1".to_string(), kind: tk, items: vec![], metadata: serde_json::Value::Null }))
}

#[no_mangle]
pub extern "C" fn otio_media_source_new(url_ptr: *const c_char) -> *mut MediaSource {
    if url_ptr.is_null() { return std::ptr::null_mut(); }
    let s = unsafe { CStr::from_ptr(url_ptr) }.to_string_lossy().into_owned();
    Box::into_raw(Box::new(MediaSource { otio_schema: "ExternalReference.1".to_string(), url: s, media_start: 0.0, media_duration: None, metadata: serde_json::Value::Null }))
}

#[no_mangle]
pub extern "C" fn otio_clip_new(duration: f64, ms_ptr: *mut MediaSource, name_ptr: *const c_char) -> *mut Clip {
    if ms_ptr.is_null() { return std::ptr::null_mut(); }
    let name = if name_ptr.is_null() { None } else { Some(unsafe { CStr::from_ptr(name_ptr) }.to_string_lossy().into_owned()) };
    let ms = unsafe { &*ms_ptr };
    Box::into_raw(Box::new(Clip { otio_schema: "Clip.2".to_string(), name, duration, source: ms.clone(), metadata: serde_json::Value::Null }))
}

#[no_mangle]
pub extern "C" fn otio_gap_new(duration: f64) -> *mut Gap {
    Box::into_raw(Box::new(Gap { otio_schema: "Gap.1".to_string(), duration, metadata: serde_json::Value::Null }))
}

#[no_mangle]
pub extern "C" fn otio_item_from_clip(c_ptr: *mut Clip) -> *mut Item {
    if c_ptr.is_null() { return std::ptr::null_mut(); }
    let c = unsafe { &*c_ptr };
    Box::into_raw(Box::new(Item::Clip(c.clone())))
}

#[no_mangle]
pub extern "C" fn otio_item_from_gap(g_ptr: *mut Gap) -> *mut Item {
    if g_ptr.is_null() { return std::ptr::null_mut(); }
    let g = unsafe { &*g_ptr };
    Box::into_raw(Box::new(Item::Gap(g.clone())))
}

#[no_mangle]
pub extern "C" fn otio_track_append(track_ptr: *mut Track, item_ptr: *mut Item) {
    if track_ptr.is_null() || item_ptr.is_null() { return; }
    let track = unsafe { &mut *track_ptr };
    let item = unsafe { Box::from_raw(item_ptr) };
    track.append(*item);
}

#[no_mangle]
pub extern "C" fn otio_track_insert_at_index(track_ptr: *mut Track, index: usize, item_ptr: *mut Item) {
    if track_ptr.is_null() || item_ptr.is_null() { return; }
    let track = unsafe { &mut *track_ptr };
    let item = unsafe { Box::from_raw(item_ptr) };
    track.insert_at_index(index, *item);
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

#[no_mangle]
pub extern "C" fn otio_track_insert_at_time_with(track_ptr: *mut Track, start_time: f64, item_ptr: *mut Item, overlap_policy_ptr: *const c_char, insert_policy_ptr: *const c_char) {
    if track_ptr.is_null() || item_ptr.is_null() { return; }
    let track = unsafe { &mut *track_ptr };
    let item = unsafe { Box::from_raw(item_ptr) };
    let op_s = if overlap_policy_ptr.is_null() { String::from("override") } else { unsafe { CStr::from_ptr(overlap_policy_ptr) }.to_string_lossy().into_owned() };
    let ip_s = if insert_policy_ptr.is_null() { String::from("insert_before_or_after") } else { unsafe { CStr::from_ptr(insert_policy_ptr) }.to_string_lossy().into_owned() };
    let op = overlap_policy_from_str(&op_s);
    let ip = insert_policy_from_str(&ip_s);
    track.insert_at_time_with(start_time, *item, op, ip);
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE: &str = include_str!("../../../spec/examples/simple.json");

    #[test]
    fn round_trip_simple() {
        let c = CString::new(SIMPLE).unwrap();
        let tl = otio_parse_json_c(c.as_ptr());
        assert!(!tl.is_null());
        let json_ptr = otio_timeline_to_json_c(tl);
        assert!(!json_ptr.is_null());
        let json = unsafe { CStr::from_ptr(json_ptr) }.to_string_lossy().into_owned();
        assert!(json.contains("\"tracks\""));
        otio_free_cstring(json_ptr);
        otio_free_timeline(tl);
    }
}
