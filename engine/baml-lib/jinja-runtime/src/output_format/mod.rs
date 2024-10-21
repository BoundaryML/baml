pub mod types;

use std::str::FromStr;

use minijinja::{value::Kwargs, ErrorKind, Value};
use strum::VariantNames;

use crate::{types::RenderOptions, RenderContext};

use self::types::OutputFormatContent;

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
            .map_err(|e| std::fmt::Error {})?;

        match content {
            Some(content) => write!(f, "{content}"),
            None => Ok(()),
        }
    }
}

// TODO: do this but for a class. Use the display method to render the alias.
impl minijinja::value::Object for OutputFormat {
    fn call(
        &self,
        _state: &minijinja::State<'_, '_>,
        args: &[minijinja::value::Value],
    ) -> Result<minijinja::value::Value, minijinja::Error> {
        use minijinja::{value::from_args, Error};

        let (args, kwargs): (&[Value], Kwargs) = from_args(args)?;
        if !args.is_empty() {
            return Err(Error::new(
                ErrorKind::TooManyArguments,
                format!("output_format() may only be called with named arguments"),
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
                        format!(
                            "Invalid value for always_hoist_enums (expected bool): {e}"
                        ),
                    ))
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

        let Ok(_) = kwargs.assert_all_used() else {
            return Err(Error::new(
                ErrorKind::TooManyArguments,
                "output_format() got an unexpected keyword argument (only 'prefix', 'always_host_enums', 'enum_value_prefix', and 'or_splitter' are allowed)",
            ));
        };

        let content = self.text.render(RenderOptions::new(
            prefix,
            or_splitter,
            enum_value_prefix,
            always_hoist_enums,
            map_style,
        ))?;

        match content {
            Some(content) => Ok(Value::from_safe_string(content)),
            None => Ok(Value::from_serialize("")),
        }
    }
    fn call_method(
        &self,
        _state: &minijinja::State<'_, '_>,
        name: &str,
        _args: &[minijinja::value::Value],
    ) -> Result<minijinja::value::Value, minijinja::Error> {
        Err(minijinja::Error::new(
            ErrorKind::UnknownMethod,
            format!("output_format has no callable attribute '{name}'"),
        ))
    }
}
