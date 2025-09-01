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
        self.metadata = metadata;
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
        self.metadata = metadata;
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
        self.metadata = metadata;
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
        self.metadata = metadata;
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
        self.metadata = metadata;
    }
}

impl MetadataExt for MediaReference {
    fn get_metadata(&self) -> &serde_json::Value {
        &self.metadata
    }
    fn get_metadata_mut(&mut self) -> &mut serde_json::Value {
        &mut self.metadata
    }
    fn set_metadata(&mut self, metadata: serde_json::Value) {
        self.metadata = metadata;
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
        match self {
            Item::Clip(c) => c.metadata = metadata,
            Item::Gap(g) => g.metadata = metadata,
        }
    }
}
