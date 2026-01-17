mod docstring;
mod objects;
mod ty;
use std::path::PathBuf;

use crate::objects::{Function, Object};

pub fn to_source_code(
    generators: &baml_codegen_types::ObjectPool,
    _baml_client_path: &std::path::Path,
) -> std::collections::HashMap<PathBuf, String> {
    let types = Object::load_types(generators);
    let stream_types = Object::load_stream_types(generators);
    let functions = Function::load_functions(generators);
    let stream_functions = Function::load_stream_functions(generators);

    let types_py = objects::get_types_py(&types);
    let stream_types_py = objects::get_stream_types_py(&stream_types);
    let functions_pyi = objects::get_functions_pyi(&functions);
    let stream_functions_pyi = objects::get_functions_pyi(&stream_functions);
    [
        (PathBuf::from("types.py"), types_py),
        (PathBuf::from("stream_types.py"), stream_types_py),
        (PathBuf::from("sync_client.pyi"), functions_pyi),
        (PathBuf::from("async_client.pyi"), stream_functions_pyi),
        (
            PathBuf::from("config.py"),
            include_str!("./_askama/config.py").to_string(),
        ),
        (
            PathBuf::from("globals.py"),
            include_str!("./_askama/globals.py").to_string(),
        ),
        (
            PathBuf::from("tracing.py"),
            include_str!("./_askama/tracing.py").to_string(),
        ),
    ]
    .into_iter()
    .collect()
}
