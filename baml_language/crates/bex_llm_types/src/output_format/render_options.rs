//! Render options for output format rendering.
//!
//! These types control how `ctx.output_format` renders type schemas.

/// A setting that can be auto-determined, always set, or never set.
#[derive(Debug, Clone, Default)]
pub enum RenderSetting<T> {
    /// Let the renderer decide based on context.
    #[default]
    Auto,
    /// Always use the specified value.
    Always(T),
    /// Never use this setting (explicit null).
    Never,
}

/// Map rendering style.
#[derive(Debug, Clone, Copy, Default)]
pub enum MapStyle {
    /// Render as `map<K, V>` (angle bracket style).
    #[default]
    TypeParameters,
    /// Render as `{K: V}` (object literal style).
    ObjectLiteral,
}

impl std::str::FromStr for MapStyle {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "angle" => Ok(MapStyle::TypeParameters),
            "object" => Ok(MapStyle::ObjectLiteral),
            _ => Err(format!(
                "Invalid map_style '{}', expected 'angle' or 'object'",
                s
            )),
        }
    }
}

/// Hoist classes setting.
///
/// Recursive classes are always hoisted regardless of this setting.
#[derive(Debug, Clone, Default)]
pub enum HoistClasses {
    /// Hoist all classes.
    All,
    /// Hoist only the specified subset.
    Subset(Vec<String>),
    /// Default behavior: hoist only recursive classes.
    #[default]
    Auto,
}

/// Maximum number of variants in an enum before it gets hoisted.
pub const INLINE_RENDER_ENUM_MAX_VALUES: usize = 6;

/// Options for rendering output format.
///
/// These correspond to the kwargs accepted by `ctx.output_format()`.
#[derive(Debug, Clone)]
pub struct OutputFormatOptions {
    /// Custom prefix text (e.g., "Answer in JSON using this schema:").
    /// Auto = determine based on type, Never = no prefix, Always = use specified.
    pub prefix: RenderSetting<String>,

    /// Delimiter for union types (default: " or ").
    pub or_splitter: String,

    /// Prefix for enum values (Auto = "- ", Never = "", Always = specified).
    pub enum_value_prefix: RenderSetting<String>,

    /// Whether to always hoist enums (default: Auto = hoist if >6 variants or has descriptions).
    pub always_hoist_enums: RenderSetting<bool>,

    /// Prefix for hoisted class definitions (Auto = "schema", Never = "", Always = specified).
    pub hoisted_class_prefix: RenderSetting<String>,

    /// Control which classes to hoist.
    pub hoist_classes: HoistClasses,

    /// Map rendering style (angle brackets vs object literal).
    pub map_style: MapStyle,

    /// Whether to quote class field names.
    pub quote_class_fields: bool,
}

impl Default for OutputFormatOptions {
    fn default() -> Self {
        Self {
            prefix: RenderSetting::Auto,
            or_splitter: Self::DEFAULT_OR_SPLITTER.to_string(),
            enum_value_prefix: RenderSetting::Auto,
            always_hoist_enums: RenderSetting::Auto,
            hoisted_class_prefix: RenderSetting::Auto,
            hoist_classes: HoistClasses::Auto,
            map_style: MapStyle::TypeParameters,
            quote_class_fields: false,
        }
    }
}

impl OutputFormatOptions {
    /// Default delimiter for union types.
    pub const DEFAULT_OR_SPLITTER: &'static str = " or ";

    /// Default type prefix used in "Answer in JSON using this {prefix}:" messages.
    pub const DEFAULT_TYPE_PREFIX: &'static str = "schema";

    /// Create OutputFormatOptions from kwargs values.
    ///
    /// The `Option<Option<T>>` pattern means:
    /// - `None` = user didn't provide the parameter
    /// - `Some(None)` = user explicitly set to null
    /// - `Some(Some(value))` = user provided a value
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        prefix: Option<Option<String>>,
        or_splitter: Option<String>,
        enum_value_prefix: Option<Option<String>>,
        always_hoist_enums: Option<bool>,
        map_style: Option<MapStyle>,
        hoisted_class_prefix: Option<Option<String>>,
        hoist_classes: Option<HoistClasses>,
        quote_class_fields: Option<bool>,
    ) -> Self {
        Self {
            prefix: prefix.map_or(RenderSetting::Auto, |p| {
                p.map_or(RenderSetting::Never, RenderSetting::Always)
            }),
            or_splitter: or_splitter.unwrap_or_else(|| Self::DEFAULT_OR_SPLITTER.to_string()),
            enum_value_prefix: enum_value_prefix.map_or(RenderSetting::Auto, |p| {
                p.map_or(RenderSetting::Never, RenderSetting::Always)
            }),
            always_hoist_enums: always_hoist_enums
                .map_or(RenderSetting::Auto, RenderSetting::Always),
            hoisted_class_prefix: hoisted_class_prefix.map_or(RenderSetting::Auto, |p| {
                p.map_or(RenderSetting::Never, RenderSetting::Always)
            }),
            hoist_classes: hoist_classes.unwrap_or(HoistClasses::Auto),
            map_style: map_style.unwrap_or(MapStyle::TypeParameters),
            quote_class_fields: quote_class_fields.unwrap_or(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_style_from_str() {
        assert!(matches!(
            "angle".parse::<MapStyle>().unwrap(),
            MapStyle::TypeParameters
        ));
        assert!(matches!(
            "object".parse::<MapStyle>().unwrap(),
            MapStyle::ObjectLiteral
        ));
        assert!("invalid".parse::<MapStyle>().is_err());
    }

    #[test]
    fn test_render_options_default() {
        let opts = OutputFormatOptions::default();
        assert_eq!(opts.or_splitter, " or ");
        assert!(!opts.quote_class_fields);
        assert!(matches!(opts.map_style, MapStyle::TypeParameters));
    }
}
