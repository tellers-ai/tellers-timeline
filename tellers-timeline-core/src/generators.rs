//! Typed `parameters` of the player's own generator clips.
//!
//! A `GeneratorReference.1` whose `generator_kind` is `"Text"` is a plain
//! string (or timed words) the player draws live; one whose kind is
//! `"Shape"` is a rectangle, ellipse or line. Both are placed by the clip's
//! effects (Transform, Cropping), like a video clip, not by Resolve
//! parameters. The source of truth is `definitions.MediaReference` in the
//! player's `schema/timeline.schema.json`.
//!
//! The parameters stay stored as raw JSON in
//! [`GeneratorParameters::other`](crate::GeneratorParameters); these types are
//! read from and written back to it. Every struct keeps keys it does not know
//! in `other`, so a read-modify-write never drops fields a newer player wrote.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::types::{Clip, GeneratorParameters, MediaReference, Seconds, TimeRange};

pub const TEXT_GENERATOR_KIND: &str = "Text";
pub const SHAPE_GENERATOR_KIND: &str = "Shape";

type Extra = serde_json::Map<String, serde_json::Value>;

/// One word of a Text clip, timed in seconds from the clip start.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
pub struct TextWord {
    pub text: String,
    pub start: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<f64>,
    #[serde(flatten)]
    pub other: Extra,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TextFontStyle {
    Normal,
    Italic,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

/// `fontWeight`: a CSS keyword (`"bold"`) or a number (`700`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(untagged)]
pub enum TextFontWeight {
    Number(serde_json::Number),
    Name(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
pub struct TextStroke {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(flatten)]
    pub other: Extra,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
pub struct TextDropShadow {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alpha: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blur: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle: Option<f64>,
    #[serde(flatten)]
    pub other: Extra,
}

/// What a text background box wraps: the whole text, each line, or the word
/// being spoken.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TextBackgroundMode {
    Block,
    Line,
    Word,
}

/// Background `padding`: one number for both axes, or `[x]` / `[x, y]`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(untagged)]
pub enum TextPadding {
    Uniform(f64),
    Axes(Vec<f64>),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
pub struct TextBackground {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<TextBackgroundMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding: Option<TextPadding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<f64>,
    #[serde(flatten)]
    pub other: Extra,
}

/// The look of a Text clip. Sizes are pixels of the timeline's text render
/// size; colours are CSS strings.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TextClipStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_weight: Option<TextFontWeight>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_style: Option<TextFontStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<TextAlign>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub letter_spacing: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<TextStroke>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drop_shadow: Option<TextDropShadow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<TextBackground>,
    #[serde(flatten)]
    pub other: Extra,
}

/// `parameters` of a `generator_kind: "Text"` reference: `text`, or `words`
/// with timings.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TextParameters {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words: Option<Vec<TextWord>>,
    /// Wrap width in frame widths.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<TextClipStyle>,
    #[serde(flatten)]
    pub other: Extra,
}

impl TextParameters {
    pub fn with_text(text: impl Into<String>) -> Self {
        TextParameters {
            text: Some(text.into()),
            ..Default::default()
        }
    }

    /// The displayed string: `text`, else the `words` joined by spaces.
    pub fn display_text(&self) -> Option<String> {
        if let Some(text) = &self.text {
            return Some(text.clone());
        }
        self.words.as_ref().map(|words| {
            words
                .iter()
                .map(|word| word.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        })
    }

    /// Checks the bounds the schema sets beyond types.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(max_width) = self.max_width {
            if !(max_width > 0.0) {
                return Err(format!("maxWidth must be > 0, got {max_width}"));
            }
        }
        if let Some(style) = &self.style {
            if let Some(font_size) = style.font_size {
                if !(font_size > 0.0) {
                    return Err(format!("style.fontSize must be > 0, got {font_size}"));
                }
            }
            if let Some(TextPadding::Axes(axes)) =
                style.background.as_ref().and_then(|b| b.padding.as_ref())
            {
                if axes.is_empty() || axes.len() > 2 {
                    return Err(format!(
                        "style.background.padding must have 1 or 2 numbers, got {}",
                        axes.len()
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ShapeKind {
    Rectangle,
    Ellipse,
    Line,
}

/// `parameters` of a `generator_kind: "Shape"` reference. Widths and radii
/// are pixels of a 1080-high frame; line ends are fractions of the clip's box.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct ShapeParameters {
    pub shape: ShapeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner_radius: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x1: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y1: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x2: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y2: Option<f64>,
    #[serde(flatten)]
    pub other: Extra,
}

impl ShapeParameters {
    pub fn new(shape: ShapeKind) -> Self {
        ShapeParameters {
            shape,
            fill: None,
            stroke: None,
            stroke_width: None,
            corner_radius: None,
            x1: None,
            y1: None,
            x2: None,
            y2: None,
            other: Extra::new(),
        }
    }
}

/// Why typed generator parameters could not be read or written.
#[derive(Debug)]
pub enum GeneratorError {
    /// The reference is not a generator of the expected kind.
    WrongKind {
        expected: &'static str,
        found: Option<String>,
    },
    /// The clip has no active media reference.
    NoActiveReference,
    /// The parameters do not match the schema.
    Invalid(String),
}

impl std::fmt::Display for GeneratorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GeneratorError::WrongKind { expected, found: Some(found) } => {
                write!(f, "expected a {expected} generator, found generator_kind {found:?}")
            }
            GeneratorError::WrongKind { expected, found: None } => {
                write!(f, "expected a {expected} generator, found an external reference")
            }
            GeneratorError::NoActiveReference => write!(f, "clip has no active media reference"),
            GeneratorError::Invalid(message) => write!(f, "invalid generator parameters: {message}"),
        }
    }
}

impl std::error::Error for GeneratorError {}

fn to_object<T: Serialize>(value: &T) -> Result<Extra, GeneratorError> {
    match serde_json::to_value(value).map_err(|e| GeneratorError::Invalid(e.to_string()))? {
        serde_json::Value::Object(map) => Ok(map),
        _ => Err(GeneratorError::Invalid("parameters must be an object".to_string())),
    }
}

fn generator_reference(kind: &str, parameters: Extra, name: Option<String>) -> MediaReference {
    MediaReference::GeneratorReference {
        generator_kind: kind.to_string(),
        available_range: None,
        name,
        available_image_bounds: None,
        // The shape reading a reference back gives, so a new clip equals its
        // own round trip.
        metadata: serde_json::json!({"tellers.ai": {}}),
        parameters: GeneratorParameters {
            resolve_otio: None,
            other: parameters,
        },
    }
}

impl MediaReference {
    pub fn is_text_generator(&self) -> bool {
        self.generator_kind().map(String::as_str) == Some(TEXT_GENERATOR_KIND)
    }

    pub fn is_shape_generator(&self) -> bool {
        self.generator_kind().map(String::as_str) == Some(SHAPE_GENERATOR_KIND)
    }

    /// A `generator_kind: "Text"` reference.
    pub fn new_text(parameters: &TextParameters, name: Option<String>) -> Result<Self, GeneratorError> {
        parameters.validate().map_err(GeneratorError::Invalid)?;
        Ok(generator_reference(TEXT_GENERATOR_KIND, to_object(parameters)?, name))
    }

    /// A `generator_kind: "Shape"` reference.
    pub fn new_shape(parameters: &ShapeParameters, name: Option<String>) -> Result<Self, GeneratorError> {
        Ok(generator_reference(SHAPE_GENERATOR_KIND, to_object(parameters)?, name))
    }

    fn generator_object(&self, expected: &'static str) -> Result<&Extra, GeneratorError> {
        match self {
            MediaReference::GeneratorReference { generator_kind, parameters, .. }
                if generator_kind == expected =>
            {
                Ok(&parameters.other)
            }
            _ => Err(GeneratorError::WrongKind {
                expected,
                found: self.generator_kind().cloned(),
            }),
        }
    }

    fn generator_object_mut(&mut self, expected: &'static str) -> Result<&mut Extra, GeneratorError> {
        let found = self.generator_kind().cloned();
        match self {
            MediaReference::GeneratorReference { generator_kind, parameters, .. }
                if generator_kind == expected =>
            {
                Ok(&mut parameters.other)
            }
            _ => Err(GeneratorError::WrongKind { expected, found }),
        }
    }

    /// The typed parameters of a Text generator.
    pub fn text_parameters(&self) -> Result<TextParameters, GeneratorError> {
        let object = self.generator_object(TEXT_GENERATOR_KIND)?;
        serde_json::from_value(serde_json::Value::Object(object.clone()))
            .map_err(|e| GeneratorError::Invalid(e.to_string()))
    }

    /// Replaces the parameters of a Text generator; keys the struct keeps in
    /// `other` are written back as they are.
    pub fn set_text_parameters(&mut self, parameters: &TextParameters) -> Result<(), GeneratorError> {
        parameters.validate().map_err(GeneratorError::Invalid)?;
        let object = to_object(parameters)?;
        *self.generator_object_mut(TEXT_GENERATOR_KIND)? = object;
        Ok(())
    }

    /// The typed parameters of a Shape generator.
    pub fn shape_parameters(&self) -> Result<ShapeParameters, GeneratorError> {
        let object = self.generator_object(SHAPE_GENERATOR_KIND)?;
        serde_json::from_value(serde_json::Value::Object(object.clone()))
            .map_err(|e| GeneratorError::Invalid(e.to_string()))
    }

    /// Replaces the parameters of a Shape generator; keys the struct keeps in
    /// `other` are written back as they are.
    pub fn set_shape_parameters(&mut self, parameters: &ShapeParameters) -> Result<(), GeneratorError> {
        let object = to_object(parameters)?;
        *self.generator_object_mut(SHAPE_GENERATOR_KIND)? = object;
        Ok(())
    }
}

impl Clip {
    fn active_reference(&self) -> Result<&MediaReference, GeneratorError> {
        self.active_media_reference_key
            .as_ref()
            .and_then(|key| self.media_references.get(key))
            .ok_or(GeneratorError::NoActiveReference)
    }

    fn active_reference_mut(&mut self) -> Result<&mut MediaReference, GeneratorError> {
        self.active_media_reference_key
            .as_ref()
            .and_then(|key| self.media_references.get_mut(key))
            .ok_or(GeneratorError::NoActiveReference)
    }

    /// A clip of `duration` seconds playing a Text generator.
    pub fn new_text(
        duration: Seconds,
        parameters: &TextParameters,
        name: Option<String>,
        id: Option<String>,
    ) -> Result<Self, GeneratorError> {
        let reference = MediaReference::new_text(parameters, Some("Text".to_string()))?;
        Ok(Clip::new_single_media_reference(
            TimeRange::new(duration, 0.0),
            reference,
            name,
            id,
        ))
    }

    /// A clip of `duration` seconds playing a Shape generator.
    pub fn new_shape(
        duration: Seconds,
        parameters: &ShapeParameters,
        name: Option<String>,
        id: Option<String>,
    ) -> Result<Self, GeneratorError> {
        let reference = MediaReference::new_shape(parameters, Some("Shape".to_string()))?;
        Ok(Clip::new_single_media_reference(
            TimeRange::new(duration, 0.0),
            reference,
            name,
            id,
        ))
    }

    pub fn is_text_clip(&self) -> bool {
        self.active_reference().is_ok_and(MediaReference::is_text_generator)
    }

    pub fn is_shape_clip(&self) -> bool {
        self.active_reference().is_ok_and(MediaReference::is_shape_generator)
    }

    pub fn text_parameters(&self) -> Result<TextParameters, GeneratorError> {
        self.active_reference()?.text_parameters()
    }

    pub fn set_text_parameters(&mut self, parameters: &TextParameters) -> Result<(), GeneratorError> {
        self.active_reference_mut()?.set_text_parameters(parameters)
    }

    pub fn shape_parameters(&self) -> Result<ShapeParameters, GeneratorError> {
        self.active_reference()?.shape_parameters()
    }

    pub fn set_shape_parameters(&mut self, parameters: &ShapeParameters) -> Result<(), GeneratorError> {
        self.active_reference_mut()?.set_shape_parameters(parameters)
    }

    /// The string a Text clip shows (`text`, else its words joined).
    pub fn get_text(&self) -> Option<String> {
        self.text_parameters().ok()?.display_text()
    }

    /// Sets the string of a Text clip, dropping its word timings; the style
    /// and other parameters stay.
    pub fn set_text(&mut self, text: impl Into<String>) -> Result<(), GeneratorError> {
        let mut parameters = self.text_parameters()?;
        parameters.text = Some(text.into());
        parameters.words = None;
        self.set_text_parameters(&parameters)
    }
}
