use askama::Template;

use crate::{
    generated_types::{ClassGo, EnumGo, TypeAliasGo, UnionGo},
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

fn render_function_parse(
    function: &FunctionGo,
    pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    let parse_template = FunctionParseTemplate {
        r#fn: function,
        pkg,
    };
    parse_template.render()
}

fn render_function_parse_stream(
    function: &FunctionGo,
    pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    let parse_stream_template = FunctionParseStreamTemplate {
        r#fn: function,
        pkg,
    };
    parse_stream_template.render()
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
///     IsError   bool
///     Error     error
///     IsFinal   bool
///     as_final  *TFinal
///     as_stream *TStream
/// }
///
/// func (s *StreamValue[TStream, TFinal]) Final() *TFinal {
///     return s.as_final
/// }
///
/// func (s *StreamValue[TStream, TFinal]) Stream() *TStream {
///     return s.as_stream
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
/// type parse struct {}
/// var Parse = &parse{}
///
/// {% for function in functions %}
/// {{ crate::functions::render_function_parse(function, pkg)? }}
/// {% endfor %}
/// ```
#[derive(askama::Template)]
#[template(in_doc = true, ext = "txt", escape = "none")]
struct FunctionsParseTemplate<'a> {
    functions: &'a [FunctionGo],
    pkg: &'a CurrentRenderPackage,
    go_mod_name: &'a str,
}

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
/// type parse_stream struct {}
/// var ParseStream = &parse_stream{}
///
/// {% for function in functions %}
/// {{ crate::functions::render_function_parse_stream(function, pkg)? }}
/// {% endfor %}
/// ```
#[derive(askama::Template)]
#[template(in_doc = true, ext = "txt", escape = "none")]
struct FunctionsParseStreamTemplate<'a> {
    functions: &'a [FunctionGo],
    pkg: &'a CurrentRenderPackage,
    go_mod_name: &'a str,
}

pub fn render_functions_parse(
    functions: &[FunctionGo],
    pkg: &CurrentRenderPackage,
    go_mod_name: &str,
) -> Result<String, askama::Error> {
    FunctionsParseTemplate {
        functions,
        pkg,
        go_mod_name,
    }
    .render()
}

pub fn render_functions_parse_stream(
    functions: &[FunctionGo],
    pkg: &CurrentRenderPackage,
    go_mod_name: &str,
) -> Result<String, askama::Error> {
    FunctionsParseStreamTemplate {
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

#[derive(askama::Template)]
#[template(path = "function.parse.go.j2", escape = "none")]
struct FunctionParseTemplate<'a> {
    r#fn: &'a FunctionGo,
    pkg: &'a CurrentRenderPackage,
}

#[derive(askama::Template)]
#[template(path = "function.parse_stream.go.j2", escape = "none")]
struct FunctionParseStreamTemplate<'a> {
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
///     "TYPES.{{ class.name }}": reflect.TypeOf(types.{{ class.name }}{}),
///     "STREAM_TYPES.{{ class.name }}": reflect.TypeOf(stream_types.{{ class.name }}{}),
/// {% endfor %}
/// {% for enum_ in enums -%}
///     "TYPES.{{ enum_.name }}": reflect.TypeOf(types.{{ enum_.name}}("")),
/// {% endfor %}
/// {% for union_ in unions -%}
///     "TYPES.{{ union_.cffi_name }}": reflect.TypeOf(types.{{ union_.name }}{}),
/// {% endfor %}
/// {% for union_ in stream_unions -%}
///     "STREAM_TYPES.{{ union_.cffi_name }}": reflect.TypeOf(stream_types.{{ union_.name }}{}),
/// {% endfor %}
/// {% for type_alias in type_aliases -%}
///     "TYPES.{{ type_alias.name }}": reflect.TypeOf({{ type_alias.type_.construct_instance(pkg) }}),
/// {% endfor %}
/// {% for type_alias in stream_type_aliases -%}
///     "STREAM_TYPES.{{ type_alias.name }}": reflect.TypeOf({{ type_alias.type_.construct_instance(pkg) }}),
/// {% endfor %}
/// }
/// ```
#[derive(askama::Template)]
#[template(in_doc = true, escape = "none", ext = "txt")]
struct TypeMap<'a> {
    classes: &'a [ClassGo<'a>],
    enums: &'a [EnumGo<'a>],
    unions: &'a [UnionGo<'a>],
    stream_unions: &'a [UnionGo<'a>],
    type_aliases: &'a [TypeAliasGo<'a>],
    stream_type_aliases: &'a [TypeAliasGo<'a>],
    go_mod_name: &'a str,
    pkg: &'a CurrentRenderPackage,
}

#[allow(clippy::too_many_arguments)]
pub fn render_type_map(
    classes: &[ClassGo],
    enums: &[EnumGo],
    unions: &[UnionGo],
    stream_unions: &[UnionGo],
    type_aliases: &[TypeAliasGo],
    stream_type_aliases: &[TypeAliasGo],
    go_mod_name: &str,
    pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    TypeMap {
        classes,
        enums,
        unions,
        stream_unions,
        type_aliases,
        stream_type_aliases,
        go_mod_name,
        pkg,
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
