//! Effects that reference an asset, such as the `.cube` file of a
//! `tellers.ai.ColorLut`.
//!
//! Such an effect names the asset in its `asset_id` parameter; the player
//! (`video-player-js`) loads it from its `target_url` parameter:
//!
//! ```json
//! { "tellers.ai": { "parameters": { "asset_id": "a1b2", "target_url": "https://cdn.example.test/…" } } }
//! ```
//!
//! `target_url` is resolved from the asset id like a media reference's: set
//! it with [`Timeline::set_effect_target_urls`] before handing the timeline
//! to the player, and strip it with [`Timeline::clear_target_urls`] (or
//! [`Timeline::clear_effect_target_urls`]) so ephemeral (e.g. presigned)
//! URLs are not persisted. `asset_id` is what the timeline persists.
//!
//! These functions read only `parameters`, and cover every effect on every
//! stage (the timeline, its tracks, their clips, gaps and composite clips,
//! nested ones included), whatever its `effect_name`.

use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::{Effect, Item, Timeline, Track};

/// Parameter naming the asset an effect references.
pub const ASSET_ID_PARAMETER: &str = "asset_id";
/// Parameter holding the URL resolved from [`ASSET_ID_PARAMETER`].
pub const TARGET_URL_PARAMETER: &str = "target_url";

fn parameters(effect: &Effect) -> Option<&Map<String, Value>> {
    effect
        .metadata
        .other
        .get("tellers.ai")?
        .get("parameters")?
        .as_object()
}

fn parameters_mut(effect: &mut Effect) -> Option<&mut Map<String, Value>> {
    effect
        .metadata
        .other
        .get_mut("tellers.ai")?
        .get_mut("parameters")?
        .as_object_mut()
}

/// The asset id `effect` references: its `asset_id` parameter, trimmed;
/// `None` when it has none or it is empty.
pub fn effect_asset_id(effect: &Effect) -> Option<&str> {
    parameters(effect)?
        .get(ASSET_ID_PARAMETER)?
        .as_str()
        .map(str::trim)
        .filter(|id| !id.is_empty())
}

/// Set the `target_url` of every effect in `effects` whose asset id is in
/// `urls` (asset id → URL). Empty URLs and unknown ids are ignored. Returns
/// how many effects were updated.
pub fn set_effect_target_urls(effects: &mut [Effect], urls: &HashMap<String, String>) -> usize {
    let mut updated = 0;
    for effect in effects.iter_mut() {
        let Some(url) = effect_asset_id(effect)
            .and_then(|id| urls.get(id))
            .map(|url| url.trim().to_string())
            .filter(|url| !url.is_empty())
        else {
            continue;
        };
        if let Some(parameters) = parameters_mut(effect) {
            parameters.insert(TARGET_URL_PARAMETER.to_string(), Value::String(url));
            updated += 1;
        }
    }
    updated
}

/// Remove the `target_url` of every effect in `effects`, keeping its asset
/// id. Returns how many had one.
pub fn clear_effect_target_urls(effects: &mut [Effect]) -> usize {
    effects
        .iter_mut()
        .filter_map(parameters_mut)
        .filter_map(|parameters| parameters.remove(TARGET_URL_PARAMETER))
        .count()
}

impl Item {
    /// Every effect list of this item: its own, and for a composite clip
    /// those of its tracks and their items.
    fn for_each_effects(&self, f: &mut dyn FnMut(&[Effect])) {
        match self {
            Item::Clip(clip) => f(&clip.effects),
            Item::Gap(gap) => f(&gap.effects),
            Item::Composite(stack) => {
                f(&stack.effects);
                for track in &stack.children {
                    track.for_each_effects(f);
                }
            }
            Item::Transition(_) => {}
        }
    }

    fn for_each_effects_mut(&mut self, f: &mut dyn FnMut(&mut Vec<Effect>)) {
        match self {
            Item::Clip(clip) => f(&mut clip.effects),
            Item::Gap(gap) => f(&mut gap.effects),
            Item::Composite(stack) => {
                f(&mut stack.effects);
                for track in &mut stack.children {
                    track.for_each_effects_mut(f);
                }
            }
            Item::Transition(_) => {}
        }
    }

    /// Remove the `target_url` of every effect of this item (see
    /// [`clear_effect_target_urls`]). Returns how many had one.
    pub fn clear_effect_target_urls(&mut self) -> usize {
        let mut cleared = 0;
        self.for_each_effects_mut(&mut |effects| cleared += clear_effect_target_urls(effects));
        cleared
    }
}

impl Track {
    /// Every effect list of this track: its own, then its items'.
    fn for_each_effects(&self, f: &mut dyn FnMut(&[Effect])) {
        f(&self.effects);
        for item in &self.items {
            item.for_each_effects(f);
        }
    }

    fn for_each_effects_mut(&mut self, f: &mut dyn FnMut(&mut Vec<Effect>)) {
        f(&mut self.effects);
        for item in &mut self.items {
            item.for_each_effects_mut(f);
        }
    }
}

impl Timeline {
    /// Every effect list of the timeline: the root stack's, then each
    /// track's and its items'.
    fn for_each_effects(&self, f: &mut dyn FnMut(&[Effect])) {
        f(&self.tracks.effects);
        for track in &self.tracks.children {
            track.for_each_effects(f);
        }
    }

    fn for_each_effects_mut(&mut self, f: &mut dyn FnMut(&mut Vec<Effect>)) {
        f(&mut self.tracks.effects);
        for track in &mut self.tracks.children {
            track.for_each_effects_mut(f);
        }
    }

    /// Every distinct asset id referenced by an effect of the timeline, in
    /// timeline order: the ids to resolve for
    /// [`Timeline::set_effect_target_urls`].
    pub fn effect_asset_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        self.for_each_effects(&mut |effects| {
            for id in effects.iter().filter_map(effect_asset_id) {
                if !ids.iter().any(|known| known == id) {
                    ids.push(id.to_string());
                }
            }
        });
        ids
    }

    /// Set the `target_url` of every effect of the timeline whose asset id
    /// is in `urls` (asset id → URL). Returns how many were updated.
    pub fn set_effect_target_urls(&mut self, urls: &HashMap<String, String>) -> usize {
        let mut updated = 0;
        self.for_each_effects_mut(&mut |effects| updated += set_effect_target_urls(effects, urls));
        updated
    }

    /// [`Timeline::set_effect_target_urls`] for the single asset `asset_id`.
    pub fn set_effect_target_url(&mut self, asset_id: &str, url: &str) -> usize {
        let urls = HashMap::from([(asset_id.trim().to_string(), url.to_string())]);
        self.set_effect_target_urls(&urls)
    }

    /// Remove the `target_url` of every effect of the timeline, keeping the
    /// asset ids. Returns how many had one. [`Timeline::clear_target_urls`]
    /// does this along with the media references.
    pub fn clear_effect_target_urls(&mut self) -> usize {
        let mut cleared = 0;
        self.for_each_effects_mut(&mut |effects| cleared += clear_effect_target_urls(effects));
        cleared
    }
}
