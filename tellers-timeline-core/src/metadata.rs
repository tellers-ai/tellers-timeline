use crate::{Clip, Gap, Item, Track};

pub trait IdMetadataExt {
    fn get_id(&self) -> Option<String>;
    fn set_id(&mut self, id: Option<String>);
}

fn read_id_from_metadata(meta: &serde_json::Value) -> Option<String> {
    meta.get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn write_id_to_metadata(meta: &mut serde_json::Value, id: Option<String>) {
    match (meta.as_object_mut(), id) {
        (Some(map), Some(uid)) => {
            map.insert("id".to_string(), serde_json::Value::String(uid));
        }
        (Some(map), None) => {
            map.remove("id");
        }
        (None, Some(uid)) => {
            let mut new_map = serde_json::Map::new();
            new_map.insert("id".to_string(), serde_json::Value::String(uid));
            *meta = serde_json::Value::Object(new_map);
        }
        (None, None) => {}
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
