use tellers_timeline_core::Timeline;
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
