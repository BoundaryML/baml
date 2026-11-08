//! Total, declaration-driven builtin inventory. New declarations cannot vanish
//! from coverage: every native function has a group and a concrete rt status.
use crate::{BuiltinPipeline, NativeBuiltin};

/// Includes VM natives outside `baml` as well as every package's sysops.
pub fn native_inventory() -> Result<Vec<NativeBuiltin>, crate::ExtractNativeBuiltinsError> {
    let packages: std::collections::BTreeSet<_> =
        baml_builtins2::ALL.iter().map(|f| f.package).collect();
    let mut declarations = Vec::new();
    for package in packages {
        let (vm, io, _) = crate::extract_native_builtins_for(package)?;
        declarations.extend(vm);
        declarations.extend(io);
    }
    declarations.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(declarations)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BuiltinGroup {
    Io,
    ScalarString,
    VmInternal,
    ContainerClass,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RtDisposition {
    TypedAdapter,
    Partial(&'static str),
    CompilerIntrinsic,
    RuntimeService,
    Unsupported(&'static str),
}

/// Where a declaration belongs in rt, independently of whether its adapter is
/// implemented. Container bodies stay typed Rust; no interpreter continuation
/// or local-twin entry point is retained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RtStrategy {
    SharedBody,
    TypedHeapBody,
    CompilerIntrinsic,
    RuntimeService,
    StaticTable,
    GeneratedCallStub,
    InterpreterIsland,
    OwnedSysop,
}

pub fn implementation_strategy(d: &NativeBuiltin) -> RtStrategy {
    use RtStrategy::*;
    let path = d.path.as_str();
    if path.starts_with("reflect.Session.")
        || matches!(path, "reflect.Package._compile" | "reflect.Package._finish")
    {
        InterpreterIsland
    } else if d.pipeline == BuiltinPipeline::Io {
        OwnedSysop
    } else if path == "reflect.call_any" {
        GeneratedCallStub
    } else if path.starts_with("reflect.") {
        StaticTable
    } else if path.starts_with("trace.")
        || path.starts_with("baml.future.")
        || path.starts_with("baml.spawn.")
        || path.starts_with("baml.testing.")
        || path.starts_with("testing.")
        || matches!(path, "baml.sys.argv" | "baml._cleanup_begin")
    {
        RuntimeService
    } else if path.starts_with("baml.ops.") {
        CompilerIntrinsic
    } else if path.starts_with("baml.Int.")
        || path.starts_with("baml.Float.")
        || path.starts_with("baml.String.")
        || path.starts_with("baml.Bigint.")
    {
        SharedBody
    } else {
        TypedHeapBody
    }
}

pub fn classify_builtin(d: &NativeBuiltin) -> (BuiltinGroup, RtDisposition) {
    use BuiltinGroup::*;
    use RtDisposition::*;
    let path = d.path.as_str();
    let group = if d.pipeline == BuiltinPipeline::Io {
        Io
    } else if path.starts_with("baml.Int.")
        || path.starts_with("baml.Float.")
        || path.starts_with("baml.String.")
        || path.starts_with("baml.Bigint.")
    {
        ScalarString
    } else if path.starts_with("reflect.")
        || path.starts_with("trace.")
        || path.starts_with("baml.future.")
        || path.starts_with("baml.testing.")
        || path.starts_with("baml.sys.")
        || path.starts_with("baml._")
        || path.starts_with("baml.ops.")
    {
        VmInternal
    } else {
        ContainerClass
    };
    let disposition = if matches!(
        path,
        "baml._to_string_default"
            | "baml.Array.push"
            | "baml.Array.join"
            | "baml.Array.length"
            | "baml.Map.set"
            | "baml.Map.values"
            | "baml.ops.equals_equals"
    ) {
        Partial(match path {
            "baml._to_string_default" => "int specialization",
            "baml.Map.set" | "baml.Map.values" => "map<string,int> specialization",
            "baml.ops.equals_equals" => "primitive string equality and null tests",
            _ => "string and nested-string list specializations",
        })
    } else if crate::supports_rt(path) {
        TypedAdapter
    } else {
        Unsupported(match (group, implementation_strategy(d)) {
            (_, RtStrategy::InterpreterIsland) => {
                "island service exists; generated map/opaque boundary adapter unavailable"
            }
            (_, RtStrategy::StaticTable | RtStrategy::GeneratedCallStub) => {
                "static type/function values and general argument maps are not lowered"
            }
            (Io, _) => "owned sysop marshalling/typed provider adapter not implemented",
            (ScalarString, _) if path.starts_with("baml.Bigint.") => {
                "managed bigint representation and byte-array arguments are not lowered"
            }
            (ScalarString, _) => "string adapter belongs to the strings job",
            (VmInternal, RtStrategy::CompilerIntrinsic) => {
                "operator syntax is lowered; explicit native method adapter unavailable"
            }
            (VmInternal, _) if path.starts_with("trace.") => {
                "static declaration hooks are lowered; dynamic trace service unavailable"
            }
            (VmInternal, _) => "compiler intrinsic/runtime service not implemented",
            (ContainerClass, _)
                if path.starts_with("baml.Array.") || path.starts_with("baml.Map.") =>
            {
                "typed container adapter belongs to the lists/maps jobs"
            }
            (ContainerClass, _) => "typed class/opaque/bytes adapter not implemented",
        })
    };
    (group, disposition)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inventory_covers_every_native_declaration() {
        let declarations = native_inventory().unwrap();
        let mut counts = std::collections::BTreeMap::<_, (usize, usize)>::new();
        let mut names = std::collections::BTreeSet::new();
        for d in &declarations {
            assert!(names.insert(&d.path), "duplicate {}", d.path);
            let (group, status) = classify_builtin(d);
            let counts = counts.entry(group).or_default();
            counts.0 += 1;
            counts.1 += usize::from(matches!(
                status,
                RtDisposition::TypedAdapter
                    | RtDisposition::CompilerIntrinsic
                    | RtDisposition::RuntimeService
            ));
            if crate::codegen_rt_scalar::supports(&d.path) {
                crate::codegen_rt_scalar::generate(d).unwrap_or_else(|e| panic!("{e}"));
            }
            if crate::codegen_rt_io::supports(&d.path) {
                crate::codegen_rt_io::generate(d).unwrap_or_else(|e| panic!("{e}"));
            }
        }
        assert_eq!(
            counts.values().map(|c| c.0).sum::<usize>(),
            declarations.len()
        );
        eprintln!("native builtin classification (total, implemented): {counts:?}");
    }
}
