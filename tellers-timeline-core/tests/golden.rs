use std::path::PathBuf;
use tellers_timeline_core::{validate_timeline, Timeline};

fn example_path(name: &str) -> PathBuf {
    let crate_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(crate_dir)
        .join("..")
        .join("spec")
        .join("examples")
        .join(name)
}

fn read_example(name: &str) -> String {
    std::fs::read_to_string(example_path(name)).expect("example exists")
}

#[test]
fn round_trip_simple() {
    let json = read_example("simple.json");
    let tl: Timeline = serde_json::from_str(&json).expect("parse");
    let errors = validate_timeline(&tl);
    assert!(errors.is_empty(), "validation errors: {:?}", errors);
    let out = serde_json::to_string_pretty(&tl).unwrap();
    let tl2: Timeline = serde_json::from_str(&out).unwrap();
    assert_eq!(tl, tl2);
}

#[test]
fn round_trip_two_tracks() {
    let json = read_example("two_tracks.json");
    let tl: Timeline = serde_json::from_str(&json).expect("parse");
    let errors = validate_timeline(&tl);
    assert!(errors.is_empty(), "validation errors: {:?}", errors);
    let out = serde_json::to_string_pretty(&tl).unwrap();
    let tl2: Timeline = serde_json::from_str(&out).unwrap();
    assert_eq!(tl, tl2);
}

#[test]
fn round_trip_preserves_metadata() {
    let json = read_example("two_tracks.json");
    let tl: Timeline = serde_json::from_str(&json).expect("parse");
    let errors = validate_timeline(&tl);
    assert!(errors.is_empty(), "validation errors: {:?}", errors);
    let out = serde_json::to_string_pretty(&tl).unwrap();
    let tl2: Timeline = serde_json::from_str(&out).unwrap();
    assert_eq!(tl, tl2);
}

#[test]
fn round_trip_arbitrary_metadata() {
    let json = read_example("arbitrary_metadata.json");
    let tl: Timeline = serde_json::from_str(&json).expect("parse");
    let errors = validate_timeline(&tl);
    assert!(errors.is_empty(), "validation errors: {:?}", errors);
    let out = serde_json::to_string_pretty(&tl).unwrap();
    let tl2: Timeline = serde_json::from_str(&out).unwrap();
    assert_eq!(tl, tl2);
}
