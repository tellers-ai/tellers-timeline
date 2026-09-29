//! Per-clip composite modes: how a clip's pixels combine with the tracks
//! rendered beneath it.
//!
//! DaVinci Resolve exports the clip's Inspector "Composite Mode" as a Resolve
//! OTIO clip effect named `Composite` (type 1) whose single `UInt` parameter
//! `"composite mode"` holds the index of the mode in Resolve's scripting-API
//! enum (`COMPOSITE_NORMAL = 0`, then in declaration order). The player
//! (`video-player-js`) reads the same effect, so [`Clip::set_composite_mode`]
//! writes exactly that shape.
//!
//! Modes fall into two families:
//! - blend modes, drawn on top of the composite below with a blend function;
//! - mask modes ("composite masking"): the clip itself is not shown, its alpha
//!   or luminance becomes a matte that cuts out everything below it.
//!
//! [`Clip::set_composite_mode`]: crate::Clip::set_composite_mode

use std::fmt;
use std::str::FromStr;

use crate::{
    default_effect_schema, Clip, Effect, EffectMetadata, Item, ResolveOTIOEffect,
    ResolveOTIOParameter, ResolveOTIOParameterNumber,
};

/// `Resolve_OTIO["Effect Name"]` of the composite effect.
pub const RESOLVE_COMPOSITE_EFFECT_NAME: &str = "Composite";
/// `Resolve_OTIO["Type"]` of the composite effect.
pub const RESOLVE_COMPOSITE_EFFECT_TYPE: u64 = 1;
/// `Parameter ID` carrying the mode code.
pub const RESOLVE_COMPOSITE_MODE_PARAMETER_ID: &str = "composite mode";

/// A clip composite mode. The discriminant is Resolve's numeric code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum CompositeMode {
    #[default]
    Normal = 0,
    Add = 1,
    Subtract = 2,
    Difference = 3,
    Multiply = 4,
    Screen = 5,
    Overlay = 6,
    HardLight = 7,
    SoftLight = 8,
    Darken = 9,
    Lighten = 10,
    ColorDodge = 11,
    ColorBurn = 12,
    Exclusion = 13,
    Hue = 14,
    Saturation = 15,
    Color = 16,
    LumaMask = 17,
    Divide = 18,
    LinearDodge = 19,
    LinearBurn = 20,
    LinearLight = 21,
    VividLight = 22,
    PinLight = 23,
    HardMix = 24,
    LighterColor = 25,
    DarkerColor = 26,
    /// The clip is shown fully opaque, ignoring its alpha.
    Foreground = 27,
    /// Mask: the clip's alpha mattes the composite below.
    Alpha = 28,
    /// Mask: the inverse of the clip's alpha mattes the composite below.
    InvertedAlpha = 29,
    /// Mask: the clip's luminance mattes the composite below.
    Luminosity = 30,
    /// Mask: the inverse of the clip's luminance mattes the composite below.
    InvertedLuminosity = 31,
}

impl CompositeMode {
    /// Every mode, in Resolve code order.
    pub const ALL: [CompositeMode; 32] = [
        CompositeMode::Normal,
        CompositeMode::Add,
        CompositeMode::Subtract,
        CompositeMode::Difference,
        CompositeMode::Multiply,
        CompositeMode::Screen,
        CompositeMode::Overlay,
        CompositeMode::HardLight,
        CompositeMode::SoftLight,
        CompositeMode::Darken,
        CompositeMode::Lighten,
        CompositeMode::ColorDodge,
        CompositeMode::ColorBurn,
        CompositeMode::Exclusion,
        CompositeMode::Hue,
        CompositeMode::Saturation,
        CompositeMode::Color,
        CompositeMode::LumaMask,
        CompositeMode::Divide,
        CompositeMode::LinearDodge,
        CompositeMode::LinearBurn,
        CompositeMode::LinearLight,
        CompositeMode::VividLight,
        CompositeMode::PinLight,
        CompositeMode::HardMix,
        CompositeMode::LighterColor,
        CompositeMode::DarkerColor,
        CompositeMode::Foreground,
        CompositeMode::Alpha,
        CompositeMode::InvertedAlpha,
        CompositeMode::Luminosity,
        CompositeMode::InvertedLuminosity,
    ];

    /// The numeric `"composite mode"` value Resolve writes for this mode.
    pub fn resolve_code(self) -> u64 {
        self as u64
    }

    /// The mode for a Resolve `"composite mode"` value, or None when unknown.
    pub fn from_resolve_code(code: u64) -> Option<CompositeMode> {
        CompositeMode::ALL.get(code as usize).copied()
    }

    /// The stable kebab-case name of this mode (`"hard-light"`, `"inverted-alpha"`),
    /// shared with the player's `compositeMode` output field.
    pub fn as_str(self) -> &'static str {
        match self {
            CompositeMode::Normal => "normal",
            CompositeMode::Add => "add",
            CompositeMode::Subtract => "subtract",
            CompositeMode::Difference => "difference",
            CompositeMode::Multiply => "multiply",
            CompositeMode::Screen => "screen",
            CompositeMode::Overlay => "overlay",
            CompositeMode::HardLight => "hard-light",
            CompositeMode::SoftLight => "soft-light",
            CompositeMode::Darken => "darken",
            CompositeMode::Lighten => "lighten",
            CompositeMode::ColorDodge => "color-dodge",
            CompositeMode::ColorBurn => "color-burn",
            CompositeMode::Exclusion => "exclusion",
            CompositeMode::Hue => "hue",
            CompositeMode::Saturation => "saturation",
            CompositeMode::Color => "color",
            CompositeMode::LumaMask => "luma-mask",
            CompositeMode::Divide => "divide",
            CompositeMode::LinearDodge => "linear-dodge",
            CompositeMode::LinearBurn => "linear-burn",
            CompositeMode::LinearLight => "linear-light",
            CompositeMode::VividLight => "vivid-light",
            CompositeMode::PinLight => "pin-light",
            CompositeMode::HardMix => "hard-mix",
            CompositeMode::LighterColor => "lighter-color",
            CompositeMode::DarkerColor => "darker-color",
            CompositeMode::Foreground => "foreground",
            CompositeMode::Alpha => "alpha",
            CompositeMode::InvertedAlpha => "inverted-alpha",
            CompositeMode::Luminosity => "luminosity",
            CompositeMode::InvertedLuminosity => "inverted-luminosity",
        }
    }

    /// True for the modes that matte the composite below instead of drawing
    /// the clip (Alpha, Inverted Alpha, Luminosity, Inverted Luminosity, Luma Mask).
    pub fn is_mask(self) -> bool {
        matches!(
            self,
            CompositeMode::LumaMask
                | CompositeMode::Alpha
                | CompositeMode::InvertedAlpha
                | CompositeMode::Luminosity
                | CompositeMode::InvertedLuminosity
        )
    }
}

impl fmt::Display for CompositeMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error returned when a string names no composite mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownCompositeMode(pub String);

impl fmt::Display for UnknownCompositeMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown composite mode: {:?}", self.0)
    }
}

impl std::error::Error for UnknownCompositeMode {}

impl FromStr for CompositeMode {
    type Err = UnknownCompositeMode;

    /// Parses a mode name in any casing, with `-`, `_` or spaces as separators
    /// (`"Hard Light"`, `"hard_light"`, `"hard-light"`), plus Resolve's own
    /// display names that differ from the enum (`"Colorize"`, `"Hardlight"`).
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut normalized = String::with_capacity(value.len());
        let mut pending_separator = false;
        for ch in value.trim().chars() {
            if ch == '-' || ch == '_' || ch.is_whitespace() {
                pending_separator = !normalized.is_empty();
                continue;
            }
            if pending_separator {
                normalized.push('-');
                pending_separator = false;
            }
            normalized.extend(ch.to_lowercase());
        }
        if let Some(mode) = CompositeMode::ALL.iter().find(|mode| mode.as_str() == normalized) {
            return Ok(*mode);
        }
        match normalized.as_str() {
            "hardlight" => Ok(CompositeMode::HardLight),
            "softlight" => Ok(CompositeMode::SoftLight),
            "colorize" => Ok(CompositeMode::Color),
            "saturate" => Ok(CompositeMode::Saturation),
            "diff" => Ok(CompositeMode::Difference),
            "lum" => Ok(CompositeMode::Luminosity),
            "inverted-lum" => Ok(CompositeMode::InvertedLuminosity),
            _ => Err(UnknownCompositeMode(value.to_string())),
        }
    }
}

fn is_composite_effect(effect: &Effect) -> Option<&ResolveOTIOEffect> {
    if effect.effect_name != "Resolve Effect" {
        return None;
    }
    let resolve_otio = effect.metadata.resolve_otio.as_ref()?;
    if resolve_otio.effect_name == RESOLVE_COMPOSITE_EFFECT_NAME
        || resolve_otio.name == RESOLVE_COMPOSITE_EFFECT_NAME
    {
        Some(resolve_otio)
    } else {
        None
    }
}

fn composite_mode_code(resolve_otio: &ResolveOTIOEffect) -> Option<u64> {
    resolve_otio.parameters.iter().find_map(|parameter| match parameter {
        ResolveOTIOParameter::UInt(param)
            if param.parameter_id == RESOLVE_COMPOSITE_MODE_PARAMETER_ID =>
        {
            Some(param.parameter_value)
        }
        ResolveOTIOParameter::Int(param)
            if param.parameter_id == RESOLVE_COMPOSITE_MODE_PARAMETER_ID =>
        {
            u64::try_from(param.parameter_value).ok()
        }
        _ => None,
    })
}

impl Clip {
    /// The clip's composite mode, read from its Resolve `Composite` effect.
    ///
    /// Normal when the effect is absent, disabled, has no `"composite mode"`
    /// parameter (Resolve exports the default that way), or carries a code
    /// this crate does not know.
    pub fn get_composite_mode(&self) -> CompositeMode {
        self.effects
            .iter()
            .find_map(is_composite_effect)
            .filter(|resolve_otio| resolve_otio.enabled)
            .and_then(composite_mode_code)
            .and_then(CompositeMode::from_resolve_code)
            .unwrap_or_default()
    }

    /// Set the clip's composite mode by replacing its Resolve `Composite`
    /// effect with one Resolve and the player both read.
    pub fn set_composite_mode(&mut self, mode: CompositeMode) {
        self.effects.retain(|effect| is_composite_effect(effect).is_none());
        self.effects.push(Effect {
            otio_schema: default_effect_schema(),
            name: "".to_string(),
            effect_name: "Resolve Effect".to_string(),
            metadata: EffectMetadata {
                resolve_otio: Some(ResolveOTIOEffect {
                    effect_name: RESOLVE_COMPOSITE_EFFECT_NAME.to_string(),
                    enabled: true,
                    name: RESOLVE_COMPOSITE_EFFECT_NAME.to_string(),
                    parameters: vec![ResolveOTIOParameter::UInt(ResolveOTIOParameterNumber {
                        variant_type: "UInt".to_string(),
                        parameter_id: RESOLVE_COMPOSITE_MODE_PARAMETER_ID.to_string(),
                        parameter_value: mode.resolve_code(),
                        default_parameter_value: Some(CompositeMode::Normal.resolve_code()),
                        max_value: None,
                        min_value: None,
                    })],
                    effect_type: RESOLVE_COMPOSITE_EFFECT_TYPE,
                }),
                other: serde_json::Map::new(),
            },
        });
    }
}

impl Item {
    /// The composite mode of a clip; Normal for gaps.
    pub fn get_composite_mode(&self) -> CompositeMode {
        match self {
            Item::Clip(c) => c.get_composite_mode(),
            Item::Gap(_g) => CompositeMode::Normal,
        }
    }

    /// Set the composite mode of a clip; no-op for gaps.
    pub fn set_composite_mode(&mut self, mode: CompositeMode) {
        if let Item::Clip(c) = self {
            c.set_composite_mode(mode);
        }
    }
}
