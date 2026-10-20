//! Legacy forms of the `tellers.ai.ColorLut` effect.
//!
//! A `.cube` LUT is an ordinary tellers effect (see [`crate::effects`]):
//!
//! ```json
//! {
//!   "OTIO_SCHEMA": "Effect.1",
//!   "effect_name": "tellers.ai.ColorLut",
//!   "name": "slog3_to_709.cube",
//!   "metadata": { "tellers.ai": { "parameters": { "asset_id": "a1b2", "target_url": "https://cdn.example.test/…" } } }
//! }
//! ```
//!
//! Its `target_url` is resolved from its `asset_id` like every effect's (see
//! [`crate::effect_assets`]).
//!
//! Timelines written earlier may hold LUTs in a legacy form: named
//! `ColorLut`, with `asset_id` and the URL written beside `parameters`
//! (directly in `tellers.ai`), and the URL named `cube_url`.
//! [`upgrade_legacy_color_lut_effects`], run by [`crate::Stack::sanitize`],
//! brings them to the current form.

use crate::Effect;

/// `effect_name` of a LUT effect.
const COLOR_LUT_EFFECT_NAME: &str = "tellers.ai.ColorLut";
/// `effect_name` of a LUT effect written before tellers effects were prefixed.
const LEGACY_COLOR_LUT_EFFECT_NAME: &str = "ColorLut";

const ASSET_ID_KEY: &str = "asset_id";
const TARGET_URL_KEY: &str = "target_url";
/// Former name of [`TARGET_URL_KEY`].
const LEGACY_CUBE_URL_KEY: &str = "cube_url";
const PARAMETERS_KEY: &str = "parameters";

fn is_lut_effect(effect: &Effect) -> bool {
    effect.effect_name == COLOR_LUT_EFFECT_NAME
        || effect.effect_name == LEGACY_COLOR_LUT_EFFECT_NAME
}

/// Bring every LUT effect in `effects` written in a legacy form up to date:
/// the `ColorLut` name becomes `tellers.ai.ColorLut`, an `asset_id` or URL
/// beside `parameters` moves into it (one already in `parameters` wins), and
/// `cube_url` is renamed `target_url` (an existing `target_url` wins).
/// Returns how many effects changed.
pub fn upgrade_legacy_color_lut_effects(effects: &mut [Effect]) -> usize {
    effects
        .iter_mut()
        .filter(|effect| is_lut_effect(effect))
        .map(|effect| {
            let mut changed = false;
            if effect.effect_name == LEGACY_COLOR_LUT_EFFECT_NAME {
                effect.effect_name = COLOR_LUT_EFFECT_NAME.to_string();
                changed = true;
            }
            if let Some(ai) = effect
                .metadata
                .other
                .get_mut("tellers.ai")
                .and_then(serde_json::Value::as_object_mut)
            {
                changed |= move_legacy_keys_into_parameters(ai);
            }
            changed
        })
        .filter(|changed| *changed)
        .count()
}

/// The `parameters` object of a LUT's `tellers.ai` object, created (or
/// replaced, when it is not an object) as needed.
fn parameters_mut(
    ai: &mut serde_json::Map<String, serde_json::Value>,
) -> &mut serde_json::Map<String, serde_json::Value> {
    let parameters = ai
        .entry(PARAMETERS_KEY)
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if !parameters.is_object() {
        *parameters = serde_json::Value::Object(serde_json::Map::new());
    }
    parameters.as_object_mut().expect("parameters is an object")
}

/// Move an `asset_id` or URL written beside `parameters` into it, and name
/// the URL `target_url` rather than `cube_url`; a value already in
/// `parameters` (and a `target_url` over a `cube_url`) wins. Returns whether
/// anything changed.
fn move_legacy_keys_into_parameters(ai: &mut serde_json::Map<String, serde_json::Value>) -> bool {
    let mut moved = false;
    for (key, to) in [
        (ASSET_ID_KEY, ASSET_ID_KEY),
        (TARGET_URL_KEY, TARGET_URL_KEY),
        (LEGACY_CUBE_URL_KEY, TARGET_URL_KEY),
    ] {
        let Some(value) = ai.remove(key) else {
            continue;
        };
        parameters_mut(ai).entry(to).or_insert(value);
        moved = true;
    }
    if let Some(parameters) = ai.get_mut(PARAMETERS_KEY).and_then(|p| p.as_object_mut()) {
        if let Some(value) = parameters.remove(LEGACY_CUBE_URL_KEY) {
            parameters.entry(TARGET_URL_KEY).or_insert(value);
            moved = true;
        }
    }
    moved
}
