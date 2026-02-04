//! `OutputFormat` object exposed to Jinja templates as `ctx.output_format`.
//! Ported from engine/baml-lib/jinja-runtime/src/output_format/mod.rs

use std::sync::Arc;

use llm_types::{HoistClasses, MapStyle, OutputFormatContent, RenderOptions, RenderSetting};
use minijinja::{
    ErrorKind,
    value::{Kwargs, Value},
};

/// Wrapper around `OutputFormatContent` that implements `minijinja::value::Object`.
#[derive(Debug)]
pub struct OutputFormatObject {
    content: OutputFormatContent,
}

impl OutputFormatObject {
    pub fn new(content: OutputFormatContent) -> Self {
        Self { content }
    }
}

impl std::fmt::Display for OutputFormatObject {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let rendered = self
            .content
            .render(&RenderOptions::default())
            .map_err(|_| std::fmt::Error)?;
        match rendered {
            Some(s) => write!(f, "{s}"),
            None => Ok(()),
        }
    }
}

impl minijinja::value::Object for OutputFormatObject {
    fn call(
        self: &Arc<Self>,
        _state: &minijinja::State<'_, '_>,
        args: &[Value],
    ) -> Result<Value, minijinja::Error> {
        use minijinja::{Error, value::from_args};

        let (args, kwargs): (&[Value], Kwargs) = from_args(args)?;
        if !args.is_empty() {
            return Err(Error::new(
                ErrorKind::TooManyArguments,
                "output_format() may only be called with named arguments",
            ));
        }

        // Parse kwargs - matching engine/baml-lib/jinja-runtime/src/output_format/mod.rs
        let prefix = parse_render_setting_string(&kwargs, "prefix")?;
        let or_splitter = parse_render_setting_string(&kwargs, "or_splitter")?;
        let enum_value_prefix = parse_render_setting_string(&kwargs, "enum_value_prefix")?;
        let always_hoist_enums = parse_render_setting_bool(&kwargs, "always_hoist_enums")?;
        let hoisted_class_prefix = parse_render_setting_string(&kwargs, "hoisted_class_prefix")?;
        let hoist_classes = parse_hoist_classes_kwarg(&kwargs)?;
        let map_style = parse_map_style_kwarg(&kwargs)?;
        let quote_class_fields = parse_render_setting_bool(&kwargs, "quote_class_fields")?;

        kwargs.assert_all_used().map_err(|_| {
            Error::new(
                ErrorKind::TooManyArguments,
                "output_format() got an unexpected keyword argument",
            )
        })?;

        let options = RenderOptions {
            prefix,
            or_splitter,
            enum_value_prefix,
            hoisted_class_prefix,
            hoist_classes,
            always_hoist_enums,
            map_style,
            quote_class_fields,
        };

        let rendered = self
            .content
            .render(&options)
            .map_err(|e| Error::new(ErrorKind::BadSerialization, e.to_string()))?;

        match rendered {
            Some(s) => Ok(Value::from_safe_string(s)),
            None => Ok(Value::from("")),
        }
    }

    fn render(self: &Arc<Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

// Helper functions for parsing kwargs

/// Parse a kwarg that returns `RenderSetting<String>`:
/// - Not present -> Auto
/// - Present with null/None -> Never
/// - Present with value -> Always(value)
fn parse_render_setting_string(
    kwargs: &Kwargs,
    name: &str,
) -> Result<RenderSetting<String>, minijinja::Error> {
    if !kwargs.has(name) {
        return Ok(RenderSetting::Auto);
    }
    match kwargs.get::<Option<String>>(name) {
        Ok(Some(v)) => Ok(RenderSetting::Always(v)),
        Ok(None) => Ok(RenderSetting::Never),
        Err(e) => Err(minijinja::Error::new(
            ErrorKind::SyntaxError,
            format!("Invalid value for {name}: {e}"),
        )),
    }
}

/// Parse a kwarg that returns `RenderSetting<bool>`:
/// - Not present -> Auto
/// - Present with value -> Always(value)
fn parse_render_setting_bool(
    kwargs: &Kwargs,
    name: &str,
) -> Result<RenderSetting<bool>, minijinja::Error> {
    if !kwargs.has(name) {
        return Ok(RenderSetting::Auto);
    }
    match kwargs.get::<bool>(name) {
        Ok(v) => Ok(RenderSetting::Always(v)),
        Err(e) => Err(minijinja::Error::new(
            ErrorKind::SyntaxError,
            format!("Invalid value for {name}: {e}"),
        )),
    }
}

/// Parse `hoist_classes` kwarg:
/// - Not present -> Auto
/// - true -> All
/// - false -> Auto
/// - "auto" -> Auto
/// - string[] -> Subset(classes)
fn parse_hoist_classes_kwarg(kwargs: &Kwargs) -> Result<HoistClasses, minijinja::Error> {
    if !kwargs.has("hoist_classes") {
        return Ok(HoistClasses::Auto);
    }
    // Try bool first
    if let Ok(b) = kwargs.get::<bool>("hoist_classes") {
        return Ok(if b {
            HoistClasses::All
        } else {
            HoistClasses::Auto
        });
    }
    // Try "auto" string
    if let Ok(s) = kwargs.get::<String>("hoist_classes") {
        if s == "auto" {
            return Ok(HoistClasses::Auto);
        }
    }
    // Try array of strings
    if let Ok(classes) = kwargs.get::<Vec<String>>("hoist_classes") {
        return Ok(HoistClasses::Subset(classes));
    }
    Err(minijinja::Error::new(
        ErrorKind::SyntaxError,
        "Invalid value for hoist_classes (expected bool | \"auto\" | string[])",
    ))
}

/// Parse `map_style` kwarg:
/// - Not present -> `TypeParameters` (default)
/// - "`type_parameters`" -> `TypeParameters`
/// - "`object_literal`" -> `ObjectLiteral`
fn parse_map_style_kwarg(kwargs: &Kwargs) -> Result<MapStyle, minijinja::Error> {
    if !kwargs.has("map_style") {
        return Ok(MapStyle::default());
    }
    match kwargs.get::<String>("map_style") {
        Ok(s) => match s.as_str() {
            "type_parameters" => Ok(MapStyle::TypeParameters),
            "object_literal" => Ok(MapStyle::ObjectLiteral),
            _ => Err(minijinja::Error::new(
                ErrorKind::SyntaxError,
                format!("Invalid map_style: {s} (expected 'type_parameters' or 'object_literal')"),
            )),
        },
        Err(e) => Err(minijinja::Error::new(
            ErrorKind::SyntaxError,
            format!("Invalid value for map_style: {e}"),
        )),
    }
}
