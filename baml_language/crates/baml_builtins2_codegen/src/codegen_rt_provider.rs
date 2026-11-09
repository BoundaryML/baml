//! Native provider adapters whose argument/result schemas are emitted by rt.
use crate::{BuiltinPipeline, NativeBuiltin};
pub fn supports(path: &str) -> bool {
    matches!(
        path,
        "baml.fs.File.close"
            | "baml.fs.File.seek_from"
            | "baml.fs.Lock.close"
            | "baml.fs.baml.io.Read$for$File.bytes"
            | "baml.fs.baml.io.Read$for$File.read"
            | "baml.fs.baml.io.Write$for$File.flush"
            | "baml.fs.baml.io.Write$for$File.write_some"
            | "baml.fs.chmod"
            | "baml.fs.lock"
            | "baml.fs.metadata"
            | "baml.fs.mkdir"
            | "baml.fs.open"
            | "baml.fs.read_dir"
            | "baml.fs.symlink"
            | "baml.fs.write_atomic"
            | "baml.fs.write_bytes"
            | "baml.path._absolute"
            | "baml.path.is_absolute"
            | "baml.path.join"
            | "baml.path.parent"
            | "baml.sys._run"
            | "baml.sys._shell"
    )
}
pub(crate) fn validate(d: &NativeBuiltin) -> Result<(), String> {
    if d.pipeline != BuiltinPipeline::Io || !d.generics.is_empty() {
        return Err(format!(
            "unsupported in baml_rt: changed provider signature {}",
            d.path
        ));
    }
    Ok(())
}
