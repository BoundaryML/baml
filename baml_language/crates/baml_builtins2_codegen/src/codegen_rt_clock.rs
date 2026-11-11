//! Declaration checks for clock sysops lowered with pack-specific class layouts.
use crate::{BamlType as T, BuiltinPipeline, NativeBuiltin};
pub(crate) fn supports(path: &str) -> bool {
    matches!(
        path,
        "baml.time.Instant.now"
            | "baml.time.system_timezone"
            | "baml.time._tz_offset_at"
            | "baml.time._tz_to_instant"
            | "baml.sys._sleep"
            | "baml.sys.host_info"
            | "baml.sys.collect_garbage"
    )
}
pub(crate) fn generate(d: &NativeBuiltin) -> Result<String, String> {
    let (params, ret) = match d.path.as_str() {
        "baml.sys._sleep" => (vec![T::Bigint], T::Null),
        "baml.time.Instant.now" => (vec![], T::Named("Instant".into())),
        "baml.sys.host_info" => (vec![], T::Named("HostInfo".into())),
        "baml.sys.collect_garbage" => (vec![], T::Null),
        "baml.time.system_timezone" => (vec![], T::String),
        "baml.time._tz_offset_at" => (vec![T::String, T::Bigint], T::Optional(Box::new(T::Int))),
        "baml.time._tz_to_instant" => (
            vec![T::String, T::Bigint, T::String],
            T::Optional(Box::new(T::Bigint)),
        ),
        _ => unreachable!(),
    };
    if d.pipeline != BuiltinPipeline::Io
        || !d.generics.is_empty()
        || d.params.iter().map(|p| &p.ty).ne(params.iter())
        || d.return_type != ret
    {
        return Err(format!(
            "unsupported in baml_rt: changed builtin signature {}",
            d.path
        ));
    }
    Ok(String::new())
}
