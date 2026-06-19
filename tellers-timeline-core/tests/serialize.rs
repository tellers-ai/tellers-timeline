use std::path::PathBuf;
use tellers_timeline_core::{to_json_with_precision, Timeline};

fn read_example(name: &str) -> String {
    let crate_dir = env!("CARGO_MANIFEST_DIR");
    let path = PathBuf::from(crate_dir)
        .join("..")
        .join("spec")
        .join("examples")
        .join(name);
    std::fs::read_to_string(path).expect("example exists")
}

// `to_json_with_precision` is the serialization entry point tellers-app (Flutter)
// uses to write a timeline back to JSON, e.g. `to_json_with_precision(&timeline, None, false)`.

#[test]
fn to_json_with_precision_round_trips_timeline_compact() {
    let timeline: Timeline = serde_json::from_str(&read_example("simple.json")).expect("parse");
    let out = to_json_with_precision(&timeline, None, false).expect("serialize");
    assert!(!out.contains('\n'), "compact output should be single-line");
    let reparsed: Timeline = serde_json::from_str(&out).expect("reparse");
    assert_eq!(timeline, reparsed);
}

#[test]
fn to_json_with_precision_round_trips_timeline_pretty() {
    let timeline: Timeline = serde_json::from_str(&read_example("two_tracks.json")).expect("parse");
    let out = to_json_with_precision(&timeline, None, true).expect("serialize pretty");
    assert!(out.contains('\n'), "pretty output should be multi-line");
    let reparsed: Timeline = serde_json::from_str(&out).expect("reparse");
    assert_eq!(timeline, reparsed);
}

#[test]
fn timeline_to_json_round_trips() {
    // `Timeline::to_json()` is the serialization method tellers-backend (Python) uses.
    let timeline: Timeline = serde_json::from_str(&read_example("simple.json")).expect("parse");
    let out = timeline.to_json().expect("to_json");
    assert!(out.contains('\n'), "to_json() is pretty-printed");
    let reparsed: Timeline = serde_json::from_str(&out).expect("reparse");
    assert_eq!(timeline, reparsed);
}

#[test]
fn timeline_to_json_with_options_round_trips() {
    let timeline: Timeline = serde_json::from_str(&read_example("two_tracks.json")).expect("parse");
    let compact = timeline.to_json_with_options(None, false).expect("compact");
    assert!(!compact.contains('\n'), "compact output is single-line");
    let reparsed: Timeline = serde_json::from_str(&compact).expect("reparse");
    assert_eq!(timeline, reparsed);
}

#[test]
fn to_json_with_precision_rounds_floats_to_requested_decimals() {
    let value = serde_json::json!({ "x": 1.23456_f64 });

    // No precision: the full value is preserved.
    let full = to_json_with_precision(&value, None, false).expect("serialize");
    assert!(full.contains("1.23456"), "full precision expected, got: {full}");

    // precision = 2 rounds to two decimals (round-half-away-from-zero).
    let rounded = to_json_with_precision(&value, Some(2), false).expect("serialize");
    assert!(rounded.contains("1.23"), "expected rounded value, got: {rounded}");
    assert!(
        !rounded.contains("1.23456"),
        "precision should have dropped trailing digits, got: {rounded}"
    );
}
