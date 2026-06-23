pub mod track_item_delete;
pub mod track_item_get;
pub mod track_item_insert;
pub mod track_item_split;

pub use track_item_insert::{
    ClampPolicy, DeletedClipInfo, InsertPolicy, OverlapPolicy, SplitClipInfo, TrackInsertResult,
};
