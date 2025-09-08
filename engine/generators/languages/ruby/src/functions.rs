use askama::Template;
use baml_types::GeneratorDefaultClientMode;

use crate::{
    generated_types::{ClassRb, EnumRb},
    package::CurrentRenderPackage,
    r#type::{SerializeType, TypeRb},
};

pub struct FunctionRb {
    pub(crate) documentation: Option<String>,
    pub(crate) name: String,
    pub(crate) args: Vec<FunctionArgRb>,
    pub(crate) return_type: TypeRb,
    pub(crate) stream_return_type: TypeRb,
}

pub struct FunctionArgRb {
    pub(crate) name: String,
    pub(crate) type_: TypeRb,
    pub(crate) default_value: Option<String>,
}

#[derive(askama::Template)]
#[template(path = "client.rb.j2", escape = "none")]
struct Client<'a> {
    functions: &'a [FunctionRb],
    pkg: &'a CurrentRenderPackage,
}

pub fn render_client(
    functions: &[FunctionRb],
    pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    Client { functions, pkg }.render()
}

#[derive(askama::Template)]
#[template(path = "parser.rb.j2", escape = "none")]
struct Parser<'a> {
    functions: &'a [FunctionRb],
    pkg: &'a CurrentRenderPackage,
}

pub fn render_parser(
    functions: &[FunctionRb],
    pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    Parser { functions, pkg }.render()
}

/// A map of type names to their Rb types.
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
    classes: &'a [ClassRb<'a>],
    enums: &'a [EnumRb],
}

pub fn render_type_map(classes: &[ClassRb], enums: &[EnumRb]) -> Result<String, askama::Error> {
    TypeMap { classes, enums }.render()
}

/// A map of file paths to their contents.
///
/// ```askama
/// module Internal
///   extend T::Sig
///   FILE_MAP = T.let(
///     {
///     {% for (path, contents) in file_map %}
///       {{ path }} => {{ contents }},
///     {%- endfor %}
///     }.freeze, T::Hash[String, String]
///   )
/// end
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
#[template(path = "runtime.rb.j2", escape = "none", ext = "txt")]
struct Runtime {}

pub fn render_globals(_pkg: &CurrentRenderPackage) -> Result<String, askama::Error> {
    Ok(include_str!("./_templates/globals.rb").to_string())
}

pub fn render_config(_pkg: &CurrentRenderPackage) -> Result<String, askama::Error> {
    Ok(include_str!("./_templates/config.rb").to_string())
}

pub fn render_tracing(_pkg: &CurrentRenderPackage) -> Result<String, askama::Error> {
    Ok("".to_string())
    // Ok(include_str!("./_templates/tracing.rb").to_string())
}

#[derive(askama::Template)]
#[template(path = "baml.rb.j2", escape = "none", ext = "txt")]
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
