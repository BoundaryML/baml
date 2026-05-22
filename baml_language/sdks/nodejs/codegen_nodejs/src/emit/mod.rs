//! Emitter-internal representation of TypeScript-side symbols.
//!
//! Phase 2 mirrors `codegen_python::emit` but drops every field that
//! requires `translate_ty` (arg types, return type, defaults, …) — those
//! come back in Phase 3 / Phase 4.

pub(crate) mod class;
pub(crate) mod enum_;
pub(crate) mod function;
pub(crate) mod method;
pub(crate) mod type_alias;
pub(crate) mod typemap_file;

use baml_codegen_types::{FunctionArgumentDefault, Name, Symbol, SymbolPool, Ty};

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

/// One rendered TypeScript symbol. Mirrors `codegen_python::emit::EmittedSymbol`.
pub(crate) enum EmittedSymbol {
    Class(NodeClass),
    Enum(NodeEnum),
    TypeAlias(NodeTypeAlias),
    Function(NodeFunction),
}

impl EmittedSymbol {
    #[allow(dead_code)] // Used by Phase 4 `__all__`-style trailers.
    pub(crate) fn name(&self) -> &str {
        match self {
            EmittedSymbol::Class(c) => &c.name,
            EmittedSymbol::Enum(e) => &e.name,
            EmittedSymbol::TypeAlias(a) => &a.name,
            EmittedSymbol::Function(f) => &f.name,
        }
    }
}

/// Build-time sort key. `(source_file_path, span_start)` derived from a
/// `Symbol`'s `Origin`. Used to order symbols within a leaf in
/// source-declaration order.
pub(crate) type SortKey = (String, u32);

/// Walk every `(Name, Symbol)` in the pool and build the
/// `(LeafPath, EmittedSymbol, SortKey)` triples that drive Phase 2
/// emission. Function symbols fan out 2× (sync + async); other variants
/// are 1:1.
pub(crate) fn build_emitted(pool: &SymbolPool) -> Vec<(LeafPath, EmittedSymbol, SortKey)> {
    // Determinism: SymbolPool is a HashMap, so iteration order is
    // nondeterministic. Sort pool entries by Name before the walk so
    // triples with identical sort keys stay in a stable order.
    let mut entries: Vec<(&Name, &Symbol)> = pool.iter().collect();
    entries.sort_by_key(|e| e.0);

    let mut out: Vec<(LeafPath, EmittedSymbol, SortKey)> = Vec::new();

    for (key, symbol) in entries {
        let leaf = route(key, symbol);
        let bare = sanitize_identifier(key.bare_name());

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
/// Companions arrive as their own pool entries.
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
    expand_callable(
        &bare,
        &fqn_root,
        &f.arguments,
        &f.return_type,
        |name, fqn, mode, params, arg_tys, arg_defaults, return_ty| {
            out.push((
                leaf.clone(),
                EmittedSymbol::Function(NodeFunction {
                    name,
                    baml_fqn: fqn,
                    mode,
                    param_names: params,
                    arg_defaults,
                    arg_tys,
                    return_ty,
                    generic_params: func_generic_params.clone(),
                    docstring: func_docstring.clone(),
                }),
                sort_key.clone(),
            ));
        },
    );
}

/// Fan out class methods (statics + instances). Each emits two
/// `NodeMethodBinding`s — sync + async — kept on the parent class so
/// they surface inside the rendered class body.
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
        expand_callable(
            &bare,
            &fqn_root,
            &m.arguments,
            &m.return_type,
            |name, fqn, mode, params, arg_tys, arg_defaults, return_ty| {
                // For instance methods, prepend `"self"` so the factory
                // sees the receiver as positional arg 0 (matches the
                // `_define_function` zip in `define_function.ts`).
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
                    arg_defaults,
                    kind,
                    arg_tys,
                    return_ty,
                    generic_params: method_generic_params.clone(),
                    docstring: method_docstring.clone(),
                });
            },
        );
    }
    out
}

/// Translate a callable's BAML name (which may carry a `$<suffix>`) into
/// the TS bare identifier used as the LHS of the sync binding (the async
/// sibling appends `_async`).
///
/// - Plain `foo` → `foo`.
/// - `foo$stream` → `foo_stream`.
/// - `foo$<other>` → `foo__<other>`.
///
/// JS reserved words get a trailing underscore so the rendered
/// `export const <name>` line stays valid TypeScript.
fn bare_callable_name(name: &str) -> String {
    let raw = match name.split_once('$') {
        None => name.to_string(),
        Some((parent, "stream")) => format!("{parent}_stream"),
        Some((parent, suffix)) => format!("{parent}__{suffix}"),
    };
    sanitize_identifier(&raw)
}

/// JS reserved words can't appear as a variable binding (`export const new …`
/// is a syntax error). Append a trailing underscore to disambiguate. The
/// BAML FQN is unaffected because it's built from `Name`, not from this
/// identifier.
fn sanitize_identifier(s: &str) -> String {
    if is_reserved_word(s) {
        format!("{s}_")
    } else {
        s.to_string()
    }
}

fn is_reserved_word(s: &str) -> bool {
    matches!(
        s,
        "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "continue"
            | "debugger"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "enum"
            | "export"
            | "extends"
            | "false"
            | "finally"
            | "for"
            | "function"
            | "if"
            | "import"
            | "in"
            | "instanceof"
            | "new"
            | "null"
            | "return"
            | "super"
            | "switch"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "var"
            | "void"
            | "while"
            | "with"
            | "yield"
            | "let"
            | "static"
            | "implements"
            | "interface"
            | "package"
            | "private"
            | "protected"
            | "public"
    )
}

/// Shared fan-out for free functions and methods. Calls `emit` twice:
/// once for sync and once for async, carrying through the `param_names`
/// / `arg_tys` / `arg_defaults` / `return_ty` so the renderer can emit a
/// fully typed signature.
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
        Vec<Option<FunctionArgumentDefault>>,
        Ty,
    ),
{
    let params: Vec<String> = arguments
        .iter()
        .map(|a| a.name.as_str().to_string())
        .collect();
    let arg_types: Vec<Ty> = arguments.iter().map(|a| a.ty.clone()).collect();
    let arg_defaults: Vec<Option<FunctionArgumentDefault>> =
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
