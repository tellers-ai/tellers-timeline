//! Generates `docs/otio-reference.md`, the reference of the tellers OTIO
//! timeline format, from the core types' JSON schema and the tellers effect
//! catalog ([`tellers_timeline_core::EFFECT_SPECS`]).
//!
//! The schema gives the objects, their fields and types. Their descriptions
//! live here, in [`TYPE_DOCS`] and [`FIELD_DOCS`]: the tests fail when a field
//! of the schema has none, and when the checked-in reference is stale. Run
//! `just regen-docs` after changing a type or the effect catalog.
//!
//! `docs/reading-otio.md` is the hand-written guide to reading a timeline;
//! the reference links to it.

use std::fmt::Write as _;

use serde_json::Value;
use tellers_timeline_core::{
    Ease, EffectTarget, ParameterKind, ATTACH_PARAMETER, DETECT_MASK_PARAMETERS, DETECT_PARAMETER,
    EFFECT_SPECS, EQUALIZER_BAND_TYPES, FOLLOW_PARAMETER, FOLLOW_GROUP_TARGETS, MOVED_DETECTION_SETTINGS,
    COCO_CATEGORIES,
    TELLERS_EFFECT_PREFIX,
};

/// Path of the generated reference, relative to the repository root.
pub const REFERENCE_PATH: &str = "docs/otio-reference.md";

/// The JSON schema of a timeline.
pub fn timeline_schema() -> Value {
    serde_json::to_value(schemars::schema_for!(tellers_timeline_core::types::Timeline))
        .expect("the timeline schema serializes")
}

/// The object types of the reference, in reading order, with their
/// description. `Timeline` is the schema root; the others are definitions.
pub const TYPE_DOCS: &[(&str, &str)] = &[
    (
        "Timeline",
        "The document. Its `tracks` stack holds every track; timeline-wide settings \
         (fonts) live in its `metadata[\"tellers.ai\"]`.",
    ),
    (
        "Stack",
        "The tracks of the timeline, composited together. Tracks render in list order: the \
         first is the bottom layer, the last the top one (painter's order). Of its `effects`, \
         the player applies `ColorLut`s to the whole composited image and uses a `MotionBlur` \
         as every clip's default; it ignores the others there. The player also reads \
         `enabled: false` on this stack (nothing plays), a field this library drops on load.",
    ),
    (
        "Track",
        "A row of items played one after the other: each item starts where the previous one \
         ends, so a track's timing is the sum of its items' durations. Video tracks carry \
         picture only and audio tracks sound only.",
    ),
    (
        "TrackKind",
        "What a track carries. Read case-insensitively (`Video` and `video` both parse); \
         written lowercase.",
    ),
    (
        "Item",
        "An entry of a track: a clip, a gap of empty time, a composite clip, or a \
         transition between two neighbours.",
    ),
    (
        "Clip",
        "A piece of media (or generated content) placed on a track for the duration of its \
         `source_range`.",
    ),
    (
        "Gap",
        "Empty time on a track: nothing is drawn or heard for its duration.",
    ),
    (
        "CompositeStack",
        "A composite clip: a `Stack.1` nested inside a video track, drawn as one clip made \
         of its own tracks. It lasts `source_range.duration` when a source range is set, \
         otherwise as long as its longest child track. The player draws one level of \
         nesting: a `Stack.1` inside a composite's track is skipped.",
    ),
    (
        "Transition",
        "A transition between the two items around it on a track. It takes no time of its \
         own: it overlaps the end of the item before it by `in_offset` and the start of the \
         item after it by `out_offset`.",
    ),
    (
        "MediaReference",
        "What a clip plays, chosen by `OTIO_SCHEMA`. `MissingReference.1` is also accepted \
         and read as an external reference with an empty `target_url` (and written back as \
         `ExternalReference.1`); the player draws it as a gap.",
    ),
    (
        "GeneratorParameters",
        "The parameters of a generator reference: a Resolve `Rich Text` block for \
         `generator_kind: \"Rich\"`, or the player's own keys for `Text` and `Shape` \
         generators (see the reading guide). Other keys are kept as they are.",
    ),
    (
        "Effect",
        "An effect on a clip, a track or the stack. Tellers effects are named \
         `tellers.ai.<Effect>` and keep their parameters in `metadata[\"tellers.ai\"]\
         .parameters` (see [Tellers effects](#tellers-effects)); Resolve transforms and \
         crop are `Resolve Effect`s with a `Resolve_OTIO` block, as is the legacy clip volume \
         (read, never written: the volume is `tellers.ai.Volume`). Other OTIO keys (e.g. \
         `time_scalar`) are kept as they are.",
    ),
    (
        "EffectMetadata",
        "The metadata of an effect. Keys other than `Resolve_OTIO` (notably `tellers.ai`) \
         are kept as they are.",
    ),
    (
        "Marker",
        "A note on a point or range of the edit. `marked_range` is in the time of the object \
         holding it: timeline time on the stack and tracks, source (media) time on a clip.",
    ),
    (
        "TimeRange",
        "A span of time: `start_time` and `duration`.",
    ),
    (
        "RationalTime",
        "A time as `value / rate` seconds (e.g. `{ value: 48, rate: 24 }` is 2 s).",
    ),
    (
        "ResolveOTIOEffect",
        "A DaVinci Resolve effect, as Resolve's OTIO export writes it: in an `Effect`'s \
         `metadata.Resolve_OTIO`, or in a generator's `parameters.Resolve_OTIO` list.",
    ),
    (
        "ResolveOTIOParameter",
        "A parameter of a Resolve effect, typed by its `Variant Type`.",
    ),
    (
        "ResolveOTIOParameterNumber_for_double",
        "A `Double` Resolve parameter.",
    ),
    (
        "ResolveOTIOParameterNumber_for_int64",
        "An `Int` Resolve parameter.",
    ),
    (
        "ResolveOTIOParameterNumber_for_uint64",
        "A `UInt` Resolve parameter.",
    ),
    (
        "ResolveOTIOParameterSimple_for_Boolean",
        "A `Bool` Resolve parameter.",
    ),
    (
        "ResolveOTIOParameterSimple_for_String",
        "A `String` or `Color` Resolve parameter.",
    ),
    (
        "ResolveOTIOParameterPointF",
        "A `POINTF` Resolve parameter: an `[x, y]` point.",
    ),
    (
        "ResolveOTIOParameterUnknown",
        "Any other Resolve parameter, kept as it is (e.g. the `title blob` holding a Rich \
         Text clip's `Title HTML`).",
    ),
];

/// `(type, field, description)`. A type ending in `*` matches every type
/// with that prefix.
pub const FIELD_DOCS: &[(&str, &str, &str)] = &[
    ("Timeline", "OTIO_SCHEMA", "Always `Timeline.1`."),
    ("Timeline", "name", "Display name."),
    ("Timeline", "tracks", "The stack of every track."),
    (
        "Timeline",
        "metadata",
        "Free JSON. `tellers.ai.availableFonts` lists the fonts text clips may use, each \
         `{family, url}` (linked) or `{family, asset_id}` (uploaded; see \
         [Asset references](#asset-references)), with optional `weight` and `style`. The player also \
         reads `tellers.ai.groups[<group_id>].parameters` (a keyframable transform of a clip \
         group: `x`, `y`, `scale`, `rotation`, `opacity`, …, edited with the clip-group API) \
         and the top-level `tellers_aspect_ratio` (`16_9`, `9_16`, …), which this library \
         keeps as plain metadata, with no typed API.",
    ),
    ("Stack", "OTIO_SCHEMA", "Always `Stack.1`."),
    ("Stack", "name", "Display name."),
    ("Stack", "children", "The tracks, bottom layer first."),
    (
        "Stack",
        "effects",
        "The timeline stage: `ColorLut`s grade the whole composited image and a `MotionBlur` \
         is every clip's default; the player applies no other effect here.",
    ),
    ("Stack", "markers", "Timeline markers, in timeline time."),
    ("Stack", "metadata", "Free JSON."),
    ("Track", "OTIO_SCHEMA", "Always `Track.1`."),
    ("Track", "name", "Display name, e.g. `V1` or `A2`."),
    ("Track", "kind", "`video`, `audio` or `other`."),
    ("Track", "children", "The items, in play order."),
    ("Track", "effects", "Effects applied to each clip of the track (the track stage)."),
    ("Track", "enabled", "A disabled track is neither drawn nor heard."),
    ("Track", "markers", "Track markers, in timeline time."),
    (
        "Track",
        "metadata",
        "Free JSON. `tellers.ai.timeline_id` is the track id; `tellers.ai.created_by: \"auto\"` \
         marks tracks the library created.",
    ),
    ("Clip", "OTIO_SCHEMA", "`Clip.2` (`Clip.1` is read too)."),
    ("Clip", "name", "Display name."),
    (
        "Clip",
        "source_range",
        "The part of the media played: `start_time` in the media's own time, and `duration`, \
         which is also how long the clip lasts on the track.",
    ),
    ("Clip", "media_references", "The clip's media, by key; usually a single `DEFAULT_MEDIA`."),
    (
        "Clip",
        "active_media_reference_key",
        "Which `media_references` entry plays; `DEFAULT_MEDIA` when unset.",
    ),
    ("Clip", "effects", "Effects applied to this clip only (the clip stage), in order."),
    ("Clip", "enabled", "A disabled clip is neither drawn nor heard; it still takes its time."),
    ("Clip", "markers", "Clip markers, in source (media) time."),
    (
        "Clip",
        "metadata",
        "Free JSON. `tellers.ai.timeline_id` is the item id, `tellers.ai.group_id` names the \
         player group whose transform applies (see the timeline's `tellers.ai.groups`), \
         `tellers.ai[\"Tellers Group ID\"]` is this library's edit group, and the Resolve \
         `Link Group ID` ties a video to its audio (sync partners).",
    ),
    ("Gap", "OTIO_SCHEMA", "`Gap.1`."),
    ("Gap", "name", "Display name."),
    ("Gap", "source_range", "Only `duration` matters: how long the gap lasts."),
    ("Gap", "effects", "Kept for OTIO round trips; nothing applies them."),
    ("Gap", "markers", "Kept for OTIO round trips; the API places no markers on gaps."),
    ("Gap", "metadata", "Free JSON. `tellers.ai.timeline_id` is the item id."),
    ("CompositeStack", "OTIO_SCHEMA", "`Stack.1`."),
    ("CompositeStack", "name", "Display name."),
    ("CompositeStack", "children", "Its own tracks, bottom layer first, timed like the timeline's."),
    (
        "CompositeStack",
        "source_range",
        "Optional. `start_time` is where in its child tracks it starts playing, and \
         `duration` how long it lasts on the outer track.",
    ),
    ("CompositeStack", "effects", "Effects applied to the composite as a whole, like a clip's."),
    ("CompositeStack", "enabled", "A disabled composite is neither drawn nor heard; it still takes its time."),
    ("CompositeStack", "markers", "Markers, kept for OTIO round trips."),
    ("CompositeStack", "metadata", "Free JSON. `tellers.ai.timeline_id` is the item id."),
    ("Transition", "OTIO_SCHEMA", "`Transition.1`."),
    ("Transition", "name", "Display name."),
    (
        "Transition",
        "transition_type",
        "OTIO's `SMPTE_Dissolve` (a dissolve) or `Custom_Transition`. The player also reads \
         `Dissolve`, `CrossDissolve`, `DipToBlack`, `DipToWhite`, `DipToColor`, `Wipe`, `Push`, \
         `Slide` and `Zoom`, compared without case or punctuation, and dissolves on anything \
         else. `Transition::kind` / `set_kind` read and write it as a `TransitionKind`. Sound \
         fades over every transition (a crossfade between two clips, a fade from or to silence \
         next to a gap): `tellers.ai.audio-constant-power` (equal power) and \
         `tellers.ai.audio-constant-gain` (linear) pick the curve, read and written by \
         `Transition::audio_kind` / `set_audio_kind`; any other type fades at constant power \
         between two clips and at constant gain from or to silence. Pictures draw an audio \
         type as a dissolve.",
    ),
    ("Transition", "in_offset", "How far it reaches back into the item before it."),
    ("Transition", "out_offset", "How far it reaches into the item after it."),
    (
        "Transition",
        "metadata",
        "Free JSON. `tellers.ai.parameters` holds the player's `color` (dips, `#000000` by \
         default; `DipToBlack` / `DipToWhite` override it) and `direction` (`left`, `right`, \
         `up`, `down`; `left` by default), read and written by `Transition::color` and \
         `Transition::direction`.",
    ),
    ("MediaReference", "OTIO_SCHEMA", "Picks the kind of reference."),
    ("MediaReference", "name", "Display name."),
    (
        "MediaReference",
        "target_url",
        "URL of the media file. Resolved from the asset before playback and stripped before \
         saving, so a stored timeline may have it empty.",
    ),
    (
        "MediaReference",
        "available_range",
        "The time range the media file covers, in its own time; `source_range` stays inside it.",
    ),
    ("MediaReference", "available_image_bounds", "OTIO image bounds; unused by the player."),
    (
        "MediaReference",
        "generator_kind",
        "`Rich` (Resolve Rich Text: HTML), `Text` (plain text), `Shape` (rectangle, ellipse, \
         polygon or star, line, path) or another generator.",
    ),
    ("MediaReference", "parameters", "The generator's parameters; see GeneratorParameters."),
    (
        "MediaReference",
        "metadata",
        "Free JSON. `tellers.ai` holds asset details such as the asset id.",
    ),
    (
        "GeneratorParameters",
        "Resolve_OTIO",
        "Resolve effects of the generator: for Rich Text, a `Rich Text` effect with the \
         `title blob` HTML, `position`, zoom and rotation.",
    ),
    ("Effect", "OTIO_SCHEMA", "`Effect.1`; OTIO's `LinearTimeWarp.1` and `FreezeFrame.1` are read too."),
    ("Effect", "name", "Display label."),
    (
        "Effect",
        "effect_name",
        "Which effect: `tellers.ai.<Effect>` for tellers effects, `Resolve Effect` for Resolve \
         ones.",
    ),
    ("Effect", "enabled", "`false` turns the effect off without removing it; absent means enabled."),
    ("Effect", "metadata", "Where the effect's settings live; see EffectMetadata."),
    (
        "EffectMetadata",
        "Resolve_OTIO",
        "The Resolve effect of a `Resolve Effect` (Transform, Cropping, legacy volume).",
    ),
    ("Marker", "OTIO_SCHEMA", "`Marker.2`."),
    ("Marker", "name", "Title of the note."),
    ("Marker", "comment", "Body of the note."),
    ("Marker", "color", "An OTIO marker color (`RED`, `GREEN`, `BLUE`, ...)."),
    ("Marker", "marked_range", "The flagged point (zero duration) or range."),
    (
        "Marker",
        "metadata",
        "`tellers.ai.timeline_id` is the marker id, `tellers.ai.created_by` its author.",
    ),
    ("TimeRange", "OTIO_SCHEMA", "`TimeRange.1`."),
    ("TimeRange", "start_time", "Where the range starts."),
    ("TimeRange", "duration", "How long it lasts."),
    ("RationalTime", "OTIO_SCHEMA", "`RationalTime.1`."),
    ("RationalTime", "value", "Number of `1 / rate` units."),
    ("RationalTime", "rate", "Units per second, e.g. a frame rate (`1` for plain seconds)."),
    ("ResolveOTIOEffect", "Effect Name", "Resolve's name for the effect, e.g. `Transform` or `Rich Text`."),
    ("ResolveOTIOEffect", "Name", "Display name."),
    ("ResolveOTIOEffect", "Enabled", "Whether Resolve applies it."),
    ("ResolveOTIOEffect", "Parameters", "The effect's parameters."),
    ("ResolveOTIOEffect", "Type", "Resolve's numeric effect type (e.g. `2` Transform, `24` Rich Text)."),
    ("ResolveOTIOParameter*", "Variant Type", "The parameter's type: `Double`, `Int`, `Bool`, `POINTF`, ..."),
    (
        "ResolveOTIOParameter*",
        "Parameter ID",
        "Which parameter, e.g. `transformationPan`, `transformationZoomX`, `position`.",
    ),
    ("ResolveOTIOParameter*", "Parameter Value", "Its value."),
    ("ResolveOTIOParameter*", "Default Parameter Value", "Resolve's default."),
    ("ResolveOTIOParameter*", "minValue", "Lower bound in Resolve's UI."),
    ("ResolveOTIOParameter*", "maxValue", "Upper bound in Resolve's UI."),
    ("ResolveOTIOParameter*", "Key Frames", "Resolve keyframes, kept as they are."),
    ("ResolveOTIOParameter*", "Title HTML", "The HTML of a Rich Text clip (`title blob` parameter)."),
];

fn field_doc(type_name: &str, field: &str) -> Option<&'static str> {
    let matches = |pattern: &str| match pattern.strip_suffix('*') {
        Some(prefix) => type_name.starts_with(prefix),
        None => pattern == type_name,
    };
    FIELD_DOCS
        .iter()
        .find(|(ty, name, _)| *name == field && matches(ty))
        .map(|(_, _, doc)| *doc)
}

fn anchor(type_name: &str) -> String {
    type_name.to_lowercase()
}

fn ref_name(value: &Value) -> Option<&str> {
    value.get("$ref")?.as_str()?.rsplit('/').next()
}

/// The type of a schema value, as Markdown.
fn type_string(value: &Value) -> String {
    if let Some(name) = ref_name(value) {
        return format!("[{name}](#{})", anchor(name));
    }
    if let Some(all) = value.get("allOf").and_then(Value::as_array) {
        if let [only] = all.as_slice() {
            return type_string(only);
        }
    }
    if let Some(any) = value
        .get("anyOf")
        .or_else(|| value.get("oneOf"))
        .and_then(Value::as_array)
    {
        return any.iter().map(type_string).collect::<Vec<_>>().join(" \\| ");
    }
    if let Some(options) = value.get("enum").and_then(Value::as_array) {
        return options
            .iter()
            .map(|o| format!("`{o}`"))
            .collect::<Vec<_>>()
            .join(" \\| ");
    }
    match value.get("type") {
        Some(Value::Array(types)) => types
            .iter()
            .map(|t| type_string(&serde_json::json!({ "type": t, "items": value.get("items") })))
            .collect::<Vec<_>>()
            .join(" \\| "),
        Some(Value::String(t)) if t == "array" => match value.get("items") {
            Some(items) if !items.is_null() => format!("array of {}", type_string(items)),
            _ => "array".to_string(),
        },
        Some(Value::String(t)) if t == "object" => match value.get("additionalProperties") {
            Some(inner) if inner.is_object() => format!("map of string → {}", type_string(inner)),
            _ => "object".to_string(),
        },
        Some(Value::String(t)) => match t.as_str() {
            "number" | "integer" => "number".to_string(),
            other => other.to_string(),
        },
        _ => "any JSON".to_string(),
    }
}

/// A scalar default, as Markdown; objects and arrays are left out (some,
/// like a marker's, embed a fresh id).
fn default_string(value: &Value) -> Option<String> {
    match value.get("default")? {
        Value::Null => None,
        scalar @ (Value::String(_) | Value::Number(_) | Value::Bool(_)) => Some(format!("`{scalar}`")),
        _ => None,
    }
}

fn escape_cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

fn field_table(out: &mut String, type_name: &str, object: &Value) {
    let Some(properties) = object.get("properties").and_then(Value::as_object) else {
        return;
    };
    let required: Vec<&str> = object
        .get("required")
        .and_then(Value::as_array)
        .map(|r| r.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    // OTIO_SCHEMA first, then the schema's (alphabetical) order.
    let mut fields: Vec<(&String, &Value)> = properties.iter().collect();
    fields.sort_by_key(|(name, _)| (name.as_str() != "OTIO_SCHEMA", name.to_string()));
    out.push_str("| Field | Type | Required | Default | Description |\n");
    out.push_str("|---|---|---|---|---|\n");
    for (name, value) in fields {
        let doc = field_doc(type_name, name).unwrap_or("");
        let _ = writeln!(
            out,
            "| `{name}` | {} | {} | {} | {} |",
            type_string(value),
            if required.contains(&name.as_str()) { "yes" } else { "" },
            default_string(value).unwrap_or_default(),
            escape_cell(doc),
        );
    }
    out.push('\n');
}

/// The schema object of `type_name`: the root for `Timeline`, else its
/// definition.
fn type_schema<'a>(schema: &'a Value, type_name: &str) -> Option<&'a Value> {
    if type_name == "Timeline" {
        return Some(schema);
    }
    schema.get("definitions")?.get(type_name)
}

fn type_section(out: &mut String, schema: &Value, type_name: &str, doc: &str) {
    let Some(object) = type_schema(schema, type_name) else {
        return;
    };
    let _ = writeln!(out, "### {type_name}\n\n{doc}\n");
    if let Some(variants) = object
        .get("oneOf")
        .or_else(|| object.get("anyOf"))
        .and_then(Value::as_array)
    {
        let tagged: Vec<&Value> = variants.iter().filter(|v| is_tagged_variant(v)).collect();
        if tagged.is_empty() {
            // Plain refs, or (ResolveOTIOParameter) single-key wrappers the
            // schema derives for a type serialized by its own key: list the
            // types they point to.
            let names: Vec<String> = variants
                .iter()
                .map(|v| match v.get("properties").and_then(Value::as_object) {
                    Some(p) if p.len() == 1 => type_string(p.values().next().unwrap()),
                    _ => type_string(v),
                })
                .collect();
            let _ = writeln!(out, "One of {}.\n", names.join(", "));
        }
        for variant in tagged {
            let tag = variant
                .pointer("/properties/OTIO_SCHEMA/enum/0")
                .and_then(Value::as_str)
                .unwrap_or("variant");
            let _ = writeln!(out, "#### `{tag}`\n");
            field_table(out, type_name, variant);
        }
        return;
    }
    if object.get("enum").is_some() {
        let _ = writeln!(out, "Values: {}.\n", type_string(object));
        return;
    }
    field_table(out, type_name, object);
}

/// A `oneOf` variant told apart by its `OTIO_SCHEMA`, documented with its
/// own field table.
fn is_tagged_variant(variant: &Value) -> bool {
    variant.pointer("/properties/OTIO_SCHEMA").is_some()
}

fn target_name(target: EffectTarget) -> &'static str {
    match target {
        EffectTarget::Video => "video (picture)",
        EffectTarget::Audio => "audio (sound)",
        EffectTarget::Text => "text clips",
        EffectTarget::Time => "playback speed",
    }
}

fn kind_string(kind: ParameterKind) -> String {
    match kind {
        ParameterKind::Animatable => "[animatable](#animatable-parameters) number".to_string(),
        ParameterKind::Color => "CSS color string".to_string(),
        ParameterKind::String => "string".to_string(),
        ParameterKind::Choice(options) => options
            .iter()
            .map(|o| format!("`\"{o}\"`"))
            .collect::<Vec<_>>()
            .join(" \\| "),
        ParameterKind::Bool => "boolean".to_string(),
        ParameterKind::Points => {
            "array of `[x, y]` or `{x, y, in_x?, in_y?, out_x?, out_y?}` (animatable coordinates)"
                .to_string()
        }
        ParameterKind::Regions => {
            "array of [Mask](#effect-mask) parameter objects; the effect shows only inside them, \
             nowhere when empty"
                .to_string()
        }
        ParameterKind::Follow => {
            "object: the auto-tracking zoom recipe `x`, `y` and `scale` were computed from; \
             see [Follow](#follow)"
                .to_string()
        }
        ParameterKind::Attach => {
            "object: the subject of another track `x` and `y` (and `opacity`) keep the clip \
             on; see [Attach](#attach)"
                .to_string()
        }
        ParameterKind::Detect => {
            "object: the subject the player detects to fill `regions` (a filter) or the shape \
             (a Mask); see [Detect](#detect)"
                .to_string()
        }
        ParameterKind::Bands => format!(
            "array of filter bands run in order, each `{{ \"type\", \"frequency\"?, \"gain\"?, \"q\"? }}`: \
             `type` one of {}; `frequency` in Hz (default 1000), `gain` in dB (shelves and peaking \
             only, default 0) and `q` (default 0.707) animatable numbers",
            EQUALIZER_BAND_TYPES
                .iter()
                .map(|t| format!("`\"{t}\"`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn ease_curve(ease: Ease) -> &'static str {
    match ease {
        Ease::Linear => "straight line (the default)",
        Ease::Hold => "keeps this keyframe's value until the next keyframe",
        Ease::EaseIn => "starts slow: `x³`",
        Ease::EaseOut => "ends slow: `1 − (1 − x)³`",
        Ease::EaseInOut => "slow at both ends (cubic)",
    }
}

const EASES: [Ease; 5] = [Ease::Linear, Ease::Hold, Ease::EaseIn, Ease::EaseOut, Ease::EaseInOut];

fn effects_section(out: &mut String) {
    let _ = writeln!(
        out,
        "## Tellers effects\n\n\
         Every effect the player renders has the same shape: an `Effect.1` whose `effect_name` \
         is `{TELLERS_EFFECT_PREFIX}<Effect>` and whose parameters are in \
         `metadata[\"tellers.ai\"].parameters`. A bare `<Effect>` name (written before the \
         prefix) is still read. Unknown names are ignored, and an effect with \
         `\"enabled\": false` is skipped.\n\n\
         ```json\n\
         {{\n  \"OTIO_SCHEMA\": \"Effect.1\",\n  \"effect_name\": \"{TELLERS_EFFECT_PREFIX}Blur\",\n  \
         \"metadata\": {{ \"tellers.ai\": {{ \"parameters\": {{ \"strength\": 6 }} }} }}\n}}\n\
         ```\n\n\
         The same effect can sit on a clip or a track (applied to each of its clips); clip \
         effects apply first, then the track's. On the stack the player applies only a \
         `MotionBlur` (every clip's default) and `ColorLut`s. Where an effect applies once (`Transform`, `Blend`, `Volume`, \
         ...), the first enabled one wins; the sound effects `Equalizer`, `Compressor` and `Limiter` all \
         apply, in order, before the `Volume`, `Pan` and fades of their clip or track. Parameters \
         left out take the player's defaults.\n"
    );
    out.push_str("| Effect | `effect_name` | Applies to | Parameters |\n|---|---|---|---|\n");
    for spec in EFFECT_SPECS {
        let params: Vec<String> = spec.parameters.iter().map(|p| format!("`{}`", p.name)).collect();
        let _ = writeln!(
            out,
            "| [{name}](#effect-{anchor}) | `{TELLERS_EFFECT_PREFIX}{name}` | {} | {} |",
            target_name(spec.target),
            params.join(", "),
            name = spec.name,
            anchor = spec.name.to_lowercase(),
        );
    }
    out.push('\n');
    for spec in EFFECT_SPECS {
        let _ = writeln!(
            out,
            "<a id=\"effect-{}\"></a>\n### {}\n\n`{TELLERS_EFFECT_PREFIX}{}`, applies to {}.\n",
            spec.name.to_lowercase(),
            spec.name,
            spec.name,
            target_name(spec.target),
        );
        out.push_str("| Parameter | Value |\n|---|---|\n");
        for param in spec.parameters {
            let _ = writeln!(out, "| `{}` | {} |", param.name, kind_string(param.kind));
        }
        out.push('\n');
    }
    let _ = writeln!(
        out,
        "### Asset references\n\n\
         An effect that loads an asset (a `ColorLut`'s `.cube` file) names it in its \
         `asset_id` parameter, which is what the timeline stores, and the player loads it \
         from its `target_url` parameter. `target_url` is resolved from `asset_id` before \
         playback and stripped before saving, like a media reference's `target_url`; this \
         holds for every effect with an `asset_id`, whatever its name.\n\n\
         An uploaded font in `metadata[\"tellers.ai\"].availableFonts` works the same \
         way: its entry stores `asset_id` instead of `url`, and the player loads it from the \
         entry's `target_url`, resolved before playback (`set_font_target_urls`) and \
         stripped before saving (`clear_target_urls`). Until it resolves the player skips \
         the font.\n\n\
         A `ColorLut`'s `asset_id` and URL used to be written beside `parameters`, directly \
         in `tellers.ai`, and the URL used to be named `cube_url`; `sanitize` upgrades \
         those forms.\n"
    );
    let _ = writeln!(
        out,
        "### Follow\n\n\
         A `Transform` that follows a detected subject (the player's \
         `VideoPlayer.followTransform`) keeps its recipe in its `{FOLLOW_PARAMETER}` parameter, \
         beside the `x`, `y` and `scale` keyframes computed from it. Playback reads only those \
         keyframes; `{FOLLOW_PARAMETER}` is what recomputes them after re-tuning. An item's \
         follow is the one on its first enabled `Transform`.\n\n\
         A follow can be declared before anything is detected, with only `target` and `pick` \
         (and no `x`, `y`, `scale` yet): on load the player detects `target` in the clip, takes \
         the subject at `pick`, computes `region` and the keyframes and reports them \
         (`onFollowResolved`) for the host to save; the host saves it only if the follow did not \
         change during detection. A follow needs `region`, or both `target` \
         and `pick`.\n\n\
         | Key | Value |\n|---|---|\n\
         | `region` | the detected region followed, an object of [Mask](#effect-mask) parameters in clip space; required unless `target` and `pick` are given |\n\
         | `pick` | `{{ \"t\", \"x\", \"y\" }}`, plain numbers, all required: where the subject was chosen, at `t` clip seconds and (`x`, `y`) in clip space before crop ((0, 0) the clip's center, ±0.5 its edges); with `target`, enough to detect it again |\n\
         | `target` | the detection target it came from: {}, or one COCO category: {} |\n\
         | `detect` | how the detected region looks, kept as given (left out: the player's defaults): the plain numbers `padX`, `padY`, the region's size relative to the detected box; no other key. How well subjects are found ({}) is the player's `detection` option, not the timeline's; `sanitize` drops those keys and `tolerance` from timelines written earlier |\n\
         | `fill`, `aspect`, `minZoom`, `maxZoom`, `deadZone`, `zoomDeadZone`, `panSmoothing`, `zoomSmoothing`, `anchorX`, `anchorY`, `hold` | plain numbers: the `followTransform` options of the same names; left out, the player's defaults |\n",
        FOLLOW_GROUP_TARGETS.iter().map(|t| format!("`\"{t}\"`")).collect::<Vec<_>>().join(" \\| "),
        COCO_CATEGORIES.iter().map(|t| format!("`{t}`")).collect::<Vec<_>>().join(", "),
        MOVED_DETECTION_SETTINGS.iter().map(|k| format!("`{k}`")).collect::<Vec<_>>().join(", "),
    );
    let _ = writeln!(
        out,
        "### Attach\n\n\
         A `Transform`'s `{ATTACH_PARAMETER}` keeps its clip (a label, a sticker) on a subject \
         detected in the clip of another video track playing alongside. On load the player \
         detects `target` in the clip of `track` that plays longest alongside this one, takes \
         the subject at `pick` (else the one seen longest) and writes `x` and `y` keyframes (and \
         `opacity`, with `hide`) on the Transform and `region` (and, with `hide`, the clip's own \
         `opacity`) in `{ATTACH_PARAMETER}`, then reports them (`onFollowResolved`, like a \
         follow) for the host to save; the host saves it only if the attach did not change \
         during detection. Playback reads only `x`, `y` and `opacity`; dropping them beside a \
         `region` recomputes them without detection. An item's attach is the one on its first \
         enabled `Transform`. It is meant for a clip centered in the frame (text, full-frame \
         images): Placement, rotation and groups of either clip are not taken into account. \
         Clips with an attach are resolved after the others, so a follow-zoomed source is read \
         with its zoom.\n\n\
         | Key | Value |\n|---|---|\n\
         | `track` | required: the video track (its `name`, `id` or `tellers.ai.timeline_id`) whose clip the subject is detected in |\n\
         | `target` | what to detect, as a follow's `target` (see [Follow](#follow)); required unless `region` is given |\n\
         | `pick` | `{{ \"t\", \"x\", \"y\" }}`, plain numbers, all required: the subject there, in this clip's seconds and frame coordinates ((0, 0) the frame's center, ±0.5 its edges) |\n\
         | `detect` | as a follow's `detect`: `padX`, `padY` |\n\
         | `anchorX`, `anchorY` | plain numbers: where this clip's center sits on the subject, as an offset from the subject's center in its widths and heights (`anchorY` -0.5 is its top edge); left out, 0 |\n\
         | `smoothing` | plain number: smoothing of the subject's motion, seconds; left out, 0.2 |\n\
         | `hide` | boolean: the clip disappears while the subject is unseen, instead of staying where it was last seen; left out, false |\n\
         | `opacity` | [animatable](#animatable-parameters): with `hide`, the clip's own opacity, multiplied into the resolved one; written on resolution |\n\
         | `region` | the subject's region, an object of [Mask](#effect-mask) parameters in frame coordinates and this clip's seconds; written on resolution |\n"
    );
    let _ = writeln!(
        out,
        "### Detect\n\n\
         Any filter (an effect taking `regions`) and a `Mask` can declare a subject to detect \
         in their `{DETECT_PARAMETER}` parameter instead of drawn shapes, e.g. \
         `{{ \"strength\": 40, \"detect\": {{ \"target\": \"faces\" }} }}` on a `Blur`. On \
         load the player detects `target` over the clip's source range and resolves the \
         effect: a filter gets `regions` (every subject, or the ones at `pick`), a `Mask` the \
         keyframed shape of one subject (the first one `pick` finds, else the one seen longest) as its \
         {}, keeping its other parameters (`feather`, `invert`, ...). It reports the result \
         (`onDetectResolved`, with the effect's index among all the clip's effects) for the \
         host to save; the host saves it only while the effect and its `detect` did not change \
         during detection. A filter without `regions` or a `Mask` without `x` is unresolved; \
         `detect` stays once resolved, so dropping `regions` (or `x`) detects again. Only \
         clips' effects are resolved, not tracks' or composite clips'.\n\n\
         | Key | Value |\n|---|---|\n\
         | `target` | required: what to detect, as a follow's `target` (see [Follow](#follow)) |\n\
         | `track` | detect in the clip of this video track (its `name`, `id` or `tellers.ai.timeline_id`) playing longest alongside this clip, instead of in this clip's media: the regions are moved into this clip's time and box, so an overlay's `Mask` with `invert: true` cuts out a subject crossing it; `pick` is then in this clip's seconds and frame coordinates. Those clips are resolved after the others |\n\
         | `pick` | `{{ \"t\", \"x\", \"y\" }}`, as a follow's: only the subject there; or a non-empty list of them, e.g. to blur several chosen faces with one filter (points finding nobody are skipped, two on one subject give it one region; a `Mask` takes the first subject found) |\n\
         | `shape` | `\"rectangle\"` \\| `\"ellipse\"` (the subject's box; faces default to ellipses, others to rounded rectangles) \\| `\"outline\"` (its exact shape, keyframed as a `path`) |\n\
         | `padX`, `padY` | the look of the regions, as in a follow's `detect`, here beside `target`; no other key (the detection settings are the player's, see [Follow](#follow)) |\n",
        DETECT_MASK_PARAMETERS.iter().map(|k| format!("`{k}`")).collect::<Vec<_>>().join(", "),
    );
    out.push_str(
        "### Track references\n\n\
         A `TrackMatte`'s `track`, a `Transform`'s `attach.track` and a `detect`'s `track` name \
         another track: the video track whose `name` or `tellers.ai.timeline_id` it is, never the \
         effect's own track (the track it is on, or its item is on). A name another video track \
         also carries is ambiguous, so a `tellers.ai.timeline_id` is the safer reference. One \
         that matches no video track does nothing (a `TrackMatte` without `track` reads the \
         track directly above instead), and `validate_timeline` reports it.\n\n\
         `rename_track` points the effects that named a track by its old name at it again: by \
         its new name, or by its `tellers.ai.timeline_id` when it gets no name or one another \
         video track carries. `delete_track` unlinks the effects that named only the deleted \
         track, keeping what they produced: a `TrackMatte` is removed, an `attach` is dropped \
         (its baked `x` and `y` stay), and a `detect` is dropped (its `regions` or shape stay; \
         an unresolved one's effect is removed).\n\n",
    );
    out.push_str(
        "### Animatable parameters\n\n\
         A numeric parameter is either a plain number or keyframes:\n\n\
         ```json\n\
         { \"keyframes\": [ { \"t\": 0, \"v\": 0, \"ease\": \"easeOut\" }, { \"t\": 1.5, \"v\": 8 } ] }\n\
         ```\n\n\
         | Key | Meaning |\n|---|---|\n\
         | `t` | Seconds from the start of the clip on the timeline (not source time). |\n\
         | `v` | The value at `t`. |\n\
         | `ease` | The curve from this keyframe to the next. |\n\n\
         Before the first keyframe the value is the first one's, after the last the last \
         one's. Keyframes are read sorted by `t`.\n\n\
         | `ease` | Curve |\n|---|---|\n",
    );
    for ease in EASES {
        let name = serde_json::to_value(ease).expect("an ease serializes");
        let _ = writeln!(out, "| `{}` | {} |", name.as_str().unwrap_or_default(), ease_curve(ease));
    }
    out.push('\n');
}

/// The Markdown reference of the timeline format.
pub fn reference_markdown() -> String {
    let schema = timeline_schema();
    let mut out = String::new();
    out.push_str(
        "<!-- Generated by `just regen-docs` (tellers-timeline-schema). Do not edit by hand: \
         edit tellers-timeline-schema/src/lib.rs or the core types instead. -->\n\n\
         # Tellers OTIO timeline reference\n\n\
         A tellers timeline is an [OpenTimelineIO](https://opentimelineio.readthedocs.io) JSON \
         document with extra data under `metadata[\"tellers.ai\"]` and in tellers effects. \
         New to the format? Start with the [reading guide](reading-otio.md).\n\n\
         - [Objects](#objects)\n\
         - [Tellers effects](#tellers-effects)\n\n\
         ## Objects\n\n",
    );
    for (type_name, doc) in TYPE_DOCS {
        type_section(&mut out, &schema, type_name, doc);
    }
    effects_section(&mut out);
    out.trim_end().to_string() + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every object type and field of the schema, as `(type, field)`.
    fn schema_fields(schema: &Value) -> Vec<(String, String)> {
        let mut fields = Vec::new();
        let mut collect = |type_name: &str, object: &Value| {
            let variants = object
                .get("oneOf")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_else(|| vec![object.clone()]);
            for variant in variants.iter().filter(|v| is_tagged_variant(v) || !object.get("oneOf").is_some()) {
                for name in variant
                    .get("properties")
                    .and_then(Value::as_object)
                    .into_iter()
                    .flat_map(|p| p.keys())
                {
                    fields.push((type_name.to_string(), name.clone()));
                }
            }
        };
        collect("Timeline", schema);
        for (name, object) in schema["definitions"].as_object().unwrap() {
            collect(name, object);
        }
        fields
    }

    #[test]
    fn every_type_and_field_is_documented() {
        let schema = timeline_schema();
        for name in schema["definitions"].as_object().unwrap().keys() {
            assert!(
                TYPE_DOCS.iter().any(|(ty, _)| ty == name),
                "add {name} to TYPE_DOCS in tellers-timeline-schema/src/lib.rs"
            );
        }
        for (ty, field) in schema_fields(&schema) {
            assert!(
                field_doc(&ty, &field).is_some(),
                "add {ty}.{field} to FIELD_DOCS in tellers-timeline-schema/src/lib.rs"
            );
        }
    }

    #[test]
    fn checked_in_reference_is_up_to_date() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/otio-reference.md");
        let checked_in = std::fs::read_to_string(path).unwrap_or_default();
        assert!(
            checked_in == reference_markdown(),
            "{REFERENCE_PATH} is stale: run `just regen-docs`"
        );
    }

    #[test]
    fn reference_covers_every_effect() {
        let reference = reference_markdown();
        for spec in EFFECT_SPECS {
            assert!(reference.contains(&format!("`{TELLERS_EFFECT_PREFIX}{}`", spec.name)));
        }
    }
}
