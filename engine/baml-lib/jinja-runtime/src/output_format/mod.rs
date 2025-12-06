pub mod types;

use std::str::FromStr;

use minijinja::{value::Kwargs, ErrorKind, Value};
use strum::VariantNames;
use types::HoistClasses;

use self::types::OutputFormatContent;
use crate::{types::RenderOptions, RenderContext};

// TODO: Rename the field to `content`.
#[derive(Debug)]
pub struct OutputFormat {
    text: OutputFormatContent,
}

impl OutputFormat {
    pub fn new(ctx: RenderContext) -> Self {
        Self {
            text: ctx.output_format,
        }
    }
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let content = self
            .text
            .render(RenderOptions::default())
            .map_err(|_e| std::fmt::Error {})?;

        match content {
            Some(content) => write!(f, "{content}"),
            None => Ok(()),
        }
    }
}

// TODO: do this but for a class. Use the display method to render the alias.
impl minijinja::value::Object for OutputFormat {
    fn call(
        self: &std::sync::Arc<Self>,
        _state: &minijinja::State<'_, '_>,
        args: &[minijinja::value::Value],
    ) -> Result<minijinja::value::Value, minijinja::Error> {
        use minijinja::{value::from_args, Error};

        let (args, kwargs): (&[Value], Kwargs) = from_args(args)?;
        if !args.is_empty() {
            return Err(Error::new(
                ErrorKind::TooManyArguments,
                "output_format() may only be called with named arguments".to_string(),
            ));
        }

        let prefix = if kwargs.has("prefix") {
            match kwargs.get::<Option<String>>("prefix") {
                Ok(prefix) => Some(prefix),
                Err(e) => {
                    return Err(Error::new(
                        ErrorKind::SyntaxError,
                        format!("Invalid value for prefix (expected string | null): {e}"),
                    ))
                }
            }
        } else {
            None
        };

        let or_splitter = if kwargs.has("or_splitter") {
            match kwargs.get::<String>("or_splitter") {
                Ok(prefix) => Some(prefix),
                Err(e) => {
                    return Err(Error::new(
                        ErrorKind::SyntaxError,
                        format!("Invalid value for or_splitter (expected string): {e}"),
                    ))
                }
            }
        } else {
            None
        };

        let enum_value_prefix = if kwargs.has("enum_value_prefix") {
            match kwargs.get::<Option<String>>("enum_value_prefix") {
                Ok(prefix) => Some(prefix),
                Err(e) => {
                    return Err(Error::new(
                        ErrorKind::SyntaxError,
                        format!(
                            "Invalid value for enum_value_prefix (expected string | null): {e}"
                        ),
                    ))
                }
            }
        } else {
            None
        };

        let always_hoist_enums = if kwargs.has("always_hoist_enums") {
            match kwargs.get::<bool>("always_hoist_enums") {
                Ok(always_hoist_enums) => Some(always_hoist_enums),
                Err(e) => {
                    return Err(Error::new(
                        ErrorKind::SyntaxError,
                        format!("Invalid value for always_hoist_enums (expected bool): {e}"),
                    ))
                }
            }
        } else {
            None
        };

        let hoisted_class_prefix = if kwargs.has("hoisted_class_prefix") {
            match kwargs.get::<Option<String>>("hoisted_class_prefix") {
                Ok(hoisted_class_prefix) => Some(hoisted_class_prefix),
                Err(e) => {
                    return Err(Error::new(
                        ErrorKind::SyntaxError,
                        format!(
                            "Invalid value for hoisted_class_prefix (expected string | null): {e}"
                        ),
                    ))
                }
            }
        } else {
            None
        };

        let hoist_classes = if kwargs.has("hoist_classes") {
            // true | false
            match kwargs.get::<bool>("hoist_classes") {
                Ok(true) => Some(HoistClasses::All),
                Ok(false) => Some(HoistClasses::Auto),
                // auto
                Err(_) => match kwargs.get::<String>("hoist_classes") {
                    Ok(s) if s == "auto" => Some(HoistClasses::Auto),
                    // subset
                    _ => match kwargs.get::<Vec<String>>("hoist_classes") {
                        Ok(classes) => Some(HoistClasses::Subset(classes)),
                        Err(e) => return Err(Error::new(
                            ErrorKind::SyntaxError,
                            format!("Invalid value for hoist_classes (expected one of bool | \"auto\" | string[]): {e}")
                        ))
                    }
                }
            }
        } else {
            None
        };

        let map_style = if kwargs.has("map_style") {
            match kwargs
                .get::<String>("map_style")
                .map(|s| types::MapStyle::from_str(s.as_str()))
            {
                Ok(Ok(map_style)) => Some(map_style),
                Ok(Err(e)) => {
                    return Err(Error::new(
                        ErrorKind::SyntaxError,
                        format!(
                            "Invalid value for map_style (expected one of {}): {}",
                            types::MapStyle::VARIANTS.join(", "),
                            e
                        ),
                    ))
                }
                Err(e) => {
                    return Err(Error::new(
                        ErrorKind::SyntaxError,
                        format!(
                            "Invalid value for map_style (expected one of {}): {}",
                            types::MapStyle::VARIANTS.join(", "),
                            e
                        ),
                    ))
                }
            }
        } else {
            None
        };

        let quote_class_fields = if kwargs.has("quote_class_fields") {
            match kwargs.get::<bool>("quote_class_fields") {
                Ok(quote_class_fields) => Some(quote_class_fields),
                Err(e) => {
                    return Err(Error::new(
                        ErrorKind::SyntaxError,
                        format!("Invalid value for quote_class_fields (expected bool): {e}"),
                    ))
                }
            }
        } else {
            None
        };

        let Ok(_) = kwargs.assert_all_used() else {
            return Err(Error::new(
                ErrorKind::TooManyArguments,
                "output_format() got an unexpected keyword argument (only 'prefix', 'always_hoist_enums', 'enum_value_prefix', 'or_splitter', 'hoisted_class_prefix', 'hoist_classes', 'map_style', and 'quote_class_fields' are allowed)",
            ));
        };

        let content = self.text.render(RenderOptions::new(
            prefix,
            or_splitter,
            enum_value_prefix,
            always_hoist_enums,
            map_style,
            hoisted_class_prefix,
            hoist_classes,
            quote_class_fields,
        ))?;

        match content {
            Some(content) => Ok(Value::from_safe_string(content)),
            None => Ok(Value::from_serialize("")),
        }
    }

    fn call_method(
        self: &std::sync::Arc<Self>,
        _state: &minijinja::State<'_, '_>,
        name: &str,
        _args: &[minijinja::value::Value],
    ) -> Result<minijinja::value::Value, minijinja::Error> {
        Err(minijinja::Error::new(
            ErrorKind::UnknownMethod,
            format!("output_format has no callable attribute '{name}'"),
        ))
    }

    fn render(self: &std::sync::Arc<Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}
