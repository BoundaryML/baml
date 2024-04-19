use crate::callable_jinja::CallableJinja;
use minijinja::{value::Kwargs, ErrorKind, Value};

#[derive(Debug, Clone)]
pub(crate) struct OutputFormat {
    text: String,
}

// DO NOT CHANGE THIS w/o adding versioning.
pub(crate) const DEFAULT_PREFIX: &str = "OUTPUT_JSON_SCHEMA:\n";

impl OutputFormat {
    pub(crate) fn new(text: String) -> OutputFormat {
        OutputFormat { text }
    }

    pub(crate) fn render(&self, mode: OutputFormatMode) -> String {
        match mode {
            OutputFormatMode::WithPrefix(prefix) => {
                format!(
                    "{}{}",
                    prefix.as_deref().unwrap_or(DEFAULT_PREFIX),
                    self.text
                )
            }
            OutputFormatMode::WithoutPrefix => self.text.clone(),
        }
    }
}

pub(crate) enum OutputFormatMode {
    WithPrefix(Option<String>),
    WithoutPrefix,
}

impl CallableJinja for OutputFormat {
    type Params = OutputFormatMode;

    fn params(&self) -> &[&'static str] {
        &["prefix"]
    }

    fn parse_args(
        &self,
        _state: &minijinja::State,
        args: &[Value],
        kwargs: &mut Kwargs,
    ) -> Result<Self::Params, minijinja::Error> {
        if !args.is_empty() {
            return Err(minijinja::Error::new(
                ErrorKind::TooManyArguments,
                format!("may only be called with named arguments"),
            ));
        }

        match kwargs.get::<Value>("prefix") {
            Ok(prefix) => {
                if prefix.is_none() || prefix.is_undefined() {
                    Ok(OutputFormatMode::WithoutPrefix)
                } else if let Some(prefix) = prefix.as_str() {
                    Ok(OutputFormatMode::WithPrefix(Some(prefix.to_string())))
                } else {
                    Err(minijinja::Error::new(
                        ErrorKind::TooManyArguments,
                        format!(
                            "expected 'prefix' to be string or none, but was type '{}'",
                            prefix.kind()
                        ),
                    ))
                }
            }
            Err(_) => Ok(OutputFormatMode::WithPrefix(None)),
        }
    }

    fn call(
        &self,
        _state: &minijinja::State,
        mode: Self::Params,
    ) -> Result<Value, minijinja::Error> {
        Ok(Value::from_safe_string(self.render(mode)))
    }
}
