// Tests for the timeline custom-font accessors (`Timeline::get_fonts`,
// `add_font`, `remove_font` and the underlying metadata helpers). These read
// and write `metadata["tellers.ai"]["availableFonts"]`, the array the player
// takes as its `availableFonts` option, so the stored shape is part of the
// contract with video-player-js.

use tellers_timeline_core::{
    add_available_font, remove_available_font, resolve_available_fonts, FontFace, Timeline,
};

fn timeline() -> Timeline {
    Timeline::default()
}

fn brand_sans() -> FontFace {
    FontFace::new("Brand Sans", "https://cdn.example.test/brand-sans.woff2")
        .with_weight("400 700")
        .with_style("normal")
}

#[test]
fn timeline_without_fonts_has_none() {
    assert!(timeline().get_fonts().is_empty());
}

#[test]
fn add_font_then_get_fonts_round_trips() {
    let mut tl = timeline();
    assert!(tl.add_font(brand_sans()));

    assert_eq!(tl.get_fonts(), vec![brand_sans()]);
    let stored = &tl.metadata["tellers.ai"]["availableFonts"];
    assert_eq!(
        stored,
        &serde_json::json!([{
            "family": "Brand Sans",
            "url": "https://cdn.example.test/brand-sans.woff2",
            "weight": "400 700",
            "style": "normal",
        }])
    );
}

#[test]
fn add_font_omits_absent_weight_and_style() {
    let mut tl = timeline();
    assert!(tl.add_font(FontFace::new("Plain", "data:font/woff2;base64,AAAA")));

    assert_eq!(
        tl.metadata["tellers.ai"]["availableFonts"],
        serde_json::json!([{ "family": "Plain", "url": "data:font/woff2;base64,AAAA" }])
    );
}

#[test]
fn add_font_trims_and_keeps_the_tellers_id() {
    let mut tl = timeline();
    let id = tl.metadata["tellers.ai"]["timeline_id"].clone();
    assert!(tl.add_font(FontFace::new(
        "  Spaced  ",
        "  https://cdn.example.test/spaced.woff2  "
    )));

    assert_eq!(
        tl.get_fonts(),
        vec![FontFace::new(
            "Spaced",
            "https://cdn.example.test/spaced.woff2"
        )]
    );
    assert_eq!(tl.metadata["tellers.ai"]["timeline_id"], id);
}

#[test]
fn add_font_replaces_the_same_family_weight_and_style() {
    let mut tl = timeline();
    assert!(tl.add_font(brand_sans()));
    assert!(tl.add_font(
        FontFace::new("brand sans", "https://cdn.example.test/brand-sans-v2.woff2")
            .with_weight("400 700")
            .with_style("normal")
    ));

    let fonts = tl.get_fonts();
    assert_eq!(fonts.len(), 1);
    assert_eq!(fonts[0].url, "https://cdn.example.test/brand-sans-v2.woff2");
    // The replacement keeps its own spelling of the family.
    assert_eq!(fonts[0].family, "brand sans");
}

#[test]
fn add_font_keeps_other_weights_of_the_same_family() {
    let mut tl = timeline();
    assert!(tl.add_font(
        FontFace::new("Brand Sans", "https://cdn.example.test/regular.woff2").with_weight("400")
    ));
    assert!(tl.add_font(
        FontFace::new("Brand Sans", "https://cdn.example.test/bold.woff2").with_weight("700")
    ));

    let fonts = tl.get_fonts();
    assert_eq!(fonts.len(), 2);
    assert_eq!(fonts[0].weight.as_deref(), Some("400"));
    assert_eq!(fonts[1].weight.as_deref(), Some("700"));
}

#[test]
fn add_font_rejects_fonts_the_player_cannot_use() {
    let mut tl = timeline();
    assert!(!tl.add_font(FontFace::new(
        "   ",
        "https://cdn.example.test/nameless.woff2"
    )));
    assert!(!tl.add_font(FontFace::new("Ftp", "ftp://cdn.example.test/ftp.woff2")));
    assert!(!tl.add_font(FontFace::new("Relative", "/fonts/relative.woff2")));

    assert!(tl.get_fonts().is_empty());
    assert!(tl.metadata["tellers.ai"].get("availableFonts").is_none());
}

#[test]
fn add_font_drops_an_unusable_weight_or_style() {
    let mut tl = timeline();
    assert!(tl.add_font(
        FontFace::new("Brand Sans", "https://cdn.example.test/brand-sans.woff2")
            .with_weight("extra-bold")
            .with_style("slanted")
    ));

    let fonts = tl.get_fonts();
    assert_eq!(fonts[0].weight, None);
    assert_eq!(fonts[0].style, None);
}

#[test]
fn remove_font_matches_the_family_case_insensitively() {
    let mut tl = timeline();
    assert!(tl.add_font(brand_sans()));
    assert!(tl.add_font(FontFace::new(
        "Other",
        "https://cdn.example.test/other.woff2"
    )));

    assert!(tl.remove_font("BRAND sans"));
    assert_eq!(
        tl.get_fonts(),
        vec![FontFace::new(
            "Other",
            "https://cdn.example.test/other.woff2"
        )]
    );
}

#[test]
fn remove_font_removes_every_weight_of_the_family() {
    let mut tl = timeline();
    assert!(tl.add_font(
        FontFace::new("Brand Sans", "https://cdn.example.test/regular.woff2").with_weight("400")
    ));
    assert!(tl.add_font(
        FontFace::new("Brand Sans", "https://cdn.example.test/bold.woff2").with_weight("700")
    ));

    assert!(tl.remove_font("Brand Sans"));
    assert!(tl.get_fonts().is_empty());
    // The empty array is dropped rather than left behind.
    assert!(tl.metadata["tellers.ai"].get("availableFonts").is_none());
}

#[test]
fn remove_font_reports_an_unknown_family() {
    let mut tl = timeline();
    assert!(!tl.remove_font("Brand Sans"));
    assert!(tl.add_font(brand_sans()));
    assert!(!tl.remove_font("Nothing Like It"));
    assert_eq!(tl.get_fonts(), vec![brand_sans()]);
}

#[test]
fn get_fonts_skips_entries_the_player_would_ignore() {
    let mut tl = timeline();
    tl.metadata["tellers.ai"]["availableFonts"] = serde_json::json!([
        { "family": "Brand Sans", "url": "https://cdn.example.test/brand-sans.woff2" },
        { "family": "", "url": "https://cdn.example.test/nameless.woff2" },
        { "family": "Ftp", "url": "ftp://cdn.example.test/ftp.woff2" },
        { "family": "No Url" },
        { "url": "https://cdn.example.test/no-family.woff2" },
        "not-an-object",
    ]);

    assert_eq!(
        tl.get_fonts(),
        vec![FontFace::new(
            "Brand Sans",
            "https://cdn.example.test/brand-sans.woff2"
        )]
    );
}

#[test]
fn get_fonts_reads_a_numeric_weight_as_a_string() {
    let mut tl = timeline();
    tl.metadata["tellers.ai"]["availableFonts"] = serde_json::json!([
        { "family": "Brand Sans", "url": "https://cdn.example.test/brand-sans.woff2", "weight": 700 },
    ]);

    assert_eq!(tl.get_fonts()[0].weight.as_deref(), Some("700"));
}

#[test]
fn fonts_survive_a_json_round_trip() {
    let mut tl = timeline();
    assert!(tl.add_font(brand_sans()));

    let json = tl.to_json().expect("serializes");
    let parsed: Timeline = serde_json::from_str(&json).expect("parses");
    assert_eq!(parsed.get_fonts(), vec![brand_sans()]);
}

#[test]
fn metadata_helpers_work_on_bare_metadata() {
    // The helpers create the tellers.ai namespace when it is missing, so they
    // also work on metadata that has never been through the timeline reader.
    let mut metadata = serde_json::Value::Null;
    assert!(add_available_font(&mut metadata, brand_sans()));
    assert_eq!(resolve_available_fonts(&metadata), vec![brand_sans()]);
    assert!(remove_available_font(&mut metadata, "Brand Sans"));
    assert!(resolve_available_fonts(&metadata).is_empty());
}
