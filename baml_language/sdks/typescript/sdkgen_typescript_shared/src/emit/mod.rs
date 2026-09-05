//! Shared emitter-internal representation of TypeScript-side symbols.
//!
//! These types describe what the emitter will render to TypeScript, as
//! opposed to `baml_codegen_types` which describes BAML-side input
//! symbols. The fan-out logic (sync + async per callable, companion
//! suffix rules) is a verbatim port of `sdkgen_python_pydantic2/src/emit/mod.rs`.

pub(crate) mod class;
pub(crate) mod enum_;
pub(crate) mod function;
pub(crate) mod method;
pub(crate) mod type_alias;
pub(crate) mod typemap_file;

use baml_codegen_types::{FunctionArgument, Name, Symbol, SymbolPool, Ty};

use crate::{
    emit::{
        class::{TypeScriptClass, TypeScriptClassProperty},
        enum_::{TypeScriptEnum, TypeScriptEnumVariant},
        function::{SyncAsync, TypeScriptFunction},
        method::{MethodKind, OptionalArg, RequiredArg, TypeScriptMethodBinding},
        type_alias::TypeScriptTypeAlias,
    },
    leaf::safe_decl_name,
    routing::{LeafPath, route},
};

/// Emitter-internal representation of one rendered TypeScript symbol.
pub(crate) enum EmittedSymbol {
    Class(TypeScriptClass),
    Enum(TypeScriptEnum),
    TypeAlias(TypeScriptTypeAlias),
    Function(TypeScriptFunction),
}

/// Build-time sort key. Tuple of `(source_file_path, span_start)`
/// derived from a `Symbol`'s `Origin`.
pub(crate) type SortKey = (String, u32);

/// Walk every `(Name, Symbol)` in the pool and build the
/// `(LeafPath, EmittedSymbol, SortKey)` triples that drive emission.
/// Function symbols fan out into sync + async bindings; all other
/// variants are 1:1.
pub(crate) fn build_emitted(pool: &SymbolPool) -> Vec<(LeafPath, EmittedSymbol, SortKey)> {
    // Determinism: SymbolPool is a HashMap, so iteration order is
    // nondeterministic. Sort pool entries by Name before the walk.
    let mut entries: Vec<(&Name, &Symbol)> = pool.iter().collect();
    entries.sort_by_key(|e| e.0);

    let mut out: Vec<(LeafPath, EmittedSymbol, SortKey)> = Vec::new();

    for (key, symbol) in entries {
        let leaf = route(key);
        // spec2: preserve the BAML name verbatim — `$` is a valid TS
        // identifier char, so a `$stream` companion class is emitted as
        // e.g. `Resume$stream` (not stripped to `Resume`). Non-stream
        // symbols are unaffected (their name carries no `$stream`).
        //
        // `bare` stays raw. The class / enum / type-alias arms below wrap it in
        // `safe_decl_name` because those three names bind a module-scope
        // identifier, where a reserved word is a parse error. Every wire-facing
        // field on the emitted symbol (`source`, `baml_fqn`, enum member
        // `value`) keeps the raw spelling.
        let bare = key.name().as_str().to_string();

        match symbol {
            Symbol::Class(c) => {
                let sort_key = origin_key(&c.origin);
                let properties = c
                    .properties
                    .iter()
                    .map(|p| TypeScriptClassProperty {
                        name: p.name.as_str().to_string(),
                        ty: p.ty.clone(),
                        docstring: p.docstring.clone(),
                    })
                    .collect();
                let class_fqn_root = key.to_string();
                let static_methods =
                    expand_methods(&c.static_methods, &class_fqn_root, MethodKind::Static);
                let instance_methods =
                    expand_methods(&c.instance_methods, &class_fqn_root, MethodKind::Instance);
                let generic_params = c
                    .generic_params
                    .iter()
                    .map(|n| n.as_str().to_string())
                    .collect();
                out.push((
                    leaf,
                    EmittedSymbol::Class(TypeScriptClass {
                        name: safe_decl_name(&bare),
                        source: key.clone(),
                        generic_params,
                        docstring: c.docstring.clone(),
                        properties,
                        static_methods,
                        instance_methods,
                    }),
                    sort_key,
                ));
            }
            Symbol::Enum(e) => {
                let sort_key = origin_key(&e.origin);
                let variants = e
                    .variants
                    .iter()
                    .map(|v| TypeScriptEnumVariant {
                        ident: v.name.as_str().to_string(),
                        value: v.value.clone(),
                        docstring: v.docstring.clone(),
                    })
                    .collect();
                out.push((
                    leaf,
                    EmittedSymbol::Enum(TypeScriptEnum {
                        name: safe_decl_name(&bare),
                        source: key.clone(),
                        variants,
                        docstring: e.docstring.clone(),
                    }),
                    sort_key,
                ));
            }
            Symbol::TypeAlias(t) => {
                let sort_key = origin_key(&t.origin);
                out.push((
                    leaf,
                    EmittedSymbol::TypeAlias(TypeScriptTypeAlias {
                        name: safe_decl_name(&bare),
                        source: key.clone(),
                        resolves_to: t.resolves_to.clone(),
                        recursive: t.recursive,
                    }),
                    sort_key,
                ));
            }
            Symbol::Function(f) => {
                let sort_key = origin_key(&f.origin);
                expand_function(&leaf, key, f, &sort_key, &mut out);
            }
        }
    }

    out
}

/// Fan out a `Symbol::Function` into its sync and async bindings.
/// Companions arrive as their own pool entries (keyed on the suffixed
/// name) and flow through this same path; they share the parent's span
/// so `group_and_sort` keeps them contiguous within the leaf.
fn expand_function(
    leaf: &LeafPath,
    key: &Name,
    f: &baml_codegen_types::Function,
    sort_key: &SortKey,
    out: &mut Vec<(LeafPath, EmittedSymbol, SortKey)>,
) {
    let fqn_root = key.to_string();
    let bare = bare_callable_name(key.name().as_str());
    let func_generic_params: Vec<String> = f
        .generic_params
        .iter()
        .map(|n| n.as_str().to_string())
        .collect();
    let func_docstring = f.docstring.clone();
    let raises_names = collect_raises_names(f.throws.as_ref());
    expand_callable(
        &bare,
        &fqn_root,
        &f.arguments,
        &f.return_type,
        |name, fqn, mode, params, arg_tys, arg_defaults, return_ty| {
            out.push((
                leaf.clone(),
                EmittedSymbol::Function(TypeScriptFunction {
                    name,
                    baml_fqn: fqn,
                    mode,
                    param_names: params,
                    arg_tys,
                    arg_defaults,
                    return_ty,
                    generic_params: func_generic_params.clone(),
                    docstring: func_docstring.clone(),
                    raises_names: raises_names.clone(),
                }),
                sort_key.clone(),
            ));
        },
    );
}

/// Collect the unqualified leaf names of the thrown types in a `throws`
/// `Ty`, in source order, de-duping exact-equal names. Class/Enum/
/// `TypeAlias` contribute their unqualified leaf name; a union contributes
/// each member's; an optional unwraps; anything else contributes nothing.
fn collect_raises_names(throws: Option<&baml_codegen_types::Ty>) -> Vec<String> {
    use baml_codegen_types::Ty;

    fn walk(ty: &Ty, out: &mut Vec<String>) {
        match ty {
            Ty::Class(name, _, _) | Ty::Enum(name, _) | Ty::TypeAlias(name, _) => {
                let n = name.name().as_str().to_string();
                if !out.contains(&n) {
                    out.push(n);
                }
            }
            Ty::Union(members, _) => members.iter().for_each(|m| walk(m, out)),
            _ => {}
        }
    }

    let mut out = Vec::new();
    if let Some(ty) = throws {
        walk(ty, &mut out);
    }
    out
}

/// Fan out source-declared methods (parents and companions) into one
/// `TypeScriptMethodBinding` per emitted line. Methods are sorted by `(file,
/// span, name)` so a parent and its companions cluster together.
fn expand_methods(
    methods: &[baml_codegen_types::Function],
    class_fqn_root: &str,
    kind: MethodKind,
) -> Vec<TypeScriptMethodBinding> {
    let mut sorted: Vec<&baml_codegen_types::Function> = methods.iter().collect();
    sorted.sort_by_key(|m| (origin_key(&m.origin), m.name.as_str()));

    let mut out: Vec<TypeScriptMethodBinding> = Vec::new();
    for m in sorted {
        let m_name = m.name.as_str();
        let bare = bare_callable_name(m_name);
        let fqn_root = format!("{class_fqn_root}.{m_name}");
        let method_generic_params: Vec<String> = m
            .generic_params
            .iter()
            .map(|n| n.as_str().to_string())
            .collect();
        let method_docstring = m.docstring.clone();
        let raises_names = collect_raises_names(m.throws.as_ref());
        let (required_args, optional_args) = split_arguments(&m.arguments);
        for (name, mode) in [
            (bare.clone(), SyncAsync::Sync),
            (format!("{bare}_async"), SyncAsync::Async),
        ] {
            out.push(TypeScriptMethodBinding {
                name,
                baml_fqn: fqn_root.clone(),
                mode,
                kind,
                required_args: required_args.clone(),
                optional_args: optional_args.clone(),
                return_ty: m.return_type.clone(),
                generic_params: method_generic_params.clone(),
                docstring: method_docstring.clone(),
                raises_names: raises_names.clone(),
            });
        }
    }
    out
}

fn split_arguments(arguments: &[FunctionArgument]) -> (Vec<RequiredArg>, Vec<OptionalArg>) {
    let first_optional = arguments
        .iter()
        .position(|arg| arg.default.is_some())
        .unwrap_or(arguments.len());
    (
        arguments[..first_optional]
            .iter()
            .map(|arg| RequiredArg {
                name: arg.name.as_str().to_string(),
                ty: arg.ty.clone(),
            })
            .collect(),
        arguments[first_optional..]
            .iter()
            .map(|arg| OptionalArg {
                name: arg.name.as_str().to_string(),
                ty: arg.ty.clone(),
                default: arg
                    .default
                    .clone()
                    .expect("arguments after the first defaulted method arg must have defaults"),
            })
            .collect(),
    )
}

/// The TS-side bare identifier for a callable's BAML name, used as the LHS
/// of the sync binding (the async sibling appends `_async`).
///
/// A `@stream` companion becomes `$stream` (`$` is a valid TypeScript
/// identifier character), and every other `@` becomes `_`. Thus `foo` stays
/// `foo`, `foo@stream` becomes `foo$stream`, and `foo@spec` becomes `foo_spec`.
fn bare_callable_name(name: &str) -> String {
    if let Some(base) = name.strip_suffix("@stream") {
        format!("{base}$stream")
    } else {
        name.replace('@', "_")
    }
}

/// Shared fan-out for free functions. Calls `emit` twice: once for the sync
/// binding and once for the async binding.
#[allow(clippy::type_complexity)]
fn expand_callable<F>(
    bare: &str,
    fqn_root: &str,
    arguments: &[baml_codegen_types::FunctionArgument],
    return_type: &Ty,
    mut emit: F,
) where
    F: FnMut(
        String,
        String,
        SyncAsync,
        Vec<String>,
        Vec<Ty>,
        Vec<Option<baml_codegen_types::FunctionArgumentDefault>>,
        Ty,
    ),
{
    let params: Vec<String> = arguments
        .iter()
        .map(|a| a.name.as_str().to_string())
        .collect();
    let arg_types: Vec<Ty> = arguments.iter().map(|a| a.ty.clone()).collect();
    let arg_defaults: Vec<Option<baml_codegen_types::FunctionArgumentDefault>> =
        arguments.iter().map(|a| a.default.clone()).collect();
    emit(
        bare.to_string(),
        fqn_root.to_string(),
        SyncAsync::Sync,
        params.clone(),
        arg_types.clone(),
        arg_defaults.clone(),
        return_type.clone(),
    );
    emit(
        format!("{bare}_async"),
        fqn_root.to_string(),
        SyncAsync::Async,
        params,
        arg_types,
        arg_defaults,
        return_type.clone(),
    );
}

fn origin_key(origin: &baml_codegen_types::Origin) -> SortKey {
    (origin.source_file_path.clone(), origin.span_start)
}

#[cfg(test)]
mod tests {
    use super::bare_callable_name;

    #[test]
    fn bare_callable_name_maps_companion_suffixes() {
        assert_eq!(bare_callable_name("foo"), "foo");
        assert_eq!(bare_callable_name("foo@stream"), "foo$stream");
        assert_eq!(bare_callable_name("foo@spec"), "foo_spec");
    }
}
