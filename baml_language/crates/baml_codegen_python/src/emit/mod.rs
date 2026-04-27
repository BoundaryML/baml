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

use baml_codegen_types::{Name, Symbol, SymbolPool};

use crate::{
    emit::{
        class::{PyClass, PyClassProperty},
        enum_::{PyEnum, PyEnumVariant},
        function::{PyFunction, SyncAsync},
        method::{MethodKind, PyMethodBinding},
        type_alias::PyTypeAlias,
    },
    routing::{LeafPath, route},
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
/// `(LeafPath, EmittedSymbol, SortKey)` triples that drive G2
/// emission. Function symbols fan out into up to 6 `PyFunction` stubs
/// per §4.4 of the G2 plan; all other variants are 1:1.
pub(crate) fn build_emitted(pool: &SymbolPool) -> Vec<(LeafPath, EmittedSymbol, SortKey)> {
    // Determinism: SymbolPool is a HashMap, so iteration order is
    // nondeterministic. Sort pool entries by Name before the walk so
    // triples with identical sort keys (shouldn't happen in practice,
    // but if they do) stay in a stable order across runs.
    let mut entries: Vec<(&Name, &Symbol)> = pool.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));

    let mut out: Vec<(LeafPath, EmittedSymbol, SortKey)> = Vec::new();

    for (key, symbol) in entries {
        let leaf = route(key);
        let bare = key.bare_name().to_string();

        match symbol {
            Symbol::Class(c) => {
                let sort_key = origin_key(&c.origin);
                let properties = c
                    .properties
                    .iter()
                    .map(|p| PyClassProperty {
                        name: p.name.as_str().to_string(),
                        ty: p.ty.clone(),
                    })
                    .collect();
                // The class's pool key Display form is already the
                // method-FQN root (`<pkg>.<ns…>.<ClassName>`). Methods
                // append `.<method_bare>`; companions further append
                // `$<suffix>` per the existing free-function rule.
                let class_fqn_root = key.to_string();
                let static_methods =
                    expand_methods(&c.static_methods, &class_fqn_root, MethodKind::Static);
                let instance_methods =
                    expand_methods(&c.instance_methods, &class_fqn_root, MethodKind::Instance);
                out.push((
                    leaf,
                    EmittedSymbol::Class(PyClass {
                        py_name: bare,
                        source: key.clone(),
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
                        ident: v.name.as_str().to_string(),
                        value: v.value.clone(),
                    })
                    .collect();
                out.push((
                    leaf,
                    EmittedSymbol::Enum(PyEnum {
                        py_name: bare,
                        source: key.clone(),
                        variants,
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
                expand_function(&leaf, key, &bare, f, &sort_key, &mut out);
            }
        }
    }

    out
}

/// Fan out a `Symbol::Function` into its base sync/async bindings
/// plus two bindings per companion (sync + async). Per §5 the ≤6
/// emitted bindings share the parent's sort key and are pushed
/// contiguously in the fixed intra-parent order: base sync → base
/// async → companions in declaration order, each sync then async.
fn expand_function(
    leaf: &LeafPath,
    key: &Name,
    bare: &str,
    f: &baml_codegen_types::Function,
    sort_key: &SortKey,
    out: &mut Vec<(LeafPath, EmittedSymbol, SortKey)>,
) {
    // Base sync + async. The FQN is just the codegen-facing `Name`'s
    // Display form: `<pkg>.<ns…>.<bare>`. No translation — emit fully
    // qualifies all symbols, so `pkg = "user"` lands on the wire as
    // `"user.…"` end-to-end.
    let fqn_root = key.to_string();
    expand_callable(
        bare,
        &fqn_root,
        &f.arguments,
        &f.companions,
        |py_name, fqn, mode, params| {
            out.push((
                leaf.clone(),
                EmittedSymbol::Function(PyFunction {
                    py_name,
                    baml_fqn: fqn,
                    mode,
                    param_names: params,
                }),
                sort_key.clone(),
            ));
        },
    );
}

/// Fan out one source-declared method (and its companions) into one
/// `PyMethodBinding` per emitted line. Sorted before fan-out so the
/// sibling order (sync → async → companion sync → companion async)
/// matches free-function expansion exactly.
fn expand_methods(
    methods: &[baml_codegen_types::Function],
    class_fqn_root: &str,
    kind: MethodKind,
) -> Vec<PyMethodBinding> {
    let mut sorted: Vec<&baml_codegen_types::Function> = methods.iter().collect();
    sorted.sort_by(|a, b| origin_key(&a.origin).cmp(&origin_key(&b.origin)));

    let mut out: Vec<PyMethodBinding> = Vec::new();
    for m in sorted {
        let bare = m.name.as_str();
        let fqn_root = format!("{class_fqn_root}.{bare}");
        expand_callable(
            bare,
            &fqn_root,
            &m.arguments,
            &m.companions,
            |py_name, fqn, mode, params| {
                let param_names = match kind {
                    MethodKind::Static => params,
                    MethodKind::Instance => {
                        let mut with_self = Vec::with_capacity(params.len() + 1);
                        with_self.push("self".to_string());
                        with_self.extend(params);
                        with_self
                    }
                };
                out.push(PyMethodBinding {
                    py_name,
                    baml_fqn: fqn,
                    mode,
                    param_names,
                    kind,
                });
            },
        );
    }
    out
}

/// Shared fan-out for free functions and methods. Calls `emit` once
/// for each of: base sync, base async, then for each companion sync
/// and async (in declaration order). The `emit` closure receives the
/// per-line py-name, FQN, mode, and parameter names; the caller
/// adapts those into the appropriate emitted-symbol struct.
fn expand_callable<F>(
    bare: &str,
    fqn_root: &str,
    arguments: &[baml_codegen_types::FunctionArgument],
    companions: &[(String, baml_codegen_types::Function)],
    mut emit: F,
) where
    F: FnMut(String, String, SyncAsync, Vec<String>),
{
    let base_params: Vec<String> = arguments
        .iter()
        .map(|a| a.name.as_str().to_string())
        .collect();
    emit(
        bare.to_string(),
        fqn_root.to_string(),
        SyncAsync::Sync,
        base_params.clone(),
    );
    emit(
        format!("{bare}_async"),
        fqn_root.to_string(),
        SyncAsync::Async,
        base_params,
    );

    for (suffix, inner) in companions {
        let (py_sync, py_async) = if suffix == "stream" {
            (format!("{bare}_stream"), format!("{bare}_stream_async"))
        } else {
            (
                format!("{bare}__{suffix}"),
                format!("{bare}__{suffix}_async"),
            )
        };
        let companion_fqn = format!("{fqn_root}${suffix}");
        let companion_params: Vec<String> = inner
            .arguments
            .iter()
            .map(|a| a.name.as_str().to_string())
            .collect();
        emit(
            py_sync,
            companion_fqn.clone(),
            SyncAsync::Sync,
            companion_params.clone(),
        );
        emit(py_async, companion_fqn, SyncAsync::Async, companion_params);
    }
}

fn origin_key(origin: &baml_codegen_types::Origin) -> SortKey {
    (origin.source_file_path.clone(), origin.span_start)
}
