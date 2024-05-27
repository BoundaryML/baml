use minijinja::{value::Kwargs, ErrorKind, Value};

use crate::RenderContext;

#[derive(Debug)]
pub struct OutputFormat {
    text: String,
}

impl OutputFormat {
    pub fn new(ctx: &RenderContext) -> Self {
        Self {
            text: ctx.output_format.clone(),
        }
    }
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Answer in JSON using this schema:\n{}", self.text)
    }
}

impl minijinja::value::Object for OutputFormat {
    fn call(
        &self,
        _state: &minijinja::State<'_, '_>,
        args: &[minijinja::value::Value],
    ) -> Result<minijinja::value::Value, minijinja::Error> {
        use minijinja::{
            value::{from_args, ValueKind},
            Error,
        };

        let (args, kwargs): (&[Value], Kwargs) = from_args(args)?;
        if !args.is_empty() {
            return Err(Error::new(
                ErrorKind::TooManyArguments,
                format!("output_format() may only be called with named arguments"),
            ));
        }

        let Ok(prefix) = kwargs.get::<Value>("prefix") else {
            // prefix was not specified, defaults to "Use this output format:"
            return Ok(Value::from_safe_string(format!("{}", self)));
        };

        let Ok(_) = kwargs.assert_all_used() else {
            return Err(Error::new(
                ErrorKind::TooManyArguments,
                "output_format() got an unexpected keyword argument (only 'prefix' is allowed)",
            ));
        };

        match prefix.kind() {
            ValueKind::Undefined | ValueKind::None => {
                // prefix specified as none appears to result in ValueKind::Undefined
                return Ok(Value::from_safe_string(self.text.to_string()));
            }
            // prefix specified as a string
            ValueKind::String => {
                return Ok(Value::from_safe_string(format!(
                    "{}\n{}",
                    prefix.to_string(),
                    self.text
                )));
            }
            _ => {
                return Err(Error::new(
                    ErrorKind::TooManyArguments,
                    format!(
                        "output_format() expected 'prefix' to be string or none, but was type '{}'",
                        prefix.kind()
                    ),
                ));
            }
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
            format!("output_format has no callable attribute '{}'", name),
        ))
    }
}
