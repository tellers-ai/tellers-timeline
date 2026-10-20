//! Typed reads and writes of a [`Transition`] as the player draws it.
//!
//! `transition_type` stays a free string, so Resolve's `Custom_Transition`
//! and anything else round-trips untouched. The player compares it without
//! case or punctuation (`clip-transition.ts`): `Dissolve`, `CrossDissolve`,
//! `DipToColor`, `DipToBlack`, `DipToWhite`, `Wipe`, `Push`, `Slide` and
//! `Zoom`; anything else, OTIO's `SMPTE_Dissolve` included, is a dissolve.
//! A dip's colour and a wipe/push/slide's direction live in
//! `metadata["tellers.ai"].parameters`:
//!
//! ```json
//! { "OTIO_SCHEMA": "Transition.1", "transition_type": "Push",
//!   "metadata": { "tellers.ai": { "parameters": { "direction": "up" } } } }
//! ```

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::Transition;

/// The colour a dip goes through when none is set.
pub const DEFAULT_TRANSITION_COLOR: &str = "#000000";

/// How the player draws a transition (its `TransitionType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TransitionKind {
    Dissolve,
    DipToColor,
    Wipe,
    Push,
    Slide,
    Zoom,
}

impl TransitionKind {
    pub const ALL: [TransitionKind; 6] = [
        TransitionKind::Dissolve,
        TransitionKind::DipToColor,
        TransitionKind::Wipe,
        TransitionKind::Push,
        TransitionKind::Slide,
        TransitionKind::Zoom,
    ];

    /// The player's name for the kind.
    pub fn as_str(self) -> &'static str {
        match self {
            TransitionKind::Dissolve => "Dissolve",
            TransitionKind::DipToColor => "DipToColor",
            TransitionKind::Wipe => "Wipe",
            TransitionKind::Push => "Push",
            TransitionKind::Slide => "Slide",
            TransitionKind::Zoom => "Zoom",
        }
    }

    /// The `transition_type` written for the kind; a dissolve is written as
    /// OTIO's own `SMPTE_Dissolve`.
    pub fn transition_type(self) -> &'static str {
        match self {
            TransitionKind::Dissolve => "SMPTE_Dissolve",
            kind => kind.as_str(),
        }
    }

    /// Reads a kind name like the player reads a `transition_type`, except
    /// that an unknown name is `None` instead of a dissolve.
    pub fn parse(name: &str) -> Option<TransitionKind> {
        alias(name).map(|(kind, _)| kind)
    }

    /// The kind the player draws for a `transition_type`.
    pub fn from_transition_type(transition_type: &str) -> TransitionKind {
        TransitionKind::parse(transition_type).unwrap_or(TransitionKind::Dissolve)
    }
}

/// The side a wipe, push or slide moves towards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransitionDirection {
    #[default]
    Left,
    Right,
    Up,
    Down,
}

impl TransitionDirection {
    pub const ALL: [TransitionDirection; 4] = [
        TransitionDirection::Left,
        TransitionDirection::Right,
        TransitionDirection::Up,
        TransitionDirection::Down,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            TransitionDirection::Left => "left",
            TransitionDirection::Right => "right",
            TransitionDirection::Up => "up",
            TransitionDirection::Down => "down",
        }
    }

    /// Exact lowercase names only, as the player reads them.
    pub fn parse(name: &str) -> Option<TransitionDirection> {
        TransitionDirection::ALL
            .into_iter()
            .find(|d| d.as_str() == name)
    }
}

/// The player's `TYPE_ALIASES`: the kind, and the colour the name implies.
fn alias(name: &str) -> Option<(TransitionKind, Option<&'static str>)> {
    let key: String = name
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    Some(match key.as_str() {
        "dissolve" | "crossdissolve" => (TransitionKind::Dissolve, None),
        "diptocolor" => (TransitionKind::DipToColor, None),
        "diptoblack" => (TransitionKind::DipToColor, Some("#000000")),
        "diptowhite" => (TransitionKind::DipToColor, Some("#ffffff")),
        "wipe" => (TransitionKind::Wipe, None),
        "push" => (TransitionKind::Push, None),
        "slide" => (TransitionKind::Slide, None),
        "zoom" => (TransitionKind::Zoom, None),
        _ => return None,
    })
}

impl Transition {
    /// How the player draws this transition.
    pub fn kind(&self) -> TransitionKind {
        TransitionKind::from_transition_type(&self.transition_type)
    }

    /// Writes `kind` as the `transition_type`. A transition already drawn as
    /// `kind` keeps its `transition_type`, so `DipToBlack` stays black.
    pub fn set_kind(&mut self, kind: TransitionKind) {
        if self.kind() != kind {
            self.transition_type = kind.transition_type().to_string();
        }
    }

    /// [`Transition::set_kind`] from a name the player reads, e.g.
    /// `"cross dissolve"`; `DipToBlack` and `DipToWhite` also write their
    /// colour. `false`, and nothing changed, for an unknown name.
    pub fn set_kind_named(&mut self, name: &str) -> bool {
        let Some((kind, implied)) = alias(name) else {
            return false;
        };
        self.set_kind(kind);
        if implied.is_some() {
            self.set_color(implied);
        }
        true
    }

    /// The direction the player uses: `parameters.direction`, `left` when
    /// missing or unknown.
    pub fn direction(&self) -> TransitionDirection {
        self.parameter("direction")
            .and_then(Value::as_str)
            .and_then(TransitionDirection::parse)
            .unwrap_or_default()
    }

    /// Writes `parameters.direction`; `None` removes it.
    pub fn set_direction(&mut self, direction: Option<TransitionDirection>) {
        self.set_parameter("direction", direction.map(|d| Value::from(d.as_str())));
    }

    /// The colour the player dips through: the one `DipToBlack` or
    /// `DipToWhite` implies, else a non-blank `parameters.color` (trimmed),
    /// else black.
    pub fn color(&self) -> String {
        if let Some((_, Some(implied))) = alias(&self.transition_type) {
            return implied.to_string();
        }
        self.parameter("color")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .unwrap_or(DEFAULT_TRANSITION_COLOR)
            .to_string()
    }

    /// Writes `parameters.color`; `None` or a blank colour removes it. A
    /// `DipToBlack` or `DipToWhite` transition becomes `DipToColor`, since
    /// the colour its name implies would hide the one written.
    pub fn set_color(&mut self, color: Option<&str>) {
        let color = color.map(str::trim).filter(|c| !c.is_empty());
        if color.is_some() && matches!(alias(&self.transition_type), Some((_, Some(_)))) {
            self.transition_type = TransitionKind::DipToColor.transition_type().to_string();
        }
        self.set_parameter("color", color.map(Value::from));
    }

    fn parameter(&self, key: &str) -> Option<&Value> {
        self.metadata.get("tellers.ai")?.get("parameters")?.get(key)
    }

    fn set_parameter(&mut self, key: &str, value: Option<Value>) {
        if value.is_none() && self.parameter(key).is_none() {
            return;
        }
        if !self.metadata.is_object() {
            self.metadata = Value::Object(Map::new());
        }
        let root = self.metadata.as_object_mut().expect("an object");
        let parameters = object_entry(object_entry(root, "tellers.ai"), "parameters");
        match value {
            Some(value) => {
                parameters.insert(key.to_string(), value);
            }
            None => {
                parameters.remove(key);
            }
        }
    }
}

/// `map[key]` as an object, replacing the entry when it is not one.
fn object_entry<'a>(map: &'a mut Map<String, Value>, key: &str) -> &'a mut Map<String, Value> {
    let entry = map
        .entry(key.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    entry.as_object_mut().expect("an object")
}
