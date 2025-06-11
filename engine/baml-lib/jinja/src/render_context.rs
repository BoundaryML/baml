use std::collections::HashMap;

use minijinja::{value::{Object, ObjectRepr, Enumerator}, ErrorKind};
use serde::Serialize;
use serde_json::json;
use std::sync::Arc;

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

// TODO: (Greg) This type is duplicated in `src/lib.rs`. Are they both
// needed? If not, delete one.
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

impl Object for RenderContext {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Map
    }

    fn get_value(self: &Arc<Self>, key: &minijinja::Value) -> Option<minijinja::Value> {
        match key.as_str()? {
            "client" => Some(minijinja::Value::from_object(self.client.clone())),
            "output_format" => Some(minijinja::Value::from_safe_string(
                self.output_format
                    .render(OutputFormatMode::WithPrefix(None)),
            )),
            "env" => Some(minijinja::Value::from(self.env.clone())),
            _ => None,
        }
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::Str(&["client", "output_format", "env"])
    }

    fn call_method(
        self: &Arc<Self>,
        state: &minijinja::State,
        method: &str,
        args: &[minijinja::Value],
    ) -> Result<minijinja::Value, minijinja::Error> {
        match method {
            "output_format" => self.output_format.call_method(method, state, args),
            _ => Err(minijinja::Error::new(
                ErrorKind::UnknownMethod,
                format!("RenderContext has no method named '{}'", method),
            )),
        }
    }
}

impl Object for RenderContext_Client {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Map
    }

    fn get_value(self: &Arc<Self>, key: &minijinja::Value) -> Option<minijinja::Value> {
        match key.as_str()? {
            "name" => Some(minijinja::Value::from(self.name.clone())),
            "provider" => Some(minijinja::Value::from(self.provider.clone())),
            _ => None,
        }
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::Str(&["name", "provider"])
    }
}
