use wasm_bindgen_test::*;
use tellers_timeline::TimelineWasm;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_node);

const SIMPLE: &str = include_str!("../../spec/examples/simple.json");
const TWO: &str = include_str!("../../spec/examples/two_tracks.json");

#[wasm_bindgen_test]
fn round_trip_simple() {
    let tl = TimelineWasm::parse_json(SIMPLE).expect("parse");
    let errs = tl.validate();
    assert_eq!(errs.length(), 0);
    let out = tl.to_json().unwrap();
    let tl2 = TimelineWasm::parse_json(&out).unwrap();
    assert_eq!(tl2.to_json().unwrap(), out);
}

#[wasm_bindgen_test]
fn round_trip_two() {
    let tl = TimelineWasm::parse_json(TWO).expect("parse");
    let errs = tl.validate();
    assert_eq!(errs.length(), 0);
}
