use crate::{Clip, Gap, Item, MediaReference, Stack, Timeline, Track};

pub trait IdMetadataExt {
    fn get_id(&self) -> Option<String>;
    fn set_id(&mut self, id: Option<String>);
}

fn read_id_from_metadata(meta: &serde_json::Value) -> Option<String> {
    meta.get("tellers.ai")
        .and_then(|v| v.get("timeline_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn write_id_to_metadata(meta: &mut serde_json::Value, id: Option<String>) {
    // Ensure we have an object at the root
    if meta.as_object().is_none() {
        *meta = serde_json::Value::Object(serde_json::Map::new());
    }

    let map = meta.as_object_mut().unwrap();
    match id {
        Some(uid) => {
            // Ensure we have an object at metadata["tellers.ai"]
            let ai_entry = map
                .entry("tellers.ai".to_string())
                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
            if ai_entry.as_object().is_none() {
                *ai_entry = serde_json::Value::Object(serde_json::Map::new());
            }
            let ai_map = ai_entry.as_object_mut().unwrap();
            ai_map.insert("timeline_id".to_string(), serde_json::Value::String(uid));
        }
        None => {
            if let Some(ai_entry) = map.get_mut("tellers.ai") {
                if let Some(ai_map) = ai_entry.as_object_mut() {
                    ai_map.remove("timeline_id");
                    // If the tellers.ai object is now empty, leave it in place to preserve shape
                }
            }
        }
    }
}

impl IdMetadataExt for Clip {
    fn get_id(&self) -> Option<String> {
        read_id_from_metadata(&self.metadata)
    }
    fn set_id(&mut self, id: Option<String>) {
        write_id_to_metadata(&mut self.metadata, id)
    }
}

impl IdMetadataExt for Track {
    fn get_id(&self) -> Option<String> {
        read_id_from_metadata(&self.metadata)
    }
    fn set_id(&mut self, id: Option<String>) {
        write_id_to_metadata(&mut self.metadata, id)
    }
}

impl IdMetadataExt for Gap {
    fn get_id(&self) -> Option<String> {
        read_id_from_metadata(&self.metadata)
    }
    fn set_id(&mut self, id: Option<String>) {
        write_id_to_metadata(&mut self.metadata, id)
    }
}

impl IdMetadataExt for Item {
    fn get_id(&self) -> Option<String> {
        match self {
            Item::Clip(c) => c.get_id(),
            Item::Gap(g) => g.get_id(),
        }
    }
    fn set_id(&mut self, id: Option<String>) {
        match self {
            Item::Clip(c) => c.set_id(id),
            Item::Gap(g) => g.set_id(id),
        }
    }
}

pub trait MetadataExt {
    fn get_metadata(&self) -> &serde_json::Value;
    fn get_metadata_mut(&mut self) -> &mut serde_json::Value;
    fn set_metadata(&mut self, metadata: serde_json::Value);
}

impl MetadataExt for Timeline {
    fn get_metadata(&self) -> &serde_json::Value {
        &self.metadata
    }
    fn get_metadata_mut(&mut self) -> &mut serde_json::Value {
        &mut self.metadata
    }
    fn set_metadata(&mut self, metadata: serde_json::Value) {
        let mut v = metadata;
        if v.as_object().is_none() {
            v = serde_json::Value::Object(serde_json::Map::new());
        }
        self.metadata = v;
    }
}

impl MetadataExt for Stack {
    fn get_metadata(&self) -> &serde_json::Value {
        &self.metadata
    }
    fn get_metadata_mut(&mut self) -> &mut serde_json::Value {
        &mut self.metadata
    }
    fn set_metadata(&mut self, metadata: serde_json::Value) {
        let mut v = metadata;
        if v.as_object().is_none() {
            v = serde_json::Value::Object(serde_json::Map::new());
        }
        self.metadata = v;
    }
}

impl MetadataExt for Track {
    fn get_metadata(&self) -> &serde_json::Value {
        &self.metadata
    }
    fn get_metadata_mut(&mut self) -> &mut serde_json::Value {
        &mut self.metadata
    }
    fn set_metadata(&mut self, metadata: serde_json::Value) {
        let mut v = metadata;
        if v.as_object().is_none() {
            v = serde_json::Value::Object(serde_json::Map::new());
        }
        self.metadata = v;
    }
}

impl MetadataExt for Clip {
    fn get_metadata(&self) -> &serde_json::Value {
        &self.metadata
    }
    fn get_metadata_mut(&mut self) -> &mut serde_json::Value {
        &mut self.metadata
    }
    fn set_metadata(&mut self, metadata: serde_json::Value) {
        let mut v = metadata;
        if v.as_object().is_none() {
            v = serde_json::Value::Object(serde_json::Map::new());
        }
        self.metadata = v;
    }
}

impl MetadataExt for Gap {
    fn get_metadata(&self) -> &serde_json::Value {
        &self.metadata
    }
    fn get_metadata_mut(&mut self) -> &mut serde_json::Value {
        &mut self.metadata
    }
    fn set_metadata(&mut self, metadata: serde_json::Value) {
        let mut v = metadata;
        if v.as_object().is_none() {
            v = serde_json::Value::Object(serde_json::Map::new());
        }
        self.metadata = v;
    }
}

impl MetadataExt for MediaReference {
    fn get_metadata(&self) -> &serde_json::Value {
        self.metadata()
    }
    fn get_metadata_mut(&mut self) -> &mut serde_json::Value {
        self.metadata_mut()
    }
    fn set_metadata(&mut self, metadata: serde_json::Value) {
        let mut v = metadata;
        if v.as_object().is_none() {
            v = serde_json::Value::Object(serde_json::Map::new());
        }
        *self.metadata_mut() = v;
    }
}

impl MetadataExt for Item {
    fn get_metadata(&self) -> &serde_json::Value {
        match self {
            Item::Clip(c) => &c.metadata,
            Item::Gap(g) => &g.metadata,
        }
    }
    fn get_metadata_mut(&mut self) -> &mut serde_json::Value {
        match self {
            Item::Clip(c) => &mut c.metadata,
            Item::Gap(g) => &mut g.metadata,
        }
    }
    fn set_metadata(&mut self, metadata: serde_json::Value) {
        let mut v = metadata;
        if v.as_object().is_none() {
            v = serde_json::Value::Object(serde_json::Map::new());
        }
        match self {
            Item::Clip(c) => c.metadata = v,
            Item::Gap(g) => g.metadata = v,
        }
    }
}

// ---- Group-id metadata accessors -------------------------------------------
//
// Timeline items carry two distinct grouping ids in their metadata:
//
// * the Resolve "Link Group ID" (`metadata["Resolve_OTIO"]["Link Group ID"]`),
//   which ties synchronised clips together, and
// * the Tellers Group ID (`metadata["tellers.ai"]["Tellers Group ID"]`), the
//   Tellers-native "move together" grouping.
//
// These accessors expose the read/write conventions for both so callers (the
// editor bridge, the stack methods) don't re-implement the metadata layout.

/// The Resolve "Link Group ID" of an item (the sync-clip group), or `None` for
/// an ungrouped clip or a gap.
pub fn item_link_group_id(item: &Item) -> Option<i64> {
    match item {
        Item::Clip(clip) => clip.sync_clips_id(),
        Item::Gap(_) => None,
    }
}

/// The Tellers Group ID of an item, or `None` for an ungrouped clip or a gap.
pub fn item_tellers_group_id(item: &Item) -> Option<i64> {
    match item {
        Item::Clip(clip) => resolve_tellers_group_id(&clip.metadata),
        Item::Gap(_) => None,
    }
}

/// Set (or clear, when `group_id` is `None`) the Tellers Group ID on an item.
pub fn set_item_tellers_group_id(item: &mut Item, group_id: Option<i64>) {
    let metadata = item.get_metadata_mut();
    match group_id {
        Some(group_id) => set_tellers_group_id(metadata, group_id),
        None => {
            remove_tellers_group_id(metadata);
        }
    }
}

/// Read the Tellers group id from `metadata["tellers.ai"]["Tellers Group ID"]`.
///
/// This is the Tellers-native grouping concept, kept separate from the Resolve
/// "Link Group ID" (sync clips) and stored in the Tellers metadata namespace
/// alongside `timeline_id`. An int, uint, or stringified value all read back.
pub fn resolve_tellers_group_id(metadata: &serde_json::Value) -> Option<i64> {
    let raw = metadata
        .get("tellers.ai")
        .and_then(|v| v.get("Tellers Group ID"))?;
    raw.as_i64()
        .or_else(|| raw.as_u64().and_then(|value| i64::try_from(value).ok()))
        .or_else(|| raw.as_str().and_then(|value| value.parse::<i64>().ok()))
}

/// Write `group_id` to `metadata["tellers.ai"]["Tellers Group ID"]`, creating
/// the `tellers.ai` object if it is missing or not an object.
pub fn set_tellers_group_id(metadata: &mut serde_json::Value, group_id: i64) {
    if metadata.as_object().is_none() {
        *metadata = serde_json::Value::Object(serde_json::Map::new());
    }
    let map = metadata.as_object_mut().unwrap();
    let ai = map
        .entry("tellers.ai".to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if ai.as_object().is_none() {
        *ai = serde_json::Value::Object(serde_json::Map::new());
    }
    ai.as_object_mut().unwrap().insert(
        "Tellers Group ID".to_string(),
        serde_json::Value::Number(serde_json::Number::from(group_id)),
    );
}

/// Remove the Tellers Group ID from `metadata`, returning whether one was
/// present.
pub fn remove_tellers_group_id(metadata: &mut serde_json::Value) -> bool {
    let Some(ai) = metadata
        .get_mut("tellers.ai")
        .and_then(|value| value.as_object_mut())
    else {
        return false;
    };
    ai.remove("Tellers Group ID").is_some()
}
