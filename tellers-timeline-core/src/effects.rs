//! Tellers effects: the filters, transforms, masks, audio and text effects
//! the player (`video-player-js`) renders, attached to a clip, a track or —
//! for the timeline — its root stack (`timeline.tracks`).
//!
//! Every tellers effect has the same shape, an OTIO effect whose parameters
//! live under `metadata["tellers.ai"].parameters`:
//!
//! ```json
//! {
//!   "OTIO_SCHEMA": "Effect.1",
//!   "effect_name": "tellers.ai.Blur",
//!   "name": "soft focus",
//!   "enabled": true,
//!   "metadata": { "tellers.ai": { "parameters": {
//!     "strength": { "keyframes": [ { "t": 0, "v": 0, "ease": "easeOut" }, { "t": 1.5, "v": 8 } ] },
//!     "quality": 4
//!   } } }
//! }
//! ```
//!
//! - `effect_name` is `tellers.ai.<Effect>`; a bare `<Effect>`, written
//!   before tellers effects were prefixed, is still read, and rewritten with
//!   the prefix when the effect is next written. [`TellersEffect`] holds the
//!   bare name. [`EFFECT_SPECS`] lists the effects the
//!   player knows and their parameters. The player ignores other names.
//! - `enabled: false` turns the effect off without removing it; absent means
//!   enabled.
//! - A numeric parameter is an [`Animatable`]: a number, or keyframes with
//!   `t` in seconds from the start of the clip on the timeline. Strings
//!   (colors, presets, modes) and booleans are fixed.
//! - A filter (`Blur`, `Pixelate`, `Adjustment`, ... `ChromaKey`) can take
//!   `regions`, a list of `Mask` shapes: it then shows only inside them
//!   (one keyframed shape per tracked face or plate), and nowhere when the
//!   list is empty. A filter or a `Mask` can instead declare `detect`, what
//!   the player detects to fill them (see [`crate::detect`]).
//!
//! The player applies a clip's effects first, then its track's. Tellers
//! effects can be written on the root stack, but the player applies only a
//! `MotionBlur` there, as every clip's default.
//!
//! The sound effects `Equalizer`, `Compressor` and `Limiter` all apply, in
//! list order, on a clip's sound before its `Volume`, `Pan` and fades, or on
//! the sum of a track's clips before the track's `Volume` and `Pan`. An
//! `Equalizer`'s `bands` are filters run in order, each an object with a
//! `type` and animatable `frequency`, `gain` and `q`. `Pitch` shifts a
//! clip's pitch by `semitones` (clamped to -24..24) without changing its
//! speed or length; on a track it applies to each clip without its own.
//!
//! Each effect has a target ([`EffectTarget`]) deciding where it may go:
//! picture effects on video (and other) tracks and their clips, text clips
//! included; text effects (`Text*`) on text clips only (and on non-audio
//! tracks, which apply them to their text clips); sound effects (`Volume`,
//! `Pan`) on audio tracks and their clips only; a `TimeWarp` anywhere. The
//! track accessors and the timeline's `*_track_*` and `*_item_*` accessors
//! refuse an effect on the wrong kind of track or clip; the clip accessors
//! cannot tell (a clip does not know its track), nor can the free functions
//! on an effect list.
//!
//! A composite clip (a nested `Stack`, [`crate::CompositeStack`]) is an item
//! like a clip: the player applies its filters, `Transform` and `Blend` to
//! the composited image, so the item accessors (`*_item_tellers_effect*`)
//! read and edit a composite's effects too. They find items at any depth,
//! inside composite clips as well. Gaps have none.
//!
//! An effect referencing an asset (a `ColorLut`'s `.cube` file) names it in
//! its `asset_id` parameter and is loaded from its `target_url` parameter,
//! resolved from the id (see [`crate::effect_assets`]).
//!
//! The tellers effects of a stage, in list order, are its effect list. Other
//! effects (Resolve transforms and crop; OTIO time warps) are left where they are. Indices
//! passed to the list methods count tellers effects only, which are exactly
//! what [`resolve_tellers_effects`] returns.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{Clip, CompositeStack, Effect, Item, Timeline, Track, TrackKind};

/// Key, inside a tellers effect's `tellers.ai` object, holding its
/// parameters.
const PARAMETERS_KEY: &str = "parameters";

/// Prefix of a tellers effect's `effect_name`, as in `tellers.ai.Blur`.
pub const TELLERS_EFFECT_PREFIX: &str = "tellers.ai.";

/// `effect_name` without its [`TELLERS_EFFECT_PREFIX`].
pub fn bare_effect_name(effect_name: &str) -> &str {
    effect_name
        .strip_prefix(TELLERS_EFFECT_PREFIX)
        .unwrap_or(effect_name)
}

/// What a tellers effect applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectTarget {
    /// The picture of a clip, or of every clip of a video track.
    Video,
    /// The sound of a clip, or of every clip of a track.
    Audio,
    /// The letters of a Text clip (a `Text` generator); no other clip.
    Text,
    /// The playback speed of a clip.
    Time,
}

impl EffectTarget {
    /// Whether an effect of this target belongs on a track of `kind`, or on
    /// a clip of one: picture and text effects on any track but an audio
    /// one, sound effects on audio tracks only (video tracks carry no
    /// sound; it plays from their bound audio clips), speed on a clip of
    /// any track. A text effect also needs a text clip, and speed a media
    /// clip; see [`Self::allowed_on_item`].
    pub fn allowed_on(self, kind: &TrackKind) -> bool {
        match self {
            EffectTarget::Video | EffectTarget::Text => *kind != TrackKind::Audio,
            EffectTarget::Audio => *kind == TrackKind::Audio,
            EffectTarget::Time => true,
        }
    }

    /// Whether an effect of this target belongs on a track of `kind` itself:
    /// as [`Self::allowed_on`], but no speed, which the player reads from
    /// clips only.
    pub fn allowed_on_track(self, kind: &TrackKind) -> bool {
        self != EffectTarget::Time && self.allowed_on(kind)
    }

    /// Whether an effect of this target belongs on `item`, on a track of
    /// `kind`: as [`Self::allowed_on`], a text effect only on a text clip,
    /// and speed only on a media clip (not a composite clip, nor a Text,
    /// Shape or Rich Text clip, which the player shows at normal speed).
    /// Picture effects go on text clips too.
    pub fn allowed_on_item(self, kind: &TrackKind, item: &Item) -> bool {
        self.allowed_on(kind)
            && match self {
                EffectTarget::Text => matches!(item, Item::Clip(clip) if clip.is_text_clip()),
                EffectTarget::Time => matches!(item, Item::Clip(clip) if !clip.is_generator_clip()),
                EffectTarget::Video | EffectTarget::Audio => true,
            }
    }
}

/// The type of a tellers effect parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterKind {
    /// A number or keyframes; see [`Animatable`].
    Animatable,
    /// A CSS color string, e.g. `#ffd400`.
    Color,
    /// A free string (e.g. a track name or id).
    String,
    /// One of the listed strings.
    Choice(&'static [&'static str]),
    Bool,
    /// A `Mask` path: `[x, y]` pairs or `{x, y, in_x, in_y, out_x, out_y}`
    /// bezier points, each coordinate an [`Animatable`].
    Points,
    /// A filter's `regions`: a list of mask shapes, each an object of `Mask`
    /// parameters; the filter shows only inside them (none: nowhere).
    Regions,
    /// A `Transform`'s `follow`: the auto-tracking zoom recipe its `x`, `y`
    /// and `scale` were computed from; see [`crate::Follow`].
    Follow,
    /// A `Transform`'s `attach`: the subject of another track its `x`, `y`
    /// (and `opacity`) keep it on; see [`crate::Attach`].
    Attach,
    /// A filter's or a `Mask`'s `detect`: the subject the player detects to
    /// fill its `regions` or shape; see [`crate::EffectDetect`].
    Detect,
    /// An `Equalizer`'s `bands`: a list of filters run in order, each an
    /// object with a `type` (one of [`EQUALIZER_BAND_TYPES`]) and the
    /// [`Animatable`]s `frequency` (Hz), `gain` (dB, shelves and peaking
    /// only) and `q`.
    Bands,
}

/// The `type` of an `Equalizer` band.
pub const EQUALIZER_BAND_TYPES: &[&str] = &["highpass", "lowpass", "lowshelf", "highshelf", "peaking"];
/// The numeric keys of an `Equalizer` band, each an [`Animatable`].
pub const EQUALIZER_BAND_NUMBERS: &[&str] = &["frequency", "gain", "q"];

/// A parameter of a tellers effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParameterSpec {
    pub name: &'static str,
    pub kind: ParameterKind,
}

/// A tellers effect the player knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectSpec {
    pub name: &'static str,
    pub target: EffectTarget,
    pub parameters: &'static [ParameterSpec],
}

impl EffectSpec {
    /// The spec of the parameter `name`.
    pub fn parameter(&self, name: &str) -> Option<&'static ParameterSpec> {
        self.parameters.iter().find(|p| p.name == name)
    }
}

const fn anim(name: &'static str) -> ParameterSpec {
    ParameterSpec { name, kind: ParameterKind::Animatable }
}
const fn string(name: &'static str) -> ParameterSpec {
    ParameterSpec { name, kind: ParameterKind::String }
}
const fn color(name: &'static str) -> ParameterSpec {
    ParameterSpec { name, kind: ParameterKind::Color }
}
const fn choice(name: &'static str, options: &'static [&'static str]) -> ParameterSpec {
    ParameterSpec { name, kind: ParameterKind::Choice(options) }
}
const fn boolean(name: &'static str) -> ParameterSpec {
    ParameterSpec { name, kind: ParameterKind::Bool }
}
/// `regions` of a filter effect; see [`ParameterKind::Regions`].
const REGIONS: ParameterSpec = ParameterSpec { name: "regions", kind: ParameterKind::Regions };
/// `detect` of a filter effect or a `Mask`; see [`ParameterKind::Detect`].
const DETECT: ParameterSpec = ParameterSpec { name: "detect", kind: ParameterKind::Detect };
const fn effect(
    name: &'static str,
    target: EffectTarget,
    parameters: &'static [ParameterSpec],
) -> EffectSpec {
    EffectSpec { name, target, parameters }
}

use EffectTarget::{Audio, Text, Time, Video};

/// Every tellers effect the player renders, with its parameters (mirrors
/// `schema/timeline.schema.json` in `video-player-js`).
pub const EFFECT_SPECS: &[EffectSpec] = &[
    effect("Blur", Video, &[anim("strength"), anim("quality"), REGIONS, DETECT]),
    effect("Pixelate", Video, &[anim("size"), REGIONS, DETECT]),
    effect(
        "Adjustment",
        Video,
        &[
            anim("gamma"),
            anim("saturation"),
            anim("contrast"),
            anim("brightness"),
            anim("red"),
            anim("green"),
            anim("blue"),
            anim("alpha"),
            REGIONS,
            DETECT,
        ],
    ),
    effect(
        "ColorMatrix",
        Video,
        &[
            choice(
                "preset",
                &[
                    "sepia",
                    "vintage",
                    "polaroid",
                    "kodachrome",
                    "technicolor",
                    "browni",
                    "lsd",
                    "blackAndWhite",
                    "negative",
                    "desaturate",
                ],
            ),
            anim("hue"),
            anim("amount"),
            REGIONS,
            DETECT,
        ],
    ),
    effect(
        "HslAdjustment",
        Video,
        &[
            anim("hue"),
            anim("saturation"),
            anim("lightness"),
            anim("colorize"),
            anim("amount"),
            REGIONS,
            DETECT,
        ],
    ),
    // `target_url` is resolved from `asset_id`; see `crate::effect_assets`.
    effect("ColorLut", Video, &[string("asset_id"), string("target_url")]),
    effect("Noise", Video, &[anim("amount"), anim("speed"), anim("seed"), REGIONS, DETECT]),
    effect("RGBSplit", Video, &[anim("amount"), anim("angle"), REGIONS, DETECT]),
    effect(
        "ZoomBlur",
        Video,
        &[
            anim("strength"),
            anim("centerX"),
            anim("centerY"),
            anim("innerRadius"),
            REGIONS,
            DETECT,
        ],
    ),
    effect(
        "DropShadow",
        Video,
        &[
            anim("offsetX"),
            anim("offsetY"),
            anim("blur"),
            anim("alpha"),
            color("color"),
            REGIONS,
            DETECT,
        ],
    ),
    effect(
        "ChromaKey",
        Video,
        &[
            color("color"),
            anim("similarity"),
            anim("smoothness"),
            anim("spill"),
            anim("choke"),
            REGIONS,
            DETECT,
        ],
    ),
    effect(
        "Border",
        Video,
        &[
            anim("width"),
            anim("top"),
            anim("right"),
            anim("bottom"),
            anim("left"),
            anim("opacity"),
            color("color"),
        ],
    ),
    effect(
        "MotionBlur",
        Video,
        &[anim("shutterAngle"), anim("shutterPhase"), anim("samples")],
    ),
    effect(
        "Transform",
        Video,
        &[
            anim("x"),
            anim("y"),
            anim("scale"),
            anim("scaleX"),
            anim("scaleY"),
            anim("rotation"),
            anim("opacity"),
            ParameterSpec { name: "follow", kind: ParameterKind::Follow },
            ParameterSpec { name: "attach", kind: ParameterKind::Attach },
        ],
    ),
    // A Transform measured to cancel camera shake (`VideoPlayer.stabilize`),
    // applied under the clip's own Transform.
    effect(
        "Stabilize",
        Video,
        &[anim("x"), anim("y"), anim("scale"), anim("rotation")],
    ),
    effect(
        "Blend",
        Video,
        &[choice(
            "mode",
            &[
                "normal",
                "add",
                "screen",
                "lighten",
                "multiply",
                "darken",
                "overlay",
                "soft-light",
                "hard-light",
                "difference",
            ],
        )],
    ),
    effect(
        "Mask",
        Video,
        &[
            choice("shape", &["rectangle", "ellipse", "path"]),
            anim("x"),
            anim("y"),
            anim("width"),
            anim("height"),
            anim("rotation"),
            anim("corner_radius"),
            anim("feather"),
            anim("opacity"),
            boolean("invert"),
            choice("mode", &["add", "subtract", "intersect"]),
            boolean("closed"),
            ParameterSpec { name: "points", kind: ParameterKind::Points },
            DETECT,
        ],
    ),
    effect(
        "TrackMatte",
        Video,
        &[
            choice("mode", &["alpha", "luma", "alpha_inverted", "luma_inverted"]),
            string("track"),
        ],
    ),
    effect("Volume", Audio, &[anim("volume"), anim("fadeIn"), anim("fadeOut")]),
    effect("Pan", Audio, &[anim("pan")]),
    effect("Pitch", Audio, &[anim("semitones")]),
    effect("Equalizer", Audio, &[ParameterSpec { name: "bands", kind: ParameterKind::Bands }]),
    effect(
        "Compressor",
        Audio,
        &[
            anim("threshold"),
            anim("ratio"),
            anim("knee"),
            anim("attack"),
            anim("release"),
            anim("makeup"),
        ],
    ),
    effect("Limiter", Audio, &[anim("ceiling"), anim("release")]),
    effect("TimeWarp", Time, &[anim("speed"), boolean("preserve_pitch")]),
    effect("TextTypewriter", Text, &[anim("progress")]),
    effect(
        "TextWordReveal",
        Text,
        &[anim("progress"), choice("style", &["fade", "appear", "pop", "slideUp"])],
    ),
    effect(
        "TextKaraoke",
        Text,
        &[anim("progress"), anim("scale"), color("color")],
    ),
    effect(
        "TextWave",
        Text,
        &[anim("amplitude"), anim("speed"), anim("wavelength")],
    ),
    effect(
        "TextColor",
        Text,
        &[
            string("colors"),
            anim("period"),
            anim("position"),
        ],
    ),
];

/// The spec of the tellers effect `effect_name` (prefixed or bare), if the
/// player knows it.
pub fn effect_spec(effect_name: &str) -> Option<&'static EffectSpec> {
    let name = bare_effect_name(effect_name);
    EFFECT_SPECS.iter().find(|spec| spec.name == name)
}

/// Whether `effect` is a tellers effect: its `effect_name` is in
/// [`EFFECT_SPECS`].
pub fn is_tellers_effect(effect: &Effect) -> bool {
    effect_spec(&effect.effect_name).is_some()
}

/// The curve from a keyframe to the next one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Ease {
    #[default]
    Linear,
    /// Keep this keyframe's value until the next one.
    Hold,
    EaseIn,
    EaseOut,
    EaseInOut,
}

impl Ease {
    /// The eased progress for a linear progress `x` in `[0, 1]`, as the
    /// player computes it (cubic curves).
    pub fn apply(self, x: f64) -> f64 {
        match self {
            Ease::Linear => x,
            Ease::Hold => 0.0,
            Ease::EaseIn => x * x * x,
            Ease::EaseOut => 1.0 - (1.0 - x).powi(3),
            Ease::EaseInOut => {
                if x < 0.5 {
                    4.0 * x * x * x
                } else {
                    1.0 - (-2.0 * x + 2.0).powi(3) / 2.0
                }
            }
        }
    }
}

/// A keyframe of an [`Animatable`] parameter.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Keyframe {
    /// Seconds from the start of the clip on the timeline (not source time).
    pub t: f64,
    pub v: f64,
    /// Curve from this keyframe to the next one; linear when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ease: Option<Ease>,
}

impl Keyframe {
    pub fn new(t: f64, v: f64) -> Self {
        Keyframe { t, v, ease: None }
    }

    pub fn with_ease(mut self, ease: Ease) -> Self {
        self.ease = Some(ease);
        self
    }
}

/// A numeric effect parameter: a number, or `{ "keyframes": [...] }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Animatable {
    Value(f64),
    Keyframes { keyframes: Vec<Keyframe> },
}

impl From<f64> for Animatable {
    fn from(value: f64) -> Self {
        Animatable::Value(value)
    }
}

impl From<Vec<Keyframe>> for Animatable {
    fn from(keyframes: Vec<Keyframe>) -> Self {
        Animatable::Keyframes { keyframes }
    }
}

impl Animatable {
    /// Read a parameter as the player does: a finite number, or keyframes
    /// with the non-finite ones dropped and the rest sorted by time. `None`
    /// when nothing usable is left.
    pub fn from_value(value: &Value) -> Option<Animatable> {
        match serde_json::from_value::<Animatable>(value.clone()).ok()? {
            Animatable::Value(v) => v.is_finite().then_some(Animatable::Value(v)),
            Animatable::Keyframes { keyframes } => {
                let mut keyframes: Vec<Keyframe> = keyframes
                    .into_iter()
                    .filter(|k| k.t.is_finite() && k.v.is_finite())
                    .collect();
                keyframes.sort_by(|a, b| a.t.total_cmp(&b.t));
                (!keyframes.is_empty()).then_some(Animatable::Keyframes { keyframes })
            }
        }
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    /// Whether this parameter changes over time.
    pub fn is_animated(&self) -> bool {
        matches!(self, Animatable::Keyframes { .. })
    }

    /// The value at `t` seconds from the start of the clip, holding the
    /// first and last keyframes outside their range, as the player does.
    pub fn value_at(&self, t: f64) -> f64 {
        let frames = match self {
            Animatable::Value(v) => return *v,
            Animatable::Keyframes { keyframes } => keyframes,
        };
        let (Some(first), Some(last)) = (frames.first(), frames.last()) else {
            return 0.0;
        };
        if t <= first.t {
            return first.v;
        }
        for pair in frames.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if t < b.t {
                let span = b.t - a.t;
                let x = if span > 0.0 { (t - a.t) / span } else { 1.0 };
                return a.v + (b.v - a.v) * a.ease.unwrap_or_default().apply(x);
            }
        }
        last.v
    }
}

/// A tellers effect: an `effect_name` from [`EFFECT_SPECS`] and its
/// parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TellersEffect {
    /// Which effect this is, bare, e.g. `Blur` (stored as `tellers.ai.Blur`);
    /// see [`EFFECT_SPECS`].
    pub effect_name: String,
    /// Optional display label, stored as the OTIO `name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default = "enabled_default")]
    pub enabled: bool,
    /// Parameter name → JSON value: a number or keyframes for animatable
    /// parameters, else a string, a boolean or `Mask` points.
    #[serde(default)]
    pub parameters: Map<String, Value>,
}

fn enabled_default() -> bool {
    true
}

impl TellersEffect {
    /// An enabled effect with no parameters, which the player draws with
    /// its defaults.
    pub fn new(effect_name: impl Into<String>) -> Self {
        TellersEffect {
            effect_name: effect_name.into(),
            name: None,
            enabled: true,
            parameters: Map::new(),
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Set the parameter `key` to `value` (e.g. `2.0`, `"#00ff00"`, `true`,
    /// or an [`Animatable`] via [`Animatable::to_value`]).
    pub fn with_parameter(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.parameters.insert(key.into(), value.into());
        self
    }

    /// The spec of this effect, if the player knows it.
    pub fn spec(&self) -> Option<&'static EffectSpec> {
        effect_spec(&self.effect_name)
    }

    /// Whether this effect is known and may go on a track of `kind`, or on
    /// a clip of one; see [`EffectTarget::allowed_on`].
    pub fn allowed_on(&self, kind: &TrackKind) -> bool {
        self.spec().is_some_and(|spec| spec.target.allowed_on(kind))
    }

    /// Whether this effect belongs on a track of `kind` itself; see
    /// [`EffectTarget::allowed_on_track`].
    pub fn allowed_on_track(&self, kind: &TrackKind) -> bool {
        self.spec().is_some_and(|spec| spec.target.allowed_on_track(kind))
    }

    fn is_time(&self) -> bool {
        self.spec().is_some_and(|spec| spec.target == EffectTarget::Time)
    }

    /// Whether the player plays this effect's speed as written (see
    /// [`crate::unplayable_speed`]); `true` for any other effect.
    fn speed_is_playable(&self) -> bool {
        crate::unplayable_speed(&self.to_effect()).is_none()
    }

    /// The numeric parameter `key`, read as the player does; see
    /// [`Animatable::from_value`].
    pub fn animatable(&self, key: &str) -> Option<Animatable> {
        Animatable::from_value(self.parameters.get(key)?)
    }

    pub fn set_animatable(&mut self, key: impl Into<String>, value: Animatable) {
        self.parameters.insert(key.into(), value.to_value());
    }

    /// The value of the numeric parameter `key` at `t` seconds from the
    /// start of the clip, or `None` when it is unset or unusable.
    pub fn value_at(&self, key: &str, t: f64) -> Option<f64> {
        Some(self.animatable(key)?.value_at(t))
    }

    /// Check this effect against [`EFFECT_SPECS`]: a known name, known
    /// parameters, and values of the right type.
    pub fn validate(&self) -> Result<(), String> {
        let spec = self
            .spec()
            .ok_or_else(|| format!("unknown effect_name {:?}", self.effect_name))?;
        for (key, value) in &self.parameters {
            let param = spec
                .parameter(key)
                .ok_or_else(|| format!("{} has no parameter {:?}", spec.name, key))?;
            if !parameter_value_is_valid(param.kind, value) {
                return Err(format!(
                    "{}.{}: invalid value {}",
                    spec.name, key, value
                ));
            }
        }
        Ok(())
    }

    /// This effect as an OTIO effect.
    pub fn to_effect(&self) -> Effect {
        let mut effect = Effect::default();
        self.write_into(&mut effect);
        effect
    }

    /// The tellers effect `effect` holds, if it is one.
    pub fn from_effect(effect: &Effect) -> Option<TellersEffect> {
        if !is_tellers_effect(effect) {
            return None;
        }
        let parameters = effect
            .metadata
            .other
            .get("tellers.ai")
            .and_then(|ai| ai.get(PARAMETERS_KEY))
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        Some(TellersEffect {
            effect_name: bare_effect_name(&effect.effect_name).to_string(),
            name: Some(effect.name.clone()).filter(|name| !name.is_empty()),
            enabled: effect.enabled != Some(false),
            parameters,
        })
    }

    /// Write this effect into `effect`, keeping its other keys (other
    /// `tellers.ai` keys, Resolve data, extra OTIO fields).
    pub(crate) fn write_into(&self, effect: &mut Effect) {
        effect.effect_name = format!(
            "{TELLERS_EFFECT_PREFIX}{}",
            bare_effect_name(&self.effect_name)
        );
        effect.name = self.name.clone().unwrap_or_default();
        // Keep an explicit `enabled: true` as it was; only write what differs.
        effect.enabled = match (self.enabled, effect.enabled) {
            (false, _) => Some(false),
            (true, Some(true)) => Some(true),
            (true, _) => None,
        };
        let ai = effect
            .metadata
            .other
            .entry("tellers.ai")
            .or_insert_with(|| Value::Object(Map::new()));
        if !ai.is_object() {
            *ai = Value::Object(Map::new());
        }
        ai.as_object_mut()
            .expect("tellers.ai is an object")
            .insert(PARAMETERS_KEY.to_string(), Value::Object(self.parameters.clone()));
    }
}

fn parameter_value_is_valid(kind: ParameterKind, value: &Value) -> bool {
    match kind {
        ParameterKind::Animatable => Animatable::from_value(value).is_some(),
        ParameterKind::Color | ParameterKind::String => value.is_string(),
        ParameterKind::Choice(options) => value.as_str().is_some_and(|v| {
            options.iter().any(|option| option.eq_ignore_ascii_case(v.trim()))
        }),
        ParameterKind::Bool => value.is_boolean(),
        ParameterKind::Points => value.as_array().is_some_and(|points| {
            points.iter().all(|point| match point {
                Value::Array(xy) => {
                    xy.len() == 2 && xy.iter().all(|c| Animatable::from_value(c).is_some())
                }
                Value::Object(p) => {
                    ["x", "y"].iter().all(|k| p.contains_key(*k))
                        && p.iter().all(|(k, c)| {
                            ["x", "y", "in_x", "in_y", "out_x", "out_y"].contains(&k.as_str())
                                && Animatable::from_value(c).is_some()
                        })
                }
                _ => false,
            })
        }),
        ParameterKind::Regions => value
            .as_array()
            .is_some_and(|regions| regions.iter().all(mask_shape_is_valid)),
        ParameterKind::Follow => crate::follow::follow_value_is_valid(value),
        ParameterKind::Attach => crate::attach::attach_value_is_valid(value),
        ParameterKind::Detect => crate::detect::detect_value_is_valid(value),
        ParameterKind::Bands => value
            .as_array()
            .is_some_and(|bands| bands.iter().all(equalizer_band_is_valid)),
    }
}

/// Whether `band` is an `Equalizer` band: an object with a known `type` and
/// animatable `frequency`, `gain` and `q`, nothing else.
fn equalizer_band_is_valid(band: &Value) -> bool {
    band.as_object().is_some_and(|band| {
        band.get("type")
            .and_then(Value::as_str)
            .is_some_and(|t| EQUALIZER_BAND_TYPES.contains(&t))
            && band.iter().all(|(key, v)| {
                key == "type"
                    || (EQUALIZER_BAND_NUMBERS.contains(&key.as_str())
                        && Animatable::from_value(v).is_some())
            })
    })
}

/// Whether `shape` is an object of `Mask` parameters (a filter region, a
/// followed region), without `detect`: a region is what was detected.
pub(crate) fn mask_shape_is_valid(shape: &Value) -> bool {
    let mask = effect_spec("Mask").expect("Mask is a tellers effect");
    shape.as_object().is_some_and(|shape| {
        shape.iter().all(|(key, v)| {
            key != crate::detect::DETECT_PARAMETER && mask.parameter(key)
                .is_some_and(|p| parameter_value_is_valid(p.kind, v))
        })
    })
}

/// The tellers effects of a stage's `effects`, in order.
pub fn resolve_tellers_effects(effects: &[Effect]) -> Vec<TellersEffect> {
    effects.iter().filter_map(TellersEffect::from_effect).collect()
}

/// Positions, in `effects`, of its tellers effects.
fn tellers_positions(effects: &[Effect]) -> Vec<usize> {
    effects
        .iter()
        .enumerate()
        .filter(|(_, effect)| is_tellers_effect(effect))
        .map(|(i, _)| i)
        .collect()
}

/// Replace every tellers effect of the stage with `new_effects`, at the
/// position of the first one (appended when there was none). Returns `false`
/// — leaving the stage untouched — when one of them has an unknown
/// `effect_name`.
pub fn set_tellers_effects(effects: &mut Vec<Effect>, new_effects: Vec<TellersEffect>) -> bool {
    if new_effects.iter().any(|e| e.spec().is_none()) {
        return false;
    }
    let at = effects
        .iter()
        .take_while(|effect| !is_tellers_effect(effect))
        .count();
    effects.retain(|effect| !is_tellers_effect(effect));
    effects.splice(at..at, new_effects.iter().map(TellersEffect::to_effect));
    true
}

/// Append a tellers effect, after the stage's last one (at the end when it
/// has none). `false` when its `effect_name` is unknown.
pub fn push_tellers_effect(effects: &mut Vec<Effect>, effect: TellersEffect) -> bool {
    let len = tellers_positions(effects).len();
    insert_tellers_effect_at(effects, len, effect)
}

/// Insert a tellers effect so it becomes the `index`-th one, right before
/// the effect now at that index (after the last one when `index` is the
/// count). `false` when `index` is past the count or the `effect_name` is
/// unknown.
pub fn insert_tellers_effect_at(
    effects: &mut Vec<Effect>,
    index: usize,
    effect: TellersEffect,
) -> bool {
    if effect.spec().is_none() {
        return false;
    }
    let positions = tellers_positions(effects);
    let at = match (positions.get(index), positions.last()) {
        (Some(&at), _) => at,
        (None, Some(&last)) if index == positions.len() => last + 1,
        (None, None) if index == 0 => effects.len(),
        _ => return false,
    };
    effects.insert(at, effect.to_effect());
    true
}

/// Replace the `index`-th tellers effect in place, keeping the stored
/// effect's other keys, and return the previous one. `None` when `index` is
/// out of range or the `effect_name` is unknown.
pub fn replace_tellers_effect_at(
    effects: &mut [Effect],
    index: usize,
    effect: TellersEffect,
) -> Option<TellersEffect> {
    effect.spec()?;
    let at = *tellers_positions(effects).get(index)?;
    let previous = TellersEffect::from_effect(&effects[at])?;
    effect.write_into(&mut effects[at]);
    Some(previous)
}

/// Remove the `index`-th tellers effect and return it.
pub fn remove_tellers_effect_at(effects: &mut Vec<Effect>, index: usize) -> Option<TellersEffect> {
    let at = *tellers_positions(effects).get(index)?;
    TellersEffect::from_effect(&effects.remove(at))
}

/// Remove every tellers effect; `true` when there was one.
pub fn clear_tellers_effects(effects: &mut Vec<Effect>) -> bool {
    let before = effects.len();
    effects.retain(|effect| !is_tellers_effect(effect));
    effects.len() != before
}

/// Turn the `index`-th tellers effect on or off; `false` when out of range.
pub fn set_tellers_effect_enabled(effects: &mut [Effect], index: usize, enabled: bool) -> bool {
    let Some(&at) = tellers_positions(effects).get(index) else {
        return false;
    };
    let Some(mut effect) = TellersEffect::from_effect(&effects[at]) else {
        return false;
    };
    effect.enabled = enabled;
    effect.write_into(&mut effects[at]);
    true
}

/// Set (or, with `None`, remove) the parameter `key` of the `index`-th
/// tellers effect; `false` when out of range.
pub fn set_tellers_effect_parameter(
    effects: &mut [Effect],
    index: usize,
    key: &str,
    value: Option<Value>,
) -> bool {
    let Some(&at) = tellers_positions(effects).get(index) else {
        return false;
    };
    let Some(mut effect) = TellersEffect::from_effect(&effects[at]) else {
        return false;
    };
    match value {
        Some(value) => effect.parameters.insert(key.to_string(), value),
        None => effect.parameters.remove(key),
    };
    effect.write_into(&mut effects[at]);
    true
}

/// A stage holding tellers effects; `None` for one that has none (a gap).
trait EffectStage {
    fn stage_effects(&self) -> Option<&[Effect]>;
    fn stage_effects_mut(&mut self) -> Option<&mut Vec<Effect>>;
    /// Whether `effect` may be added here; a track takes only the effects
    /// of its kind (see [`EffectTarget::allowed_on_track`]), and only a
    /// media clip takes speed, one the player plays as written. A clip does
    /// not know its track, so the [`Timeline`] checks it for `*_item_*` edits.
    fn stage_accepts(&self, _effect: &TellersEffect) -> bool {
        true
    }
}

macro_rules! impl_effect_stage {
    ($ty:ty, |$s:ident| $effects:expr $(, accepts |$a:ident, $e:ident| $accepts:expr)?) => {
        impl EffectStage for $ty {
            fn stage_effects(&self) -> Option<&[Effect]> {
                let $s = self;
                Some(&$effects)
            }
            fn stage_effects_mut(&mut self) -> Option<&mut Vec<Effect>> {
                let $s = self;
                Some(&mut $effects)
            }
            $(fn stage_accepts(&self, $e: &TellersEffect) -> bool {
                let $a = self;
                $accepts
            })?
        }
    };
}

impl_effect_stage!(Clip, |c| c.effects, accepts |c, e|
    !e.is_time() || (!c.is_generator_clip() && e.speed_is_playable()));
impl_effect_stage!(CompositeStack, |s| s.effects, accepts |_s, e| !e.is_time());
impl_effect_stage!(Track, |t| t.effects, accepts |t, e| e.allowed_on_track(&t.kind));
// OTIO timelines have no effects of their own; the root stack holds them.
impl_effect_stage!(Timeline, |t| t.tracks.effects, accepts |_t, e| !e.is_time());

/// A clip's or a composite clip's effects; gaps and transitions have none.
impl EffectStage for Item {
    fn stage_effects(&self) -> Option<&[Effect]> {
        match self {
            Item::Clip(clip) => clip.stage_effects(),
            Item::Composite(stack) => stack.stage_effects(),
            Item::Gap(_) | Item::Transition(_) => None,
        }
    }
    fn stage_effects_mut(&mut self) -> Option<&mut Vec<Effect>> {
        match self {
            Item::Clip(clip) => clip.stage_effects_mut(),
            Item::Composite(stack) => stack.stage_effects_mut(),
            Item::Gap(_) | Item::Transition(_) => None,
        }
    }
    fn stage_accepts(&self, effect: &TellersEffect) -> bool {
        match self {
            Item::Clip(clip) => clip.stage_accepts(effect),
            Item::Composite(stack) => stack.stage_accepts(effect),
            Item::Gap(_) | Item::Transition(_) => false,
        }
    }
}

/// Generates the effect list accessors for an [`EffectStage`], so the
/// timeline, track and clip stages stay identical.
macro_rules! impl_tellers_effects {
    ($ty:ty, $stage:literal) => {
        impl $ty {
            #[doc = concat!("The tellers effects of ", $stage, ", in order.")]
            pub fn get_tellers_effects(&self) -> Vec<TellersEffect> {
                self.stage_effects().map(resolve_tellers_effects).unwrap_or_default()
            }

            #[doc = concat!("The first tellers effect of ", $stage, " named `effect_name`, with its index.")]
            pub fn find_tellers_effect(&self, effect_name: &str) -> Option<(usize, TellersEffect)> {
                self.get_tellers_effects()
                    .into_iter()
                    .enumerate()
                    .find(|(_, effect)| effect.effect_name == bare_effect_name(effect_name))
            }

            #[doc = concat!("Replace every tellers effect of ", $stage, "; see [`set_tellers_effects`].")]
            pub fn set_tellers_effects(&mut self, effects: Vec<TellersEffect>) -> bool {
                effects.iter().all(|e| self.stage_accepts(e))
                    && self.stage_effects_mut().is_some_and(|e| set_tellers_effects(e, effects))
            }

            #[doc = concat!("Append a tellers effect to ", $stage, "; see [`push_tellers_effect`].")]
            pub fn push_tellers_effect(&mut self, effect: TellersEffect) -> bool {
                self.stage_accepts(&effect)
                    && self.stage_effects_mut().is_some_and(|e| push_tellers_effect(e, effect))
            }

            #[doc = concat!("Insert a tellers effect into ", $stage, "; see [`insert_tellers_effect_at`].")]
            pub fn insert_tellers_effect_at(&mut self, index: usize, effect: TellersEffect) -> bool {
                self.stage_accepts(&effect)
                    && self.stage_effects_mut().is_some_and(|e| insert_tellers_effect_at(e, index, effect))
            }

            #[doc = concat!("Replace a tellers effect of ", $stage, "; see [`replace_tellers_effect_at`].")]
            pub fn replace_tellers_effect_at(
                &mut self,
                index: usize,
                effect: TellersEffect,
            ) -> Option<TellersEffect> {
                if !self.stage_accepts(&effect) {
                    return None;
                }
                replace_tellers_effect_at(self.stage_effects_mut()?, index, effect)
            }

            #[doc = concat!("Remove a tellers effect of ", $stage, "; see [`remove_tellers_effect_at`].")]
            pub fn remove_tellers_effect_at(&mut self, index: usize) -> Option<TellersEffect> {
                remove_tellers_effect_at(self.stage_effects_mut()?, index)
            }

            #[doc = concat!("Remove every tellers effect of ", $stage, "; see [`clear_tellers_effects`].")]
            pub fn clear_tellers_effects(&mut self) -> bool {
                self.stage_effects_mut().is_some_and(clear_tellers_effects)
            }

            #[doc = concat!("Turn a tellers effect of ", $stage, " on or off; see [`set_tellers_effect_enabled`].")]
            pub fn set_tellers_effect_enabled(&mut self, index: usize, enabled: bool) -> bool {
                self.stage_effects_mut().is_some_and(|e| set_tellers_effect_enabled(e, index, enabled))
            }

            #[doc = concat!("Set a parameter of a tellers effect of ", $stage, "; see [`set_tellers_effect_parameter`].")]
            pub fn set_tellers_effect_parameter(
                &mut self,
                index: usize,
                key: &str,
                value: Option<Value>,
            ) -> bool {
                self.stage_effects_mut().is_some_and(|e| set_tellers_effect_parameter(e, index, key, value))
            }
        }
    };
}

impl_tellers_effects!(Timeline, "the timeline (applied over the whole composited image)");
impl_tellers_effects!(Track, "this track (applied to each of its clips)");
impl_tellers_effects!(Clip, "this clip");
impl_tellers_effects!(
    CompositeStack,
    "this composite clip (applied to its composited image)"
);
impl_tellers_effects!(Item, "this clip or composite clip (none for a gap)");

/// Generates the by-id accessors of [`Timeline`] for a stage found by id
/// (`$find` / `$find_mut` return `Option<&Stage>` / `Option<&mut Stage>`).
macro_rules! impl_timeline_stage_effects {
    ($what:literal, $find:ident, $find_mut:ident, $accepts:ident,
     $get:ident, $set:ident, $push:ident, $insert:ident, $replace:ident,
     $remove:ident, $clear:ident, $enable:ident, $param:ident) => {
        impl Timeline {
            #[doc = concat!("The tellers effects of the ", $what, " `id`; `None` when no ", $what, " has this id.")]
            pub fn $get(&self, id: &str) -> Option<Vec<TellersEffect>> {
                Some(self.$find(id)?.get_tellers_effects())
            }

            #[doc = concat!("`set_tellers_effects` on the ", $what, " `id`; `false` when it is unknown.")]
            pub fn $set(&mut self, id: &str, effects: Vec<TellersEffect>) -> bool {
                effects.iter().all(|e| self.$accepts(id, e))
                    && self.$find_mut(id).is_some_and(|s| s.set_tellers_effects(effects))
            }

            #[doc = concat!("`push_tellers_effect` on the ", $what, " `id`; `false` when it is unknown.")]
            pub fn $push(&mut self, id: &str, effect: TellersEffect) -> bool {
                self.$accepts(id, &effect)
                    && self.$find_mut(id).is_some_and(|s| s.push_tellers_effect(effect))
            }

            #[doc = concat!("`insert_tellers_effect_at` on the ", $what, " `id`; `false` when it is unknown.")]
            pub fn $insert(&mut self, id: &str, index: usize, effect: TellersEffect) -> bool {
                self.$accepts(id, &effect)
                    && self.$find_mut(id)
                        .is_some_and(|s| s.insert_tellers_effect_at(index, effect))
            }

            #[doc = concat!("`replace_tellers_effect_at` on the ", $what, " `id`.")]
            pub fn $replace(
                &mut self,
                id: &str,
                index: usize,
                effect: TellersEffect,
            ) -> Option<TellersEffect> {
                if !self.$accepts(id, &effect) {
                    return None;
                }
                self.$find_mut(id)?.replace_tellers_effect_at(index, effect)
            }

            #[doc = concat!("`remove_tellers_effect_at` on the ", $what, " `id`.")]
            pub fn $remove(&mut self, id: &str, index: usize) -> Option<TellersEffect> {
                self.$find_mut(id)?.remove_tellers_effect_at(index)
            }

            #[doc = concat!("`clear_tellers_effects` on the ", $what, " `id`.")]
            pub fn $clear(&mut self, id: &str) -> bool {
                self.$find_mut(id).is_some_and(|s| s.clear_tellers_effects())
            }

            #[doc = concat!("`set_tellers_effect_enabled` on the ", $what, " `id`.")]
            pub fn $enable(&mut self, id: &str, index: usize, enabled: bool) -> bool {
                self.$find_mut(id)
                    .is_some_and(|s| s.set_tellers_effect_enabled(index, enabled))
            }

            #[doc = concat!("`set_tellers_effect_parameter` on the ", $what, " `id`.")]
            pub fn $param(
                &mut self,
                id: &str,
                index: usize,
                key: &str,
                value: Option<Value>,
            ) -> bool {
                self.$find_mut(id)
                    .is_some_and(|s| s.set_tellers_effect_parameter(index, key, value))
            }
        }
    };
}

impl_timeline_stage_effects!(
    "track",
    effect_track,
    effect_track_mut,
    track_accepts_effect,
    get_track_tellers_effects,
    set_track_tellers_effects,
    push_track_tellers_effect,
    insert_track_tellers_effect_at,
    replace_track_tellers_effect_at,
    remove_track_tellers_effect_at,
    clear_track_tellers_effects,
    set_track_tellers_effect_enabled,
    set_track_tellers_effect_parameter
);

impl_timeline_stage_effects!(
    "clip or composite clip",
    effect_item,
    effect_item_mut,
    item_accepts_effect,
    get_item_tellers_effects,
    set_item_tellers_effects,
    push_item_tellers_effect,
    insert_item_tellers_effect_at,
    replace_item_tellers_effect_at,
    remove_item_tellers_effect_at,
    clear_item_tellers_effects,
    set_item_tellers_effect_enabled,
    set_item_tellers_effect_parameter
);

impl Timeline {
    /// The kind of the track holding the clip `item_id` (at any depth, inside
    /// composite clips too), to tell which [`EffectTarget`]s apply to it.
    pub fn item_track_kind(&self, item_id: &str) -> Option<TrackKind> {
        Some(self.tracks.get_nested_item_track(item_id)?.kind.clone())
    }

    /// Whether `effect` may go on the track `track_id`: the track checks
    /// its own kind, so this only needs the track to exist.
    fn track_accepts_effect(&self, track_id: &str, effect: &TellersEffect) -> bool {
        self.effect_track(track_id).is_some_and(|t| t.stage_accepts(effect))
    }

    /// Whether `effect` may go on the clip or composite clip `item_id`,
    /// given the kind of the track holding it (see
    /// [`EffectTarget::allowed_on_item`]): no Volume on a video clip, no
    /// Blur on an audio clip, no TextWave on anything but a text clip. Inside
    /// a composite clip, no speed and no `ColorLut`: the player draws
    /// neither there.
    fn item_accepts_effect(&self, item_id: &str, effect: &TellersEffect) -> bool {
        let (Some(kind), Some(item), Some(spec)) =
            (self.item_track_kind(item_id), self.effect_item(item_id), effect.spec())
        else {
            return false;
        };
        let nested = self.tracks.composite_path(item_id).is_some_and(|path| !path.is_empty());
        spec.target.allowed_on_item(&kind, item)
            && item.stage_accepts(effect)
            && !(nested && (spec.target == EffectTarget::Time || spec.name == "ColorLut"))
    }

    fn effect_track(&self, track_id: &str) -> Option<&Track> {
        Some(self.tracks.get_track_by_id(track_id)?.1)
    }

    fn effect_track_mut(&mut self, track_id: &str) -> Option<&mut Track> {
        let (index, _) = self.tracks.get_track_by_id(track_id)?;
        self.tracks.children.get_mut(index)
    }

    /// The clip or composite clip `item_id`, at any depth (effects do not
    /// change lengths, so nested items need no [`crate::Stack::with_composite_mut`]);
    /// gaps hold no effects.
    fn effect_item(&self, item_id: &str) -> Option<&Item> {
        self.tracks
            .get_nested_item(item_id)
            .filter(|item| item.stage_effects().is_some())
    }

    fn effect_item_mut(&mut self, item_id: &str) -> Option<&mut Item> {
        self.tracks
            .get_nested_item_mut(item_id)
            .filter(|item| item.stage_effects().is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_clip() -> Clip {
        Clip::new(crate::TimeRange::new(1.0, 0.0), Default::default(), None, None, None)
    }

    fn effect_json(value: Value) -> Effect {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn effect_keeps_enabled_and_extra_keys() {
        let raw = json!({
            "OTIO_SCHEMA": "LinearTimeWarp.1",
            "name": "",
            "effect_name": "",
            "enabled": false,
            "time_scalar": 2.0,
            "metadata": {}
        });
        let effect = effect_json(raw.clone());
        assert_eq!(effect.enabled, Some(false));
        assert_eq!(serde_json::to_value(&effect).unwrap(), raw);
    }

    #[test]
    fn reads_parameters_and_enabled() {
        let effect = effect_json(json!({
            "OTIO_SCHEMA": "Effect.1",
            "effect_name": "Blur",
            "enabled": false,
            "metadata": { "tellers.ai": { "parameters": {
                "strength": { "keyframes": [ { "t": 1.0, "v": 8.0 }, { "t": 0.0, "v": 0.0, "ease": "easeOut" } ] }
            } } }
        }));
        let tellers = TellersEffect::from_effect(&effect).unwrap();
        assert!(!tellers.enabled);
        assert_eq!(tellers.name, None);
        let strength = tellers.animatable("strength").unwrap();
        // Sorted by time, like the player.
        assert_eq!(
            strength,
            Animatable::from(vec![
                Keyframe::new(0.0, 0.0).with_ease(Ease::EaseOut),
                Keyframe::new(1.0, 8.0)
            ])
        );
        assert_eq!(strength.value_at(-1.0), 0.0);
        assert_eq!(strength.value_at(0.5), 8.0 * Ease::EaseOut.apply(0.5));
        assert_eq!(strength.value_at(3.0), 8.0);
    }

    #[test]
    fn writes_prefixed_names_and_reads_both() {
        let prefixed = effect_json(json!({ "OTIO_SCHEMA": "Effect.1", "effect_name": "tellers.ai.Pan" }));
        let bare = effect_json(json!({ "OTIO_SCHEMA": "Effect.1", "effect_name": "Pan" }));
        assert_eq!(TellersEffect::from_effect(&prefixed).unwrap().effect_name, "Pan");
        assert_eq!(TellersEffect::from_effect(&bare).unwrap().effect_name, "Pan");
        assert_eq!(TellersEffect::new("Pan").to_effect().effect_name, "tellers.ai.Pan");
        assert_eq!(
            TellersEffect::new("tellers.ai.Pan").to_effect().effect_name,
            "tellers.ai.Pan"
        );

        let mut clip = test_clip();
        clip.effects = vec![bare];
        assert!(clip.set_tellers_effect_enabled(0, false));
        assert_eq!(clip.effects[0].effect_name, "tellers.ai.Pan");
        assert_eq!(clip.find_tellers_effect("tellers.ai.Pan").unwrap().0, 0);
        assert!(TellersEffect::new("TimeWarp")
            .with_parameter("preserve_pitch", false)
            .validate()
            .is_ok());
    }

    #[test]
    fn hold_keeps_value_until_next_keyframe() {
        let a = Animatable::from(vec![
            Keyframe::new(0.0, 1.0).with_ease(Ease::Hold),
            Keyframe::new(1.0, 5.0),
        ]);
        assert_eq!(a.value_at(0.99), 1.0);
        assert_eq!(a.value_at(1.0), 5.0);
    }

    #[test]
    fn non_tellers_effects_are_skipped_and_kept() {
        let other = effect_json(json!({
            "OTIO_SCHEMA": "LinearTimeWarp.1",
            "name": "",
            "effect_name": "",
            "time_scalar": 2.0,
            "metadata": {}
        }));
        let mut clip = test_clip();
        clip.effects = vec![other.clone()];
        assert!(clip.get_tellers_effects().is_empty());

        assert!(clip.push_tellers_effect(TellersEffect::new("Blur").with_parameter("strength", 4.0)));
        assert!(clip.insert_tellers_effect_at(0, TellersEffect::new("Pixelate")));
        assert_eq!(clip.effects[0], other);
        let names: Vec<_> = clip
            .get_tellers_effects()
            .into_iter()
            .map(|e| e.effect_name)
            .collect();
        assert_eq!(names, ["Pixelate", "Blur"]);

        assert!(!clip.push_tellers_effect(TellersEffect::new("NotAnEffect")));
        assert!(!clip.insert_tellers_effect_at(5, TellersEffect::new("Blur")));

        let removed = clip.remove_tellers_effect_at(0).unwrap();
        assert_eq!(removed.effect_name, "Pixelate");
        assert!(clip.clear_tellers_effects());
        assert_eq!(clip.effects, vec![other]);
    }

    #[test]
    fn replace_keeps_other_keys() {
        let mut clip = test_clip();
        clip.effects = vec![effect_json(json!({
            "OTIO_SCHEMA": "Effect.1",
            "effect_name": "Blur",
            "metadata": { "tellers.ai": { "parameters": { "strength": 2 }, "note": "keep" }, "other": 1 }
        }))];
        let previous = clip
            .replace_tellers_effect_at(0, TellersEffect::new("Pixelate").with_parameter("size", 10.0))
            .unwrap();
        assert_eq!(previous.effect_name, "Blur");
        let raw = serde_json::to_value(&clip.effects[0]).unwrap();
        assert_eq!(raw["effect_name"], "tellers.ai.Pixelate");
        assert_eq!(raw["metadata"]["tellers.ai"]["note"], "keep");
        assert_eq!(raw["metadata"]["other"], 1);
        assert_eq!(raw["metadata"]["tellers.ai"]["parameters"], json!({ "size": 10.0 }));
    }

    #[test]
    fn enable_and_parameter_writes() {
        let mut track = Track { kind: TrackKind::Audio, ..Track::default() };
        assert!(track.push_tellers_effect(TellersEffect::new("Volume")));
        assert!(track.set_tellers_effect_parameter(
            0,
            "volume",
            Some(Animatable::from(vec![Keyframe::new(0.0, 0.0), Keyframe::new(2.0, 1.0)]).to_value())
        ));
        assert!(track.set_tellers_effect_enabled(0, false));
        let raw = serde_json::to_value(&track.effects[0]).unwrap();
        assert_eq!(raw["enabled"], false);
        assert!(track.set_tellers_effect_enabled(0, true));
        let raw = serde_json::to_value(&track.effects[0]).unwrap();
        assert!(raw.get("enabled").is_none());
        let (index, volume) = track.find_tellers_effect("Volume").unwrap();
        assert_eq!(index, 0);
        assert_eq!(volume.value_at("volume", 1.0), Some(0.5));
        assert!(track.set_tellers_effect_parameter(0, "volume", None));
        assert!(track.get_tellers_effects()[0].parameters.is_empty());
        assert!(!track.set_tellers_effect_enabled(3, true));
    }

    #[test]
    fn validate_checks_names_and_types() {
        assert!(TellersEffect::new("ChromaKey")
            .with_parameter("color", "#00ff00")
            .with_parameter("similarity", 0.4)
            .validate()
            .is_ok());
        assert!(TellersEffect::new("Nope").validate().is_err());
        assert!(TellersEffect::new("Blur").with_parameter("size", 1.0).validate().is_err());
        assert!(TellersEffect::new("Blur").with_parameter("strength", "x").validate().is_err());
        assert!(TellersEffect::new("Blend").with_parameter("mode", "Multiply").validate().is_ok());
        assert!(TellersEffect::new("Blend").with_parameter("mode", "burn").validate().is_err());
        assert!(TellersEffect::new("Mask")
            .with_parameter("shape", "path")
            .with_parameter(
                "points",
                json!([[0.1, 0.2], { "x": 0.5, "y": { "keyframes": [ { "t": 0, "v": 0.1 } ] }, "in_x": 0.4 }])
            )
            .validate()
            .is_ok());
        assert!(TellersEffect::new("Mask")
            .with_parameter("points", json!([[0.1]]))
            .validate()
            .is_err());
        // What `VideoPlayer.stabilize` writes: one held keyframe per frame.
        let held = json!({ "keyframes": [
            { "t": 0, "v": 0.01, "ease": "hold" },
            { "t": 0.04, "v": -0.02, "ease": "hold" }
        ] });
        assert!(TellersEffect::new("Stabilize")
            .with_parameter("x", held.clone())
            .with_parameter("y", held.clone())
            .with_parameter("rotation", held)
            .with_parameter("scale", 1.06)
            .validate()
            .is_ok());
        assert!(TellersEffect::new("Stabilize").with_parameter("opacity", 1.0).validate().is_err());
        assert!(!TellersEffect::new("Stabilize").allowed_on(&TrackKind::Audio));
    }

    #[test]
    fn audio_chain_effects_are_audio_and_bands_are_checked() {
        for name in ["Equalizer", "Compressor", "Limiter"] {
            assert!(TellersEffect::new(name).allowed_on(&TrackKind::Audio), "{name}");
            assert!(!TellersEffect::new(name).allowed_on(&TrackKind::Video), "{name}");
        }
        assert!(TellersEffect::new("Compressor")
            .with_parameter("threshold", -30.0)
            .with_parameter("ratio", 4.0)
            .with_parameter("makeup", json!({ "keyframes": [ { "t": 0, "v": 0 }, { "t": 1, "v": 12 } ] }))
            .validate()
            .is_ok());
        assert!(TellersEffect::new("Compressor").with_parameter("volume", 1.0).validate().is_err());
        assert!(TellersEffect::new("Limiter").with_parameter("ceiling", -1.0).validate().is_ok());
        assert!(TellersEffect::new("Pitch").allowed_on(&TrackKind::Audio));
        assert!(!TellersEffect::new("Pitch").allowed_on(&TrackKind::Video));
        assert!(TellersEffect::new("Pitch")
            .with_parameter("semitones", json!({ "keyframes": [ { "t": 0, "v": -12 }, { "t": 2, "v": 12 } ] }))
            .validate()
            .is_ok());
        assert!(TellersEffect::new("Pitch").with_parameter("pitch", 3.0).validate().is_err());
        let bands = json!([
            { "type": "highpass", "frequency": { "keyframes": [ { "t": 0, "v": 900 }, { "t": 1.5, "v": 90, "ease": "easeOut" } ] } },
            { "type": "peaking", "frequency": 3500, "gain": 4, "q": 1 }
        ]);
        assert!(TellersEffect::new("Equalizer").with_parameter("bands", bands).validate().is_ok());
        assert!(TellersEffect::new("Equalizer").with_parameter("bands", json!([])).validate().is_ok());
        // A band needs a known type, and only its numbers.
        for bad in [
            json!([{ "frequency": 100 }]),
            json!([{ "type": "notch", "frequency": 100 }]),
            json!([{ "type": "peaking", "gain": "loud" }]),
            json!([{ "type": "peaking", "width": 1 }]),
            json!({ "type": "peaking" }),
        ] {
            assert!(TellersEffect::new("Equalizer").with_parameter("bands", bad.clone()).validate().is_err(), "{bad}");
        }
    }

    #[test]
    fn filters_take_mask_shaped_regions() {
        // The player's face-blur example: one keyframed ellipse.
        let face = json!({
            "shape": "ellipse", "width": 0.12, "height": 0.3, "feather": 20,
            "x": { "keyframes": [ { "t": 0, "v": -0.1 }, { "t": 2, "v": 0.05 } ] }, "y": -0.15
        });
        let blur = |regions: Value| {
            TellersEffect::new("Blur")
                .with_parameter("strength", 40.0)
                .with_parameter("regions", regions)
                .validate()
        };
        assert!(blur(json!([face])).is_ok());
        assert!(blur(json!([])).is_ok());
        assert!(blur(json!([{ "shape": "path", "points": [[0.1, 0.2], [0.3, 0.4]] }])).is_ok());
        assert!(blur(json!([{ "shape": "star" }])).is_err());
        assert!(blur(json!([{ "track": "v1" }])).is_err());
        assert!(blur(json!([{ "x": "left" }])).is_err());
        assert!(blur(json!(face)).is_err());
        assert!(TellersEffect::new("ChromaKey").with_parameter("regions", json!([])).validate().is_ok());
        // Only the filters: not Transform, Border, ColorLut or Stabilize.
        for name in ["Transform", "Border", "ColorLut", "Stabilize", "Mask"] {
            assert!(effect_spec(name).unwrap().parameter("regions").is_none(), "{name}");
        }
    }

    #[test]
    fn timeline_stages_by_id() {
        let timeline_json = json!({
            "OTIO_SCHEMA": "Timeline.1",
            "tracks": {
                "OTIO_SCHEMA": "Stack.1",
                "children": [{
                    "OTIO_SCHEMA": "Track.1",
                    "kind": "Video",
                    "metadata": { "tellers.ai": { "timeline_id": "t1" } },
                    "children": [{
                        "OTIO_SCHEMA": "Clip.2",
                        "metadata": { "tellers.ai": { "timeline_id": "c1" } },
                        "source_range": {
                            "OTIO_SCHEMA": "TimeRange.1",
                            "start_time": { "OTIO_SCHEMA": "RationalTime.1", "value": 0, "rate": 1 },
                            "duration": { "OTIO_SCHEMA": "RationalTime.1", "value": 1, "rate": 1 }
                        }
                    }]
                }]
            }
        });
        let mut timeline: Timeline = serde_json::from_value(timeline_json).unwrap();
        assert!(timeline.push_tellers_effect(TellersEffect::new("Noise")));
        assert!(timeline.push_track_tellers_effect("t1", TellersEffect::new("Blend").with_parameter("mode", "screen")));
        assert!(timeline.push_item_tellers_effect("c1", TellersEffect::new("ChromaKey")));
        assert!(!timeline.push_item_tellers_effect("missing", TellersEffect::new("Blur")));
        assert_eq!(timeline.item_track_kind("c1"), Some(TrackKind::Video));

        let reparsed: Timeline =
            serde_json::from_str(&serde_json::to_string(&timeline).unwrap()).unwrap();
        assert_eq!(reparsed.get_tellers_effects()[0].effect_name, "Noise");
        assert_eq!(
            reparsed.get_track_tellers_effects("t1").unwrap()[0].parameters["mode"],
            "screen"
        );
        assert_eq!(
            reparsed.get_item_tellers_effects("c1").unwrap()[0].effect_name,
            "ChromaKey"
        );
        assert!(timeline.set_item_tellers_effect_enabled("c1", 0, false));
        assert!(!timeline.get_item_tellers_effects("c1").unwrap()[0].enabled);
        assert!(timeline.clear_track_tellers_effects("t1"));
        assert_eq!(timeline.get_track_tellers_effects("t1"), Some(vec![]));
    }

    #[test]
    fn effects_go_on_their_kind_of_track() {
        assert!(EffectTarget::Video.allowed_on(&TrackKind::Video));
        assert!(EffectTarget::Video.allowed_on(&TrackKind::Other));
        assert!(!EffectTarget::Video.allowed_on(&TrackKind::Audio));
        assert!(!EffectTarget::Text.allowed_on(&TrackKind::Audio));
        assert!(EffectTarget::Audio.allowed_on(&TrackKind::Audio));
        assert!(!EffectTarget::Audio.allowed_on(&TrackKind::Video));
        assert!(!EffectTarget::Audio.allowed_on(&TrackKind::Other));
        assert!(EffectTarget::Time.allowed_on(&TrackKind::Audio));
        assert!(EffectTarget::Time.allowed_on(&TrackKind::Video));
        assert!(!TellersEffect::new("Nope").allowed_on(&TrackKind::Video));

        let mut video = Track::default();
        assert!(!video.push_tellers_effect(TellersEffect::new("Volume")));
        assert!(!video.set_tellers_effects(vec![TellersEffect::new("Blur"), TellersEffect::new("Pan")]));
        assert!(video.push_tellers_effect(TellersEffect::new("Blur")));
        assert!(video.replace_tellers_effect_at(0, TellersEffect::new("Pan")).is_none());
        assert!(!video.insert_tellers_effect_at(0, TellersEffect::new("Pan")));
        assert_eq!(video.get_tellers_effects().len(), 1);

        let mut audio = Track { kind: TrackKind::Audio, ..Track::default() };
        assert!(!audio.push_tellers_effect(TellersEffect::new("Blur")));
        assert!(audio.push_tellers_effect(TellersEffect::new("Pan")));
        // Speed goes on clips of any track, never on a track itself.
        assert!(!audio.push_tellers_effect(TellersEffect::new("TimeWarp")));
        assert!(!video.push_tellers_effect(TellersEffect::new("TimeWarp")));
        assert!(!EffectTarget::Time.allowed_on_track(&TrackKind::Audio));
        assert!(EffectTarget::Video.allowed_on_track(&TrackKind::Video));

        let clip = |id: &str| json!({
            "OTIO_SCHEMA": "Clip.2",
            "metadata": { "tellers.ai": { "timeline_id": id } },
            "source_range": {
                "OTIO_SCHEMA": "TimeRange.1",
                "start_time": { "OTIO_SCHEMA": "RationalTime.1", "value": 0, "rate": 1 },
                "duration": { "OTIO_SCHEMA": "RationalTime.1", "value": 1, "rate": 1 }
            }
        });
        let mut timeline: Timeline = serde_json::from_value(json!({
            "OTIO_SCHEMA": "Timeline.1",
            "tracks": {
                "OTIO_SCHEMA": "Stack.1",
                "children": [
                    { "OTIO_SCHEMA": "Track.1", "kind": "Video",
                      "metadata": { "tellers.ai": { "timeline_id": "v" } }, "children": [clip("vc")] },
                    { "OTIO_SCHEMA": "Track.1", "kind": "Audio",
                      "metadata": { "tellers.ai": { "timeline_id": "a" } }, "children": [clip("ac")] }
                ]
            }
        }))
        .unwrap();
        assert!(!timeline.push_track_tellers_effect("v", TellersEffect::new("Volume")));
        assert!(!timeline.push_track_tellers_effect("a", TellersEffect::new("Blur")));
        assert!(timeline.push_track_tellers_effect("a", TellersEffect::new("Volume")));
        assert!(!timeline.push_item_tellers_effect("vc", TellersEffect::new("Volume")));
        assert!(!timeline.insert_item_tellers_effect_at("ac", 0, TellersEffect::new("Transform")));
        assert!(!timeline.set_item_tellers_effects("ac", vec![TellersEffect::new("Mask")]));
        assert!(timeline.push_item_tellers_effect("vc", TellersEffect::new("Blur")));
        assert!(timeline.push_item_tellers_effect("ac", TellersEffect::new("Pan")));
        assert!(timeline.replace_item_tellers_effect_at("ac", 0, TellersEffect::new("Blur")).is_none());
        assert!(timeline.replace_item_tellers_effect_at("vc", 0, TellersEffect::new("Volume")).is_none());
        assert_eq!(timeline.get_item_tellers_effects("vc").unwrap()[0].effect_name, "Blur");
        assert_eq!(timeline.get_item_tellers_effects("ac").unwrap()[0].effect_name, "Pan");

        // Text effects on text clips only; picture effects on text clips too.
        let text = Clip::new_text(
            1.0,
            &crate::TextParameters::with_text("Title"),
            None,
            Some("tc".to_string()),
        )
        .unwrap();
        timeline.tracks.children[0].items.push(Item::Clip(text));
        assert!(!timeline.push_item_tellers_effect("vc", TellersEffect::new("TextWave")));
        assert!(!timeline.push_item_tellers_effect("ac", TellersEffect::new("TextWave")));
        assert!(timeline.push_item_tellers_effect("tc", TellersEffect::new("TextWave")));
        assert!(timeline.push_item_tellers_effect("tc", TellersEffect::new("DropShadow")));
        assert!(!timeline.push_item_tellers_effect("tc", TellersEffect::new("Volume")));
        assert!(!timeline.set_item_tellers_effects("vc", vec![TellersEffect::new("TextKaraoke")]));
        assert!(timeline.replace_item_tellers_effect_at("vc", 0, TellersEffect::new("TextColor")).is_none());
        // A track applies its text effects to its text clips.
        assert!(timeline.push_track_tellers_effect("v", TellersEffect::new("TextWave")));
        assert!(!timeline.push_track_tellers_effect("a", TellersEffect::new("TextWave")));
        assert!(EffectTarget::Text.allowed_on_item(
            &TrackKind::Video,
            timeline.tracks.get_nested_item("tc").unwrap()
        ));
        assert!(!EffectTarget::Text.allowed_on_item(
            &TrackKind::Video,
            timeline.tracks.get_nested_item("vc").unwrap()
        ));
    }

    #[test]
    fn composite_items_hold_effects() {
        let range = json!({
            "OTIO_SCHEMA": "TimeRange.1",
            "start_time": { "OTIO_SCHEMA": "RationalTime.1", "value": 0, "rate": 1 },
            "duration": { "OTIO_SCHEMA": "RationalTime.1", "value": 2, "rate": 1 }
        });
        let timeline_json = json!({
            "OTIO_SCHEMA": "Timeline.1",
            "tracks": {
                "OTIO_SCHEMA": "Stack.1",
                "children": [{
                    "OTIO_SCHEMA": "Track.1",
                    "kind": "Video",
                    "children": [
                        {
                            "OTIO_SCHEMA": "Gap.1",
                            "metadata": { "tellers.ai": { "timeline_id": "g1" } },
                            "source_range": range
                        },
                        {
                            "OTIO_SCHEMA": "Stack.1",
                            "metadata": { "tellers.ai": { "timeline_id": "s1" } },
                            "effects": [
                                { "OTIO_SCHEMA": "LinearTimeWarp.1", "name": "", "effect_name": "",
                                  "time_scalar": 2.0, "metadata": {} },
                                { "OTIO_SCHEMA": "Effect.1", "effect_name": "tellers.ai.Blur",
                                  "metadata": { "tellers.ai": { "parameters": { "strength": 3 } } } }
                            ],
                            "children": [{
                                "OTIO_SCHEMA": "Track.1",
                                "kind": "Video",
                                "children": [{ "OTIO_SCHEMA": "Gap.1", "source_range": range }]
                            }]
                        }
                    ]
                }]
            }
        });
        let mut timeline: Timeline = serde_json::from_value(timeline_json).unwrap();

        let effects = timeline.get_item_tellers_effects("s1").unwrap();
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].value_at("strength", 0.0), Some(3.0));
        assert_eq!(timeline.item_track_kind("s1"), Some(TrackKind::Video));

        // The effects the player applies to a composite: filters, Transform, Blend.
        assert!(timeline.push_item_tellers_effect(
            "s1",
            TellersEffect::new("Transform").with_parameter("scale", 0.5)
        ));
        assert!(timeline.insert_item_tellers_effect_at(
            "s1",
            0,
            TellersEffect::new("Blend").with_parameter("mode", "multiply")
        ));
        assert!(timeline.set_item_tellers_effect_parameter("s1", 2, "rotation", Some(json!(90.0))));
        assert!(timeline.set_item_tellers_effect_enabled("s1", 1, false));
        let replaced = timeline
            .replace_item_tellers_effect_at("s1", 1, TellersEffect::new("Pixelate"))
            .unwrap();
        assert_eq!(replaced.effect_name, "Blur");

        let reparsed: Timeline =
            serde_json::from_str(&serde_json::to_string(&timeline).unwrap()).unwrap();
        let names: Vec<_> = reparsed
            .get_item_tellers_effects("s1")
            .unwrap()
            .into_iter()
            .map(|e| e.effect_name)
            .collect();
        assert_eq!(names, ["Blend", "Pixelate", "Transform"]);
        let Item::Composite(stack) = &reparsed.tracks.children[0].items[1] else {
            panic!("expected a composite");
        };
        assert_eq!(stack.effects.len(), 4);
        assert_eq!(stack.find_tellers_effect("Transform").unwrap().1.parameters["rotation"], 90.0);

        assert_eq!(
            timeline.remove_item_tellers_effect_at("s1", 0).unwrap().effect_name,
            "Blend"
        );
        assert!(timeline.clear_item_tellers_effects("s1"));
        assert_eq!(timeline.get_item_tellers_effects("s1"), Some(vec![]));
        let Item::Composite(stack) = &timeline.tracks.children[0].items[1] else {
            panic!("expected a composite");
        };
        // The time warp is not a tellers effect and stays.
        assert_eq!(stack.effects.len(), 1);

        // Gaps hold no effects.
        assert_eq!(timeline.get_item_tellers_effects("g1"), None);
        assert!(!timeline.push_item_tellers_effect("g1", TellersEffect::new("Blur")));
        let gap = &mut timeline.tracks.children[0].items[0];
        assert!(gap.get_tellers_effects().is_empty());
        assert!(!gap.push_tellers_effect(TellersEffect::new("Blur")));
        assert!(gap.remove_tellers_effect_at(0).is_none());
    }
}
