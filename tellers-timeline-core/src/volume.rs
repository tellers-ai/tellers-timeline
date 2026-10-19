//! Clip volume: the `volume` (dB) of the clip's `tellers.ai.Volume` effect.
//!
//! Timelines written before tellers effects carried the volume as a Resolve
//! effect (`Resolve_OTIO` named `Volume`, `Fairlight Clip Volume and Fades`).
//! It is still read — the player applies it on top of `tellers.ai.Volume`, so
//! a clip's volume is the sum of both in dB — but never written:
//! [`crate::Clip::set_volume`] drops it, and
//! [`upgrade_legacy_resolve_volume_effects`] (run by [`crate::Stack::sanitize`])
//! folds it into `tellers.ai.Volume`.

use crate::{
    is_tellers_effect, push_tellers_effect, set_tellers_effect_parameter, Animatable, Effect,
    ResolveOTIOParameter, TellersEffect,
};

/// `effect_name` of the effects holding Resolve data.
const RESOLVE_EFFECT_NAME: &str = "Resolve Effect";
const VOLUME: &str = "Volume";
const VOLUME_PARAMETER: &str = "volume";

/// Whether `effect` is a legacy Resolve clip volume.
pub fn is_legacy_resolve_volume(effect: &Effect) -> bool {
    effect.effect_name == RESOLVE_EFFECT_NAME
        && effect.metadata.resolve_otio.as_ref().is_some_and(|resolve| {
            resolve.name == VOLUME || resolve.effect_name == "Fairlight Clip Volume and Fades"
        })
}

/// The dB a legacy Resolve volume adds, as the player reads it: `None` when it
/// is disabled or has no `volume` parameter.
fn legacy_resolve_volume_db(effect: &Effect) -> Option<f64> {
    let resolve = effect.metadata.resolve_otio.as_ref()?;
    if !is_legacy_resolve_volume(effect) || !resolve.enabled {
        return None;
    }
    resolve.parameters.iter().find_map(|parameter| match parameter {
        ResolveOTIOParameter::Double(p) if p.parameter_id == VOLUME_PARAMETER => {
            Some(p.parameter_value)
        }
        _ => None,
    })
}

/// Position, in `effects`, of the enabled `tellers.ai.Volume` the player
/// applies (the first one).
fn tellers_volume_position(effects: &[Effect]) -> Option<usize> {
    effects.iter().position(|effect| {
        TellersEffect::from_effect(effect)
            .is_some_and(|e| e.enabled && e.effect_name == VOLUME)
    })
}

/// The clip volume in dB at the start of the clip: `tellers.ai.Volume`'s
/// `volume` plus a legacy Resolve volume, or `None` when neither is set.
pub fn volume_db(effects: &[Effect]) -> Option<f64> {
    let tellers = tellers_volume_position(effects)
        .and_then(|at| TellersEffect::from_effect(&effects[at]))
        .and_then(|e| e.value_at(VOLUME_PARAMETER, 0.0));
    let legacy = effects.iter().find_map(legacy_resolve_volume_db);
    match (tellers, legacy) {
        (None, None) => None,
        (t, l) => Some(t.unwrap_or(0.0) + l.unwrap_or(0.0)),
    }
}

/// Set the clip volume to `volume` dB on `tellers.ai.Volume` (added after the
/// last tellers effect when there is none; its fades are kept) and drop any
/// legacy Resolve volume.
pub fn set_volume_db(effects: &mut Vec<Effect>, volume: f64) {
    effects.retain(|effect| !is_legacy_resolve_volume(effect));
    write_tellers_volume(effects, |_| Animatable::Value(volume));
}

/// Fold every legacy Resolve volume of `effects` into `tellers.ai.Volume`:
/// its dB is added to the `volume` there (to every keyframe of an animated
/// one), so the clip sounds the same, and the Resolve effect is removed.
/// Returns how many Resolve effects were removed.
pub fn upgrade_legacy_resolve_volume_effects(effects: &mut Vec<Effect>) -> usize {
    let removed = effects.iter().filter(|e| is_legacy_resolve_volume(e)).count();
    if removed == 0 {
        return 0;
    }
    let legacy = effects.iter().find_map(legacy_resolve_volume_db);
    effects.retain(|effect| !is_legacy_resolve_volume(effect));
    if let Some(db) = legacy.filter(|db| *db != 0.0) {
        write_tellers_volume(effects, |current| match current {
            None => Animatable::Value(db),
            Some(Animatable::Value(v)) => Animatable::Value(v + db),
            Some(Animatable::Keyframes { mut keyframes }) => {
                for keyframe in &mut keyframes {
                    keyframe.v += db;
                }
                Animatable::Keyframes { keyframes }
            }
        });
    }
    removed
}

/// Set the `volume` of the applied `tellers.ai.Volume` to `f(current)`,
/// adding the effect after the last tellers effect when there is none.
fn write_tellers_volume(
    effects: &mut Vec<Effect>,
    f: impl FnOnce(Option<Animatable>) -> Animatable,
) {
    match tellers_volume_position(effects) {
        Some(at) => {
            let current = TellersEffect::from_effect(&effects[at])
                .and_then(|e| e.animatable(VOLUME_PARAMETER));
            // Index among tellers effects, as `set_tellers_effect_parameter` counts.
            let index = effects[..at].iter().filter(|e| is_tellers_effect(e)).count();
            set_tellers_effect_parameter(
                effects,
                index,
                VOLUME_PARAMETER,
                Some(f(current).to_value()),
            );
        }
        None => {
            let mut volume = TellersEffect::new(VOLUME);
            volume.set_animatable(VOLUME_PARAMETER, f(None));
            push_tellers_effect(effects, volume);
        }
    }
}

impl crate::Track {
    /// [`upgrade_legacy_resolve_volume_effects`] on every clip of this track,
    /// including those inside composite clips. Returns how many Resolve
    /// effects were removed.
    pub fn upgrade_legacy_resolve_volumes(&mut self) -> usize {
        self.items
            .iter_mut()
            .map(|item| match item {
                crate::Item::Clip(clip) => upgrade_legacy_resolve_volume_effects(&mut clip.effects),
                crate::Item::Composite(stack) => stack
                    .children
                    .iter_mut()
                    .map(crate::Track::upgrade_legacy_resolve_volumes)
                    .sum(),
                _ => 0,
            })
            .sum()
    }
}

impl crate::Timeline {
    /// Fold every clip's legacy Resolve volume into `tellers.ai.Volume`
    /// (see [`upgrade_legacy_resolve_volume_effects`]); nothing else changes.
    /// Returns how many Resolve effects were removed, 0 when the timeline was
    /// already migrated.
    pub fn upgrade_legacy_resolve_volumes(&mut self) -> usize {
        self.tracks
            .children
            .iter_mut()
            .map(crate::Track::upgrade_legacy_resolve_volumes)
            .sum()
    }
}
