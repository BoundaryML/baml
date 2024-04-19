use std::collections::HashMap;

use minijinja::{value::StructObject, ErrorKind};
use serde::Serialize;
use serde_json::json;

use crate::{
    callable_jinja::CallableJinja,
    output_format::{OutputFormat, OutputFormatMode},
};

#[allow(non_camel_case_types)]
#[derive(Clone, Debug, Serialize)]
pub struct RenderContext_Client {
    pub name: String,
    pub provider: String,
}

impl std::fmt::Display for RenderContext_Client {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        json!(self).to_string().fmt(f)
    }
}

#[derive(Debug)]
pub struct RenderContext {
    client: RenderContext_Client,
    output_format: OutputFormat,
    env: HashMap<String, String>,
}

impl RenderContext {
    pub fn new(
        client: RenderContext_Client,
        output_format: String,
        env: HashMap<String, String>,
    ) -> RenderContext {
        RenderContext {
            client,
            output_format: OutputFormat::new(output_format),
            env,
        }
    }
}

impl std::fmt::Display for RenderContext {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        // Print all the fields of the struct
        write!(f, "RenderContext (fields: (client, output_format, env))")
    }
}

impl StructObject for RenderContext {
    fn get_field(&self, name: &str) -> Option<minijinja::Value> {
        match name {
            "client" => Some(minijinja::Value::from_object(self.client.clone())),
            "output_format" => Some(minijinja::Value::from_safe_string(
                self.output_format
                    .render(OutputFormatMode::WithPrefix(None)),
            )),
            "env" => Some(minijinja::Value::from(self.env.clone())),
            _ => None,
        }
    }

    fn static_fields(&self) -> Option<&'static [&'static str]> {
        Some(&["client", "output_format", "env"])
    }
}

impl StructObject for RenderContext_Client {
    fn get_field(&self, name: &str) -> Option<minijinja::Value> {
        match name {
            "name" => Some(minijinja::Value::from(self.name.clone())),
            "provider" => Some(minijinja::Value::from(self.provider.clone())),
            _ => None,
        }
    }
}

impl minijinja::value::Object for RenderContext {
    fn kind(&self) -> minijinja::value::ObjectKind<'_> {
        minijinja::value::ObjectKind::Struct(self)
    }

    fn call_method(
        &self,
        state: &minijinja::State,
        name: &str,
        args: &[minijinja::Value],
    ) -> Result<minijinja::Value, minijinja::Error> {
        match name {
            "output_format" => self.output_format.call_method(name, state, args),
            _ => Err(minijinja::Error::new(
                ErrorKind::UnknownMethod,
                format!("RenderContext has no method named '{}'", name),
            )),
        }
    }
}

impl minijinja::value::Object for RenderContext_Client {
    fn kind(&self) -> minijinja::value::ObjectKind<'_> {
        minijinja::value::ObjectKind::Struct(self)
    }
}
