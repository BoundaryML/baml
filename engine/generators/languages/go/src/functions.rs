use askama::Template;

use crate::{
    generated_types::{ClassGo, EnumGo, UnionGo},
    package::CurrentRenderPackage,
    r#type::{SerializeType, TypeGo},
};

pub struct FunctionGo {
    pub(crate) documentation: Option<String>,
    pub(crate) name: String,
    pub(crate) args: Vec<(String, TypeGo)>,
    pub(crate) return_type: TypeGo,
    pub(crate) stream_return_type: TypeGo,
}

fn render_function(
    function: &FunctionGo,
    pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    let template = FunctionTemplate {
        r#fn: function,
        pkg,
    };

    template.render()
}

fn render_function_stream(
    function: &FunctionGo,
    pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    let stream_template = FunctionStreamTemplate {
        r#fn: function,
        pkg,
    };

    stream_template.render()
}

/// We use doc comments to render the functions.
///
/// ```askama
/// package baml_client
///
/// import (
///     "context"
///
///     "{{ go_mod_name }}/baml_client/types"
///     baml "github.com/boundaryml/baml/engine/language_client_go/pkg"
/// )
///
/// {% for function in functions %}
/// {{ crate::functions::render_function(function, pkg)? }}
/// {% endfor %}
/// ```
#[derive(askama::Template)]
#[template(in_doc = true, ext = "txt", escape = "none")]
struct FunctionsTemplate<'a> {
    functions: &'a [FunctionGo],
    pkg: &'a CurrentRenderPackage,
    go_mod_name: &'a str,
}

pub fn render_functions(
    functions: &[FunctionGo],
    pkg: &CurrentRenderPackage,
    go_mod_name: &str,
) -> Result<String, askama::Error> {
    FunctionsTemplate {
        functions,
        pkg,
        go_mod_name,
    }
    .render()
}

/// A map of type names to their Go types.
///
/// ```askama
/// package baml_client
///
/// import (
///     "context"
///
///     "{{ go_mod_name }}/baml_client/types"
///     "{{ go_mod_name }}/baml_client/stream_types"
///     baml "github.com/boundaryml/baml/engine/language_client_go/pkg"
/// )
///
/// type stream struct {}
/// var Stream = &stream{}
///
/// type StreamValue[TStream any, TFinal any] struct {
///     IsFinal   bool
///     as_final  *TFinal
///     as_stream *TStream
/// }
///
/// func (s *StreamValue[TStream, TFinal]) Final() TFinal {
///     return *s.as_final
/// }
///
/// func (s *StreamValue[TStream, TFinal]) Stream() TStream {
///     return *s.as_stream
/// }
///
/// {% for function in functions %}
/// {{ crate::functions::render_function_stream(function, pkg)? }}
/// {% endfor %}
/// ```
#[derive(askama::Template)]
#[template(in_doc = true, ext = "txt", escape = "none")]
struct FunctionsStreamTemplate<'a> {
    functions: &'a [FunctionGo],
    pkg: &'a CurrentRenderPackage,
    go_mod_name: &'a str,
}

pub fn render_functions_stream(
    functions: &[FunctionGo],
    pkg: &CurrentRenderPackage,
    go_mod_name: &str,
) -> Result<String, askama::Error> {
    FunctionsStreamTemplate {
        functions,
        pkg,
        go_mod_name,
    }
    .render()
}

#[derive(askama::Template)]
#[template(path = "function.go.j2", escape = "none")]
struct FunctionTemplate<'a> {
    r#fn: &'a FunctionGo,
    pkg: &'a CurrentRenderPackage,
}

#[derive(askama::Template)]
#[template(path = "function.stream.go.j2", escape = "none")]
struct FunctionStreamTemplate<'a> {
    r#fn: &'a FunctionGo,
    pkg: &'a CurrentRenderPackage,
}

/// A map of type names to their Go types.
///
/// ```askama
/// package baml_client
///
/// import (
///     "{{ go_mod_name }}/baml_client/types"
///     "{{ go_mod_name }}/baml_client/stream_types"
/// )
///
/// var typeMap = map[string]reflect.Type{
/// {% for class in classes -%}
///     "types.{{ class.name }}": reflect.TypeOf(types.{{ class.name }}{}),
///     "stream_types.{{ class.name }}": reflect.TypeOf(stream_types.{{ class.name }}{}),
/// {% endfor %}
/// {% for enum_ in enums -%}
///     "types.{{ enum_.name }}": reflect.TypeOf(types.{{ enum_.name}}("")),
/// {% endfor %}
/// {% for union_ in unions -%}
///     "types.{{ union_.cffi_name }}": reflect.TypeOf(types.{{ union_.name }}{}),
///     "stream_types.{{ union_.cffi_name }}": reflect.TypeOf(stream_types.{{ union_.name }}{}),
/// {% endfor %}
/// }
/// ```
#[derive(askama::Template)]
#[template(in_doc = true, escape = "none", ext = "txt")]
struct TypeMap<'a> {
    classes: &'a [ClassGo<'a>],
    enums: &'a [EnumGo<'a>],
    unions: &'a [UnionGo<'a>],
    go_mod_name: &'a str,
}

pub fn render_type_map(
    classes: &[ClassGo],
    enums: &[EnumGo],
    unions: &[UnionGo],
    go_mod_name: &str,
) -> Result<String, askama::Error> {
    TypeMap {
        classes,
        enums,
        unions,
        go_mod_name,
    }
    .render()
}

/// A map of file paths to their contents.
///
/// ```askama
/// package baml_client
///
/// var file_map = map[string]string{
/// {% for (path, contents) in file_map %}  
///   {{ path }}: {{ contents }},
/// {%- endfor %}  
/// }
///
/// func getBamlFiles() map[string]string {
///   return file_map
/// }
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

pub fn render_runtime_code(_pkg: &CurrentRenderPackage) -> Result<String, askama::Error> {
    RuntimeCode {}.render()
}

#[derive(askama::Template)]
#[template(path = "runtime.go.j2", escape = "none", ext = "txt")]
struct RuntimeCode {}
