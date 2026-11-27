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
//! A page lasts as long as its compound does on the timeline, like a clip in
//! a video (video-player-js #854): its `source_range` duration sets how long
//! it shows in the player's GIF playback, carousel and GIF / WebP export.
//! [`Timeline::compounds`] lists the compounds where the player draws them,
//! like its `getCompounds()`, and [`Timeline::pages`] the pages, like its
//! `getPages()`.

use serde_json::{Map, Value};

use crate::{IdMetadataExt, Item, Seconds, Timeline, Track, TrackKind};

/// Timeline metadata key, under `tellers.ai`, holding the edit mode.
pub const EDIT_MODE_KEY: &str = "edit_mode";

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
}

/// A page of an images timeline, as the player's `getPages()` lists it: it
/// shows from `time` until the next page starts, the last one until its
/// compounds end.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageInfo {
    pub time: Seconds,
    pub duration: Seconds,
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

impl Timeline {
    /// The timeline's edit mode, as the player reads it.
    pub fn edit_mode(&self) -> EditMode {
        self.metadata
            .get("tellers.ai")
            .and_then(|ai| ai.get(EDIT_MODE_KEY))
            .and_then(Value::as_str)
            .map_or(EditMode::Video, EditMode::parse)
    }

    /// Set the edit mode. Video, the default, removes the key. Images drops
    /// every audio track (see [`Timeline::remove_audio`]): an images
    /// timeline has no sound.
    pub fn set_edit_mode(&mut self, mode: EditMode) {
        let ai = tellers_ai_mut(&mut self.metadata);
        match mode {
            EditMode::Video => {
                ai.remove(EDIT_MODE_KEY);
            }
            EditMode::Images => {
                ai.insert(EDIT_MODE_KEY.to_string(), Value::from(mode.as_str()));
                self.remove_audio();
            }
        }
    }

    /// Whether audio may be added: not to an images timeline, which has no
    /// sound. [`Timeline::sanitize`] drops what is added anyway.
    pub fn accepts_audio(&self) -> bool {
        self.edit_mode() != EditMode::Images
    }

    /// Drop every audio track, at the top level and inside every compound,
    /// returning whether any was there. Video clips keep their sync ids; a
    /// sanitize then unlinks the ones whose sound is gone.
    pub fn remove_audio(&mut self) -> bool {
        remove_audio_tracks(&mut self.tracks.children)
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

    /// The pages the player shows, like its `getPages()`: one per distinct
    /// compound start, each lasting until the next starts and the last until
    /// its compounds end. Empty with no compounds.
    pub fn pages(&self) -> Vec<PageInfo> {
        let compounds = self.compounds();
        let end = compounds
            .iter()
            .map(|compound| compound.start + compound.duration)
            .fold(0.0, Seconds::max);
        let mut starts: Vec<Seconds> = compounds.iter().map(|compound| compound.start).collect();
        starts.dedup();
        (0..starts.len())
            .map(|i| PageInfo {
                time: starts[i],
                duration: starts.get(i + 1).copied().unwrap_or(end) - starts[i],
            })
            .collect()
    }
}

/// Drop the audio tracks of `tracks` and of the compounds on the rest;
/// whether any was dropped.
fn remove_audio_tracks(tracks: &mut Vec<Track>) -> bool {
    let before = tracks.len();
    tracks.retain(|track| track.kind != TrackKind::Audio);
    let mut removed = tracks.len() != before;
    for track in tracks.iter_mut() {
        for item in &mut track.items {
            if let Item::Composite(stack) = item {
                removed |= remove_audio_tracks(&mut stack.children);
            }
        }
    }
    removed
}
