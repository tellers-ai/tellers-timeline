// Tests for the timeline text-style accessors (`Timeline::get_text_styles`,
// `get_text_style`, `set_text_style`, `remove_text_style`, `text_styles_css`
// and the underlying metadata helpers). These read and write
// `metadata["tellers.ai"]["textStyles"]`, the class-name → declarations
// object the player turns into `.name { ... }` rules for rich-text clips, so
// the stored shape is part of the contract with video-player-js.

use tellers_timeline_core::{
    is_valid_text_style_name, merge_text_styles, remove_text_style, resolve_text_styles,
    set_text_style, text_styles_css, IdMetadataExt, TextStyle, Timeline, Track, TrackKind,
};

fn timeline() -> Timeline {
    Timeline::default()
}

fn subtitle() -> TextStyle {
    TextStyle::new(
        "subtitle",
        "font-family:'Brand Sans';font-size:56px;color:#fff",
    )
}

#[test]
fn timeline_without_styles_has_none() {
    assert!(timeline().get_text_styles().is_empty());
    assert_eq!(timeline().get_text_style("subtitle"), None);
    assert_eq!(timeline().text_styles_css(), "");
}

#[test]
fn set_text_style_then_get_round_trips() {
    let mut tl = timeline();
    assert!(tl.set_text_style(
        "subtitle",
        "font-family:'Brand Sans';font-size:56px;color:#fff"
    ));

    assert_eq!(tl.get_text_styles(), vec![subtitle()]);
    assert_eq!(
        tl.get_text_style("subtitle").as_deref(),
        Some("font-family:'Brand Sans';font-size:56px;color:#fff")
    );
    assert_eq!(
        tl.metadata["tellers.ai"]["textStyles"],
        serde_json::json!({ "subtitle": "font-family:'Brand Sans';font-size:56px;color:#fff" })
    );
}

#[test]
fn set_text_style_trims_and_keeps_the_tellers_id() {
    let mut tl = timeline();
    let id = tl.metadata["tellers.ai"]["timeline_id"].clone();
    assert!(tl.set_text_style("  subtitle  ", "  color:#fff;  "));

    assert_eq!(
        tl.get_text_styles(),
        vec![TextStyle::new("subtitle", "color:#fff;")]
    );
    assert_eq!(tl.metadata["tellers.ai"]["timeline_id"], id);
}

#[test]
fn set_text_style_replaces_the_same_name() {
    let mut tl = timeline();
    assert!(tl.set_text_style("subtitle", "color:#fff"));
    assert!(tl.set_text_style("subtitle", "color:#ffd400"));

    assert_eq!(
        tl.get_text_styles(),
        vec![TextStyle::new("subtitle", "color:#ffd400")]
    );
}

#[test]
fn text_style_names_are_case_sensitive() {
    let mut tl = timeline();
    assert!(tl.set_text_style("Subtitle", "color:#fff"));
    assert!(tl.set_text_style("subtitle", "color:#000"));

    assert_eq!(tl.get_text_styles().len(), 2);
    assert_eq!(tl.get_text_style("Subtitle").as_deref(), Some("color:#fff"));
    assert_eq!(tl.get_text_style("subtitle").as_deref(), Some("color:#000"));
}

#[test]
fn set_text_style_rejects_unusable_names() {
    let mut tl = timeline();
    for name in [
        "",
        "   ",
        "1st",
        "-1",
        "sub title",
        "sub.title",
        "sub{title}",
        ".subtitle",
        "sub>title",
        "é",
    ] {
        assert!(!tl.set_text_style(name, "color:#fff"), "accepted {name:?}");
        assert!(!is_valid_text_style_name(name), "valid {name:?}");
    }
    for name in [
        "subtitle",
        "Subtitle",
        "sub-title",
        "sub_title",
        "_sub",
        "-sub",
        "--sub",
        "s1",
    ] {
        assert!(is_valid_text_style_name(name), "invalid {name:?}");
    }
    assert!(tl.get_text_styles().is_empty());
    assert!(tl.metadata["tellers.ai"].get("textStyles").is_none());
}

#[test]
fn set_text_style_rejects_unusable_declarations() {
    let mut tl = timeline();
    for declarations in [
        "",
        "   ",
        "color:#fff } .other { color:#000",
        "{color:#fff}",
    ] {
        assert!(
            !tl.set_text_style("subtitle", declarations),
            "accepted {declarations:?}"
        );
    }
    assert!(tl.get_text_styles().is_empty());
}

#[test]
fn remove_text_style_drops_the_object_once_empty() {
    let mut tl = timeline();
    assert!(tl.set_text_style("subtitle", "color:#fff"));
    assert!(tl.set_text_style("accent", "color:#ffd400"));

    assert!(tl.remove_text_style("accent"));
    assert!(!tl.remove_text_style("accent"));
    assert_eq!(
        tl.get_text_styles(),
        vec![TextStyle::new("subtitle", "color:#fff")]
    );
    assert!(tl.metadata["tellers.ai"]["textStyles"].is_object());

    assert!(tl.remove_text_style(" subtitle "));
    assert!(tl.metadata["tellers.ai"].get("textStyles").is_none());
    assert!(tl.metadata["tellers.ai"].is_object());
}

#[test]
fn remove_text_style_without_styles_is_false() {
    assert!(!timeline().remove_text_style("subtitle"));
    let mut bare = serde_json::json!({});
    assert!(!remove_text_style(&mut bare, "subtitle"));
}

#[test]
fn text_styles_css_builds_one_rule_per_style() {
    let mut tl = timeline();
    assert!(tl.set_text_style("subtitle", "font-size:56px;color:#fff"));
    assert!(tl.set_text_style("accent", "color:#ffd400"));

    assert_eq!(
        tl.text_styles_css(),
        ".accent { color:#ffd400 }\n.subtitle { font-size:56px;color:#fff }"
    );
    assert_eq!(
        subtitle().to_css_rule(),
        ".subtitle { font-family:'Brand Sans';font-size:56px;color:#fff }"
    );
    assert_eq!(text_styles_css(&[]), "");
}

#[test]
fn resolve_text_styles_skips_entries_the_player_would_drop() {
    let metadata = serde_json::json!({
        "tellers.ai": {
            "textStyles": {
                "subtitle": " color:#fff ",
                "bad name": "color:#000",
                "1st": "color:#000",
                "empty": "   ",
                "braces": "color:#fff } body { display:none",
                "number": 12,
                "object": { "color": "#fff" },
                "null": null
            }
        }
    });

    assert_eq!(
        resolve_text_styles(&metadata),
        vec![TextStyle::new("subtitle", "color:#fff")]
    );
}

#[test]
fn resolve_text_styles_tolerates_wrong_shapes() {
    assert!(resolve_text_styles(&serde_json::json!(null)).is_empty());
    assert!(resolve_text_styles(&serde_json::json!({})).is_empty());
    assert!(resolve_text_styles(&serde_json::json!({ "tellers.ai": "nope" })).is_empty());
    assert!(
        resolve_text_styles(&serde_json::json!({ "tellers.ai": { "textStyles": [] } })).is_empty()
    );
    assert!(
        resolve_text_styles(&serde_json::json!({ "tellers.ai": { "textStyles": "css" } }))
            .is_empty()
    );
}

#[test]
fn set_text_style_on_bare_metadata_creates_the_namespace() {
    let mut metadata = serde_json::json!(null);
    assert!(set_text_style(&mut metadata, subtitle()));
    assert_eq!(
        metadata,
        serde_json::json!({ "tellers.ai": { "textStyles": {
            "subtitle": "font-family:'Brand Sans';font-size:56px;color:#fff"
        } } })
    );

    let mut wrong_type = serde_json::json!({ "tellers.ai": { "textStyles": "css" } });
    assert!(set_text_style(
        &mut wrong_type,
        TextStyle::new("a", "color:#fff")
    ));
    assert_eq!(
        wrong_type["tellers.ai"]["textStyles"],
        serde_json::json!({ "a": "color:#fff" })
    );
}

#[test]
fn text_styles_survive_timeline_serialization() {
    let mut tl = timeline();
    assert!(tl.set_text_style("subtitle", "color:#fff"));

    let json = serde_json::to_string(&tl).expect("serialize");
    let restored: Timeline = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(
        restored.get_text_styles(),
        vec![TextStyle::new("subtitle", "color:#fff")]
    );
}

#[test]
fn text_style_normalized_and_is_valid() {
    assert!(subtitle().is_valid());
    assert_eq!(
        TextStyle::new(" a ", " color:#fff ").normalized(),
        Some(TextStyle::new("a", "color:#fff"))
    );
    assert!(!TextStyle::new("a b", "color:#fff").is_valid());
    assert!(!TextStyle::new("a", "").is_valid());
}

// --- Track-level styles, merged over the timeline's by class name ---------

fn timeline_with_tracks() -> Timeline {
    let mut tl = timeline();
    tl.tracks
        .add_track(Track::new(TrackKind::Video, Some("subs".to_string())));
    tl.tracks
        .add_track(Track::new(TrackKind::Video, Some("titles".to_string())));
    tl
}

#[test]
fn merge_text_styles_overrides_in_place_and_appends_new_classes() {
    let base = vec![
        TextStyle::new("subtitle", "font-size:56px;color:#fff"),
        TextStyle::new("accent", "color:#ffd400"),
    ];
    let overrides = vec![
        TextStyle::new("new", "font-weight:700"),
        TextStyle::new("subtitle", "font-size:40px"),
    ];

    assert_eq!(
        merge_text_styles(&base, &overrides),
        vec![
            TextStyle::new("subtitle", "font-size:40px"),
            TextStyle::new("accent", "color:#ffd400"),
            TextStyle::new("new", "font-weight:700"),
        ]
    );
    assert_eq!(merge_text_styles(&base, &[]), base);
    assert_eq!(merge_text_styles(&[], &overrides), overrides);
}

#[test]
fn track_text_style_is_stored_on_the_track_metadata() {
    let mut tl = timeline_with_tracks();
    assert!(tl.set_track_text_style("subs", "subtitle", "font-size:40px"));

    let (_, track) = tl.tracks.get_track_by_id("subs").unwrap();
    assert_eq!(
        track.metadata["tellers.ai"]["textStyles"],
        serde_json::json!({ "subtitle": "font-size:40px" })
    );
    assert_eq!(
        track.get_text_style("subtitle").as_deref(),
        Some("font-size:40px")
    );
    assert!(tl.get_text_styles().is_empty(), "timeline styles untouched");
    assert_eq!(
        tl.tracks
            .get_track_by_id("subs")
            .unwrap()
            .1
            .get_id()
            .as_deref(),
        Some("subs"),
        "track id kept"
    );
}

#[test]
fn get_track_text_styles_merges_track_over_timeline() {
    let mut tl = timeline_with_tracks();
    assert!(tl.set_text_style("subtitle", "font-size:56px;color:#fff"));
    assert!(tl.set_text_style("accent", "color:#ffd400"));
    assert!(tl.set_track_text_style("subs", "subtitle", "font-size:40px;color:#fff"));
    assert!(tl.set_track_text_style("subs", "speaker", "font-style:italic"));

    assert_eq!(
        tl.get_track_text_styles("subs"),
        // Metadata objects keep their keys sorted, so the timeline classes come
        // first alphabetically, then the track-only ones.
        Some(vec![
            TextStyle::new("accent", "color:#ffd400"),
            TextStyle::new("subtitle", "font-size:40px;color:#fff"),
            TextStyle::new("speaker", "font-style:italic"),
        ])
    );
    assert_eq!(
        tl.get_track_text_styles("titles"),
        Some(tl.get_text_styles()),
        "a track without styles gets the timeline's"
    );
    assert_eq!(tl.get_track_text_styles("missing"), None);
    assert_eq!(
        tl.track_text_styles_css("subs").as_deref(),
        Some(".accent { color:#ffd400 }\n.subtitle { font-size:40px;color:#fff }\n.speaker { font-style:italic }")
    );
}

#[test]
fn removing_a_track_override_restores_the_timeline_class() {
    let mut tl = timeline_with_tracks();
    assert!(tl.set_text_style("subtitle", "font-size:56px"));
    assert!(tl.set_track_text_style("subs", "subtitle", "font-size:40px"));

    assert!(tl.remove_track_text_style("subs", "subtitle"));
    assert!(!tl.remove_track_text_style("subs", "subtitle"));
    assert_eq!(
        tl.get_track_text_styles("subs"),
        Some(vec![TextStyle::new("subtitle", "font-size:56px")])
    );
    let (_, track) = tl.tracks.get_track_by_id("subs").unwrap();
    assert!(track.metadata["tellers.ai"].get("textStyles").is_none());
}

#[test]
fn track_text_style_writes_fail_for_unknown_tracks_or_bad_styles() {
    let mut tl = timeline_with_tracks();
    assert!(!tl.set_track_text_style("missing", "subtitle", "color:#fff"));
    assert!(!tl.remove_track_text_style("missing", "subtitle"));
    assert!(!tl.set_track_text_style("subs", "bad name", "color:#fff"));
    assert!(!tl.set_track_text_style("subs", "subtitle", "color:#fff } x {"));
    assert_eq!(tl.get_track_text_styles("subs"), Some(vec![]));
}

#[test]
fn track_text_styles_survive_timeline_serialization() {
    let mut tl = timeline_with_tracks();
    assert!(tl.set_text_style("subtitle", "font-size:56px"));
    assert!(tl.set_track_text_style("subs", "subtitle", "font-size:40px"));

    let json = serde_json::to_string(&tl).expect("serialize");
    let restored: Timeline = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(
        restored.get_track_text_styles("subs"),
        Some(vec![TextStyle::new("subtitle", "font-size:40px")])
    );
    assert_eq!(
        restored.get_track_text_styles("titles"),
        Some(vec![TextStyle::new("subtitle", "font-size:56px")])
    );
}
