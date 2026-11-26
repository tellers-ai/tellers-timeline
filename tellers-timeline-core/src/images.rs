//! Images timelines (video-player-js #835): a timeline edited like a set of
//! images, one page per compound clip, exported as stills, a GIF or an
//! animated WebP.
//!
//! A timeline declares it with `metadata["tellers.ai"].edit_mode: "images"`
//! (`"video"`, the default, when absent or anything else). The player then
//! has no playback and no audio, and draws each page (a composite clip, an
//! OTIO `Stack` on a top-level video track) at its first frame from a
//! timeless copy of the timeline: keyframes hold the value they hold longest,
//! reveal effects and transitions are off, gaps on a track holding pages take
//! no time, and each layer's first clip fills its page. The OTIO itself is not
//! changed, so this library edits an images timeline like any other.
//!
//! A page's `metadata["tellers.ai"].frame_delay` is how many seconds it shows
//! for as a GIF / WebP frame, [`DEFAULT_FRAME_DELAY`] when absent.
//! [`Timeline::compounds`] lists the pages where the player draws them, like
//! its `getCompounds()`.

use serde_json::{Map, Value};

use crate::{IdMetadataExt, Item, Seconds, Timeline, TrackKind};

/// Timeline metadata key, under `tellers.ai`, holding the edit mode.
pub const EDIT_MODE_KEY: &str = "edit_mode";
/// Compound metadata key, under `tellers.ai`, holding its frame delay.
pub const FRAME_DELAY_KEY: &str = "frame_delay";
/// Seconds a page shows for as a GIF / WebP frame when it sets no `frame_delay`.
pub const DEFAULT_FRAME_DELAY: Seconds = 1.0;

/// How the player edits and exports a timeline.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EditMode {
    /// Played and exported as a video (the default).
    #[default]
    Video,
    /// Edited like images, compound by compound, with no audio and no
    /// playback; exported as stills, a GIF or an animated WebP.
    Images,
}

impl EditMode {
    pub fn as_str(self) -> &'static str {
        match self {
            EditMode::Video => "video",
            EditMode::Images => "images",
        }
    }

    /// The mode the player reads from `value`: `"images"`, else video.
    pub fn parse(value: &str) -> Self {
        if value == "images" {
            EditMode::Images
        } else {
            EditMode::Video
        }
    }
}

/// A compound clip on a top-level video track, as the player lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct CompoundInfo {
    pub id: Option<String>,
    pub name: String,
    /// Where it is drawn, in timeline seconds (in an images timeline, with
    /// gaps on its track taking no time).
    pub start: Seconds,
    pub duration: Seconds,
    /// Index of its track in the timeline stack.
    pub track_index: usize,
    /// Seconds it shows for as a GIF / WebP frame.
    pub frame_delay: Seconds,
}

fn tellers_ai_mut(metadata: &mut Value) -> &mut Map<String, Value> {
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
    ai.as_object_mut().expect("tellers.ai is an object")
}

fn read_frame_delay(metadata: &Value) -> Option<Seconds> {
    metadata
        .get("tellers.ai")?
        .get(FRAME_DELAY_KEY)?
        .as_f64()
        .filter(|delay| *delay > 0.0 && delay.is_finite())
}

impl Timeline {
    /// The timeline's edit mode, as the player reads it.
    pub fn edit_mode(&self) -> EditMode {
        self.metadata
            .get("tellers.ai")
            .and_then(|ai| ai.get(EDIT_MODE_KEY))
            .and_then(Value::as_str)
            .map_or(EditMode::Video, EditMode::parse)
    }

    /// Set the edit mode. Video, the default, removes the key.
    pub fn set_edit_mode(&mut self, mode: EditMode) {
        let ai = tellers_ai_mut(&mut self.metadata);
        match mode {
            EditMode::Video => {
                ai.remove(EDIT_MODE_KEY);
            }
            EditMode::Images => {
                ai.insert(EDIT_MODE_KEY.to_string(), Value::from(mode.as_str()));
            }
        }
    }

    /// The enabled compound clips of the enabled top-level video tracks,
    /// ordered by start then track, as the player's `getCompounds()`: an
    /// images timeline's pages. Compounds of no length are skipped.
    pub fn compounds(&self) -> Vec<CompoundInfo> {
        let images = self.edit_mode() == EditMode::Images;
        let mut compounds = Vec::new();
        for (track_index, track) in self.tracks.children.iter().enumerate() {
            if !track.enabled || track.kind != TrackKind::Video {
                continue;
            }
            // In an images timeline, gaps on a track holding pages take no time.
            let gaps_timeless = images
                && track
                    .items
                    .iter()
                    .any(|item| matches!(item, Item::Composite(_)));
            let mut time = 0.0;
            for item in &track.items {
                let duration = match item {
                    Item::Transition(_) => continue,
                    Item::Gap(_) if gaps_timeless => 0.0,
                    other => other.duration(),
                };
                if let Item::Composite(stack) = item {
                    if stack.enabled != Some(false) && duration > 0.0 {
                        compounds.push(CompoundInfo {
                            id: stack.get_id(),
                            name: stack.name.clone().unwrap_or_default(),
                            start: time,
                            duration,
                            track_index,
                            frame_delay: read_frame_delay(&stack.metadata)
                                .unwrap_or(DEFAULT_FRAME_DELAY),
                        });
                    }
                }
                time += duration;
            }
        }
        compounds.sort_by(|a, b| {
            a.start
                .total_cmp(&b.start)
                .then(a.track_index.cmp(&b.track_index))
        });
        compounds
    }

    /// The `frame_delay` set on the compound `compound_id` (top-level), or
    /// `None` when it has none or is not a compound.
    pub fn frame_delay(&self, compound_id: &str) -> Option<Seconds> {
        match self.tracks.get_item(compound_id)?.2 {
            Item::Composite(stack) => read_frame_delay(&stack.metadata),
            _ => None,
        }
    }

    /// Set (or, with `None`, remove) the frame delay of the top-level
    /// compound `compound_id`. Returns `false`, changing nothing, when it is
    /// not a compound or `delay` is not a positive number.
    pub fn set_frame_delay(&mut self, compound_id: &str, delay: Option<Seconds>) -> bool {
        if delay.is_some_and(|delay| !(delay > 0.0 && delay.is_finite())) {
            return false;
        }
        let Some((track_index, item_index, _)) = self.tracks.get_item(compound_id) else {
            return false;
        };
        let Item::Composite(stack) = &mut self.tracks.children[track_index].items[item_index]
        else {
            return false;
        };
        let ai = tellers_ai_mut(&mut stack.metadata);
        match delay {
            Some(delay) => {
                ai.insert(FRAME_DELAY_KEY.to_string(), Value::from(delay));
            }
            None => {
                ai.remove(FRAME_DELAY_KEY);
            }
        }
        true
    }
}
