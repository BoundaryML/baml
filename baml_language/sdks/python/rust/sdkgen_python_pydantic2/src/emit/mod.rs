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

use baml_codegen_types::{FunctionArgument, FunctionArgumentDefault, Name, Symbol, SymbolPool, Ty};

use crate::{
    emit::{
        class::{PyClass, PyClassProperty},
        enum_::{PyEnum, PyEnumVariant},
        function::{PyFunction, SyncAsync},
        method::{MethodKind, OptionalArg, PyMethodBinding, RequiredArg},
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
        let leaf = route(key, symbol);
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
                        nullable: is_nullable(&p.ty, &aliases, &mut BTreeSet::new()),
                        docstring: p.docstring.clone(),
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
                let generic_params = c
                    .generic_params
                    .iter()
                    .map(|n| n.as_str().to_string())
                    .collect();
                out.push((
                    leaf,
                    EmittedSymbol::Class(PyClass {
                        py_name: bare,
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
                    .map(|v| PyEnumVariant {
                        ident: v.name.as_str().to_string(),
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
                expand_function(&leaf, key, f, &sort_key, &mut out);
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
    // The FQN is just the codegen-facing `Name`'s Display form:
    // `<pkg>.<ns…>.<bare>`. No translation — emit fully qualifies all
    // symbols, so `pkg = "user"` lands on the wire as `"user.…"` end-to-
    // end. The Python LHS strips the `$<suffix>` form into the
    // companion's bare identifier (`__<suffix>` or `_stream`).
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
        |py_name, fqn, mode, params, arg_tys, arg_defaults, return_ty| {
            out.push((
                leaf.clone(),
                EmittedSymbol::Function(PyFunction {
                    py_name: escape_python_keyword(py_name),
                    baml_fqn: fqn,
                    mode,
                    param_names: params,
                    arg_defaults,
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

/// Collect the unqualified leaf names of the thrown types in a `throws` `Ty`,
/// in source order, de-duping exact-equal names (32d). Class/Enum/TypeAlias
/// contribute their unqualified leaf name; a union contributes each member's;
/// an optional unwraps; anything else (primitives) contributes nothing.
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
/// `PyMethodBinding` per emitted line. Methods are sorted by `(file,
/// span, name)` so a parent and its companions — which share the parent's
/// span — cluster together with the parent first (the parent name is a
/// prefix of every companion name and `$` < any alphanumeric).
fn expand_methods(
    methods: &[baml_codegen_types::Function],
    class_fqn_root: &str,
    kind: MethodKind,
) -> Vec<PyMethodBinding> {
    let mut sorted: Vec<&baml_codegen_types::Function> = methods.iter().collect();
    sorted.sort_by_key(|m| (origin_key(&m.origin), m.name.as_str()));

    let mut out: Vec<PyMethodBinding> = Vec::new();
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
        for (py_name, mode) in [
            (bare.clone(), SyncAsync::Sync),
            (format!("{bare}_async"), SyncAsync::Async),
        ] {
            out.push(PyMethodBinding {
                py_name: escape_python_keyword(py_name),
                baml_fqn: fqn_root.clone(),
                mode,
                required_args: required_args.clone(),
                optional_args: optional_args.clone(),
                kind,
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

/// Translate a callable's BAML name (which may carry a `$<suffix>` for
/// companions) into the Python-side bare identifier used as the LHS of
/// the sync binding (the async sibling appends `_async`).
///
/// - Plain name `foo` → `foo`.
/// - `foo$stream` → `foo_stream` (only companion that uses single
///   underscore — matches the longstanding free-function rule).
/// - `foo$<other>` → `foo__<other>`.
pub(crate) fn bare_callable_name(name: &str) -> String {
    match name.split_once('$') {
        None => name.to_string(),
        Some((parent, "stream")) => format!("{parent}_stream"),
        Some((parent, suffix)) => format!("{parent}__{suffix}"),
    }
}

/// Append `_` to a Python hard keyword so it is a usable identifier on the
/// Python side (`from` → `from_`), generalizing the `assert` → `assert_` rule
/// in [`crate::routing`] to callable identifiers. Only the rendered Python name
/// is affected; the runtime BAML FQN (`PyMethodBinding::baml_fqn` /
/// `PyFunction::baml_fqn`) is built from the raw `Name`, so dispatch still
/// targets the original `from`. Non-keyword names pass through unchanged.
pub(crate) fn escape_python_keyword(ident: String) -> String {
    // Python 3 hard keywords (soft keywords like `match`/`case`/`type` are
    // valid identifiers and are intentionally excluded).
    const PYTHON_KEYWORDS: &[&str] = &[
        "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class",
        "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global",
        "if", "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return",
        "try", "while", "with", "yield",
    ];
    if PYTHON_KEYWORDS.contains(&ident.as_str()) {
        format!("{ident}_")
    } else {
        ident
    }
}

/// Shared fan-out for free functions and methods. Calls `emit` twice:
/// once for the sync binding and once for the async binding. Companions
/// arrive as their own callable; the suffix-aware `bare` value is
/// computed up front by the caller via `bare_callable_name`.
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

#[cfg(test)]
mod tests {
    use super::escape_python_keyword;

    #[test]
    fn keyword_identifiers_get_a_trailing_underscore() {
        // The case that motivated this: `string.from` must not emit `def from`.
        assert_eq!(escape_python_keyword("from".into()), "from_");
        assert_eq!(escape_python_keyword("class".into()), "class_");
        assert_eq!(escape_python_keyword("lambda".into()), "lambda_");
    }

    #[test]
    fn non_keywords_pass_through_unchanged() {
        assert_eq!(escape_python_keyword("to_json".into()), "to_json");
        assert_eq!(escape_python_keyword("length".into()), "length");
        // Already-suffixed async sibling of `from` is a valid identifier.
        assert_eq!(escape_python_keyword("from_async".into()), "from_async");
        // Soft keywords are valid identifiers and must not be escaped.
        assert_eq!(escape_python_keyword("match".into()), "match");
        assert_eq!(escape_python_keyword("type".into()), "type");
    }
}
