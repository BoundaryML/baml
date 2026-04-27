//! Emitter-internal representation of Python-side symbols.
//!
//! These types describe what the emitter will render to Python, as
//! opposed to `baml_codegen_types` which describes BAML-side input
//! symbols. The split is deliberate — G3/G4/G5 will grow detail
//! fields on these types without touching the input IR.

pub(crate) mod class;
pub(crate) mod enum_;
pub(crate) mod function;
pub(crate) mod instance_method;
pub(crate) mod static_method;
pub(crate) mod type_alias;

use baml_codegen_types::{Name, Symbol, SymbolPool};

use crate::{
    emit::{
        class::{PyClass, PyClassProperty},
        enum_::{PyEnum, PyEnumVariant},
        function::{PyFunction, SyncAsync},
        instance_method::PyInstanceMethod,
        static_method::PyStaticMethod,
        type_alias::PyTypeAlias,
    },
    routing::{LeafPath, route},
};

/// Emitter-internal representation of one rendered Python symbol.
/// One variant per Python-side symbol kind the emitter will ever
/// produce. Built from `SymbolPool` entries during the render walk.
#[allow(dead_code)]
pub(crate) enum EmittedSymbol {
    Class(PyClass),
    Enum(PyEnum),
    TypeAlias(PyTypeAlias),
    Function(PyFunction),
    StaticMethod(PyStaticMethod),
    InstanceMethod(PyInstanceMethod),
}

impl EmittedSymbol {
    /// The Python identifier this symbol binds.
    pub(crate) fn py_name(&self) -> &str {
        match self {
            EmittedSymbol::Class(c) => &c.py_name,
            EmittedSymbol::Enum(e) => &e.py_name,
            EmittedSymbol::TypeAlias(a) => &a.py_name,
            EmittedSymbol::Function(f) => &f.py_name,
            EmittedSymbol::StaticMethod(m) => &m.py_name,
            EmittedSymbol::InstanceMethod(m) => &m.py_name,
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
                out.push((
                    leaf,
                    EmittedSymbol::Class(PyClass {
                        py_name: bare,
                        source: key.clone(),
                        properties,
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
    let base_fqn = key.to_string();
    let base_params: Vec<String> = f
        .arguments
        .iter()
        .map(|a| a.name.as_str().to_string())
        .collect();
    out.push((
        leaf.clone(),
        EmittedSymbol::Function(PyFunction {
            py_name: bare.to_string(),
            baml_fqn: base_fqn.clone(),
            mode: SyncAsync::Sync,
            param_names: base_params.clone(),
        }),
        sort_key.clone(),
    ));
    out.push((
        leaf.clone(),
        EmittedSymbol::Function(PyFunction {
            py_name: format!("{bare}_async"),
            baml_fqn: base_fqn,
            mode: SyncAsync::Async,
            param_names: base_params,
        }),
        sort_key.clone(),
    ));

    // Companions, in declaration order.
    for (suffix, inner) in &f.companions {
        let (py_sync, py_async) = if suffix == "stream" {
            (format!("{bare}_stream"), format!("{bare}_stream_async"))
        } else {
            (
                format!("{bare}__{suffix}"),
                format!("{bare}__{suffix}_async"),
            )
        };
        let companion_fqn = format!("{key}${suffix}");
        // §6: companion `param_names` come from the companion's own
        // arguments, not the parent's.
        let companion_params: Vec<String> = inner
            .arguments
            .iter()
            .map(|a| a.name.as_str().to_string())
            .collect();

        out.push((
            leaf.clone(),
            EmittedSymbol::Function(PyFunction {
                py_name: py_sync,
                baml_fqn: companion_fqn.clone(),
                mode: SyncAsync::Sync,
                param_names: companion_params.clone(),
            }),
            sort_key.clone(),
        ));
        out.push((
            leaf.clone(),
            EmittedSymbol::Function(PyFunction {
                py_name: py_async,
                baml_fqn: companion_fqn,
                mode: SyncAsync::Async,
                param_names: companion_params,
            }),
            sort_key.clone(),
        ));
    }
}

fn origin_key(origin: &baml_codegen_types::Origin) -> SortKey {
    (origin.source_file_path.clone(), origin.span_start)
}
