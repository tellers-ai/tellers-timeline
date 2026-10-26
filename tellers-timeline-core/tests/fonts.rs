// Tests for the timeline custom-font accessors (`Timeline::get_fonts`,
// `add_font`, `remove_font` and the underlying metadata helpers). These read
// and write `metadata["tellers.ai"]["availableFonts"]`, the array the player
// takes as its `availableFonts` option, so the stored shape is part of the
// contract with video-player-js.

use tellers_timeline_core::{
    add_available_font, remove_available_font, resolve_available_fonts, FontFace, FontSource,
    Timeline,
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
    assert_eq!(fonts[0].url(), Some("https://cdn.example.test/brand-sans-v2.woff2"));
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

// Uploaded fonts: an entry stores the font file's `asset_id`, distinct from a
// URL font's `url`; its `url` is resolved for playback and never saved, so
// the player reads both shapes the same way.

fn uploaded() -> FontFace {
    FontFace::from_asset("Brand Sans", "font_asset_1").with_weight("700")
}

#[test]
fn add_asset_font_stores_the_asset_id_not_a_url() {
    let mut tl = timeline();
    assert!(tl.add_font(uploaded()));

    assert_eq!(
        tl.metadata["tellers.ai"]["availableFonts"],
        serde_json::json!([{ "family": "Brand Sans", "asset_id": "font_asset_1", "weight": "700" }])
    );
    let fonts = tl.get_fonts();
    assert_eq!(fonts, vec![uploaded()]);
    assert_eq!(fonts[0].asset_id(), Some("font_asset_1"));
    // Listed before it resolves, with no URL for the player yet.
    assert_eq!(fonts[0].url(), None);
}

#[test]
fn add_asset_font_never_persists_a_resolved_url() {
    let mut tl = timeline();
    let mut font = uploaded();
    font.source = FontSource::Asset {
        asset_id: " font_asset_1 ".to_string(),
        url: Some("https://s3.example.test/presigned".to_string()),
    };
    assert!(tl.add_font(font));

    assert_eq!(tl.get_fonts(), vec![uploaded()]);
    assert!(tl.metadata["tellers.ai"]["availableFonts"][0]
        .get("url")
        .is_none());
}

#[test]
fn add_asset_font_rejects_an_empty_asset_id() {
    let mut tl = timeline();
    assert!(!tl.add_font(FontFace::from_asset("Brand Sans", "  ")));
    assert!(tl.get_fonts().is_empty());
}

#[test]
fn an_upload_replaces_the_url_font_of_the_same_face() {
    let mut tl = timeline();
    assert!(tl.add_font(
        FontFace::new("Brand Sans", "https://cdn.example.test/bold.woff2").with_weight("700")
    ));
    assert!(tl.add_font(uploaded()));
    assert_eq!(tl.get_fonts(), vec![uploaded()]);

    // And back: a URL for the same face replaces the upload.
    let linked = FontFace::new("Brand Sans", "https://cdn.example.test/bold.woff2").with_weight("700");
    assert!(tl.add_font(linked.clone()));
    assert_eq!(tl.get_fonts(), vec![linked]);
}

#[test]
fn font_asset_ids_lists_uploaded_fonts_once_in_order() {
    let mut tl = timeline();
    assert!(tl.add_font(FontFace::from_asset("B", "b")));
    assert!(tl.add_font(FontFace::new("Url", "https://cdn.example.test/url.woff2")));
    assert!(tl.add_font(FontFace::from_asset("A", "a")));
    assert!(tl.add_font(FontFace::from_asset("A", "b").with_style("italic")));

    assert_eq!(tl.font_asset_ids(), vec!["b", "a"]);
}

#[test]
fn set_font_asset_urls_resolves_then_clear_target_urls_strips() {
    let mut tl = timeline();
    assert!(tl.add_font(uploaded()));
    assert!(tl.add_font(FontFace::new("Url", "https://cdn.example.test/url.woff2")));
    assert!(tl.add_font(FontFace::from_asset("Unresolved", "other")));

    let urls = std::collections::HashMap::from([
        ("font_asset_1".to_string(), " https://s3.example.test/presigned ".to_string()),
        ("unknown".to_string(), "https://s3.example.test/unknown".to_string()),
    ]);
    assert_eq!(tl.set_font_asset_urls(&urls), 1);

    let fonts = tl.get_fonts();
    assert_eq!(fonts[0].url(), Some("https://s3.example.test/presigned"));
    assert_eq!(fonts[0].asset_id(), Some("font_asset_1"));
    assert_eq!(fonts[1].url(), Some("https://cdn.example.test/url.woff2"));
    assert_eq!(fonts[2].url(), None);
    assert_eq!(
        tl.metadata["tellers.ai"]["availableFonts"][0]["url"],
        "https://s3.example.test/presigned"
    );

    tl.clear_target_urls();
    assert_eq!(tl.get_fonts()[0], uploaded());
    assert!(tl.metadata["tellers.ai"]["availableFonts"][0]
        .get("url")
        .is_none());
    // A linked font keeps its own url.
    assert_eq!(tl.get_fonts()[1].url(), Some("https://cdn.example.test/url.woff2"));
    assert_eq!(tl.clear_font_asset_urls(), 0);
}

#[test]
fn set_font_asset_url_ignores_unusable_urls() {
    let mut tl = timeline();
    assert!(tl.add_font(uploaded()));
    assert_eq!(tl.set_font_asset_url("font_asset_1", ""), 0);
    assert_eq!(tl.set_font_asset_url("font_asset_1", "ftp://nope"), 0);
    assert_eq!(tl.set_font_asset_url(" font_asset_1 ", "https://ok.test/f.woff2"), 1);
    assert_eq!(tl.get_fonts()[0].url(), Some("https://ok.test/f.woff2"));
}

#[test]
fn get_fonts_reads_asset_entries() {
    let mut tl = timeline();
    tl.metadata["tellers.ai"]["availableFonts"] = serde_json::json!([
        { "family": "Up", "asset_id": "up", "url": "https://s3.example.test/up" },
        { "family": "Bad Url", "asset_id": "bad", "url": "ftp://nope" },
        { "family": "Unresolved", "asset_id": "later" },
        { "family": "Empty Id", "asset_id": "  " },
    ]);

    let fonts = tl.get_fonts();
    assert_eq!(fonts.len(), 3);
    // A resolved upload reads its url, keeping its asset id.
    assert_eq!(fonts[0].asset_id(), Some("up"));
    assert_eq!(fonts[0].url(), Some("https://s3.example.test/up"));
    // An unusable resolved URL is dropped, the font kept.
    assert_eq!(fonts[1].asset_id(), Some("bad"));
    assert_eq!(fonts[1].url(), None);
    assert_eq!(fonts[2].asset_id(), Some("later"));
    assert_eq!(fonts[2].url(), None);
}

#[test]
fn asset_fonts_survive_a_json_round_trip() {
    let mut tl = timeline();
    assert!(tl.add_font(uploaded()));
    let json = tl.to_json().expect("serializes");
    let parsed: Timeline = serde_json::from_str(&json).expect("parses");
    assert_eq!(parsed.get_fonts(), vec![uploaded()]);
}

#[test]
fn font_face_serde_uses_the_stored_shape() {
    assert_eq!(
        serde_json::to_value(uploaded()).unwrap(),
        serde_json::json!({ "family": "Brand Sans", "asset_id": "font_asset_1", "weight": "700" })
    );
    assert_eq!(
        serde_json::to_value(brand_sans()).unwrap(),
        serde_json::json!({
            "family": "Brand Sans",
            "url": "https://cdn.example.test/brand-sans.woff2",
            "weight": "400 700",
            "style": "normal",
        })
    );
    let parsed: FontFace = serde_json::from_value(serde_json::json!({ "family": "X", "asset_id": "x" })).unwrap();
    assert_eq!(parsed, FontFace::from_asset("X", "x"));
    assert!(serde_json::from_value::<FontFace>(serde_json::json!({ "family": "X" })).is_err());
}
