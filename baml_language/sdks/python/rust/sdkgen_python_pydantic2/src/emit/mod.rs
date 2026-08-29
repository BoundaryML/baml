//! Emitter-internal representation of Python-side symbols.
//!
//! These types describe what the emitter will render to Python, as
//! opposed to `baml_codegen_types` which describes BAML-side input
//! symbols. The split is deliberate — G3/G4/G5 will grow detail
//! fields on these types without touching the input IR.

pub(crate) mod class;
pub(crate) mod enum_;
pub(crate) mod function;
pub(crate) mod method;
pub(crate) mod type_alias;
pub(crate) mod typemap_file;

use std::collections::{BTreeMap, BTreeSet};

use baml_codegen_types::{FunctionArgumentDefault, Name, Symbol, SymbolPool, Ty};

use crate::{
    emit::{
        class::{PyClass, PyClassProperty},
        enum_::{PyEnum, PyEnumVariant},
        function::{PyFunction, SyncAsync},
        method::{MethodKind, OptionalArg, PyMethodBinding, RequiredArg},
        type_alias::PyTypeAlias,
    },
    names::{BindingRole, PythonNames},
    routing::LeafPath,
};

/// Emitter-internal representation of one rendered Python symbol.
/// One variant per Python-side symbol kind the emitter will ever
/// produce. Built from `SymbolPool` entries during the render walk.
pub(crate) enum EmittedSymbol {
    Class(PyClass),
    Enum(PyEnum),
    TypeAlias(PyTypeAlias),
    Function(PyFunction),
}

impl EmittedSymbol {
    /// The Python identifier this symbol binds.
    pub(crate) fn py_name(&self) -> &str {
        match self {
            EmittedSymbol::Class(c) => &c.py_name,
            EmittedSymbol::Enum(e) => &e.py_name,
            EmittedSymbol::TypeAlias(a) => &a.py_name,
            EmittedSymbol::Function(f) => &f.py_name,
        }
    }
}

/// Build-time sort key. Tuple of `(source_file_path, span_start)`
/// derived from a `Symbol`'s `Origin`. Used to order symbols within a
/// leaf in source-declaration order.
pub(crate) type SortKey = (String, u32);

/// Walk every `(Name, Symbol)` in the pool and build the
/// `(LeafPath, EmittedSymbol, SortKey)` triples that drive emission.
/// Each concrete callable symbol gets one sync and one async binding.
pub(crate) fn build_emitted(
    pool: &SymbolPool,
    names: &PythonNames,
) -> Vec<(LeafPath, EmittedSymbol, SortKey)> {
    let aliases: BTreeMap<Name, Ty> = pool
        .iter()
        .filter_map(|(name, symbol)| match symbol {
            Symbol::TypeAlias(alias) => Some((name.clone(), alias.resolves_to.clone())),
            _ => None,
        })
        .collect();

    // Determinism: SymbolPool is a HashMap, so iteration order is
    // nondeterministic. Sort pool entries by Name before the walk so
    // triples with identical sort keys (shouldn't happen in practice,
    // but if they do) stay in a stable order across runs.
    let mut entries: Vec<(&Name, &Symbol)> = pool.iter().collect();
    entries.sort_by_key(|e| e.0);

    let mut out: Vec<(LeafPath, EmittedSymbol, SortKey)> = Vec::new();

    for (key, symbol) in entries {
        let leaf = names.route(key, symbol);
        let bare = names.symbol(key).into_owned();

        match symbol {
            Symbol::Class(c) => {
                let sort_key = origin_key(&c.origin);
                let properties = c
                    .properties
                    .iter()
                    .map(|p| {
                        let wire_name = p.name.as_str().to_string();
                        PyClassProperty {
                            name: names.field(key, &wire_name).into_owned(),
                            wire_name,
                            ty: p.ty.clone(),
                            nullable: is_nullable(&p.ty, &aliases, &mut BTreeSet::new()),
                            docstring: p.docstring.clone(),
                        }
                    })
                    .collect();
                // The class's pool key Display form is already the
                // method-FQN root (`<pkg>.<ns…>.<ClassName>`). Methods
                // append `.<method_bare>`; companions further append
                // `$<suffix>` per the existing free-function rule.
                let class_fqn_root = key.to_string();
                let static_methods = expand_methods(
                    &c.static_methods,
                    &class_fqn_root,
                    MethodKind::Static,
                    names,
                );
                let instance_methods = expand_methods(
                    &c.instance_methods,
                    &class_fqn_root,
                    MethodKind::Instance,
                    names,
                );
                let generic_params = c
                    .generic_params
                    .iter()
                    .map(|n| names.generic(&class_fqn_root, n.as_str()).into_owned())
                    .collect();
                let wire_generic_params = c
                    .generic_params
                    .iter()
                    .map(|n| n.as_str().to_string())
                    .collect();
                let type_var_names = c
                    .generic_params
                    .iter()
                    .map(|n| {
                        (
                            n.as_str().to_string(),
                            names.generic(&class_fqn_root, n.as_str()).into_owned(),
                        )
                    })
                    .collect();
                out.push((
                    leaf,
                    EmittedSymbol::Class(PyClass {
                        py_name: bare,
                        source: key.clone(),
                        generic_params,
                        wire_generic_params,
                        type_var_names,
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
                    .map(|v| PyEnumVariant {
                        ident: names.enum_variant(key, v.name.as_str()).into_owned(),
                        value: v.value.clone(),
                        docstring: v.docstring.clone(),
                    })
                    .collect();
                out.push((
                    leaf,
                    EmittedSymbol::Enum(PyEnum {
                        py_name: bare,
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
                    EmittedSymbol::TypeAlias(PyTypeAlias {
                        py_name: bare,
                        source: key.clone(),
                        resolves_to: t.resolves_to.clone(),
                        recursive: t.recursive,
                    }),
                    sort_key,
                ));
            }
            Symbol::Function(f) => {
                let sort_key = origin_key(&f.origin);
                expand_function(&leaf, key, f, &sort_key, names, &mut out);
            }
        }
    }

    out
}

fn is_nullable(ty: &Ty, aliases: &BTreeMap<Name, Ty>, visiting: &mut BTreeSet<Name>) -> bool {
    match ty {
        Ty::Null { .. } => true,
        Ty::Union(items, _) => items
            .iter()
            .any(|item| is_nullable(item, aliases, visiting)),
        Ty::TypeAlias(name, _) => {
            let Some(resolved) = aliases.get(name) else {
                return false;
            };
            if !visiting.insert(name.clone()) {
                return false;
            }
            let nullable = is_nullable(resolved, aliases, visiting);
            visiting.remove(name);
            nullable
        }
        _ => false,
    }
}

/// Emit sync and async Python bindings for one exact BAML callable FQN.
fn expand_function(
    leaf: &LeafPath,
    key: &Name,
    f: &baml_codegen_types::Function,
    sort_key: &SortKey,
    names: &PythonNames,
    out: &mut Vec<(LeafPath, EmittedSymbol, SortKey)>,
) {
    // Companion symbols already carry their exact `@spec`/`@stream` suffix.
    let fqn_root = key.to_string();
    let func_generic_params: Vec<String> = f
        .generic_params
        .iter()
        .map(|n| names.generic(&fqn_root, n.as_str()).into_owned())
        .collect();
    let wire_generic_params: Vec<String> = f
        .generic_params
        .iter()
        .map(|n| n.as_str().to_string())
        .collect();
    let type_var_names: BTreeMap<String, String> = f
        .generic_params
        .iter()
        .map(|n| {
            (
                n.as_str().to_string(),
                names.generic(&fqn_root, n.as_str()).into_owned(),
            )
        })
        .collect();
    let func_docstring = f.docstring.clone();
    let raises_names = collect_raises_names(f.throws.as_ref(), names);
    for role in [BindingRole::DirectSync, BindingRole::DirectAsync] {
        let arguments: Vec<_> = f.arguments.iter().collect();
        let wire_params: Vec<String> = arguments
            .iter()
            .map(|argument| argument.name.as_str().to_string())
            .collect();
        let params: Vec<String> = wire_params
            .iter()
            .map(|param| names.param(&fqn_root, param).into_owned())
            .collect();
        let arg_tys: Vec<Ty> = arguments
            .iter()
            .map(|argument| argument.ty.clone())
            .collect();
        let arg_defaults: Vec<Option<FunctionArgumentDefault>> = arguments
            .iter()
            .map(|argument| argument.default.clone())
            .collect();
        let mode = if role.is_async() {
            SyncAsync::Async
        } else {
            SyncAsync::Sync
        };
        out.push((
            leaf.clone(),
            EmittedSymbol::Function(PyFunction {
                py_name: names.callable(&fqn_root, role).into_owned(),
                baml_fqn: fqn_root.clone(),
                mode,
                param_names: params,
                wire_param_names: wire_params,
                arg_defaults,
                arg_tys,
                return_ty: f.return_type.clone(),
                generic_params: func_generic_params.clone(),
                wire_generic_params: wire_generic_params.clone(),
                type_var_names: type_var_names.clone(),
                docstring: func_docstring.clone(),
                raises_names: raises_names.clone(),
            }),
            sort_key.clone(),
        ));
    }
}

/// Collect the unqualified leaf names of the thrown types in a `throws` `Ty`,
/// in source order, de-duping exact-equal names (32d). Class/Enum/TypeAlias
/// contribute their unqualified leaf name; a union contributes each member's;
/// an optional unwraps; anything else (primitives) contributes nothing.
fn collect_raises_names(
    throws: Option<&baml_codegen_types::Ty>,
    names: &PythonNames,
) -> Vec<String> {
    use baml_codegen_types::Ty;

    fn walk(ty: &Ty, names: &PythonNames, out: &mut Vec<String>) {
        match ty {
            Ty::Class(name, _, _) | Ty::Enum(name, _) | Ty::TypeAlias(name, _) => {
                let n = names.symbol(name).into_owned();
                if !out.contains(&n) {
                    out.push(n);
                }
            }
            Ty::Union(members, _) => members.iter().for_each(|m| walk(m, names, out)),
            _ => {}
        }
    }

    let mut out = Vec::new();
    if let Some(ty) = throws {
        walk(ty, names, &mut out);
    }
    out
}

/// Emit sync and async bindings for source-declared methods.
fn expand_methods(
    methods: &[baml_codegen_types::Function],
    class_fqn_root: &str,
    kind: MethodKind,
    names: &PythonNames,
) -> Vec<PyMethodBinding> {
    let mut sorted: Vec<&baml_codegen_types::Function> = methods.iter().collect();
    sorted.sort_by_key(|m| (origin_key(&m.origin), m.name.as_str()));

    let mut out: Vec<PyMethodBinding> = Vec::new();
    for m in sorted {
        let m_name = m.name.as_str();
        let fqn_root = format!("{class_fqn_root}.{m_name}");
        let method_generic_params: Vec<String> = m
            .generic_params
            .iter()
            .map(|n| names.generic(&fqn_root, n.as_str()).into_owned())
            .collect();
        let wire_generic_params: Vec<String> = m
            .generic_params
            .iter()
            .map(|n| n.as_str().to_string())
            .collect();
        let type_var_names: BTreeMap<String, String> = m
            .generic_params
            .iter()
            .map(|n| {
                (
                    n.as_str().to_string(),
                    names.generic(&fqn_root, n.as_str()).into_owned(),
                )
            })
            .collect();
        let method_docstring = m.docstring.clone();
        let raises_names = collect_raises_names(m.throws.as_ref(), names);
        for role in [BindingRole::DirectSync, BindingRole::DirectAsync] {
            let arguments: Vec<_> = m.arguments.iter().collect();
            let (required_args, optional_args) = split_arguments(&arguments, &fqn_root, names);
            let mode = if role.is_async() {
                SyncAsync::Async
            } else {
                SyncAsync::Sync
            };
            out.push(PyMethodBinding {
                py_name: names.callable(&fqn_root, role).into_owned(),
                baml_fqn: fqn_root.clone(),
                mode,
                required_args,
                optional_args,
                kind,
                return_ty: m.return_type.clone(),
                generic_params: method_generic_params.clone(),
                wire_generic_params: wire_generic_params.clone(),
                type_var_names: type_var_names.clone(),
                docstring: method_docstring.clone(),
                raises_names: raises_names.clone(),
            });
        }
    }
    out
}

fn split_arguments(
    arguments: &[&baml_codegen_types::FunctionArgument],
    fqn: &str,
    names: &PythonNames,
) -> (Vec<RequiredArg>, Vec<OptionalArg>) {
    let first_optional = arguments
        .iter()
        .position(|arg| arg.default.is_some())
        .unwrap_or(arguments.len());
    (
        arguments[..first_optional]
            .iter()
            .map(|arg| {
                let wire_name = arg.name.as_str().to_string();
                RequiredArg {
                    name: names.param(fqn, &wire_name).into_owned(),
                    wire_name,
                    ty: arg.ty.clone(),
                }
            })
            .collect(),
        arguments[first_optional..]
            .iter()
            .map(|arg| {
                let wire_name = arg.name.as_str().to_string();
                OptionalArg {
                    name: names.param(fqn, &wire_name).into_owned(),
                    wire_name,
                    ty: arg.ty.clone(),
                    default: arg.default.clone().expect(
                        "arguments after the first defaulted method arg must have defaults",
                    ),
                }
            })
            .collect(),
    )
}

fn origin_key(origin: &baml_codegen_types::Origin) -> SortKey {
    (origin.source_file_path.clone(), origin.span_start)
}
