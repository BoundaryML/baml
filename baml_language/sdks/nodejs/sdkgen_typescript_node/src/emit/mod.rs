//! Emitter-internal representation of TypeScript-side symbols.
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

use baml_codegen_types::{Name, Symbol, SymbolPool, Ty};

use crate::{
    emit::{
        class::{NodeClass, NodeClassProperty},
        enum_::{NodeEnum, NodeEnumVariant},
        function::{NodeFunction, SyncAsync},
        method::{MethodKind, NodeMethodBinding},
        type_alias::NodeTypeAlias,
    },
    routing::{LeafPath, route},
};

/// Emitter-internal representation of one rendered TypeScript symbol.
pub(crate) enum EmittedSymbol {
    Class(NodeClass),
    Enum(NodeEnum),
    TypeAlias(NodeTypeAlias),
    Function(NodeFunction),
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
        let bare = key.name.as_str().to_string();

        match symbol {
            Symbol::Class(c) => {
                let sort_key = origin_key(&c.origin);
                let properties = c
                    .properties
                    .iter()
                    .map(|p| NodeClassProperty {
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
                    EmittedSymbol::Class(NodeClass {
                        name: bare,
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
                    .map(|v| NodeEnumVariant {
                        ident: v.name.as_str().to_string(),
                        value: v.value.clone(),
                        docstring: v.docstring.clone(),
                    })
                    .collect();
                out.push((
                    leaf,
                    EmittedSymbol::Enum(NodeEnum {
                        name: bare,
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
                    EmittedSymbol::TypeAlias(NodeTypeAlias {
                        name: bare,
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
    let bare = bare_callable_name(key.name.as_str());
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
        |name, fqn, mode, params, arg_tys, return_ty| {
            out.push((
                leaf.clone(),
                EmittedSymbol::Function(NodeFunction {
                    name,
                    baml_fqn: fqn,
                    mode,
                    param_names: params,
                    arg_tys,
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
            Ty::Class(name, _) | Ty::Enum(name) | Ty::TypeAlias(name) => {
                let n = name.name.as_str().to_string();
                if !out.contains(&n) {
                    out.push(n);
                }
            }
            Ty::Union(members) => members.iter().for_each(|m| walk(m, out)),
            Ty::Optional(inner) => walk(inner, out),
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
/// `NodeMethodBinding` per emitted line. Methods are sorted by `(file,
/// span, name)` so a parent and its companions cluster together.
fn expand_methods(
    methods: &[baml_codegen_types::Function],
    class_fqn_root: &str,
    kind: MethodKind,
) -> Vec<NodeMethodBinding> {
    let mut sorted: Vec<&baml_codegen_types::Function> = methods.iter().collect();
    sorted.sort_by_key(|m| (origin_key(&m.origin), m.name.as_str()));

    let mut out: Vec<NodeMethodBinding> = Vec::new();
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
        expand_callable(
            &bare,
            &fqn_root,
            &m.arguments,
            &m.return_type,
            |name, fqn, mode, params, arg_tys, return_ty| {
                let param_names = match kind {
                    MethodKind::Static => params,
                    MethodKind::Instance => {
                        let mut with_self = Vec::with_capacity(params.len() + 1);
                        with_self.push("self".to_string());
                        with_self.extend(params);
                        with_self
                    }
                };
                out.push(NodeMethodBinding {
                    name,
                    baml_fqn: fqn,
                    mode,
                    param_names,
                    kind,
                    arg_tys,
                    return_ty,
                    generic_params: method_generic_params.clone(),
                    docstring: method_docstring.clone(),
                    raises_names: raises_names.clone(),
                });
            },
        );
    }
    out
}

/// The TS-side bare identifier for a callable's BAML name, used as the LHS
/// of the sync binding (the async sibling appends `_async`).
///
/// spec2: the BAML name — including any `$<suffix>` companion marker — is
/// preserved verbatim, because `$` is a valid TypeScript identifier
/// character. So `foo` → `foo`, `foo$stream` → `foo$stream`,
/// `foo$build_request` → `foo$build_request`. (Python must translate these
/// to `_stream` / `__build_request`; TypeScript does not.)
fn bare_callable_name(name: &str) -> String {
    name.to_string()
}

/// Shared fan-out for free functions and methods. Calls `emit` twice:
/// once for the sync binding and once for the async binding.
#[allow(clippy::type_complexity)]
fn expand_callable<F>(
    bare: &str,
    fqn_root: &str,
    arguments: &[baml_codegen_types::FunctionArgument],
    return_type: &Ty,
    mut emit: F,
) where
    F: FnMut(String, String, SyncAsync, Vec<String>, Vec<Ty>, Ty),
{
    let params: Vec<String> = arguments
        .iter()
        .map(|a| a.name.as_str().to_string())
        .collect();
    let arg_types: Vec<Ty> = arguments.iter().map(|a| a.ty.clone()).collect();
    emit(
        bare.to_string(),
        fqn_root.to_string(),
        SyncAsync::Sync,
        params.clone(),
        arg_types.clone(),
        return_type.clone(),
    );
    emit(
        format!("{bare}_async"),
        fqn_root.to_string(),
        SyncAsync::Async,
        params,
        arg_types,
        return_type.clone(),
    );
}

fn origin_key(origin: &baml_codegen_types::Origin) -> SortKey {
    (origin.source_file_path.clone(), origin.span_start)
}
