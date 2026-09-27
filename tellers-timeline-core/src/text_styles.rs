//! Shared CSS classes declared by a timeline for its rich-text clips.
//!
//! A Rich Text clip carries its look inline in its `Title HTML`, so a run of
//! subtitles repeats the same `style="font-family:...;font-size:...;color:..."`
//! on every clip. That makes the timeline large, and an agent writing
//! subtitles has to emit (and pay for) the same declarations over and over.
//!
//! Text styles move those declarations out of the items: the timeline declares
//! named CSS classes once, and a clip's HTML refers to them by class
//! (`<div class="subtitle">Hello</div>`). The player (`video-player-js`)
//! turns each entry into a `.name { declarations }` rule and injects it into
//! the SVG it rasterizes the clip's HTML in, and embeds any custom font the
//! declarations reference exactly as it does for inline `font-family`.
//!
//! They live in the timeline metadata at
//! `metadata["tellers.ai"]["textStyles"]`, as an object mapping a class name
//! to its CSS declaration block:
//!
//! ```json
//! {
//!   "subtitle": "font-family:'Brand Sans';font-size:56px;color:#fff",
//!   "subtitle-accent": "color:#ffd400"
//! }
//! ```
//!
//! Reads drop entries the player itself would drop (a name that is not a
//! plain CSS identifier, or a declaration block that is empty or contains a
//! brace), and writes store the normalized (trimmed) form, so anything this
//! module returns is a style the player will apply.

use serde::{Deserialize, Serialize};

use crate::Timeline;

/// Metadata key, under the `tellers.ai` namespace, holding the style object.
pub const TEXT_STYLES_KEY: &str = "textStyles";

/// A CSS class declared by a timeline for use by its rich-text clips.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextStyle {
    /// Class name, as referenced by `class="..."` in a clip's Title HTML.
    /// A plain CSS identifier: `[A-Za-z_-][A-Za-z0-9_-]*`, not starting with
    /// a digit or with `-` followed by a digit. Case-sensitive, like CSS.
    pub name: String,
    /// The CSS declaration block of the class, without the surrounding
    /// braces: `font-size:56px;color:#fff`.
    pub declarations: String,
}

impl TextStyle {
    /// A style with the given class name and declaration block.
    pub fn new(name: impl Into<String>, declarations: impl Into<String>) -> Self {
        TextStyle {
            name: name.into(),
            declarations: declarations.into(),
        }
    }

    /// This style with name and declarations trimmed. `None` when the player
    /// would ignore the style: a name that is not a plain CSS identifier, or
    /// a declaration block that is empty or contains `{` or `}`.
    pub fn normalized(&self) -> Option<TextStyle> {
        let name = self.name.trim();
        let declarations = self.declarations.trim();
        if !is_valid_text_style_name(name) || !is_valid_text_style_declarations(declarations) {
            return None;
        }
        Some(TextStyle {
            name: name.to_string(),
            declarations: declarations.to_string(),
        })
    }

    /// Whether this style would be kept by [`TextStyle::normalized`].
    pub fn is_valid(&self) -> bool {
        self.normalized().is_some()
    }

    /// The CSS rule the player injects for this style: `.name { declarations }`.
    pub fn to_css_rule(&self) -> String {
        format!(".{} {{ {} }}", self.name, self.declarations)
    }
}

/// Whether `name` can be used unescaped in a class selector: an ASCII CSS
/// identifier (`[A-Za-z_-][A-Za-z0-9_-]*`) that does not start with a digit
/// or with `-` followed by a digit.
pub fn is_valid_text_style_name(name: &str) -> bool {
    let mut chars = name.chars().peekable();
    let first = match chars.next() {
        Some(c) => c,
        None => return false,
    };
    let valid_first = match first {
        '_' => true,
        '-' => matches!(chars.peek(), Some(c) if c.is_ascii_alphabetic() || *c == '_' || *c == '-'),
        c => c.is_ascii_alphabetic(),
    };
    valid_first && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Whether `declarations` is a usable declaration block: non-empty, and free
/// of the braces that would let it close the rule and inject other rules.
pub fn is_valid_text_style_declarations(declarations: &str) -> bool {
    !declarations.is_empty() && !declarations.contains('{') && !declarations.contains('}')
}

/// The stylesheet the player builds from `styles`: one `.name { ... }` rule per
/// line, in the given order.
pub fn text_styles_css(styles: &[TextStyle]) -> String {
    styles
        .iter()
        .map(TextStyle::to_css_rule)
        .collect::<Vec<_>>()
        .join("\n")
}

/// The styles declared in `metadata["tellers.ai"]["textStyles"]`, normalized
/// and with unusable entries skipped, in the object's key order.
pub fn resolve_text_styles(metadata: &serde_json::Value) -> Vec<TextStyle> {
    raw_text_styles(metadata)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|(name, raw)| {
                    let declarations = raw.as_str()?;
                    TextStyle::new(name.as_str(), declarations).normalized()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Add `style` to `metadata["tellers.ai"]["textStyles"]`, creating the
/// `tellers.ai` object and the style object as needed. An existing entry with
/// the same name is replaced. Returns `false` — leaving the metadata untouched
/// — when the style is one the player could not use (invalid name, or an
/// empty or brace-containing declaration block).
pub fn set_text_style(metadata: &mut serde_json::Value, style: TextStyle) -> bool {
    let Some(style) = style.normalized() else {
        return false;
    };
    text_styles_object_mut(metadata)
        .insert(style.name, serde_json::Value::String(style.declarations));
    true
}

/// Remove the style named `name` (trimmed, case-sensitive) from
/// `metadata["tellers.ai"]["textStyles"]`, returning whether it was present.
/// The object itself is dropped once it is empty.
pub fn remove_text_style(metadata: &mut serde_json::Value, name: &str) -> bool {
    let Some(ai) = metadata
        .get_mut("tellers.ai")
        .and_then(|value| value.as_object_mut())
    else {
        return false;
    };
    let Some(styles) = ai.get_mut(TEXT_STYLES_KEY).and_then(|v| v.as_object_mut()) else {
        return false;
    };

    let removed = styles.remove(name.trim()).is_some();
    if styles.is_empty() {
        ai.remove(TEXT_STYLES_KEY);
    }
    removed
}

impl Timeline {
    /// The CSS classes this timeline declares for its rich-text clips. Styles
    /// the player would ignore are skipped.
    pub fn get_text_styles(&self) -> Vec<TextStyle> {
        resolve_text_styles(&self.metadata)
    }

    /// The declaration block of the class named `name`, if declared and usable.
    pub fn get_text_style(&self, name: &str) -> Option<String> {
        let wanted = name.trim();
        self.get_text_styles()
            .into_iter()
            .find(|style| style.name == wanted)
            .map(|style| style.declarations)
    }

    /// Declare (or redefine) the class `name` with `declarations` on this
    /// timeline. Returns `false`, and changes nothing, when the name is not a
    /// plain CSS identifier or the declarations are empty or contain a brace.
    pub fn set_text_style(
        &mut self,
        name: impl Into<String>,
        declarations: impl Into<String>,
    ) -> bool {
        set_text_style(&mut self.metadata, TextStyle::new(name, declarations))
    }

    /// Remove the declared class `name`, returning whether it was present.
    pub fn remove_text_style(&mut self, name: &str) -> bool {
        remove_text_style(&mut self.metadata, name)
    }

    /// The stylesheet the player injects for this timeline's text clips: one
    /// `.name { declarations }` rule per declared style.
    pub fn text_styles_css(&self) -> String {
        text_styles_css(&self.get_text_styles())
    }
}

fn raw_text_styles(
    metadata: &serde_json::Value,
) -> Option<&serde_json::Map<String, serde_json::Value>> {
    metadata
        .get("tellers.ai")?
        .get(TEXT_STYLES_KEY)?
        .as_object()
}

/// The style object at `metadata["tellers.ai"]["textStyles"]`, creating the
/// `tellers.ai` object and the style object when they are missing or the
/// wrong type.
fn text_styles_object_mut(
    metadata: &mut serde_json::Value,
) -> &mut serde_json::Map<String, serde_json::Value> {
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
    let ai_map = ai.as_object_mut().unwrap();
    let styles = ai_map
        .entry(TEXT_STYLES_KEY.to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if styles.as_object().is_none() {
        *styles = serde_json::Value::Object(serde_json::Map::new());
    }
    styles.as_object_mut().unwrap()
}
