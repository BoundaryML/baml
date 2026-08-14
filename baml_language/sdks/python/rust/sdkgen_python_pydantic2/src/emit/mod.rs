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
                // Escape keyword field names collision-aware within the class
                // scope. A field renamed off a keyword carries an `alias` = its
                // raw BAML name so it stays the JSON/wire key (pydantic
                // `Field(alias=…)`); non-escaped fields keep `alias: None` and
                // render byte-identically to today.
                let raw_prop_names: Vec<String> = c
                    .properties
                    .iter()
                    .map(|p| p.name.as_str().to_string())
                    .collect();
                let escaped_prop_names = escape_keywords_in_scope(&raw_prop_names);
                let mut properties: Vec<PyClassProperty> = c
                    .properties
                    .iter()
                    .zip(escaped_prop_names)
                    .map(|(p, escaped)| {
                        let raw = p.name.as_str();
                        let alias = (escaped != raw).then(|| raw.to_string());
                        PyClassProperty {
                            name: escaped,
                            ty: p.ty.clone(),
                            docstring: p.docstring.clone(),
                            alias,
                        }
                    })
                    .collect();
                // The class's pool key Display form is already the
                // method-FQN root (`<pkg>.<ns…>.<ClassName>`). Methods
                // append `.<method_bare>`; companions further append
                // `$<suffix>` per the existing free-function rule.
                let class_fqn_root = key.to_string();
                let mut static_methods =
                    expand_methods(&c.static_methods, &class_fqn_root, MethodKind::Static);
                let mut instance_methods =
                    expand_methods(&c.instance_methods, &class_fqn_root, MethodKind::Instance);
                // Reserve the `__baml_wire_names__` marker name across the
                // combined field+method member set when the marker will be
                // emitted, so a user member of that name is bumped rather than
                // clobbering the marker dict at class creation. Keyword-free
                // classes emit no marker, so this is a no-op for them.
                reserve_class_wire_marker(
                    &mut properties,
                    &mut static_methods,
                    &mut instance_methods,
                );
                // TypeVar names are allocated LEAF-globally by
                // `leaf::allocate_leaf_type_vars` after `group_and_sort`, so a
                // bumped keyword TypeVar can never collide with a sibling
                // class/enum/alias name and distinct raw TypeVar spellings never
                // collapse onto one emitted name (identical spellings deliberately
                // share one module-level TypeVar). Here we only carry
                // the RAW names forward; `type_var_map` is filled by that pass.
                let raw_generic_params: Vec<String> = c
                    .generic_params
                    .iter()
                    .map(|n| n.as_str().to_string())
                    .collect();
                out.push((
                    leaf,
                    EmittedSymbol::Class(PyClass {
                        py_name: escape_python_keyword(bare),
                        source: key.clone(),
                        generic_params: raw_generic_params,
                        type_var_map: TypeVarMap::new(),
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
                // Escape keyword member idents collision-aware within the enum
                // scope. The RHS `value` stays IR-verbatim, so `None` renders
                // `None_ = "None"` — the wire value is unchanged and decode
                // (which is by VALUE) keeps working with no alias machinery.
                let raw_variant_idents: Vec<String> = e
                    .variants
                    .iter()
                    .map(|v| v.name.as_str().to_string())
                    .collect();
                let escaped_idents = escape_keywords_in_scope(&raw_variant_idents);
                let mut variants: Vec<PyEnumVariant> = e
                    .variants
                    .iter()
                    .zip(escaped_idents)
                    .map(|(v, ident)| {
                        // `escaped_idents[i] != raw` exactly marks the keyword-
                        // escaped members (escape_keywords_in_scope returns
                        // non-keywords unchanged); carry the raw variant name as
                        // the wire-value provenance for `__baml_wire_values__`.
                        let raw = v.name.as_str();
                        let wire_name = (ident != raw).then(|| raw.to_string());
                        PyEnumVariant {
                            ident,
                            value: v.value.clone(),
                            docstring: v.docstring.clone(),
                            wire_name,
                        }
                    })
                    .collect();
                // Symmetric to the class marker: reserve `__baml_wire_values__`
                // across the member idents when the enum marker will be emitted.
                reserve_enum_wire_marker(&mut variants);
                out.push((
                    leaf,
                    EmittedSymbol::Enum(PyEnum {
                        py_name: escape_python_keyword(bare),
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
                        py_name: escape_python_keyword(bare),
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
    // The FQN is just the codegen-facing `Name`'s Display form:
    // `<pkg>.<ns…>.<bare>`. No translation — emit fully qualifies all
    // symbols, so `pkg = "user"` lands on the wire as `"user.…"` end-to-
    // end. The Python LHS strips the `$<suffix>` form into the
    // companion's bare identifier (`__<suffix>` or `_stream`).
    let fqn_root = key.to_string();
    let bare = bare_callable_name(key.name().as_str());
    // Raw TypeVar names only; leaf-global allocation happens later in
    // `leaf::allocate_leaf_type_vars`.
    let func_generic_params: Vec<String> = f
        .generic_params
        .iter()
        .map(|n| n.as_str().to_string())
        .collect();
    let func_type_var_map = TypeVarMap::new();
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
                    type_var_map: func_type_var_map.clone(),
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
        // Raw TypeVar names only; leaf-global allocation happens later
        // in `leaf::allocate_leaf_type_vars`.
        let method_generic_params: Vec<String> = m
            .generic_params
            .iter()
            .map(|n| n.as_str().to_string())
            .collect();
        let method_type_var_map = TypeVarMap::new();
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
                type_var_map: method_type_var_map.clone(),
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

/// The 35 Python 3 **hard** keywords (`keyword.kwlist` on `CPython` 3.9–3.13).
/// Soft keywords (`match`, `case`, `type`, `_`) are valid identifiers and are
/// intentionally excluded — the source guard `RESERVED_NAMES_PYTHON` in the
/// engine uses this same set. Kept sorted so the
/// `python_hard_keyword_set_is_exactly_the_35_cpython_hard_keywords` test can
/// diff it against a sorted expectation.
const PYTHON_HARD_KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield",
];

/// True when `ident` is a Python 3 hard keyword (and therefore an illegal
/// identifier). Shared by every escape site so the generator's escape sites
/// stay in sync with the engine's `RESERVED_NAMES_PYTHON` guard (two synced
/// copies of one list).
pub(crate) fn is_python_hard_keyword(ident: &str) -> bool {
    PYTHON_HARD_KEYWORDS.contains(&ident)
}

/// Append `_` to a Python hard keyword so it is a usable identifier on the
/// Python side (`from` → `from_`), generalizing the `assert` → `assert_` rule
/// in [`crate::routing`] to callable identifiers. Only the rendered Python name
/// is affected; the runtime BAML FQN (`PyMethodBinding::baml_fqn` /
/// `PyFunction::baml_fqn`) is built from the raw `Name`, so dispatch still
/// targets the original `from`. Non-keyword names pass through unchanged.
///
/// This is the *stateless* escape used for symbols whose escaped spelling has
/// to be reproducible from the bare `Name` alone at a distance — class / enum /
/// type-alias names (which `translate_ty::render_name_ref` re-escapes at every
/// cross-reference) and `TypeVar` names. Collision-resolving escaping (for
/// class fields and enum members, which live in a single self-contained scope)
/// goes through [`escape_keywords_in_scope`].
pub(crate) fn escape_python_keyword(ident: String) -> String {
    if is_python_hard_keyword(&ident) {
        format!("{ident}_")
    } else {
        ident
    }
}

/// Collision-aware keyword escaping for one self-contained scope (a single
/// class's fields, or a single enum's members). Mirrors the Go generator's
/// per-scope allocation (PR #4067) with a used-set instead of a content hash:
///
/// - **Pass 1** reserves every RAW non-keyword name in the scope.
/// - **Pass 2**, in declaration order, escapes each keyword name: start from
///   `name + "_"` and append `_` while the candidate is a hard keyword or is
///   already reserved, then reserve the result.
///
/// So a scope declaring both `pass` and `pass_` yields `{pass__, pass_}` — the
/// raw `pass_` is reserved in pass 1, forcing `pass` past it. Deterministic and
/// order-independent for the raw set (distinct keywords escape to disjoint
/// prefixes, and BAML forbids duplicate field / member names, so pass-2
/// insertions never perturb another keyword's resolution). Non-keyword names
/// are returned unchanged, so `escaped[i] != names[i]` exactly identifies the
/// escaped positions (used to derive the pydantic `Field(alias=…)` wire name).
pub(crate) fn escape_keywords_in_scope(names: &[String]) -> Vec<String> {
    let mut reserved: std::collections::HashSet<String> = names
        .iter()
        .filter(|n| !is_python_hard_keyword(n))
        .cloned()
        .collect();
    names
        .iter()
        .map(|name| {
            if !is_python_hard_keyword(name) {
                name.clone()
            } else {
                let mut candidate = format!("{name}_");
                while is_python_hard_keyword(&candidate) || reserved.contains(&candidate) {
                    candidate.push('_');
                }
                reserved.insert(candidate.clone());
                candidate
            }
        })
        .collect()
}

/// The synthetic wire-identity markers the generator stamps into a class / enum
/// body when at least one member is keyword-escaped (`build_wire_names_marker` /
/// `build_wire_values_marker`, `leaf.rs`). Python binds the marker as an ordinary
/// (dunder) class attribute, so a user member that lands on the same spelling
/// would clobber it at class-creation time (the later binding wins). These names
/// are therefore reserved across the body's member namespace whenever the marker
/// is emitted.
const WIRE_NAMES_MARKER: &str = "__baml_wire_names__";
const WIRE_VALUES_MARKER: &str = "__baml_wire_values__";

/// Bump `name` past everything already in `reserved` by appending `_` (the same
/// trailing-underscore rule [`escape_keywords_in_scope`] uses), then reserve and
/// return the result.
fn bump_past_reserved(name: &str, reserved: &mut std::collections::HashSet<String>) -> String {
    let mut candidate = format!("{name}_");
    while reserved.contains(&candidate) {
        candidate.push('_');
    }
    reserved.insert(candidate.clone());
    candidate
}

/// Project a class FIELD that collides with the `__baml_wire_names__` marker onto
/// a pydantic-legal attribute spelling. A METHOD collision can keep the marker's
/// leading underscores ([`bump_past_reserved`] appends one `_`) because it renders
/// as a bare class-body assignment; a FIELD cannot. pydantic rejects a model field
/// whose name begins with an underscore (`NameError: Fields must not use names with
/// leading underscores`) at class creation, which fails `import` of the whole leaf
/// module. So shed the leading underscores to form the base, then apply the same
/// trailing-underscore disambiguation against `reserved`. The raw
/// `__baml_wire_names__` is preserved as the field's `pydantic.Field(alias=…)` and
/// as the marker's wire value, so wire identity is unchanged.
fn project_field_off_marker(reserved: &mut std::collections::HashSet<String>) -> String {
    let base = WIRE_NAMES_MARKER.trim_start_matches('_');
    let mut candidate = format!("{base}_");
    while reserved.contains(&candidate) {
        candidate.push('_');
    }
    reserved.insert(candidate.clone());
    candidate
}

/// Reserve the `__baml_wire_names__` marker identifier across a class's combined
/// field + method member namespace, but ONLY when the marker will actually be
/// emitted (>= 1 field carries an `alias`, i.e. was keyword-escaped). A METHOD
/// whose emitted name equals the marker is bumped one trailing underscore past it
/// (it renders as a bare class-body assignment, so leading underscores are fine).
/// A FIELD named like the marker is instead projected onto a leading-underscore-
/// free spelling by [`project_field_off_marker`], because pydantic refuses a model
/// field whose name starts with `_` (a plain trailing-underscore bump would still
/// lead with `__` and crash `import` at class creation); it records
/// `alias = "__baml_wire_names__"` so the marker still lists it and its raw wire
/// identity is preserved. Keyword-free classes emit no marker, so nothing is
/// reserved and their output stays byte-identical (the digest gate enforces this).
fn reserve_class_wire_marker(
    properties: &mut [PyClassProperty],
    static_methods: &mut [PyMethodBinding],
    instance_methods: &mut [PyMethodBinding],
) {
    let marker_emitted = properties.iter().any(|p| p.alias.is_some());
    if !marker_emitted {
        return;
    }
    // Reserve every member name that is NOT itself the marker token (those keep
    // their spelling), plus the marker token (the marker occupies it). Bumps
    // search past this set.
    let mut reserved: std::collections::HashSet<String> = properties
        .iter()
        .map(|p| p.name.clone())
        .chain(static_methods.iter().map(|m| m.py_name.clone()))
        .chain(instance_methods.iter().map(|m| m.py_name.clone()))
        .filter(|n| n != WIRE_NAMES_MARKER)
        .collect();
    reserved.insert(WIRE_NAMES_MARKER.to_string());

    for p in properties.iter_mut() {
        if p.name == WIRE_NAMES_MARKER {
            // Fields must not lead with `_` (pydantic raises at class creation),
            // so project off the marker to a legal spelling rather than the plain
            // trailing-underscore bump the method arm uses.
            p.name = project_field_off_marker(&mut reserved);
            // Preserve the raw wire key: the marker now maps projected-attr -> raw.
            if p.alias.is_none() {
                p.alias = Some(WIRE_NAMES_MARKER.to_string());
            }
        }
    }
    for m in static_methods.iter_mut().chain(instance_methods.iter_mut()) {
        if m.py_name == WIRE_NAMES_MARKER {
            m.py_name = bump_past_reserved(WIRE_NAMES_MARKER, &mut reserved);
        }
    }
}

/// Enum counterpart of [`reserve_class_wire_marker`]. BAML enums have no methods,
/// so the only collision vector is a member literally named `__baml_wire_values__`;
/// reserve the marker across the member idents whenever the enum marker is emitted
/// (>= 1 escaped member). A bumped member records its raw wire name into the marker.
/// Keyword-free enums emit no marker and stay byte-identical.
fn reserve_enum_wire_marker(variants: &mut [PyEnumVariant]) {
    let marker_emitted = variants.iter().any(|v| v.wire_name.is_some());
    if !marker_emitted {
        return;
    }
    let mut reserved: std::collections::HashSet<String> = variants
        .iter()
        .map(|v| v.ident.clone())
        .filter(|n| n != WIRE_VALUES_MARKER)
        .collect();
    reserved.insert(WIRE_VALUES_MARKER.to_string());

    for v in variants.iter_mut() {
        if v.ident == WIRE_VALUES_MARKER {
            let bumped = bump_past_reserved(WIRE_VALUES_MARKER, &mut reserved);
            if v.wire_name.is_none() {
                v.wire_name = Some(WIRE_VALUES_MARKER.to_string());
            }
            v.ident = bumped;
        }
    }
}

/// A generic scope's raw→emitted `TypeVar` name map. `translate_ty`
/// (`Ty::TypeVar`) consults it so a reference resolves to the exact spelling the
/// declaration site allocated — including when a `{None, None_}` twin forces one
/// name past its natural stateless escape. Each scope's map is the restriction
/// of the ONE leaf-global allocation (`leaf::allocate_leaf_type_vars`) to that
/// scope's raw names. Empty for non-generic scopes and until that pass fills it.
pub(crate) type TypeVarMap = std::collections::HashMap<String, String>;

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
    use super::{
        PYTHON_HARD_KEYWORDS, escape_keywords_in_scope, escape_python_keyword,
        is_python_hard_keyword,
    };

    #[test]
    fn python_hard_keyword_set_is_exactly_the_35_cpython_hard_keywords() {
        // MUST equal `keyword.kwlist` on CPython 3.9–3.13. The Python bridge
        // test `test_reserved_keywords.py::test_rust_keyword_list_matches_python_kwlist`
        // anchors the same 35 to `keyword.kwlist` from the Python side, closing
        // the cross-language loop between the Rust and Python keyword lists.
        let mut got: Vec<&str> = PYTHON_HARD_KEYWORDS.to_vec();
        got.sort_unstable();
        let mut want: Vec<&str> = vec![
            "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class",
            "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global",
            "if", "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise",
            "return", "try", "while", "with", "yield",
        ];
        want.sort_unstable();
        assert_eq!(got, want);
        assert_eq!(PYTHON_HARD_KEYWORDS.len(), 35);
        // Soft keywords stay valid identifiers and must NOT be treated as hard.
        for soft in ["match", "case", "type", "_"] {
            assert!(
                !is_python_hard_keyword(soft),
                "soft keyword {soft} misclassified"
            );
        }
    }

    #[test]
    fn escape_keywords_in_scope_resolves_collisions_order_independently() {
        // {pass, pass_} → {pass__, pass_}, regardless of declaration order:
        // the raw `pass_` is reserved in pass 1, forcing `pass` past it.
        let forward = escape_keywords_in_scope(&["pass".into(), "pass_".into()]);
        assert_eq!(forward, vec!["pass__".to_string(), "pass_".to_string()]);
        let reversed = escape_keywords_in_scope(&["pass_".into(), "pass".into()]);
        assert_eq!(reversed, vec!["pass_".to_string(), "pass__".to_string()]);
        // Distinct keywords escape to disjoint prefixes; non-keywords untouched.
        let mixed = escape_keywords_in_scope(&["from".into(), "ok".into(), "class".into()]);
        assert_eq!(
            mixed,
            vec!["from_".to_string(), "ok".to_string(), "class_".to_string()]
        );
    }

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
