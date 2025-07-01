use askama::Template;
use baml_types::GeneratorDefaultClientMode;

use crate::{
    generated_types::{ClassPy, EnumPy},
    package::CurrentRenderPackage,
    r#type::{SerializeType, TypePy},
};

pub struct FunctionPy {
    pub(crate) documentation: Option<String>,
    pub(crate) name: String,
    pub(crate) args: Vec<FunctionArgPy>,
    pub(crate) return_type: TypePy,
    pub(crate) stream_return_type: TypePy,
}

pub struct FunctionArgPy {
    pub(crate) name: String,
    pub(crate) type_: TypePy,
    pub(crate) default_value: Option<String>,
}

#[derive(askama::Template)]
#[template(path = "async_client.py.j2", escape = "none")]
struct AsyncClient<'a> {
    functions: &'a [FunctionPy],
    pkg: &'a CurrentRenderPackage,
}

pub fn render_async_client(
    functions: &[FunctionPy],
    pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    AsyncClient { functions, pkg }.render()
}

#[derive(askama::Template)]
#[template(path = "sync_client.py.j2", escape = "none")]
struct SyncClient<'a> {
    functions: &'a [FunctionPy],
    pkg: &'a CurrentRenderPackage,
}

pub fn render_sync_client(
    functions: &[FunctionPy],
    pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    SyncClient { functions, pkg }.render()
}

#[derive(askama::Template)]
#[template(path = "parser.py.j2", escape = "none")]
struct Parser<'a> {
    functions: &'a [FunctionPy],
    pkg: &'a CurrentRenderPackage,
}

pub fn render_parser(
    functions: &[FunctionPy],
    pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    Parser { functions, pkg }.render()
}

/// A map of type names to their Py types.
///
/// ```askama
/// from . import types
/// from . import stream_types
///
///
/// type_map = {
/// {% for class in classes %}
///     "types.{{ class.name }}": types.{{ class.name }},
///     "stream_types.{{ class.name }}": stream_types.{{ class.name }},
/// {% endfor %}
/// {% for enum_ in enums %}
///     "types.{{ enum_.name }}": types.{{ enum_.name}},
/// {% endfor %}
/// }
/// ```
#[derive(askama::Template)]
#[template(in_doc = true, escape = "none", ext = "txt")]
struct TypeMap<'a> {
    classes: &'a [ClassPy<'a>],
    enums: &'a [EnumPy],
}

pub fn render_type_map(classes: &[ClassPy], enums: &[EnumPy]) -> Result<String, askama::Error> {
    TypeMap { classes, enums }.render()
}

/// A map of file paths to their contents.
///
/// ```askama
/// _file_map = {
/// {% for (path, contents) in file_map %}
///     {{ path }}: {{ contents }},
/// {%- endfor %}
/// }
///
/// def get_baml_files():
///     return _file_map
/// ```
#[derive(askama::Template)]
#[template(in_doc = true, escape = "none", ext = "txt")]
struct SourceFiles<'a> {
    file_map: &'a [(String, String)],
}

pub fn render_source_files(file_map: Vec<(String, String)>) -> Result<String, askama::Error> {
    SourceFiles {
        file_map: &file_map,
    }
    .render()
}

pub fn render_runtime(_pkg: &CurrentRenderPackage) -> Result<String, askama::Error> {
    Runtime {}.render()
}

#[derive(askama::Template)]
#[template(path = "runtime.py.j2", escape = "none", ext = "txt")]
struct Runtime {}

pub fn render_globals(_pkg: &CurrentRenderPackage) -> Result<String, askama::Error> {
    Ok(include_str!("./_templates/globals.py").to_string())
}

pub fn render_config(_pkg: &CurrentRenderPackage) -> Result<String, askama::Error> {
    Ok(include_str!("./_templates/config.py").to_string())
}

pub fn render_tracing(_pkg: &CurrentRenderPackage) -> Result<String, askama::Error> {
    Ok(include_str!("./_templates/tracing.py").to_string())
}

#[derive(askama::Template)]
#[template(path = "__init__.py.j2", escape = "none", ext = "txt")]
struct Init<'a> {
    version: &'a str,
    default_client_mode: GeneratorDefaultClientMode,
}

pub fn render_init(
    _pkg: &CurrentRenderPackage,
    client_mode: &GeneratorDefaultClientMode,
) -> Result<String, askama::Error> {
    Init {
        version: env!("CARGO_PKG_VERSION"),
        default_client_mode: client_mode.clone(),
    }
    .render()
}
