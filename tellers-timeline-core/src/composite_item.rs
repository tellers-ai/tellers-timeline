//! Editing one item of a composite clip the way the player's
//! `editCompositeItem` does (video-player-js #837, #838), so a host can save
//! what the player's item tools report.
//!
//! The player draws an item of a composite clip (a Text, Shape, video or image
//! clip on one of its video tracks) from the clip's own OTIO:
//!
//! - its **box** (`x`, `y`, `width`, `height` in frame fractions, `rotation`
//!   in degrees) is the clip's Resolve Transform (pan, tilt, zoom X / Y,
//!   rotation angle), for Shape and media items. A Text item has no box: it
//!   is placed by its `Transform`s. The **crop** (`cropLeft`... 0 to 1) is
//!   its Resolve Cropping, for media items only.
//! - its **Transforms** are its tellers `Transform` effects, nested in order,
//!   after its `Stabilize` when it has one. Their `rotationX` / `rotationY`
//!   tilt the item in 3D inside the composite (#838), as they tilt a clip.
//! - its **effects** are its filters (the effects taking `regions`: `Blur`,
//!   `Adjustment`, `Glow`...). `Border` and `MotionBlur` are not drawn on an
//!   item.
//! - a **text patch** restyles a Text item: `text`, `spans`, `maxWidth` and
//!   `wrap` replace the generator's own, other fields merge into its `style`
//!   (null removes one), `animation` (`TextTypewriter`, `TextHandwrite`,
//!   `TextWordReveal` or `none`) replaces its reveal effect and
//!   `shadowAnimation` (`pulse`, `sweep`, `rise` or `none`) its `TextShadow`,
//!   keeping that shadow's colors.
//! - a **shape patch** merges into a Shape item's generator `parameters`
//!   (null removes one); `animation` (`ShapeHandwrite` or `none`) replaces its
//!   reveal effect.
//!
//! [`CompositeItemEdit`] deserializes from the `edit` object the player takes
//! (camelCase keys, effects as `{name, parameters}`), so a host can pass the
//! player's edit through. Items are found by id, at any depth;
//! [`crate::Stack::composite_item_id`] turns the player's `itemSource`
//! (`track`, `child` indices into the composite's OTIO tracks) into one.
//! Only the clip's own effects change: effects on the composite's inner
//! track, which the player also applies to the item, stay as they are.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{
    effect_spec, resolve_tellers_effects, set_tellers_effects, Clip, GeneratorError, IdMetadataExt,
    Item, MediaReferenceCrop, Stack, TellersEffect, Timeline,
};

type Object = Map<String, Value>;

const TEXT_REVEAL_EFFECTS: &[&str] = &["TextTypewriter", "TextHandwrite", "TextWordReveal"];
const TEXT_SHADOW_ANIMATIONS: &[&str] = &["pulse", "sweep", "rise"];
const SHAPE_REVEAL_EFFECTS: &[&str] = &["ShapeHandwrite"];

/// Fields of a composite item's box; those left out keep their value.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompositeItemBox {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop_left: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop_right: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop_top: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop_bottom: Option<f64>,
}

impl CompositeItemBox {
    fn has_placement(&self) -> bool {
        [self.x, self.y, self.width, self.height, self.rotation]
            .iter()
            .any(Option::is_some)
    }

    fn has_crop(&self) -> bool {
        [
            self.crop_left,
            self.crop_right,
            self.crop_top,
            self.crop_bottom,
        ]
        .iter()
        .any(Option::is_some)
    }
}

/// A filter effect as the player passes it: a bare `name` and its `parameters`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CompositeItemEffect {
    pub name: String,
    #[serde(default)]
    pub parameters: Object,
}

/// An edit of one composite item; see the module docs for each part.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompositeItemEdit {
    /// Merged into its box (Shape and media items) and crop (media items).
    #[serde(rename = "box", default, skip_serializing_if = "Option::is_none")]
    pub item_box: Option<CompositeItemBox>,
    /// Its own `Transform` parameters, in order, replacing them; a `Stabilize` stays.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transforms: Option<Vec<Object>>,
    /// Its filters, replacing them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effects: Option<Vec<CompositeItemEffect>>,
    /// A Text item's restyle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_patch: Option<Object>,
    /// A Shape item's generator `parameters` fields or `animation`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape_patch: Option<Object>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CompositeItemEditError {
    #[error("no item {0}")]
    NoSuchItem(String),
    #[error("{0} is not inside a composite clip")]
    NotInComposite(String),
    #[error("{0} is not a clip")]
    NotAClip(String),
    #[error("{0} is a Text item, placed by its Transforms: it has no box")]
    BoxOnText(String),
    #[error("{0} is not a video or image item: only those are cropped")]
    CropNotOnMedia(String),
    #[error("{0} is not a Text item")]
    NotText(String),
    #[error("{0} is not a Shape item")]
    NotShape(String),
    #[error("{effect} is not a filter a composite item draws")]
    NotAnItemFilter { effect: String },
    #[error("invalid {effect}: {reason}")]
    InvalidEffect { effect: String, reason: String },
    #[error("unknown animation {0:?}")]
    UnknownAnimation(String),
    #[error("{0}")]
    Generator(String),
}

impl From<GeneratorError> for CompositeItemEditError {
    fn from(error: GeneratorError) -> Self {
        CompositeItemEditError::Generator(error.to_string())
    }
}

/// Whether `name` is a filter the player draws on a composite item: an effect
/// taking `regions` (Border and MotionBlur take none and are skipped there).
pub fn is_composite_item_filter(name: &str) -> bool {
    effect_spec(name).is_some_and(|spec| spec.parameter("regions").is_some())
}

impl Stack {
    /// The id of the item at `child` of track `track` of the composite clip
    /// `composite_id`: the player's `itemSource` for a composite item. `None`
    /// when there is no such composite or item, or it has no id.
    pub fn composite_item_id(
        &self,
        composite_id: &str,
        track: usize,
        child: usize,
    ) -> Option<String> {
        self.get_composite(composite_id)?
            .children
            .get(track)?
            .items
            .get(child)?
            .get_id()
    }
}

impl Timeline {
    /// Apply `edit` to the item `item_id` of a composite clip (at any depth),
    /// as the player's `editCompositeItem` does. All or nothing: on an error
    /// the item is left as it was. An edit never changes a length.
    pub fn edit_composite_item(
        &mut self,
        item_id: &str,
        edit: &CompositeItemEdit,
    ) -> Result<(), CompositeItemEditError> {
        let path = self
            .tracks
            .composite_path(item_id)
            .ok_or_else(|| CompositeItemEditError::NoSuchItem(item_id.to_string()))?;
        if path.is_empty() {
            return Err(CompositeItemEditError::NotInComposite(item_id.to_string()));
        }
        let Some(Item::Clip(clip)) = self.tracks.get_nested_item(item_id) else {
            return Err(CompositeItemEditError::NotAClip(item_id.to_string()));
        };
        let mut clip = clip.clone();
        apply_edit(&mut clip, item_id, edit)?;
        if let Some(Item::Clip(target)) = self.tracks.get_nested_item_mut(item_id) {
            *target = clip;
        }
        Ok(())
    }
}

fn apply_edit(
    clip: &mut Clip,
    id: &str,
    edit: &CompositeItemEdit,
) -> Result<(), CompositeItemEditError> {
    let is_text = clip.is_text_clip();
    let is_shape = clip.is_shape_clip();
    let is_media = !clip.is_generator_clip();
    if let Some(item_box) = &edit.item_box {
        if item_box.has_placement() {
            if is_text {
                return Err(CompositeItemEditError::BoxOnText(id.to_string()));
            }
            let mut placement = clip.resolve_placement();
            let finite = |value: Option<f64>| value.filter(|v| v.is_finite());
            if let Some(x) = finite(item_box.x) {
                placement.x = x;
            }
            if let Some(y) = finite(item_box.y) {
                placement.y = y;
            }
            if let Some(width) = finite(item_box.width) {
                placement.zoom_x = width;
            }
            if let Some(height) = finite(item_box.height) {
                placement.zoom_y = height;
            }
            if let Some(rotation) = finite(item_box.rotation) {
                placement.rotation = rotation;
            }
            clip.set_resolve_placement(placement);
        }
        if item_box.has_crop() {
            if !is_media {
                return Err(CompositeItemEditError::CropNotOnMedia(id.to_string()));
            }
            let crop = clip.get_crop();
            let pick =
                |value: Option<f64>, old: f64| value.filter(|v| v.is_finite()).unwrap_or(old);
            clip.set_crop(MediaReferenceCrop {
                crop_left: pick(item_box.crop_left, crop.crop_left),
                crop_right: pick(item_box.crop_right, crop.crop_right),
                crop_top: pick(item_box.crop_top, crop.crop_top),
                crop_bottom: pick(item_box.crop_bottom, crop.crop_bottom),
            });
        }
    }
    if let Some(transforms) = &edit.transforms {
        let new = transforms
            .iter()
            .map(|parameters| {
                checked(TellersEffect {
                    parameters: parameters.clone(),
                    ..TellersEffect::new("Transform")
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        replace_effects(clip, |e| e.enabled && e.effect_name == "Transform", new);
    }
    if let Some(effects) = &edit.effects {
        let new = effects
            .iter()
            .map(|effect| {
                if !is_composite_item_filter(&effect.name) {
                    return Err(CompositeItemEditError::NotAnItemFilter {
                        effect: effect.name.clone(),
                    });
                }
                checked(TellersEffect {
                    parameters: effect.parameters.clone(),
                    ..TellersEffect::new(effect.name.clone())
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        replace_effects(
            clip,
            |e| e.enabled && is_composite_item_filter(&e.effect_name),
            new,
        );
    }
    if let Some(patch) = &edit.text_patch {
        if !is_text {
            return Err(CompositeItemEditError::NotText(id.to_string()));
        }
        apply_text_patch(clip, patch)?;
    }
    if let Some(patch) = &edit.shape_patch {
        if !is_shape {
            return Err(CompositeItemEditError::NotShape(id.to_string()));
        }
        apply_shape_patch(clip, patch)?;
    }
    Ok(())
}

fn checked(effect: TellersEffect) -> Result<TellersEffect, CompositeItemEditError> {
    effect
        .validate()
        .map_err(|reason| CompositeItemEditError::InvalidEffect {
            effect: effect.effect_name.clone(),
            reason,
        })?;
    Ok(effect)
}

/// Replace the tellers effects matching `replaced` with `new`, put where the
/// first of them was (else at the end); the others keep their order.
fn replace_effects(
    clip: &mut Clip,
    replaced: impl Fn(&TellersEffect) -> bool,
    new: Vec<TellersEffect>,
) {
    let mut effects = resolve_tellers_effects(&clip.effects);
    let at = effects.iter().position(&replaced).unwrap_or(effects.len());
    let kept_before = effects[..at].iter().filter(|e| !replaced(e)).count();
    effects.retain(|e| !replaced(e));
    effects.splice(kept_before..kept_before, new);
    set_tellers_effects(&mut clip.effects, effects);
}

/// Replace the clip's reveal effect (one of `reveals`) with `animation`, or
/// none for `"none"`; a new reveal goes first.
fn set_reveal(
    clip: &mut Clip,
    reveals: &[&str],
    animation: &str,
) -> Result<(), CompositeItemEditError> {
    if animation != "none" && !reveals.contains(&animation) {
        return Err(CompositeItemEditError::UnknownAnimation(
            animation.to_string(),
        ));
    }
    let mut effects = resolve_tellers_effects(&clip.effects);
    effects.retain(|e| !reveals.contains(&e.effect_name.as_str()));
    if animation != "none" {
        effects.insert(0, TellersEffect::new(animation));
    }
    set_tellers_effects(&mut clip.effects, effects);
    Ok(())
}

fn set_shadow_animation(clip: &mut Clip, animation: &str) -> Result<(), CompositeItemEditError> {
    if animation != "none" && !TEXT_SHADOW_ANIMATIONS.contains(&animation) {
        return Err(CompositeItemEditError::UnknownAnimation(
            animation.to_string(),
        ));
    }
    let mut effects = resolve_tellers_effects(&clip.effects);
    let previous = effects
        .iter()
        .position(|e| e.enabled && e.effect_name == "TextShadow");
    let colors = previous.and_then(|i| effects.remove(i).parameters.get("colors").cloned());
    if animation != "none" {
        let mut shadow = TellersEffect::new("TextShadow").with_parameter("style", animation);
        if let Some(colors) = colors {
            shadow = shadow.with_parameter("colors", colors);
        }
        effects.push(shadow);
    }
    set_tellers_effects(&mut clip.effects, effects);
    Ok(())
}

fn merge_fields(target: &mut Object, fields: impl IntoIterator<Item = (String, Value)>) {
    for (key, value) in fields {
        if value.is_null() {
            target.remove(&key);
        } else {
            target.insert(key, value);
        }
    }
}

fn to_object(value: impl Serialize) -> Result<Object, CompositeItemEditError> {
    match serde_json::to_value(value) {
        Ok(Value::Object(object)) => Ok(object),
        Ok(_) => Ok(Object::new()),
        Err(e) => Err(CompositeItemEditError::Generator(e.to_string())),
    }
}

fn from_object<T: serde::de::DeserializeOwned>(
    object: Object,
) -> Result<T, CompositeItemEditError> {
    serde_json::from_value(Value::Object(object))
        .map_err(|e| CompositeItemEditError::Generator(e.to_string()))
}

fn apply_text_patch(clip: &mut Clip, patch: &Object) -> Result<(), CompositeItemEditError> {
    let mut parameters = to_object(clip.text_parameters()?)?;
    let mut style = parameters
        .get("style")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut animation = None;
    let mut shadow = None;
    for (key, value) in patch {
        match key.as_str() {
            "animation" => animation = value.as_str(),
            "shadowAnimation" => shadow = value.as_str(),
            "text" | "spans" | "maxWidth" | "wrap" => {
                if key == "text" && value.is_string() {
                    // A new string drops the word timings, as `Clip::set_text` does.
                    parameters.remove("words");
                }
                merge_fields(&mut parameters, [(key.clone(), value.clone())]);
            }
            _ => merge_fields(&mut style, [(key.clone(), value.clone())]),
        }
    }
    if style.is_empty() {
        parameters.remove("style");
    } else {
        parameters.insert("style".to_string(), Value::Object(style));
    }
    clip.set_text_parameters(&from_object(parameters)?)?;
    if let Some(animation) = animation {
        set_reveal(clip, TEXT_REVEAL_EFFECTS, animation)?;
    }
    if let Some(shadow) = shadow {
        set_shadow_animation(clip, shadow)?;
    }
    Ok(())
}

fn apply_shape_patch(clip: &mut Clip, patch: &Object) -> Result<(), CompositeItemEditError> {
    let mut parameters = to_object(clip.shape_parameters()?)?;
    let mut animation = None;
    merge_fields(
        &mut parameters,
        patch.iter().filter_map(|(key, value)| {
            if key == "animation" {
                animation = value.as_str();
                None
            } else {
                Some((key.clone(), value.clone()))
            }
        }),
    );
    clip.set_shape_parameters(&from_object(parameters)?)?;
    if let Some(animation) = animation {
        set_reveal(clip, SHAPE_REVEAL_EFFECTS, animation)?;
    }
    Ok(())
}
