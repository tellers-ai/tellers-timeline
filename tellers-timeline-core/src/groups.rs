//! Clip groups: clips (or composite clips) that the player moves, scales,
//! rotates and fades together with one shared, keyframable transform
//! (video-player-js `otio-groups.ts`).
//!
//! - A member names its group in `metadata["tellers.ai"].group_id`, a string.
//!   Members keep their own tracks and their own effects.
//! - The timeline's `metadata["tellers.ai"].groups[<group_id>].parameters` is
//!   the group's transform: `x`, `y` (frame widths / heights), `scale`,
//!   `scaleX`, `scaleY`, `rotation` (degrees, clockwise), `opacity`, and the
//!   pivot `pivotX`, `pivotY` (an offset from the frame centre), each an
//!   [`Animatable`]. Keyframe `t` is in seconds from the start of the
//!   group's earliest member.
//! - Members of a group missing from `groups` are drawn as plain clips.
//!
//! These groups are not the library's edit groups (`Tellers Group ID`, see
//! [`crate::metadata`]), which only make clips move together when edited.

use serde_json::{Map, Value};

use crate::{Animatable, IdMetadataExt, Item, Seconds, Timeline};

const GROUPS_KEY: &str = "groups";
const GROUP_ID_KEY: &str = "group_id";
const PARAMETERS_KEY: &str = "parameters";

/// The parameters a group transform can set.
pub const CLIP_GROUP_PARAMETERS: &[&str] = &[
    "x", "y", "scale", "scaleX", "scaleY", "rotation", "opacity", "pivotX", "pivotY",
];

/// A clip group's transform, as stored under the timeline's
/// `metadata["tellers.ai"].groups`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ClipGroup {
    /// The `group_id` its members carry.
    pub id: String,
    /// Parameter name → a number or keyframes; see [`CLIP_GROUP_PARAMETERS`].
    pub parameters: Map<String, Value>,
}

/// A group transform evaluated at one time, as the player computes it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClipGroupTransform {
    pub x: f64,
    pub y: f64,
    /// `scale * scaleX`.
    pub scale_x: f64,
    /// `scale * scaleY`.
    pub scale_y: f64,
    pub rotation: f64,
    /// Clamped to 0..=1.
    pub opacity: f64,
    pub pivot_x: f64,
    pub pivot_y: f64,
}

impl ClipGroup {
    /// A group with no parameters: its members are drawn as they are.
    pub fn new(id: impl Into<String>) -> Self {
        ClipGroup { id: id.into(), parameters: Map::new() }
    }

    /// Set the parameter `key` to `value` (a number, or an [`Animatable`] via
    /// [`Animatable::to_value`]).
    pub fn with_parameter(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.parameters.insert(key.into(), value.into());
        self
    }

    /// The parameter `key`, read as the player does.
    pub fn animatable(&self, key: &str) -> Option<Animatable> {
        Animatable::from_value(self.parameters.get(key)?)
    }

    pub fn set_animatable(&mut self, key: impl Into<String>, value: Animatable) {
        self.parameters.insert(key.into(), value.to_value());
    }

    /// The transform at `t` seconds from the start of the group's earliest
    /// member; unset parameters take their defaults (no change).
    pub fn transform_at(&self, t: Seconds) -> ClipGroupTransform {
        let value = |key: &str, default: f64| self.animatable(key).map_or(default, |a| a.value_at(t));
        let scale = value("scale", 1.0);
        ClipGroupTransform {
            x: value("x", 0.0),
            y: value("y", 0.0),
            scale_x: scale * value("scaleX", 1.0),
            scale_y: scale * value("scaleY", 1.0),
            rotation: value("rotation", 0.0),
            opacity: value("opacity", 1.0).clamp(0.0, 1.0),
            pivot_x: value("pivotX", 0.0),
            pivot_y: value("pivotY", 0.0),
        }
    }

    /// A non-empty id and known parameters holding numbers or keyframes.
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() {
            return Err("a clip group needs an id".to_string());
        }
        for (key, value) in &self.parameters {
            if !CLIP_GROUP_PARAMETERS.contains(&key.as_str()) {
                return Err(format!("clip groups have no parameter {key:?}"));
            }
            if Animatable::from_value(value).is_none() {
                return Err(format!("clip group {key}: invalid value {value}"));
            }
        }
        Ok(())
    }
}

/// The `group_id` in an item's metadata, as the player reads it (a string,
/// or a number written as one).
fn metadata_group_id(metadata: &Value) -> Option<String> {
    match metadata.get("tellers.ai")?.get(GROUP_ID_KEY)? {
        Value::String(id) if !id.is_empty() => Some(id.clone()),
        Value::Number(id) => Some(id.to_string()),
        _ => None,
    }
}

fn item_metadata(item: &Item) -> Option<&Value> {
    match item {
        Item::Clip(clip) => Some(&clip.metadata),
        Item::Composite(stack) => Some(&stack.metadata),
        Item::Gap(_) | Item::Transition(_) => None,
    }
}

fn item_metadata_mut(item: &mut Item) -> Option<&mut Value> {
    match item {
        Item::Clip(clip) => Some(&mut clip.metadata),
        Item::Composite(stack) => Some(&mut stack.metadata),
        Item::Gap(_) | Item::Transition(_) => None,
    }
}

/// The clip-group id of `item`: `None` for gaps, transitions and clips in
/// no group.
pub fn item_clip_group_id(item: &Item) -> Option<String> {
    item_metadata(item).and_then(metadata_group_id)
}

/// Set (or, with `None`, remove) the clip-group id of a clip or composite
/// clip. Returns `false` for gaps and transitions.
pub fn set_item_clip_group_id(item: &mut Item, group_id: Option<&str>) -> bool {
    let Some(metadata) = item_metadata_mut(item) else {
        return false;
    };
    match group_id {
        Some(id) => {
            if !metadata.is_object() {
                *metadata = Value::Object(Map::new());
            }
            let ai = metadata
                .as_object_mut()
                .expect("metadata is an object")
                .entry("tellers.ai")
                .or_insert_with(|| Value::Object(Map::new()));
            if !ai.is_object() {
                *ai = Value::Object(Map::new());
            }
            ai.as_object_mut()
                .expect("tellers.ai is an object")
                .insert(GROUP_ID_KEY.to_string(), Value::String(id.to_string()));
        }
        None => {
            if let Some(ai) = metadata.get_mut("tellers.ai").and_then(Value::as_object_mut) {
                ai.remove(GROUP_ID_KEY);
            }
        }
    }
    true
}

impl Timeline {
    fn clip_groups_object(&self) -> Option<&Map<String, Value>> {
        self.metadata.get("tellers.ai")?.get(GROUPS_KEY)?.as_object()
    }

    fn clip_groups_object_mut(&mut self) -> &mut Map<String, Value> {
        if !self.metadata.is_object() {
            self.metadata = Value::Object(Map::new());
        }
        let ai = self
            .metadata
            .as_object_mut()
            .expect("metadata is an object")
            .entry("tellers.ai")
            .or_insert_with(|| Value::Object(Map::new()));
        if !ai.is_object() {
            *ai = Value::Object(Map::new());
        }
        let groups = ai
            .as_object_mut()
            .expect("tellers.ai is an object")
            .entry(GROUPS_KEY)
            .or_insert_with(|| Value::Object(Map::new()));
        if !groups.is_object() {
            *groups = Value::Object(Map::new());
        }
        groups.as_object_mut().expect("groups is an object")
    }

    /// Every clip group of the timeline, by id.
    pub fn get_clip_groups(&self) -> Vec<ClipGroup> {
        let Some(groups) = self.clip_groups_object() else {
            return Vec::new();
        };
        let mut ids: Vec<&String> = groups.keys().collect();
        ids.sort();
        ids.into_iter().filter_map(|id| self.get_clip_group(id)).collect()
    }

    pub fn get_clip_group(&self, id: &str) -> Option<ClipGroup> {
        let entry = self.clip_groups_object()?.get(id)?.as_object()?;
        let parameters = entry
            .get(PARAMETERS_KEY)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        Some(ClipGroup { id: id.to_string(), parameters })
    }

    /// Add or replace the transform of the group `group.id`; other keys of
    /// its entry are kept. Its members are not changed.
    pub fn set_clip_group(&mut self, group: &ClipGroup) -> Result<(), String> {
        group.validate()?;
        let entry = self
            .clip_groups_object_mut()
            .entry(group.id.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        entry
            .as_object_mut()
            .expect("group entry is an object")
            .insert(PARAMETERS_KEY.to_string(), Value::Object(group.parameters.clone()));
        Ok(())
    }

    /// Group the clips and composite clips `item_ids` under a new group with
    /// `parameters`, moving them out of any group they were in. Returns the
    /// new group's id, or an error (changing nothing) when an id is not a
    /// clip or composite clip of the timeline or a parameter is invalid.
    pub fn create_clip_group(
        &mut self,
        item_ids: &[String],
        parameters: Map<String, Value>,
    ) -> Result<String, String> {
        let group = ClipGroup { id: crate::types::gen_hex_id_12(), parameters };
        group.validate()?;
        for id in item_ids {
            match self.tracks.get_item(id) {
                Some((_, _, Item::Clip(_) | Item::Composite(_))) => {}
                _ => return Err(format!("no clip or composite clip {id:?}")),
            }
        }
        self.set_clip_group(&group)?;
        for id in item_ids {
            self.set_item_clip_group_id(id, Some(&group.id));
        }
        Ok(group.id)
    }

    /// Remove the group `id` and take its members out of it. Returns whether
    /// the group existed.
    pub fn remove_clip_group(&mut self, id: &str) -> bool {
        for member in self.clip_group_member_ids(id) {
            self.set_item_clip_group_id(&member, None);
        }
        self.clip_groups_object_mut().remove(id).is_some()
    }

    /// The clip-group id of the item `item_id`.
    pub fn get_item_clip_group_id(&self, item_id: &str) -> Option<String> {
        item_clip_group_id(self.tracks.get_item(item_id)?.2)
    }

    /// Put the clip or composite clip `item_id` in the group `group_id`, or
    /// take it out of its group with `None`. The group's transform is not
    /// created: see [`Timeline::set_clip_group`]. Returns `false` when no
    /// clip or composite clip has this id.
    pub fn set_item_clip_group_id(&mut self, item_id: &str, group_id: Option<&str>) -> bool {
        let Some((track_index, item_index, _)) = self.tracks.get_item(item_id) else {
            return false;
        };
        set_item_clip_group_id(
            &mut self.tracks.children[track_index].items[item_index],
            group_id,
        )
    }

    /// The ids of the members of the group `id`, in track order then time.
    pub fn clip_group_member_ids(&self, id: &str) -> Vec<String> {
        self.tracks
            .children
            .iter()
            .flat_map(|track| track.items.iter())
            .filter(|item| item_clip_group_id(item).as_deref() == Some(id))
            .filter_map(|item| item.get_id())
            .collect()
    }

    /// Where the group's keyframe time 0 is: the timeline start of its
    /// earliest member, or `None` when it has none.
    pub fn clip_group_start(&self, id: &str) -> Option<Seconds> {
        self.tracks
            .children
            .iter()
            .flat_map(|track| {
                track
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| item_clip_group_id(item).as_deref() == Some(id))
                    .map(|(index, _)| track.start_time_of_item(index))
            })
            .min_by(f64::total_cmp)
    }
}
