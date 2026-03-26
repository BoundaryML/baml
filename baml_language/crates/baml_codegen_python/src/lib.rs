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
    let sync_client_py = objects::get_sync_client_py(&functions);
    let async_client_py = objects::get_async_client_py(&functions);
    // Keep the old .pyi stubs for stream functions (until streaming is supported)
    let stream_functions_pyi = objects::get_functions_pyi(&stream_functions);
    [
        (PathBuf::from("types.py"), types_py),
        (PathBuf::from("stream_types.py"), stream_types_py),
        (PathBuf::from("sync_client.py"), sync_client_py),
        (PathBuf::from("async_client.py"), async_client_py),
        (
            PathBuf::from("stream_client.pyi"),
            stream_functions_pyi,
        ),
        (
            PathBuf::from("runtime.py"),
            include_str!("./_askama/runtime.py").to_string(),
        ),
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
