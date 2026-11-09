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

/// Compiler-emitted reflection adapters do not need a generated native body.
pub fn supports_rt_reflection(path: &str) -> bool {
    const KINDS: &[&str] = &["Class", "Enum", "Union", "Literal", "Array", "Map", "Interface", "Primitive", "Function"];
    matches!(path, "reflect.Type._to_string_impl" | "reflect.Type.kind"
        | "reflect.Array.element_type" | "reflect.Map.key_type" | "reflect.Map.value_type"
        | "reflect.Function.return_type"
        | "reflect.Type.as_class" | "reflect.Type.as_enum" | "reflect.Type.as_union"
        | "reflect.Type.as_literal" | "reflect.Type.as_array" | "reflect.Type.as_map"
        | "reflect.Type.as_interface" | "reflect.Type.as_primitive" | "reflect.Type.as_function"
        | "reflect.Type.array" | "reflect.Type.optional" | "reflect.Type.of_value"
        | "reflect.Union.member_types" | "reflect.Function.params" | "reflect.Map._new"
        | "reflect.Literal.new" | "reflect.Literal.value" | "reflect.Type._implements"
        | "reflect.Type.to_baml" | "reflect.Class.fields" | "reflect.Class.metadata"
        | "reflect.Enum.values" | "reflect.Enum.metadata")
        || path.strip_prefix("reflect.reflect.TypeView$for$")
            .and_then(|path|path.strip_suffix(".as_type"))
            .is_some_and(|kind| KINDS.contains(&kind))
}

fn typed_list_lowering(path: &str) -> bool {
    matches!(path,
        "baml.Array.at" | "baml.Array.pop" | "baml.Array.remove_at" | "baml.Array.concat"
        | "baml.Array.reverse" | "baml.Array.slice" | "baml.Array.clear" | "baml.Array.shift"
        | "baml.Array.unshift" | "baml.Array.insert" | "baml.Array.splice" | "baml.Array.includes"
        | "baml.Array.index_of" | "baml.Array.last_index_of" | "baml.Array.filled"
        | "baml.Array.map" | "baml.Array.filter" | "baml.Array.some" | "baml.Array.every" | "baml.Array.reduce"
        | "baml._sum_int" | "baml._sum_float" | "baml._median_float" | "baml._rust_sort" | "baml._is_primitive_array")
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
    let disposition = if supports_rt_reflection(path) {
        CompilerIntrinsic
    } else if typed_list_lowering(path) {
        Partial("typed list lowering; element and erased-callback specializations are limited")
    } else if path.starts_with("baml.future.Future.") || path.starts_with("baml.spawn.CancelToken.")
        || path.starts_with("baml.spawn.Limit.") || path == "baml._cleanup_begin"
    {
        Partial("direct runtime task service; erased native method values are not admitted")
    } else if matches!(
        path,
        "baml._to_string_default"
            | "baml.Array.push"
            | "baml.Array.join"
            | "baml.Array.length"
            | "baml.Map.set"
            | "baml.Map.get"
            | "baml.Map.index"
            | "baml.Map.has"
            | "baml.Map.delete"
            | "baml.Map.get_or_insert"
            | "baml.Map.clear"
            | "baml.Map.length"
            | "baml.Map.keys"
            | "baml.Map.values"
            | "baml.ops.equals_equals"
            | "baml._equals_structural_default"
    ) {
        Partial(match path {
            "baml._to_string_default" => "int and bigint specializations",
            name if name.starts_with("baml.Map.") => {
                "typed primitive-key maps; class hash/equality callbacks are not lowered"
            }
            "baml.ops.equals_equals" | "baml._equals_structural_default" => {
                "shared structural walk with closed-world (including generic) Equals.eq witnesses"
            }
            _ => "typed list lowering; element and erased-callback specializations are limited",
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
            if crate::codegen_rt_provider::supports(&d.path) {
                crate::codegen_rt_provider::validate(d).unwrap_or_else(|e| panic!("{e}"));
            }
            if crate::supports_rt_calendar(&d.path) {
                crate::codegen_rt_calendar::validate(d).unwrap_or_else(|e| panic!("{e}"));
            }
            if crate::codegen_rt_random::supports(&d.path) {
                crate::codegen_rt_random::generate(d).unwrap_or_else(|e| panic!("{e}"));
            }
            if crate::codegen_rt_clock::supports(&d.path) {
                crate::codegen_rt_clock::generate(d).unwrap_or_else(|e| panic!("{e}"));
            }
            if crate::codegen_rt_bytes::supports(&d.path) {
                crate::codegen_rt_bytes::generate(d).unwrap_or_else(|e| panic!("{e}"));
            }
            if crate::codegen_rt_bigint::supports(&d.path) {
                crate::codegen_rt_bigint::generate(d).unwrap_or_else(|e| panic!("{e}"));
            }
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
