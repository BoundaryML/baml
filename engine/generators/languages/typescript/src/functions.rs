use std::fmt;

use askama::Template;
use baml_types::GeneratorDefaultClientMode;
use indexmap::IndexMap;

use crate::{
    package::CurrentRenderPackage,
    r#type::{SerializeType, TypeTS},
};

impl fmt::Display for TypeTS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TypeTS") // TODO
    }
}

pub struct FunctionTS {
    pub(crate) name: String,
    pub(crate) args: Vec<(String, TypeTS)>,
    pub(crate) return_type: TypeTS,
    pub(crate) stream_return_type: TypeTS,
}

#[derive(askama::Template)]
#[template(path = "async_client.ts.j2", escape = "none", ext = "txt")]
struct AsyncClient<'a> {
    functions: &'a [FunctionTS],
    types: &'a [String],
    pkg: &'a CurrentRenderPackage,
}

pub fn render_async_client(
    functions: &[FunctionTS],
    types: &[String],
    pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    AsyncClient {
        functions,
        types,
        pkg,
    }
    .render()
}

#[derive(askama::Template)]
#[template(path = "async_request.ts.j2", escape = "none")]
struct AsyncRequest<'a> {
    functions: &'a [FunctionTS],
    types: &'a [String],
    pkg: &'a CurrentRenderPackage,
}

pub fn render_async_request(
    functions: &[FunctionTS],
    types: &[String],
    pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    AsyncRequest {
        functions,
        types,
        pkg,
    }
    .render()
}

#[derive(askama::Template)]
#[template(path = "sync_client.ts.j2", escape = "none")]
struct SyncClient<'a> {
    functions: &'a [FunctionTS],
    types: &'a [String],
    pkg: &'a CurrentRenderPackage,
}

pub fn render_sync_client(
    functions: &[FunctionTS],
    types: &[String],
    pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    SyncClient {
        functions,
        types,
        pkg,
    }
    .render()
}

#[derive(askama::Template)]
#[template(path = "sync_request.ts.j2", escape = "none")]
struct SyncRequest<'a> {
    functions: &'a [FunctionTS],
    types: &'a [String],
    pkg: &'a CurrentRenderPackage,
}

pub fn render_sync_request(
    functions: &[FunctionTS],
    types: &[String],
    pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    SyncRequest {
        functions,
        types,
        pkg,
    }
    .render()
}

#[derive(askama::Template)]
#[template(path = "index.ts.j2", escape = "none", ext = "txt")]
struct Index<'a> {
    version: &'a str,
    default_client_mode: GeneratorDefaultClientMode,
}

pub fn render_index(
    default_client_mode: &GeneratorDefaultClientMode,
) -> Result<String, askama::Error> {
    Index {
        version: env!("CARGO_PKG_VERSION"),
        default_client_mode: default_client_mode.clone(),
    }
    .render()
}

pub fn render_globals(_pkg: &CurrentRenderPackage) -> Result<String, askama::Error> {
    Ok(include_str!("./_templates/globals.ts").to_string())
}

#[derive(askama::Template)]
#[template(path = "config.ts.j2", escape = "none", ext = "txt")]
struct Config {}

pub fn render_config() -> Result<String, askama::Error> {
    Config {}.render()
}

#[derive(askama::Template)]
#[template(path = "tracing.ts.j2", escape = "none", ext = "txt")]
struct Tracing<'a> {
    pkg: &'a CurrentRenderPackage,
}

pub fn render_tracing(pkg: &CurrentRenderPackage) -> Result<String, askama::Error> {
    Tracing { pkg }.render()
}

#[derive(askama::Template)]
#[template(path = "inlinedbaml.ts.j2", escape = "none", ext = "txt")]
struct InlinedBaml<'a> {
    file_map: &'a [(String, String)],
}

pub fn render_inlinedbaml(
    _pkg: &CurrentRenderPackage,
    file_map: Vec<(String, String)>,
) -> Result<String, askama::Error> {
    InlinedBaml {
        file_map: &file_map,
    }
    .render()
}

#[derive(askama::Template)]
#[template(path = "parser.ts.j2", escape = "none", ext = "txt")]
struct Parser<'a> {
    functions: &'a [FunctionTS],
    types: &'a [String],
    pkg: &'a CurrentRenderPackage,
}

pub fn render_parser(
    functions: &[FunctionTS],
    types: &[String],
    pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    Parser {
        functions,
        types,
        pkg,
    }
    .render()
}

// React-specific templates
#[derive(askama::Template)]
#[template(path = "react/hooks.tsx.j2", escape = "none")]
struct ReactHooks<'a> {
    functions: &'a [FunctionTS],
    pkg: &'a CurrentRenderPackage,
}

pub fn render_react_hooks(
    functions: &[FunctionTS],
    pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    ReactHooks { functions, pkg }.render()
}

#[derive(askama::Template)]
#[template(path = "react/server.ts.j2", escape = "none")]
struct ReactServer<'a> {
    functions: &'a [FunctionTS],
    types: &'a [String],
    pkg: &'a CurrentRenderPackage,
}

pub fn render_react_server(
    functions: &[FunctionTS],
    types: &[String],
    pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    ReactServer {
        functions,
        types,
        pkg,
    }
    .render()
}

#[derive(askama::Template)]
#[template(path = "react/server_streaming.ts.j2", escape = "none")]
struct ReactServerStreaming<'a> {
    functions: &'a [FunctionTS],
    types: &'a [String],
    pkg: &'a CurrentRenderPackage,
}

pub fn render_react_server_streaming(
    functions: &[FunctionTS],
    types: &[String],
    pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    ReactServerStreaming {
        functions,
        types,
        pkg,
    }
    .render()
}

#[derive(askama::Template)]
#[template(path = "react/server_streaming_types.ts.j2", escape = "none")]
struct ReactServerStreamingTypes<'a> {
    streaming_types: &'a IndexMap<String, String>,
    types: &'a [String],
    pkg: &'a CurrentRenderPackage,
}

pub fn render_react_server_streaming_types(
    functions: &[FunctionTS],
    types: &[String],
    pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    let mut streaming_types: IndexMap<String, String> = functions
        .iter()
        .map(|f| (f.name.clone(), f.stream_return_type.serialize_type(pkg)))
        .collect();
    streaming_types.sort_keys();

    ReactServerStreamingTypes {
        streaming_types: &streaming_types,
        types,
        pkg,
    }
    .render()
}

#[derive(askama::Template)]
#[template(path = "react/media.ts.j2", escape = "none")]
struct ReactMedia;

pub fn render_react_media() -> Result<String, askama::Error> {
    ReactMedia.render()
}
