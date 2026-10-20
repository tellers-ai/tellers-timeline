//! Clip speed: constant speed, freeze frames and speed ramps.
//!
//! The player reads three kinds of time effect, all as one `TimeWarp`
//! (video-player-js `clip-speed.ts`), and applies the first enabled one:
//!
//! - OTIO's `LinearTimeWarp.1`, whose `time_scalar` is the speed (2 is twice
//!   as fast);
//! - OTIO's `FreezeFrame.1`, a speed of 0: the clip holds its first frame,
//!   silent;
//! - a `tellers.ai.TimeWarp` effect, whose `speed` parameter may be
//!   keyframed (a ramp).
//!
//! `preserve_pitch` (time-stretched audio, the default; `false` resamples)
//! is read from `metadata["tellers.ai"].parameters`, or beside it.
//!
//! A clip keeps its `source_range.duration` on the timeline and plays the
//! integral of its speed over that duration from `source_range.start_time`.
//! The speed is clamped to [`MIN_CLIP_SPEED`]..=[`MAX_CLIP_SPEED`]; reverse is
//! not supported (the player ignores a negative speed).

use serde_json::{Map, Value};

use crate::{Animatable, Clip, Effect, Keyframe, Seconds, TellersEffect};

pub const MIN_CLIP_SPEED: f64 = 0.25;
pub const MAX_CLIP_SPEED: f64 = 4.0;

pub const LINEAR_TIME_WARP_SCHEMA: &str = "LinearTimeWarp.1";
pub const FREEZE_FRAME_SCHEMA: &str = "FreezeFrame.1";
const TIME_WARP: &str = "TimeWarp";
const PRESERVE_PITCH: &str = "preserve_pitch";

/// The speed of a clip, as the player plays it.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipSpeed {
    /// A constant or keyframed speed; 0 is a freeze frame.
    pub speed: Animatable,
    /// Audio keeps its pitch (time-stretch) instead of being resampled.
    pub preserve_pitch: bool,
}

impl ClipSpeed {
    pub fn is_freeze(&self) -> bool {
        self.speed == Animatable::Value(0.0)
    }

    /// Source seconds played by the first `t` timeline seconds of the clip.
    pub fn source_offset(&self, t: Seconds) -> Seconds {
        if t <= 0.0 || self.is_freeze() {
            return 0.0;
        }
        match &self.speed {
            Animatable::Value(v) => t * clamp_speed(*v),
            Animatable::Keyframes { keyframes } => ramp_integral(keyframes, t),
        }
    }

    /// The timeline offset at which `source` seconds have played: the
    /// inverse of [`ClipSpeed::source_offset`]. `None` for a freeze, which
    /// plays no source however long it lasts.
    pub fn timeline_offset(&self, source: Seconds) -> Option<Seconds> {
        if self.is_freeze() {
            return None;
        }
        if source <= 0.0 {
            return Some(0.0);
        }
        if let Animatable::Value(v) = self.speed {
            return Some(source / clamp_speed(v));
        }
        // source_offset is strictly increasing (speed >= MIN_CLIP_SPEED).
        let (mut low, mut high) = (0.0, source / MIN_CLIP_SPEED);
        for _ in 0..64 {
            let mid = (low + high) / 2.0;
            if self.source_offset(mid) < source {
                low = mid;
            } else {
                high = mid;
            }
        }
        Some(high)
    }
}

fn clamp_speed(v: f64) -> f64 {
    v.clamp(MIN_CLIP_SPEED, MAX_CLIP_SPEED)
}

/// ∫₀ᵗ speed, the speed clamped like the player does. Each keyframe segment
/// is a cubic (or piecewise cubic) curve, which Simpson's rule integrates
/// exactly; clamping makes it an approximation where the curve crosses a
/// limit. The first and last values hold outside the keyframes.
fn ramp_integral(keyframes: &[Keyframe], t: Seconds) -> Seconds {
    let (Some(first), Some(last)) = (keyframes.first(), keyframes.last()) else {
        return t;
    };
    let mut total = clamp_speed(first.v) * first.t.clamp(0.0, t);
    for pair in keyframes.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let span = b.t - a.t;
        let (from, to) = (a.t.max(0.0), b.t.min(t));
        if span <= 0.0 || to <= from {
            continue;
        }
        let ease = a.ease.unwrap_or_default();
        let speed = |u: f64| clamp_speed(a.v + (b.v - a.v) * ease.apply((u - a.t) / span));
        let mut simpson = |lo: f64, hi: f64| {
            const STEPS: usize = 32;
            let h = (hi - lo) / STEPS as f64;
            let inner: f64 = (1..STEPS)
                .map(|i| if i % 2 == 1 { 4.0 } else { 2.0 } * speed(lo + h * i as f64))
                .sum();
            total += h / 3.0 * (speed(lo) + inner + speed(hi));
        };
        // easeInOut is two cubics, joined at the middle of the segment.
        let middle = a.t + span / 2.0;
        if ease == crate::Ease::EaseInOut && from < middle && middle < to {
            simpson(from, middle);
            simpson(middle, to);
        } else {
            simpson(from, to);
        }
    }
    total + clamp_speed(last.v) * (t - last.t.max(0.0)).max(0.0)
}

fn schema_is(effect: &Effect, schema: &str) -> bool {
    let base = |s: &str| s.split('.').next().unwrap_or_default().to_string();
    base(&effect.otio_schema) == base(schema)
}

/// Whether `effect` is a time effect the player reads (`LinearTimeWarp`,
/// `FreezeFrame` or a tellers `TimeWarp`), enabled or not.
pub fn is_time_warp(effect: &Effect) -> bool {
    schema_is(effect, LINEAR_TIME_WARP_SCHEMA)
        || schema_is(effect, FREEZE_FRAME_SCHEMA)
        || TellersEffect::from_effect(effect).is_some_and(|e| e.effect_name == TIME_WARP)
}

fn preserve_pitch(effect: &Effect) -> bool {
    let tellers = effect.metadata.other.get("tellers.ai");
    let value = tellers
        .and_then(|ai| ai.get("parameters"))
        .and_then(|p| p.get(PRESERVE_PITCH))
        .or_else(|| tellers.and_then(|ai| ai.get(PRESERVE_PITCH)));
    value != Some(&Value::Bool(false))
}

fn read_time_warp(effect: &Effect) -> Option<ClipSpeed> {
    if effect.enabled == Some(false) {
        return None;
    }
    let speed = if schema_is(effect, FREEZE_FRAME_SCHEMA) {
        Animatable::Value(0.0)
    } else if schema_is(effect, LINEAR_TIME_WARP_SCHEMA) {
        Animatable::Value(effect.other.get("time_scalar").and_then(Value::as_f64).unwrap_or(1.0))
    } else {
        TellersEffect::from_effect(effect)
            .filter(|e| e.effect_name == TIME_WARP)?
            .animatable("speed")?
    };
    let speed = match speed {
        Animatable::Value(v) if v == 0.0 => speed,
        Animatable::Value(v) if v < 0.0 => return None,
        Animatable::Keyframes { ref keyframes } if keyframes.iter().any(|k| k.v < 0.0) => {
            return None
        }
        speed => speed,
    };
    Some(ClipSpeed { speed, preserve_pitch: preserve_pitch(effect) })
}

/// The speed the player applies for `effects`: the first enabled time effect
/// it can play, or `None` (normal speed).
pub fn clip_speed(effects: &[Effect]) -> Option<ClipSpeed> {
    effects.iter().find_map(read_time_warp)
}

/// Why a speed could not be set.
#[derive(Debug, Clone, PartialEq)]
pub enum SpeedError {
    /// Not 0 (a freeze) and outside `MIN_CLIP_SPEED..=MAX_CLIP_SPEED`.
    OutOfRange(f64),
}

impl std::fmt::Display for SpeedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpeedError::OutOfRange(speed) => write!(
                f,
                "speed must be 0 (freeze) or between {MIN_CLIP_SPEED} and {MAX_CLIP_SPEED}, got {speed}"
            ),
        }
    }
}

impl std::error::Error for SpeedError {}

/// Set a constant speed on `effects`, replacing every time effect:
/// `FreezeFrame.1` for 0, else `LinearTimeWarp.1` with `time_scalar`. A speed
/// of 1 writes nothing. `preserve_pitch: false` is
/// written in `metadata["tellers.ai"].parameters`; `None` keeps the current
/// setting.
pub fn set_clip_speed(
    effects: &mut Vec<Effect>,
    speed: f64,
    preserve_pitch: Option<bool>,
) -> Result<(), SpeedError> {
    if !(speed == 0.0 || (MIN_CLIP_SPEED..=MAX_CLIP_SPEED).contains(&speed)) {
        return Err(SpeedError::OutOfRange(speed));
    }
    let preserve_pitch = preserve_pitch
        .or_else(|| clip_speed(effects).map(|s| s.preserve_pitch))
        .unwrap_or(true);
    let at = effects.iter().position(is_time_warp).unwrap_or(effects.len());
    effects.retain(|effect| !is_time_warp(effect));
    if speed == 1.0 {
        return Ok(());
    }
    let (schema, name) = if speed == 0.0 {
        (FREEZE_FRAME_SCHEMA, "FreezeFrame")
    } else {
        (LINEAR_TIME_WARP_SCHEMA, "LinearTimeWarp")
    };
    let mut effect = Effect {
        otio_schema: schema.to_string(),
        effect_name: name.to_string(),
        ..Default::default()
    };
    effect.other.insert("time_scalar".to_string(), speed.into());
    if !preserve_pitch {
        let mut parameters = Map::new();
        parameters.insert(PRESERVE_PITCH.to_string(), false.into());
        effect
            .metadata
            .other
            .insert("tellers.ai".to_string(), serde_json::json!({ "parameters": parameters }));
    }
    effects.insert(at.min(effects.len()), effect);
    Ok(())
}

/// Remove every time effect: the clip plays at normal speed. Returns whether
/// one was removed.
pub fn clear_clip_speed(effects: &mut Vec<Effect>) -> bool {
    let before = effects.len();
    effects.retain(|effect| !is_time_warp(effect));
    effects.len() != before
}

impl Clip {
    /// The speed the player plays this clip at, `None` for normal speed.
    pub fn get_speed(&self) -> Option<ClipSpeed> {
        clip_speed(&self.effects)
    }

    /// Play this clip at a constant `speed` (0 freezes its first frame); see
    /// [`set_clip_speed`]. The clip keeps its duration on the timeline, so it
    /// plays more or less of its media.
    pub fn set_speed(&mut self, speed: f64, preserve_pitch: Option<bool>) -> Result<(), SpeedError> {
        set_clip_speed(&mut self.effects, speed, preserve_pitch)
    }

    /// Play this clip at normal speed.
    pub fn clear_speed(&mut self) -> bool {
        clear_clip_speed(&mut self.effects)
    }

    /// Source seconds played by the first `t` seconds of this clip on the
    /// timeline: `t` at normal speed.
    pub fn source_offset(&self, t: Seconds) -> Seconds {
        match self.get_speed() {
            Some(speed) => speed.source_offset(t),
            None => t.max(0.0),
        }
    }

    /// Source seconds this clip plays over its whole duration.
    pub fn source_duration(&self) -> Seconds {
        self.source_offset(self.source_range.duration.to_seconds())
    }

    /// The timeline duration over which this clip plays `source` seconds of
    /// media; `None` for a freeze frame.
    pub fn timeline_duration_for_source(&self, source: Seconds) -> Option<Seconds> {
        match self.get_speed() {
            Some(speed) => speed.timeline_offset(source),
            None => Some(source.max(0.0)),
        }
    }

    /// How far `source_range.start_time` moves when the clip's start edge
    /// moves `delta` timeline seconds (positive trims, negative extends) and
    /// what plays after the edge stays where it is: `delta` at normal speed,
    /// the source those seconds play otherwise, 0 for a freeze. Before its
    /// first keyframe a ramp holds its first speed.
    pub fn start_edge_source_delta(&self, delta: Seconds) -> Seconds {
        let Some(speed) = self.get_speed() else {
            return delta;
        };
        if delta >= 0.0 {
            return speed.source_offset(delta);
        }
        let shifted = ClipSpeed { speed: shift_keyframes(&speed.speed, -delta), ..speed };
        -shifted.source_offset(-delta)
    }

    /// Move the clip's start edge by `delta` timeline seconds, keeping what
    /// plays after the edge in place: the source start moves by
    /// [`Clip::start_edge_source_delta`] (floored at `min_source_start`) and a
    /// speed ramp's keyframes, timed from the clip's start, shift with the
    /// edge. The duration is left to the caller.
    pub fn move_start_edge(&mut self, delta: Seconds, min_source_start: Seconds) {
        let source_delta = self.start_edge_source_delta(delta);
        let source_start = (self.source_range.start_time.to_seconds() + source_delta)
            .max(min_source_start);
        self.source_range.start_time.set_from_seconds(source_start);
        if delta.abs() > 0.0 {
            shift_ramp_keyframes(&mut self.effects, -delta);
        }
    }

    /// The largest start-edge extension, up to `wanted` timeline seconds,
    /// that keeps the source start at or after `min_source_start`. A freeze
    /// plays no source, so it extends freely.
    pub fn start_extension_within(&self, wanted: Seconds, min_source_start: Seconds) -> Seconds {
        let wanted = wanted.max(0.0);
        let available = (self.source_range.start_time.to_seconds() - min_source_start).max(0.0);
        let back = |e: Seconds| -self.start_edge_source_delta(-e);
        if back(wanted) <= available + 1e-9 {
            return wanted;
        }
        // back(e) is increasing in e (the speed is at least MIN_CLIP_SPEED).
        let (mut low, mut high) = (0.0, wanted);
        for _ in 0..64 {
            let mid = (low + high) / 2.0;
            if back(mid) <= available {
                low = mid;
            } else {
                high = mid;
            }
        }
        low
    }
}

/// `speed` with its keyframes `by` seconds later; a constant is unchanged.
fn shift_keyframes(speed: &Animatable, by: Seconds) -> Animatable {
    match speed {
        Animatable::Value(_) => speed.clone(),
        Animatable::Keyframes { keyframes } => Animatable::Keyframes {
            keyframes: keyframes.iter().map(|k| Keyframe { t: k.t + by, ..*k }).collect(),
        },
    }
}

/// Shift the keyframes of the speed the player applies (a keyframed
/// tellers `TimeWarp`) `by` seconds.
fn shift_ramp_keyframes(effects: &mut [Effect], by: Seconds) {
    let Some(effect) = effects.iter_mut().find(|effect| read_time_warp(effect).is_some()) else {
        return;
    };
    let Some(mut tellers) = TellersEffect::from_effect(effect) else {
        return;
    };
    let Some(speed @ Animatable::Keyframes { .. }) = tellers.animatable("speed") else {
        return;
    };
    tellers.set_animatable("speed", shift_keyframes(&speed, by));
    let parameters = Value::Object(tellers.parameters);
    if let Some(ai) = effect.metadata.other.get_mut("tellers.ai").and_then(Value::as_object_mut) {
        ai.insert("parameters".to_string(), parameters);
    }
}
