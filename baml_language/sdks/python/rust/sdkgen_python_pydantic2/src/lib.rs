//! Python SDK emitter. Produces a structurally correct `baml_sdk/`
//! tree from a `SymbolPool`: one `__init__.py` (and `.pyi` companion)
//! per directory, plus `_inlinedbaml.py`, `_typemap.py`, and the PEP 561
//! `py.typed` marker — all at the SDK root. `_inlinedbaml.py` carries
//! the runtime payload: generated SDKs should pass serialized bytecode so
//! Python import can skip parsing/compilation, while the source-file form is
//! kept for small unit tests and compatibility. Each leaf that routes at
//! least one symbol carries stub Python definitions and an `__all__`
//! trailer.

mod emit;
mod leaf;
mod names;
mod routing;
mod translate_ty;
mod utils;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::Write as _,
    path::PathBuf,
    rc::Rc,
};

use baml_codegen_types::{Name, Symbol, SymbolPool, Ty};
pub use baml_codegen_types::{NamingConvention, OutputType};
pub use names::{IdentifierRename, IdentifierRenameReason};

use crate::{
    emit::{build_emitted, typemap_file::render_typemap_module},
    leaf::{
        LeafBody, group_and_sort_with_names, render_leaf_body, render_leaf_body_pyi,
        render_runtime_reexports_pyi,
    },
    names::{BindingRole, PythonNames},
    routing::LeafPath,
};

fn collect_interface_tys(ty: &Ty, out: &mut BTreeSet<Name>) {
    match ty {
        Ty::Interface(name, generics, associated, _) => {
            out.insert(name.clone());
            for ty in generics.iter().chain(associated.iter().map(|(_, ty)| ty)) {
                collect_interface_tys(ty, out);
            }
        }
        Ty::Class(_, args, _) => {
            for ty in args {
                collect_interface_tys(ty, out);
            }
        }
        Ty::List(inner, _) => collect_interface_tys(inner, out),
        Ty::Map { key, value, .. } => {
            collect_interface_tys(key, out);
            collect_interface_tys(value, out);
        }
        Ty::Union(items, _) => {
            for ty in items {
                collect_interface_tys(ty, out);
            }
        }
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            for param in params {
                collect_interface_tys(&param.ty, out);
            }
            collect_interface_tys(ret, out);
            collect_interface_tys(throws, out);
        }
        Ty::Future(value, error, _) => {
            collect_interface_tys(value, out);
            collect_interface_tys(error, out);
        }
        Ty::Enum(..)
        | Ty::EnumVariant(..)
        | Ty::TypeAlias(..)
        | Ty::Literal(..)
        | Ty::Int { .. }
        | Ty::Bigint { .. }
        | Ty::Float { .. }
        | Ty::String { .. }
        | Ty::Bool { .. }
        | Ty::Null { .. }
        | Ty::Uint8Array { .. }
        | Ty::Media(..)
        | Ty::TypeVar(..)
        | Ty::RustType { .. }
        | Ty::Type { .. }
        | Ty::Resource { .. }
        | Ty::PromptAst { .. }
        | Ty::Void { .. }
        | Ty::Unknown { .. }
        | Ty::Never { .. } => {}
    }
}

fn public_interface_tokens(pool: &SymbolPool) -> BTreeSet<Name> {
    fn function(function: &baml_codegen_types::Function, out: &mut BTreeSet<Name>) {
        for arg in &function.arguments {
            collect_interface_tys(&arg.ty, out);
        }
        collect_interface_tys(&function.return_type, out);
        if let Some(throws) = &function.throws {
            collect_interface_tys(throws, out);
        }
        for (_, watcher) in &function.watchers {
            collect_interface_tys(watcher, out);
        }
    }
    let mut out = BTreeSet::new();
    for symbol in pool.values() {
        match symbol {
            Symbol::Function(value) => function(value, &mut out),
            Symbol::Class(value) => {
                for property in &value.properties {
                    collect_interface_tys(&property.ty, &mut out);
                }
                for method in value.static_methods.iter().chain(&value.instance_methods) {
                    function(method, &mut out);
                }
            }
            Symbol::TypeAlias(value) => collect_interface_tys(&value.resolves_to, &mut out),
            Symbol::Enum(_) => {}
        }
    }
    out
}

fn render_interface_tokens(
    tokens: impl Iterator<Item = Name>,
    stub: bool,
    names: &PythonNames,
) -> String {
    let mut out = String::new();
    let mut public_names = Vec::new();
    for name in tokens {
        let bare = names.symbol(&name);
        let fqn = name.render_dotted(false);
        public_names.push(bare.to_string());
        if stub {
            let _ = writeln!(
                out,
                "\nclass {bare}:\n    __baml_interface_fqn__: str\n\n    def __new__(cls, *args: typing.Any, **kwargs: typing.Any) -> typing.NoReturn: ...\n\n    @classmethod\n    def __class_getitem__(cls, args: typing.Any) -> typing.Any: ...\n"
            );
        } else {
            let _ = writeln!(
                out,
                "\nclass {bare}:\n    \"\"\"Erased runtime token for BAML interface `{fqn}`.\"\"\"\n    __baml_interface_fqn__ = {fqn:?}\n\n    def __new__(cls, *args, **kwargs):\n        raise TypeError(\"BAML interface tokens cannot be instantiated\")\n\n    @classmethod\n    def __class_getitem__(cls, args):\n        import types\n        return types.GenericAlias(cls, args)\n"
            );
        }
    }
    if !public_names.is_empty() {
        let names = public_names
            .iter()
            .map(|name| format!("{name:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(
            out,
            "\ntry:\n    __all__.extend([{names}])\nexcept NameError:\n    __all__ = [{names}]\n"
        );
    }
    out
}

/// Banner prepended to every generated `.py` / `.pyi` file. Mirrors
/// the legacy `engine/generators/languages/python` `CONTENT_PREFIX`,
/// plus a block of file-level lint/format ignore directives matching
/// what the TypeScript emitter already does for its tooling — these
/// must appear at the very top so tools (ruff, flake8, mypy, pyright,
/// pylint, black/ruff-format, isort) see them before any code.
const PYTHON_BANNER: &str = "\
# ruff: noqa
# flake8: noqa
# pylint: skip-file
# mypy: ignore-errors
# pyright: reportPrivateUsage=false, reportUnknownArgumentType=false
# fmt: off
# isort: skip_file

# ----------------------------------------------------------------------------
#
#  Welcome to Baml! To use this generated code, run one of the following:
#
#  $ uv add baml-bridge
#  $ pip install baml-bridge
#  $ conda run python -m pip install baml-bridge
#
# ----------------------------------------------------------------------------

# This file was generated by BAML: please do not edit it. Instead, edit the
# BAML files and re-generate this code using `baml generate`

";

/// Full prefix every generated `.py` / `.pyi` file starts with: the
/// banner followed by `from __future__ import annotations`.
#[cfg(test)]
const HEADER: &str = concat!(
    "\
# ruff: noqa
# flake8: noqa
# pylint: skip-file
# mypy: ignore-errors
# pyright: reportPrivateUsage=false, reportUnknownArgumentType=false
# fmt: off
# isort: skip_file

# ----------------------------------------------------------------------------
#
#  Welcome to Baml! To use this generated code, run one of the following:
#
#  $ uv add baml-bridge
#  $ pip install baml-bridge
#  $ conda run python -m pip install baml-bridge
#
# ----------------------------------------------------------------------------

# This file was generated by BAML: please do not edit it. Instead, edit the
# BAML files and re-generate this code using `baml generate`

",
    "from __future__ import annotations\n"
);

/// A user BAML source file as it should appear in `_inlinedbaml.py`.
/// `rel_path` is relative to the `baml_src/` root (e.g. `"lorem/foo.baml"`).
pub type UserBamlFile = (PathBuf, String);

/// Generated Python file tree plus deterministic public host-name renames.
pub struct GeneratedPythonSdk {
    pub files: HashMap<PathBuf, String>,
    pub renames: Vec<IdentifierRename>,
}

#[derive(Clone, Copy)]
enum RuntimePayload<'a> {
    SourceFiles(&'a [UserBamlFile]),
    Bytecode(&'a [u8], Option<&'a str>, &'a [UserBamlFile]),
}
impl<'a> RuntimePayload<'a> {
    fn is_bytecode(self) -> bool {
        matches!(self, RuntimePayload::Bytecode(_, _, _))
    }

    fn source_files(self) -> &'a [UserBamlFile] {
        match self {
            RuntimePayload::SourceFiles(files) | RuntimePayload::Bytecode(_, _, files) => files,
        }
    }
}

/// Build the Python SDK output tree for `pool`. Returned paths are
/// relative to the `baml_sdk/` output root.
pub fn to_source_code(
    pool: &SymbolPool,
    user_baml_files: &[UserBamlFile],
    naming_convention: NamingConvention,
) -> HashMap<PathBuf, String> {
    to_source_code_internal(
        pool,
        RuntimePayload::SourceFiles(user_baml_files),
        naming_convention,
    )
    .files
}

/// Build the Python SDK output tree using precompiled BAML bytecode as the
/// runtime payload.
pub fn to_source_code_with_bytecode(
    pool: &SymbolPool,
    baml_bytecode: &[u8],
    naming_convention: NamingConvention,
) -> HashMap<PathBuf, String> {
    to_source_code_internal(
        pool,
        RuntimePayload::Bytecode(baml_bytecode, None, &[]),
        naming_convention,
    )
    .files
}

pub fn to_source_code_with_bytecode_and_metadata(
    pool: &SymbolPool,
    baml_bytecode: &[u8],
    embedded_baml_toml: &str,
    naming_convention: NamingConvention,
) -> HashMap<PathBuf, String> {
    to_source_code_with_bytecode_and_metadata_and_source_files(
        pool,
        baml_bytecode,
        embedded_baml_toml,
        &[],
        naming_convention,
    )
}

pub fn to_source_code_with_bytecode_and_metadata_and_source_files(
    pool: &SymbolPool,
    baml_bytecode: &[u8],
    embedded_baml_toml: &str,
    user_baml_files: &[UserBamlFile],
    naming_convention: NamingConvention,
) -> HashMap<PathBuf, String> {
    generate_with_bytecode_and_metadata_and_source_files(
        pool,
        baml_bytecode,
        embedded_baml_toml,
        user_baml_files,
        naming_convention,
    )
    .files
}

/// Report-producing counterpart of
/// [`to_source_code_with_bytecode_and_metadata_and_source_files`].
pub fn generate_with_bytecode_and_metadata_and_source_files(
    pool: &SymbolPool,
    baml_bytecode: &[u8],
    embedded_baml_toml: &str,
    user_baml_files: &[UserBamlFile],
    naming_convention: NamingConvention,
) -> GeneratedPythonSdk {
    to_source_code_internal(
        pool,
        RuntimePayload::Bytecode(baml_bytecode, Some(embedded_baml_toml), user_baml_files),
        naming_convention,
    )
}

fn to_source_code_internal(
    pool: &SymbolPool,
    runtime_payload: RuntimePayload<'_>,
    naming_convention: NamingConvention,
) -> GeneratedPythonSdk {
    // Only `PreserveCase` is wired up so far; `Language`-mode rewriting
    // is the next piece of work and panics loudly until then.
    assert!(
        matches!(naming_convention, NamingConvention::PreserveCase),
        "sdkgen_python_pydantic2 only supports naming_convention = PreserveCase \
         (got {naming_convention})",
    );
    let mut out: HashMap<PathBuf, String> = HashMap::new();
    let names = Rc::new(PythonNames::build(pool));

    // Every symbol in the pool routes to exactly one leaf. Dedup via
    // `BTreeSet` so leaf and directory enumeration below is stable.
    let mut leaves: BTreeSet<LeafPath> = BTreeSet::new();
    for (key, symbol) in pool {
        leaves.insert(names.route(key, symbol));
    }
    let interface_tokens = public_interface_tokens(pool);
    for name in &interface_tokens {
        leaves.insert(names.route_class_ref(name));
    }

    // `baml/` always exists — even if no stdlib symbols route there,
    // leaves that reference `baml.media.*` / `ai.*` need the
    // subpackage to import from. The root leaf itself is always emitted
    // as well. (25b2 Phase 2 relocated `_inlinedbaml.py` to the SDK
    // root; `baml/` is no longer load-bearing for the root init.)
    leaves.insert(LeafPath {
        segments: vec!["baml".to_string()],
    });
    leaves.insert(LeafPath {
        segments: vec!["reflect".to_string()],
    });
    leaves.insert(LeafPath {
        segments: Vec::new(),
    });

    // Walk every leaf's ancestor chain to discover all directories that
    // need an `__init__.py` and the set of immediate subdirectory
    // children for each directory. A single directory may be both a
    // routed leaf AND have subdirectory children (e.g. `stream_types/`
    // when there are no-namespace `root..Foo$stream` symbols alongside
    // namespaced stream symbols). Those cases merge into a single
    // `__init__.py` emission below.
    let mut all_dirs: BTreeSet<Vec<String>> = BTreeSet::new();
    let mut children: BTreeMap<Vec<String>, BTreeSet<String>> = BTreeMap::new();

    children.entry(Vec::new()).or_default();
    all_dirs.insert(Vec::new());

    for leaf in &leaves {
        all_dirs.insert(leaf.segments.clone());
        for i in 0..leaf.segments.len() {
            let prefix: Vec<String> = leaf.segments[..i].to_vec();
            children
                .entry(prefix.clone())
                .or_default()
                .insert(leaf.segments[i].clone());
            all_dirs.insert(prefix);
        }
    }

    // Build the populated-leaf bodies. Every directory that gets
    // at least one routed symbol ends up with a `LeafBody` here; all
    // others render with G1-identical content.
    let triples = build_emitted(pool, &names);
    let bodies: BTreeMap<LeafPath, LeafBody> = group_and_sort_with_names(triples, &names);

    // Emit every directory's `__init__.py` and a sibling `__init__.pyi`.
    for dir in &all_dirs {
        let kids = children.get(dir).cloned().unwrap_or_default();
        let leaf_path = LeafPath {
            segments: dir.clone(),
        };
        let empty_body = LeafBody {
            leaf: leaf_path.clone(),
            symbols: Vec::new(),
            names: Some(names.clone()),
        };
        let body = bodies.get(&leaf_path).unwrap_or(&empty_body);
        let callable_child_names = body.callable_child_names(&kids);

        let mut content = if dir.is_empty() {
            render_root_init(&kids, runtime_payload.is_bytecode())
        } else {
            render_package_init(&kids)
        };
        content.push_str(&render_leaf_body(body, &callable_child_names));
        content.push_str(&render_interface_tokens(
            interface_tokens
                .iter()
                .filter(|name| names.route_class_ref(name) == leaf_path)
                .cloned(),
            false,
            &names,
        ));
        if dir.is_empty() {
            content.push_str(
                "\n# BEP-066 host reflection surface.\nfrom . import reflect as reflect\n",
            );
        }
        out.insert(init_py_path(dir), content);

        // `.pyi` sibling — pyright wants the classical
        // `from . import <child>` cascade for dotted access, so we
        // emit re-exports here instead of the runtime PEP 562
        // `__getattr__` hook. The root `.pyi` drops all the
        // `BamlRuntime`/`set_type_map` runtime wiring too.
        let runtime_reexports_pyi = render_runtime_reexports_pyi(body);
        let mut pyi_content = if dir.is_empty() {
            render_root_init_pyi(&kids, &callable_child_names, &runtime_reexports_pyi)
        } else {
            render_package_init_pyi(&kids, &callable_child_names, &runtime_reexports_pyi)
        };
        let callable_child_bodies = callable_child_bodies(dir, &callable_child_names, &bodies);
        pyi_content.push_str(&render_leaf_body_pyi(body, &callable_child_bodies));
        pyi_content.push_str(&render_interface_tokens(
            interface_tokens
                .iter()
                .filter(|name| names.route_class_ref(name) == leaf_path)
                .cloned(),
            true,
            &names,
        ));
        if dir.is_empty() {
            pyi_content.push_str("\nfrom . import reflect as reflect\n");
        }
        out.insert(init_pyi_path(dir), pyi_content);
    }

    // `_inlinedbaml.py` lives at the SDK root so the root init can
    // `from . import _inlinedbaml` without loading the `baml/` subpackage
    // (25b2 Phase 2). After Phase 4 drops the eager leaf cascade, the
    // root init's only relative imports are `_inlinedbaml` and `_typemap`
    // — both root-level data modules.
    out.insert(
        PathBuf::from("_inlinedbaml.py"),
        render_inlinedbaml(runtime_payload),
    );
    out.insert(
        PathBuf::from("_baml_sources.py"),
        render_inlinedbaml_source(runtime_payload.source_files()),
    );

    // Codegen-emitted typemap (25b2 Phase 2 / 25a2 §4.1): three literal
    // dicts of `FQN → (module_path, attr_name)` lazy entries plus the
    // `BamlTypeMap.from_lazy_entries(...)` call. The root init imports
    // this module and calls `set_type_map(_TYPE_MAP)` before any leaf
    // import so the per-leaf `_register_class(...)` trailers (still
    // emitted in Phase 2) mutate the same typemap the lazy entries
    // pre-populated.
    out.insert(
        PathBuf::from("_typemap.py"),
        render_typemap_module(&bodies, "baml_sdk"),
    );
    out.insert(
        PathBuf::from("_function_registry.py"),
        render_function_registry(pool, &names),
    );

    // Emit PEP 561 marker. Stays empty — type checkers only check for
    // the file's existence, and the banner would defeat that contract.
    out.insert(PathBuf::from("py.typed"), String::new());

    // Prepend the do-not-edit banner to every `.py` / `.pyi` file.
    for (path, content) in &mut out {
        let is_python = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e == "py" || e == "pyi");
        if is_python {
            content.insert_str(0, PYTHON_BANNER);
        }
    }

    GeneratedPythonSdk {
        files: out,
        renames: names.renames().to_vec(),
    }
}

/// Data-only discovery table for authored LLM functions and their public
/// spec/stream bindings.
///
/// Consumers such as request snapshotters must not infer capabilities by
/// scanning generated suffixes: authored names can collide with projected
/// bindings, and target allocation may append underscores. The registry keeps
/// raw BAML identity separate from the resolved Python module/attribute names.
fn render_function_registry(pool: &SymbolPool, names: &PythonNames) -> String {
    let mut functions: Vec<_> = pool
        .iter()
        .filter_map(|(name, symbol)| match symbol {
            Symbol::Function(function)
                if !name.name().as_str().contains('@')
                    && pool.contains_key(&Name::new(
                        name.package().clone(),
                        name.namespace().clone(),
                        baml_base::Name::new(format!("{}@spec", name.name())),
                    )) =>
            {
                Some((name, symbol, function))
            }
            _ => None,
        })
        .collect();
    functions.sort_by_key(|(name, _, _)| *name);

    let mut out = String::from(
        "from __future__ import annotations\n\nFUNCTIONS: dict[str, dict[str, object]] = {\n",
    );
    for (name, symbol, function) in functions {
        let fqn = name.to_string();
        let leaf = names.route(name, symbol);
        let module = if leaf.segments.is_empty() {
            "baml_sdk".to_string()
        } else {
            format!("baml_sdk.{}", leaf.segments.join("."))
        };
        let _ = writeln!(out, "    {}: {{", py_string(&fqn));
        let _ = writeln!(out, "        \"module\": {},", py_string(&module));
        out.push_str("        \"type_params\": (");
        for param in &function.generic_params {
            let _ = write!(out, "{}, ", py_string(param.as_str()));
        }
        out.push_str("),\n");
        for role in [BindingRole::DirectSync, BindingRole::DirectAsync] {
            let binding = names.callable(&fqn, role);
            let _ = writeln!(
                out,
                "        {}: {},",
                py_string(role.registry_key()),
                py_string(&binding),
            );
        }
        for suffix in ["spec", "stream"] {
            let companion = Name::new(
                name.package().clone(),
                name.namespace().clone(),
                baml_base::Name::new(format!("{}@{suffix}", name.name())),
            );
            let Some(Symbol::Function(_)) = pool.get(&companion) else {
                continue;
            };
            let companion_fqn = companion.to_string();
            for role in [BindingRole::DirectSync, BindingRole::DirectAsync] {
                let key = if role.is_async() {
                    format!("{suffix}_async")
                } else {
                    suffix.to_string()
                };
                let binding = names.callable(&companion_fqn, role);
                let _ = writeln!(out, "        {}: {},", py_string(&key), py_string(&binding),);
            }
        }
        out.push_str("    },\n");
    }
    out.push_str("}\n");
    out
}

fn init_py_path(dir: &[String]) -> PathBuf {
    let mut path = PathBuf::new();
    for seg in dir {
        path.push(seg);
    }
    path.push("__init__.py");
    path
}

fn init_pyi_path(dir: &[String]) -> PathBuf {
    let mut path = PathBuf::new();
    for seg in dir {
        path.push(seg);
    }
    path.push("__init__.pyi");
    path
}

/// Render a non-root package `__init__.py`.
///
/// Submodule re-exports are lazy via PEP 562: a `__getattr__` hook
/// resolves any known child name through `importlib.import_module` on
/// first access, and Python caches the result as a normal attribute
/// for subsequent lookups. Two properties matter:
///
/// 1. **No eager descent.** `import baml_sdk.baml` no longer triggers
///    every `baml/<child>` to load — only the children the consumer
///    actually touches.
/// 2. **Partial-init resilience.** Python only binds a submodule as
///    an attribute of its parent AFTER the submodule's `__init__.py`
///    completes; that's why the eager-cascade design needed a manual
///    self-pin. With PEP 562, the lookup `parent.child` from inside a
///    grand-child triggers `parent.__getattr__('child')`, which calls
///    `importlib.import_module` — and `importlib` returns the (possibly
///    partial) `sys.modules` entry synchronously. The dotted walk
///    proceeds through each `__getattr__` until it reaches a fully
///    loaded leaf, so pydantic's eager eval of private-attribute
///    annotations like `_sse: stream_types.baml.http.SseStream`
///    resolves without manual setattr boilerplate.
///
/// The `.pyi` counterpart (`render_package_init_pyi`) emits the
/// classical `from . import <child>` cascade — pyright doesn't execute
/// `__getattr__`, and the explicit re-export is what lets it accept
/// dotted submodule access.
fn render_package_init(children: &BTreeSet<String>) -> String {
    let mut out = String::from("from __future__ import annotations\n");
    if !children.is_empty() {
        append_lazy_children_block(&mut out, children);
    }
    out
}

/// `.pyi` counterpart of `render_package_init`. Stubs aren't executed,
/// so the PEP 562 `__getattr__` hook serves no purpose; pyright wants
/// the explicit `from . import <child>` cascade instead.
fn render_package_init_pyi(
    children: &BTreeSet<String>,
    hidden_children: &BTreeSet<String>,
    runtime_reexports: &str,
) -> String {
    let mut out = String::from("from __future__ import annotations\n");
    if !runtime_reexports.is_empty() {
        out.push('\n');
        out.push_str(runtime_reexports);
    }
    if !children.is_empty() {
        out.push('\n');
        for child in children {
            if hidden_children.contains(child) {
                continue;
            }
            let _ = writeln!(out, "from . import {child}");
        }
    }
    out
}

/// Emit a `_LAZY_CHILDREN` set and a PEP 562 `__getattr__` that
/// resolves any name in the set via `importlib.import_module`. Python
/// caches the loaded module as a real attribute after the first hit,
/// so the cost is one importlib call per child per process. Shared by
/// both root and non-root package init renderers.
fn append_lazy_children_block(out: &mut String, children: &BTreeSet<String>) {
    out.push('\n');
    out.push_str("_LAZY_CHILDREN = frozenset({\n");
    for child in children {
        let _ = writeln!(out, "    \"{child}\",");
    }
    out.push_str("})\n\n");
    out.push_str("def __getattr__(name):\n");
    out.push_str("    if name in _LAZY_CHILDREN:\n");
    out.push_str("        import importlib\n");
    out.push_str("        return importlib.import_module(f\".{name}\", __name__)\n");
    out.push_str("    raise AttributeError(f\"module {__name__!r} has no attribute {name!r}\")\n");
}

/// Render the SDK root `__init__.py`. Eagerly imports the two
/// data-only modules (`_inlinedbaml`, `_typemap`) and wires up the
/// runtime + typemap. Top-level child packages (`baml`, `lorem`,
/// `vendor`, `stream_types`, …) are exposed lazily through a PEP 562
/// `__getattr__` — `import baml_sdk` no longer transitively loads any
/// leaf, restoring the 25b2 lazy-import goal. The chain of attribute
/// accesses in `<top>.<intermediate>.<bare>` annotations still works
/// because every intermediate package has its own `__getattr__` that
/// resolves children via `importlib`.
fn render_root_init(top_children: &BTreeSet<String>, use_bytecode: bool) -> String {
    let mut out = String::new();
    out.push_str("from __future__ import annotations\n\n");
    out.push_str("from baml_bridge import BamlRuntime, set_type_map\n");
    out.push_str("from . import _inlinedbaml\n");
    out.push_str("from ._typemap import _TYPE_MAP\n\n");
    if use_bytecode {
        out.push_str("BamlRuntime.initialize_runtime_from_bytecode(_inlinedbaml.BYTECODE, _inlinedbaml.EMBEDDED_BAML_TOML)\n\n");
    } else {
        out.push_str("BamlRuntime.initialize_runtime(\n");
        out.push_str("    \"baml_src\", _inlinedbaml.FILES\n");
        out.push_str(")\n\n");
    }
    out.push_str("def get_baml_source_files() -> dict[str, str]:\n");
    out.push_str("    from ._baml_sources import FILES\n");
    out.push_str("    return FILES\n\n");
    out.push_str("set_type_map(_TYPE_MAP)\n");
    if !top_children.is_empty() {
        append_lazy_children_block(&mut out, top_children);
    }
    out
}

/// `.pyi` counterpart of `render_root_init`. Stubs aren't executed —
/// pyright needs `from . import <child>` re-exports for dotted access
/// to type-check, and there's no runtime init machinery (no
/// `BamlRuntime`, no `set_type_map`).
fn render_root_init_pyi(
    top_children: &BTreeSet<String>,
    hidden_children: &BTreeSet<String>,
    runtime_reexports: &str,
) -> String {
    let mut out = String::from("from __future__ import annotations\n");
    if !runtime_reexports.is_empty() {
        out.push('\n');
        out.push_str(runtime_reexports);
    }
    out.push_str("\ndef get_baml_source_files() -> dict[str, str]: ...\n");
    if !top_children.is_empty() {
        out.push('\n');
        for child in top_children {
            if hidden_children.contains(child) {
                continue;
            }
            let _ = writeln!(out, "from . import {child}");
        }
    }
    out
}

fn callable_child_bodies<'a>(
    dir: &[String],
    callable_child_names: &BTreeSet<String>,
    bodies: &'a BTreeMap<LeafPath, LeafBody>,
) -> BTreeMap<String, &'a LeafBody> {
    let mut out = BTreeMap::new();
    for child in callable_child_names {
        let mut segments = dir.to_vec();
        segments.push(child.clone());
        let child_leaf = LeafPath { segments };
        if let Some(body) = bodies.get(&child_leaf) {
            out.insert(child.clone(), body);
        }
    }
    out
}

#[derive(askama::Template)]
#[template(
    source = r#"from __future__ import annotations

FILES: dict[str, str] = {
{% for entry in entries -%}
    {{ entry.key }}: {{ entry.contents }},
{% endfor -%}
}"#,
    ext = "py.j2",
    escape = "none"
)]
struct InlinedBaml {
    entries: Vec<InlinedEntry>,
}

struct InlinedEntry {
    key: String,
    contents: String,
}

fn render_inlinedbaml(payload: RuntimePayload<'_>) -> String {
    match payload {
        RuntimePayload::SourceFiles(_) => {
            "from __future__ import annotations\n\nfrom ._baml_sources import FILES\n".to_string()
        }
        RuntimePayload::Bytecode(bytecode, embedded_baml_toml, _) => {
            render_inlinedbaml_bytecode(bytecode, embedded_baml_toml)
        }
    }
}

fn render_inlinedbaml_source(files: &[UserBamlFile]) -> String {
    let mut out = String::from("from __future__ import annotations\n\n");
    out.push_str(&render_baml_source_files(files));
    out
}

fn render_baml_source_files(files: &[UserBamlFile]) -> String {
    use askama::Template;
    let mut entries: Vec<(&PathBuf, &String)> = files.iter().map(|(p, c)| (p, c)).collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));

    let entries: Vec<InlinedEntry> = entries
        .into_iter()
        .map(|(rel, contents)| InlinedEntry {
            key: py_string(&rel.to_string_lossy()),
            contents: py_string(contents),
        })
        .collect();

    let rendered = InlinedBaml { entries }
        .render()
        .expect("inlinedbaml template should always render");
    let mut out = rendered
        .strip_prefix("from __future__ import annotations\n\n")
        .expect("inlinedbaml template must start with its future import")
        .to_string();
    out.push('\n');
    out
}

fn render_inlinedbaml_bytecode(bytecode: &[u8], embedded_baml_toml: Option<&str>) -> String {
    let mut out = String::from("from __future__ import annotations\n\nBYTECODE: bytes = ");
    out.push_str(&py_bytes(bytecode));
    out.push_str("\nEMBEDDED_BAML_TOML: str | None = ");
    out.push_str(
        &embedded_baml_toml
            .map(py_string)
            .unwrap_or_else(|| "None".to_string()),
    );
    out.push('\n');
    out
}

/// Render `s` as a Python string literal. Uses a regular double-quoted
/// form with the usual `\\`, `\"`, `\n`, `\r`, `\t` escapes so the result
/// round-trips through `ast.literal_eval` and is byte-identical.
pub(crate) fn py_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                write!(out, "\\x{:02x}", c as u32).unwrap();
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Render bytes as adjacent Python bytes literals. Chunking keeps generated
/// lines manageable without adding any runtime decode step.
pub(crate) fn py_bytes(bytes: &[u8]) -> String {
    const CHUNK_SIZE: usize = 80;

    if bytes.is_empty() {
        return "b\"\"".to_string();
    }

    let mut out = String::from("(\n");
    for chunk in bytes.chunks(CHUNK_SIZE) {
        out.push_str("    b\"");
        for &byte in chunk {
            match byte {
                b'\\' => out.push_str("\\\\"),
                b'"' => out.push_str("\\\""),
                0x20..=0x7e => out.push(byte as char),
                _ => write!(out, "\\x{byte:02x}").unwrap(),
            }
        }
        out.push_str("\"\n");
    }
    out.push(')');
    out
}

/// Return the `cg::Name` that keys a given symbol in the pool. Not
/// load-bearing today (we iterate `pool.keys()` directly), but kept
/// near the emitter for symmetry with `SymbolPool`.
#[allow(dead_code)]
fn symbol_name(sym: &Symbol) -> Option<&Name> {
    match sym {
        Symbol::Class(c) => Some(&c.name),
        Symbol::Enum(e) => Some(&e.name),
        Symbol::TypeAlias(t) => Some(&t.name),
        // `Function.name` is a bare `baml_base::Name`; the pool key is
        // authoritative. Return None so callers keep using keys.
        Symbol::Function(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use baml_base::Name as BaseName;
    use baml_codegen_types::{
        Class, ClassProperty, DefaultLiteral, Enum, EnumVariant, Function, FunctionArgument,
        FunctionArgumentDefault, Origin, Ty, TypeAlias,
    };
    use pretty_assertions::{assert_eq, assert_ne};

    use super::*;

    fn cg_name(pkg: &str, ns: &[&str], n: &str) -> Name {
        Name::new(
            BaseName::new(pkg),
            ns.iter().map(|s| BaseName::new(*s)).collect(),
            BaseName::new(n),
        )
    }

    fn class_ty(name: Name, args: Vec<Ty>) -> Ty {
        Ty::Class(name, args.into(), baml_base::TyAttr::EMPTY)
    }

    fn enum_ty(name: Name) -> Ty {
        Ty::Enum(name, baml_base::TyAttr::EMPTY)
    }

    fn alias_ty(name: Name) -> Ty {
        Ty::TypeAlias(name, baml_base::TyAttr::EMPTY)
    }

    fn type_var(name: BaseName) -> Ty {
        Ty::TypeVar(
            baml_codegen_types::ParamTy::new(0, name),
            baml_base::TyAttr::EMPTY,
        )
    }

    fn list(inner: Box<Ty>) -> Ty {
        Ty::List(inner, baml_base::TyAttr::EMPTY)
    }

    fn union(members: Vec<Ty>) -> Ty {
        Ty::Union(members.into(), baml_base::TyAttr::EMPTY)
    }

    fn origin(file: &str, span: u32) -> Origin {
        Origin {
            source_file_path: file.to_string(),
            span_start: span,
        }
    }

    fn class(name: Name) -> Symbol {
        class_at(name, "x.baml", 0)
    }

    fn class_at(name: Name, file: &str, span: u32) -> Symbol {
        Symbol::Class(Class {
            generic_params: Vec::new(),
            name,
            docstring: None,
            properties: vec![ClassProperty {
                name: BaseName::new("a"),
                docstring: None,
                ty: Ty::Int {
                    attr: baml_base::TyAttr::EMPTY,
                },
            }],
            static_methods: vec![],
            instance_methods: vec![],
            origin: origin(file, span),
        })
    }

    fn enum_(name: Name, file: &str, span: u32) -> Symbol {
        Symbol::Enum(Enum {
            name,
            docstring: None,
            variants: vec![EnumVariant {
                name: BaseName::new("A"),
                docstring: None,
                value: "A".to_string(),
            }],
            origin: origin(file, span),
        })
    }

    fn alias(name: Name, file: &str, span: u32) -> Symbol {
        Symbol::TypeAlias(TypeAlias {
            name,
            resolves_to: Ty::Int {
                attr: baml_base::TyAttr::EMPTY,
            },
            recursive: false,
            origin: origin(file, span),
        })
    }

    fn bare_func(bare: &str, file: &str, span: u32) -> Function {
        Function {
            generic_params: Vec::new(),
            name: BaseName::new(bare),
            docstring: None,
            arguments: vec![FunctionArgument {
                injected: false,
                name: BaseName::new("x"),
                docstring: None,
                ty: Ty::Int {
                    attr: baml_base::TyAttr::EMPTY,
                },
                default: None,
            }],
            return_type: Ty::Int {
                attr: baml_base::TyAttr::EMPTY,
            },
            throws: None,
            watchers: vec![],
            origin: origin(file, span),
        }
    }

    fn func_sym(bare: &str, file: &str, span: u32) -> Symbol {
        Symbol::Function(bare_func(bare, file, span))
    }

    fn zero_arg_func(bare: &str, return_type: Ty, file: &str, span: u32) -> Symbol {
        let mut f = bare_func(bare, file, span);
        f.arguments.clear();
        f.return_type = return_type;
        Symbol::Function(f)
    }

    #[test]
    fn empty_pool_emits_structural_files() {
        let pool: SymbolPool = HashMap::new();
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);

        assert!(out.contains_key(&PathBuf::from("__init__.py")));
        assert!(out.contains_key(&PathBuf::from("baml/__init__.py")));
        // 25b2 Phase 2: `_inlinedbaml.py` and `_typemap.py` live at the
        // SDK root so the root init doesn't load the `baml/` subpackage.
        assert!(out.contains_key(&PathBuf::from("_inlinedbaml.py")));
        assert!(out.contains_key(&PathBuf::from("_typemap.py")));
        assert!(!out.contains_key(&PathBuf::from("baml/_inlinedbaml.py")));
        assert!(out.contains_key(&PathBuf::from("py.typed")));

        let root = &out[&PathBuf::from("__init__.py")];
        assert!(root.contains("from baml_bridge import BamlRuntime, set_type_map"));
        assert!(root.contains("from . import _inlinedbaml"));
        assert!(root.contains("from ._typemap import _TYPE_MAP"));
        assert!(root.contains("set_type_map(_TYPE_MAP)"));
        assert!(root.contains("from . import reflect as reflect"));
        // PEP 562 lazy re-export: root lists `baml` in `_LAZY_CHILDREN`
        // and exposes it through `__getattr__`. `to_source_code` always
        // synthesizes the `baml` leaf even when no stdlib symbols route
        // there, so the lazy children set always lists it.
        assert!(root.contains("_LAZY_CHILDREN = frozenset({\n"));
        assert!(root.contains("    \"baml\",\n"));
        assert!(root.contains("def __getattr__(name):\n"));
        assert!(!root.contains("from . import baml\n"));
        // No symbols → no __all__ emitted (preserves G1 byte shape).
        assert!(!root.contains("__all__"));

        // The `.pyi` sibling drops the runtime wiring but keeps the
        // explicit `from . import <child>` cascade so pyright can
        // resolve dotted submodule access structurally.
        let root_pyi = &out[&PathBuf::from("__init__.pyi")];
        assert!(root_pyi.contains("from . import baml\n"));
        assert!(root_pyi.contains("from . import reflect as reflect"));
        assert!(!root_pyi.contains("__getattr__"));
        assert!(!root_pyi.contains("BamlRuntime"));

        // `baml/__init__.py` has no children in the empty-pool fixture,
        // so neither the lazy `__getattr__` block nor any cascade lines
        // appear — just the bare `from __future__` header.
        let baml_init = &out[&PathBuf::from("baml/__init__.py")];
        assert!(!baml_init.contains("_inlinedbaml"));
        assert!(!baml_init.contains("__getattr__"));
        assert_eq!(baml_init, HEADER);

        assert_eq!(out[&PathBuf::from("py.typed")], "");
    }

    #[test]
    fn generated_sdk_lists_python_runtime_install_commands_in_preferred_order() {
        let out = to_source_code(&SymbolPool::new(), &[], NamingConvention::PreserveCase);
        let root = &out[&PathBuf::from("__init__.py")];
        let baml_init = &out[&PathBuf::from("baml/__init__.py")];
        let expected = "#  $ uv add baml-bridge\n\
                        #  $ pip install baml-bridge\n\
                        #  $ conda run python -m pip install baml-bridge";

        for generated_init in [root, baml_init] {
            assert!(generated_init.contains(expected));
            assert!(generated_init.contains(
                "#  Welcome to Baml! To use this generated code, run one of the following:"
            ));
            assert!(
                generated_init
                    .contains("# BAML files and re-generate this code using `baml generate`")
            );
            assert!(!generated_init.contains("install baml\n"));
            assert!(!generated_init.contains("baml package"));
            assert!(!generated_init.contains("baml-cli generate"));
        }
    }

    #[test]
    fn class_body_renders() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Resume");
        pool.insert(n.clone(), class(n));

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);

        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        assert!(leaf.starts_with(HEADER));
        assert!(leaf.contains("import typing\n"));
        assert!(leaf.contains("import pydantic\n"));
        assert!(leaf.contains("class Resume(pydantic.BaseModel):\n"));
        assert!(leaf.contains(
            "    model_config = pydantic.ConfigDict(\n        arbitrary_types_allowed=True,\n        extra=\"ignore\",\n        populate_by_name=True,\n    )\n    \
             a: int\n"
        ));
        assert!(leaf.contains("__all__ = [\n    \"Resume\",\n]\n"));
        assert!(!leaf.contains("import enum"));
    }

    #[test]
    fn reflect_kind_namespaces_are_routed_legally_across_the_generated_surface() {
        let mut pool: SymbolPool = HashMap::new();
        let kind_namespaces = [
            ("class", "class_"),
            ("enum", "enum"),
            ("interface", "interface"),
            ("function", "function"),
        ];

        for (source, _) in kind_namespaces {
            let type_name = cg_name("reflect", &[source], "Type");
            pool.insert(type_name.clone(), class(type_name));
        }

        let consumer_name = cg_name("user", &["consumer"], "KindViews");
        pool.insert(
            consumer_name.clone(),
            Symbol::Class(Class {
                generic_params: Vec::new(),
                name: consumer_name,
                docstring: None,
                properties: kind_namespaces
                    .iter()
                    .map(|(source, _)| ClassProperty {
                        name: BaseName::new(format!("{source}_type")),
                        docstring: None,
                        ty: class_ty(cg_name("reflect", &[source], "Type"), vec![]),
                    })
                    .collect(),
                static_methods: vec![],
                instance_methods: vec![],
                origin: origin("x.baml", 0),
            }),
        );

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let reflect_pyi = &out[&PathBuf::from("reflect/__init__.pyi")];
        let consumer_py = &out[&PathBuf::from("consumer/__init__.py")];
        let consumer_pyi = &out[&PathBuf::from("consumer/__init__.pyi")];
        let typemap = &out[&PathBuf::from("_typemap.py")];

        for (source, routed) in kind_namespaces {
            assert!(out.contains_key(&PathBuf::from(format!("reflect/{routed}/__init__.py"))));
            if source != routed {
                assert!(!out.contains_key(&PathBuf::from(format!("reflect/{source}/__init__.py"))));
            }
            assert!(reflect_pyi.contains(&format!("from . import {routed}\n")));

            let reference = format!("reflect.{routed}.Type");
            assert!(consumer_py.contains(&reference));
            assert!(consumer_pyi.contains(&reference));
            assert!(typemap.contains(&format!(
                "\"reflect.{source}.Type\": (\"baml_sdk.reflect.{routed}\", \"Type\")"
            )));
        }

        assert!(!consumer_py.contains("reflect.class.Type"));
        assert!(!consumer_pyi.contains("reflect.class.Type"));
    }

    #[test]
    fn enum_body_renders() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Sentiment");
        pool.insert(n.clone(), enum_(n, "x.baml", 0));

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        assert!(leaf.contains("import enum\n"));
        assert!(leaf.contains("class Sentiment(str, enum.Enum):\n    A = \"A\"\n"));
    }

    #[test]
    fn type_alias_body_renders() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Foo");
        pool.insert(n.clone(), alias(n, "x.baml", 0));

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        assert!(leaf.contains("import typing\n"));
        assert!(leaf.contains("Foo: typing.TypeAlias = int\n"));
    }

    #[test]
    fn callable_child_collision_uses_function_namespace_surface() {
        let mut pool: SymbolPool = HashMap::new();
        let stream_name = cg_name("ai", &["stream"], "Stream");
        let done_name = cg_name("ai", &["stream"], "Done");
        let partial_name = cg_name("boundary", &["id"], "Partial");
        let final_name = cg_name("boundary", &["id"], "Final");
        pool.insert(stream_name.clone(), class(stream_name.clone()));
        pool.insert(done_name.clone(), class(done_name));
        pool.insert(partial_name.clone(), class(partial_name.clone()));
        pool.insert(final_name.clone(), class(final_name.clone()));
        pool.insert(
            cg_name("boundary", &[], "id"),
            zero_arg_func(
                "id",
                Ty::String {
                    attr: baml_base::TyAttr::EMPTY,
                },
                "core.baml",
                0,
            ),
        );
        pool.insert(
            cg_name("boundary", &["id"], "current"),
            zero_arg_func(
                "current",
                class_ty(
                    stream_name,
                    vec![class_ty(partial_name, vec![]), class_ty(final_name, vec![])],
                ),
                "id.baml",
                0,
            ),
        );

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("vendor/boundary/__init__.py")];
        assert!(leaf.contains("import importlib\n"));
        assert!(leaf.contains("_id_namespace = importlib.import_module(\".id\", __name__)"));
        assert!(leaf.contains(
            "    setattr(id, _baml_child_name, getattr(_id_namespace, _baml_child_name))"
        ));

        let pyi = &out[&PathBuf::from("vendor/boundary/__init__.pyi")];
        assert!(!pyi.contains("from . import id\n"));
        assert!(pyi.contains("class _BamlCallableNamespace_id(typing.Protocol):\n"));
        assert!(pyi.contains("    def __call__(self) -> str: ...\n"));
        assert!(pyi.contains(
            "from ...ai.stream import Done as _BamlStreamDone\nfrom baml_bridge import BamlStream as _BamlStream\n"
        ));
        assert!(pyi.contains(
            "    def current(self) -> _BamlStream[typing.Union[vendor.boundary.id.Partial, _BamlStreamDone], vendor.boundary.id.Partial, vendor.boundary.id.Final]: ...\n"
        ));
        assert!(pyi.contains("    from ... import vendor\n"));
        assert!(pyi.contains("\nid: _BamlCallableNamespace_id\n"));
        assert!(!pyi.contains("def id() -> str: ..."));
    }

    // ── /// docstring emission ──────────────────────────────────────────────

    fn class_with_docs(
        name: Name,
        docstring: Option<&str>,
        properties: Vec<ClassProperty>,
    ) -> Symbol {
        Symbol::Class(Class {
            generic_params: Vec::new(),
            name,
            docstring: docstring.map(String::from),
            properties,
            static_methods: vec![],
            instance_methods: vec![],
            origin: origin("x.baml", 0),
        })
    }

    #[test]
    fn class_summary_only_emits_single_line_docstring() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Resume");
        pool.insert(
            n.clone(),
            class_with_docs(
                n,
                Some("Job applicant resume."),
                vec![ClassProperty {
                    name: BaseName::new("a"),
                    docstring: None,
                    ty: Ty::Int {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                }],
            ),
        );

        let leaf = &to_source_code(&pool, &[], NamingConvention::PreserveCase)
            [&PathBuf::from("lorem/__init__.py")];
        assert!(
            leaf.contains(
                "class Resume(pydantic.BaseModel):\n    \"\"\"Job applicant resume.\"\"\"\n"
            ),
            "got:\n{leaf}"
        );
    }

    #[test]
    fn field_doc_folds_into_attributes_section() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Resume");
        pool.insert(
            n.clone(),
            class_with_docs(
                n,
                None,
                vec![ClassProperty {
                    name: BaseName::new("a"),
                    docstring: Some("Identifier".to_string()),
                    ty: Ty::Int {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                }],
            ),
        );

        let leaf = &to_source_code(&pool, &[], NamingConvention::PreserveCase)
            [&PathBuf::from("lorem/__init__.py")];
        // Field /// goes into the class body docstring as an
        // Attributes: section — there is no inline `# ` comment.
        assert!(
            leaf.contains(
                "class Resume(pydantic.BaseModel):\n    \"\"\"\n    Attributes:\n        a: Identifier\n    \"\"\"\n"
            ),
            "got:\n{leaf}"
        );
        assert!(
            !leaf.contains("# Identifier"),
            "field /// must not emit inline `# …` comments; got:\n{leaf}"
        );
    }

    #[test]
    fn class_summary_plus_field_docs_block_form() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Doc");
        pool.insert(
            n.clone(),
            class_with_docs(
                n,
                Some("A document with a title and an optional body."),
                vec![
                    ClassProperty {
                        name: BaseName::new("title"),
                        docstring: Some("Title shown in lists.".to_string()),
                        ty: Ty::String {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                    },
                    ClassProperty {
                        name: BaseName::new("body"),
                        docstring: Some("Free-form body text.".to_string()),
                        ty: union(vec![
                            Ty::String {
                                attr: baml_base::TyAttr::EMPTY,
                            },
                            Ty::Null {
                                attr: baml_base::TyAttr::EMPTY,
                            },
                        ]),
                    },
                ],
            ),
        );

        let leaf = &to_source_code(&pool, &[], NamingConvention::PreserveCase)
            [&PathBuf::from("lorem/__init__.py")];
        assert!(
            leaf.contains(
                "class Doc(pydantic.BaseModel):\n    \"\"\"\n    A document with a title and an optional body.\n\n    Attributes:\n        title: Title shown in lists.\n        body: Free-form body text.\n    \"\"\"\n"
            ),
            "got:\n{leaf}"
        );
    }

    #[test]
    fn class_multiline_summary_uses_block_form() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Resume");
        pool.insert(
            n.clone(),
            class_with_docs(
                n,
                Some("first line\nsecond line"),
                vec![ClassProperty {
                    name: BaseName::new("a"),
                    docstring: None,
                    ty: Ty::Int {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                }],
            ),
        );

        let leaf = &to_source_code(&pool, &[], NamingConvention::PreserveCase)
            [&PathBuf::from("lorem/__init__.py")];
        assert!(
            leaf.contains("    \"\"\"\n    first line\n    second line\n    \"\"\""),
            "got:\n{leaf}"
        );
    }

    #[test]
    fn enum_doc_folds_variants_into_members_section() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Sentiment");
        pool.insert(
            n.clone(),
            Symbol::Enum(Enum {
                name: n,
                docstring: Some("Sentiment scale".to_string()),
                variants: vec![
                    EnumVariant {
                        name: BaseName::new("HAPPY"),
                        docstring: Some("Smiling".to_string()),
                        value: "HAPPY".to_string(),
                    },
                    EnumVariant {
                        name: BaseName::new("SAD"),
                        docstring: None,
                        value: "SAD".to_string(),
                    },
                ],
                origin: origin("x.baml", 0),
            }),
        );

        let leaf = &to_source_code(&pool, &[], NamingConvention::PreserveCase)
            [&PathBuf::from("lorem/__init__.py")];
        // Once at least one variant carries a `///`, the Members: block
        // appears and lists *every* variant — undocumented ones render
        // as bare names. No inline `"""…"""` attribute docstrings
        // remain.
        assert!(
            leaf.contains(
                "class Sentiment(str, enum.Enum):\n    \"\"\"\n    Sentiment scale\n\n    Members:\n        HAPPY: Smiling\n        SAD\n    \"\"\"\n    HAPPY = \"HAPPY\"\n    SAD = \"SAD\"\n"
            ),
            "got:\n{leaf}"
        );
    }

    /// Enum has a summary but no variant carries a `///` — the
    /// Members: section is suppressed entirely.
    #[test]
    fn enum_summary_only_skips_members_section() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Sentiment");
        pool.insert(
            n.clone(),
            Symbol::Enum(Enum {
                name: n,
                docstring: Some("Sentiment scale".to_string()),
                variants: vec![
                    EnumVariant {
                        name: BaseName::new("HAPPY"),
                        docstring: None,
                        value: "HAPPY".to_string(),
                    },
                    EnumVariant {
                        name: BaseName::new("SAD"),
                        docstring: None,
                        value: "SAD".to_string(),
                    },
                ],
                origin: origin("x.baml", 0),
            }),
        );

        let leaf = &to_source_code(&pool, &[], NamingConvention::PreserveCase)
            [&PathBuf::from("lorem/__init__.py")];
        assert!(
            leaf.contains(
                "class Sentiment(str, enum.Enum):\n    \"\"\"Sentiment scale\"\"\"\n    HAPPY = \"HAPPY\"\n    SAD = \"SAD\"\n"
            ),
            "got:\n{leaf}"
        );
        assert!(
            !leaf.contains("Members:"),
            "Members: section must not appear when no variant is documented; got:\n{leaf}"
        );
    }

    /// Class has a summary but no field carries a `///` — the
    /// Attributes: section is suppressed entirely.
    #[test]
    fn class_summary_only_skips_attributes_section() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Resume");
        pool.insert(
            n.clone(),
            class_with_docs(
                n,
                Some("Job applicant resume."),
                vec![
                    ClassProperty {
                        name: BaseName::new("a"),
                        docstring: None,
                        ty: Ty::Int {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                    },
                    ClassProperty {
                        name: BaseName::new("b"),
                        docstring: None,
                        ty: Ty::Int {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                    },
                ],
            ),
        );

        let leaf = &to_source_code(&pool, &[], NamingConvention::PreserveCase)
            [&PathBuf::from("lorem/__init__.py")];
        assert!(
            !leaf.contains("Attributes:"),
            "Attributes: section must not appear when no field is documented; got:\n{leaf}"
        );
    }

    /// Once any field carries a `///`, the Attributes: section appears
    /// and lists *every* field — undocumented ones render as bare
    /// names.
    #[test]
    fn class_partial_field_docs_lists_all_fields_in_attributes() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Resume");
        pool.insert(
            n.clone(),
            class_with_docs(
                n,
                None,
                vec![
                    ClassProperty {
                        name: BaseName::new("a"),
                        docstring: Some("First.".to_string()),
                        ty: Ty::Int {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                    },
                    ClassProperty {
                        name: BaseName::new("b"),
                        docstring: None,
                        ty: Ty::Int {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                    },
                ],
            ),
        );

        let leaf = &to_source_code(&pool, &[], NamingConvention::PreserveCase)
            [&PathBuf::from("lorem/__init__.py")];
        assert!(
            leaf.contains(
                "class Resume(pydantic.BaseModel):\n    \"\"\"\n    Attributes:\n        a: First.\n        b\n    \"\"\"\n"
            ),
            "got:\n{leaf}"
        );
    }

    #[test]
    fn class_no_docs_unchanged() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Resume");
        pool.insert(n.clone(), class(n));

        let leaf = &to_source_code(&pool, &[], NamingConvention::PreserveCase)
            [&PathBuf::from("lorem/__init__.py")];
        assert!(!leaf.contains("\"\"\""));
        assert!(
            !leaf.contains("    # "),
            "unexpected docstring comment in:\n{leaf}"
        );
    }

    #[test]
    fn function_docstring_emits_in_pyi_body() {
        let mut pool: SymbolPool = HashMap::new();
        let mut f = bare_func("ExtractResume", "x.baml", 0);
        f.docstring = Some("Extract resume from PDF.".to_string());
        pool.insert(
            cg_name("user", &["lorem"], "ExtractResume"),
            Symbol::Function(f),
        );

        let leaf = &to_source_code(&pool, &[], NamingConvention::PreserveCase)
            [&PathBuf::from("lorem/__init__.pyi")];
        assert!(
            leaf.contains(
                "def ExtractResume(x: int) -> int:\n    \"\"\"Extract resume from PDF.\"\"\"\n"
            ),
            "got:\n{leaf}"
        );
    }

    #[test]
    fn function_no_docstring_keeps_ellipsis() {
        let mut pool: SymbolPool = HashMap::new();
        let f = bare_func("ExtractResume", "x.baml", 0);
        pool.insert(
            cg_name("user", &["lorem"], "ExtractResume"),
            Symbol::Function(f),
        );

        let leaf = &to_source_code(&pool, &[], NamingConvention::PreserveCase)
            [&PathBuf::from("lorem/__init__.pyi")];
        assert!(leaf.contains("def ExtractResume(x: int) -> int: ..."));
    }

    #[test]
    fn instance_method_docstring_emits_in_pyi_body() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Resume");
        let mut method = bare_func("summarize", "x.baml", 100);
        method.docstring = Some("Summarize the resume.".to_string());
        method.arguments = vec![FunctionArgument {
            injected: false,
            name: BaseName::new("self"),
            docstring: None,
            ty: class_ty(n.clone(), vec![]),
            default: None,
        }];
        pool.insert(
            n.clone(),
            Symbol::Class(Class {
                generic_params: Vec::new(),
                name: n,
                docstring: None,
                properties: vec![ClassProperty {
                    name: BaseName::new("a"),
                    docstring: None,
                    ty: Ty::Int {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                }],
                static_methods: vec![],
                instance_methods: vec![method],
                origin: origin("x.baml", 0),
            }),
        );

        let leaf = &to_source_code(&pool, &[], NamingConvention::PreserveCase)
            [&PathBuf::from("lorem/__init__.pyi")];
        assert!(
            leaf.contains(":\n        \"\"\"Summarize the resume.\"\"\"\n"),
            "got:\n{leaf}"
        );
    }

    #[test]
    fn function_fans_out_sync_and_async() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "extract_resume");
        pool.insert(n, func_sym("extract_resume", "x.baml", 0));

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        let sync_line = "extract_resume       = _define_function(\"user.lorem.extract_resume\", \"sync\",  [\"x\"]";
        let async_line = "extract_resume_async = _define_function(\"user.lorem.extract_resume\", \"async\", [\"x\"]";
        assert!(leaf.contains(sync_line), "missing sync line in:\n{leaf}");
        assert!(leaf.contains(async_line), "missing async line in:\n{leaf}");
        assert!(!leaf.contains("extract_resume_stream"));

        // Fan-out siblings should be adjacent (no blank between).
        let idx_sync = leaf.find(sync_line).unwrap();
        let idx_async = leaf.find(async_line).unwrap();
        let sync_end = idx_sync + leaf[idx_sync..].find('\n').unwrap() + 1;
        let between = &leaf[sync_end..idx_async];
        assert_eq!(between, "");
    }

    #[test]
    fn stream_return_stub_imports_runtime_type_directly() {
        let mut pool: SymbolPool = HashMap::new();
        let stream_name = cg_name("ai", &["stream"], "Stream");
        let done_name = cg_name("ai", &["stream"], "Done");
        pool.insert(stream_name.clone(), class(stream_name.clone()));
        pool.insert(done_name.clone(), class(done_name));

        let mut f = bare_func("extract_resume_stream", "x.baml", 0);
        f.return_type = class_ty(
            stream_name,
            vec![
                Ty::Int {
                    attr: baml_base::TyAttr::EMPTY,
                },
                Ty::String {
                    attr: baml_base::TyAttr::EMPTY,
                },
            ],
        );
        pool.insert(
            cg_name("user", &["lorem"], "extract_resume_stream"),
            Symbol::Function(f),
        );

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let stub = &out[&PathBuf::from("lorem/__init__.pyi")];
        assert!(stub.contains("from ..ai.stream import Done as _BamlStreamDone\n"));
        assert!(stub.contains("from baml_bridge import BamlStream as _BamlStream\n"));
        assert!(stub.contains(
            "def extract_resume_stream(x: int) -> _BamlStream[typing.Union[int, _BamlStreamDone], int, str]:"
        ));
        assert!(!stub.contains("ai.stream.Stream"));
    }

    #[test]
    fn stream_state_class_is_not_rewritten_as_host_handle() {
        let mut pool: SymbolPool = HashMap::new();
        let stream_state_name = cg_name("ai", &["stream"], "Stream$stream");
        let holder_name = cg_name("user", &["lorem"], "PartialHolder");
        pool.insert(stream_state_name.clone(), class(stream_state_name.clone()));
        pool.insert(
            holder_name.clone(),
            class_with_props(
                holder_name,
                vec![(
                    "stream_state",
                    class_ty(
                        stream_state_name,
                        vec![
                            Ty::Int {
                                attr: baml_base::TyAttr::EMPTY,
                            },
                            Ty::String {
                                attr: baml_base::TyAttr::EMPTY,
                            },
                        ],
                    ),
                )],
                "x.baml",
                0,
            ),
        );

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        for path in ["lorem/__init__.py", "lorem/__init__.pyi"] {
            let leaf = &out[&PathBuf::from(path)];
            assert!(
                leaf.contains("stream_state: stream_types.ai.stream.Stream[int, str]"),
                "{path} lowered a partial-state class incorrectly:\n{leaf}"
            );
            assert!(!leaf.contains("_BamlStream["));
        }
    }

    #[test]
    fn partial_alias_hoisting_ignores_the_synthetic_stream_types_prefix() {
        let partial_ai = LeafPath {
            segments: vec!["stream_types".into(), "ai".into()],
        };

        assert!(crate::leaf::routes_outside_package(
            &partial_ai,
            &cg_name("baml", &["media"], "Image"),
        ));
        assert!(!crate::leaf::routes_outside_package(
            &partial_ai,
            &cg_name("ai", &["content"], "Media"),
        ));
    }

    #[test]
    fn function_does_not_emit_removed_utility_companions() {
        let mut pool: SymbolPool = HashMap::new();
        pool.insert(
            cg_name("user", &["lorem"], "extract_resume"),
            func_sym("extract_resume", "x.baml", 0),
        );
        pool.insert(
            cg_name("user", &["lorem"], "extract_resume@spec"),
            func_sym("extract_resume@spec", "x.baml", 1),
        );

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        for removed in ["build_request", "render_prompt", "__parse"] {
            assert!(
                !leaf.contains(removed),
                "found removed {removed} binding:\n{leaf}"
            );
        }
        assert!(leaf.contains("extract_resume_spec       ="), "{leaf}");
        assert!(
            leaf.contains("\"user.lorem.extract_resume@spec\""),
            "{leaf}"
        );
        assert!(!leaf.contains("user.lorem.extract_resume$spec"), "{leaf}");
    }

    #[test]
    fn stream_class_routes_to_stream_types() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Resume$stream");
        pool.insert(n.clone(), class(n));

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("stream_types/lorem/__init__.py")];
        assert!(leaf.contains("class Resume(pydantic.BaseModel):\n"));
        // The Python identifier strips `$stream`; the engine FQN keeps
        // it (stored on the `_baml_type_name` ClassVar) so the engine
        // and the typemap key agree on the wire name.
        assert!(!leaf.contains("class Resume$stream"));
        assert!(!leaf.contains("Resume$stream =")); // no register-as-suffix

        // The non-stream `lorem/` dir isn't emitted — no non-stream
        // user.lorem symbols routed here.
        assert!(!out.contains_key(&PathBuf::from("lorem/__init__.py")));
    }

    #[test]
    fn source_order_sorting() {
        // Two classes in the same file at different spans should render
        // in span order, regardless of insertion order into the pool.
        let mut pool: SymbolPool = HashMap::new();
        let late = cg_name("user", &["lorem"], "Bar");
        let early = cg_name("user", &["lorem"], "Foo");
        pool.insert(late.clone(), class_at(late, "x.baml", 200));
        pool.insert(early.clone(), class_at(early, "x.baml", 100));

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        let idx_foo = leaf.find("class Foo(pydantic.BaseModel):").unwrap();
        let idx_bar = leaf.find("class Bar(pydantic.BaseModel):").unwrap();
        assert!(idx_foo < idx_bar);
    }

    #[test]
    fn multi_file_interleave() {
        // Two classes from different files land in the same leaf and
        // interleave lexicographically by file path.
        let mut pool: SymbolPool = HashMap::new();
        let a = cg_name("user", &["lorem"], "A");
        let b = cg_name("user", &["lorem"], "B");
        pool.insert(a.clone(), class_at(a, "b.baml", 0));
        pool.insert(b.clone(), class_at(b, "a.baml", 0));

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        // B (a.baml) sorts before A (b.baml).
        let idx_a = leaf.find("class A(pydantic.BaseModel):").unwrap();
        let idx_b = leaf.find("class B(pydantic.BaseModel):").unwrap();
        assert!(idx_b < idx_a);
    }

    #[test]
    fn all_lists_public_names_only() {
        let mut pool: SymbolPool = HashMap::new();
        let c = cg_name("user", &["lorem"], "Resume");
        let e = cg_name("user", &["lorem"], "Sentiment");
        pool.insert(c.clone(), class_at(c, "x.baml", 0));
        pool.insert(e.clone(), enum_(e, "x.baml", 50));

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        assert!(leaf.contains("__all__ = [\n    \"Resume\",\n    \"Sentiment\",\n]"));
    }

    #[test]
    fn vendor_creates_interior_dirs() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("aws", &["s3"], "Bucket");
        pool.insert(n.clone(), class(n));

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);

        assert!(out.contains_key(&PathBuf::from("vendor/__init__.py")));
        assert!(out.contains_key(&PathBuf::from("vendor/aws/__init__.py")));
        assert!(out.contains_key(&PathBuf::from("vendor/aws/s3/__init__.py")));

        // Each intermediate package exposes its immediate child via the
        // PEP 562 `__getattr__` hook (rather than an eager
        // `from . import <child>` cascade); dotted refs like
        // `vendor.aws.s3.Bucket` resolve on first access.
        let vendor_init = &out[&PathBuf::from("vendor/__init__.py")];
        assert!(vendor_init.contains("    \"aws\",\n"));
        assert!(vendor_init.contains("def __getattr__(name):\n"));
        assert!(!vendor_init.contains("from . import aws\n"));
        let aws_init = &out[&PathBuf::from("vendor/aws/__init__.py")];
        assert!(aws_init.contains("    \"s3\",\n"));
        assert!(aws_init.contains("def __getattr__(name):\n"));
        assert!(!aws_init.contains("from . import s3\n"));

        // The `.pyi` siblings keep the classical cascade so pyright
        // resolves dotted access structurally.
        let vendor_pyi = &out[&PathBuf::from("vendor/__init__.pyi")];
        assert!(vendor_pyi.contains("from . import aws\n"));
        let aws_pyi = &out[&PathBuf::from("vendor/aws/__init__.pyi")];
        assert!(aws_pyi.contains("from . import s3\n"));

        // Leaf carries the symbol.
        let s3_leaf = &out[&PathBuf::from("vendor/aws/s3/__init__.py")];
        assert!(s3_leaf.contains("class Bucket(pydantic.BaseModel):"));
    }

    #[test]
    fn root_stub_populates_root_init() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &[], "Foo");
        pool.insert(n.clone(), class(n));

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let root = &out[&PathBuf::from("__init__.py")];
        assert!(root.contains("BamlRuntime.initialize_runtime("));
        // Body appended after the runtime init + re-exports.
        assert!(root.contains("class Foo(pydantic.BaseModel):\n"));
        assert!(root.contains("__all__ = [\n    \"Foo\",\n]"));
    }

    #[test]
    fn factory_import_present_only_in_leaves_with_functions() {
        // G5 emits `define_function as _define_function` exactly once
        // per leaf that carries any function/companion binding, and
        // never in leaves that don't. 25b Phase 2: every leaf with a
        // class/enum/alias also pulls `register_class`/`register_enum`/
        // `register_type_alias`, so the surrounding `from baml_bridge ...`
        // block is no longer factory-exclusive — we assert on the
        // factory name itself.
        let mut pool: SymbolPool = HashMap::new();
        // lorem leaf: class + function → factory import expected.
        let c = cg_name("user", &["lorem"], "Resume");
        pool.insert(c.clone(), class(c));
        let f = cg_name("user", &["lorem"], "extract_resume");
        pool.insert(f, func_sym("extract_resume", "x.baml", 100));
        // ipsum leaf: class only → no factory import.
        let c2 = cg_name("user", &["ipsum"], "Tag");
        pool.insert(c2.clone(), class(c2));

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);

        let lorem = &out[&PathBuf::from("lorem/__init__.py")];
        assert!(
            lorem.contains("define_function as _define_function"),
            "lorem missing factory import:\n{lorem}"
        );
        assert_eq!(
            lorem.matches("define_function as _define_function").count(),
            1,
            "factory import should appear exactly once"
        );

        let ipsum = &out[&PathBuf::from("ipsum/__init__.py")];
        assert!(
            !ipsum.contains("_define_function"),
            "ipsum leaf must not reference _define_function:\n{ipsum}"
        );

        // Stream-types leaves carry only stream-companion classes — no
        // factories — so they must not pull a function-factory helper.
        for (path, content) in &out {
            let s = path.to_string_lossy();
            if s.starts_with("stream_types/") && s.ends_with("__init__.py") {
                assert!(
                    !content.contains("_define_function"),
                    "stream_types leaf {} must not import factory:\n{}",
                    path.display(),
                    content
                );
            }
        }
    }

    #[test]
    fn stream_variant_under_stream_types() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Resume$stream");
        pool.insert(n.clone(), class(n));

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);

        assert!(out.contains_key(&PathBuf::from("stream_types/__init__.py")));
        assert!(out.contains_key(&PathBuf::from("stream_types/lorem/__init__.py")));

        // 25b2 Phase 4: subpackage cascade is gone — root no longer pulls
        // in stream_types. The leaf still carries the routed companion.
        let stream_leaf = &out[&PathBuf::from("stream_types/lorem/__init__.py")];
        assert!(stream_leaf.contains("class Resume(pydantic.BaseModel):"));
    }

    #[test]
    fn inlinedbaml_round_trips() {
        let pool: SymbolPool = HashMap::new();
        let files = vec![
            (PathBuf::from("main.baml"), "class Foo {}\n".to_string()),
            (
                // `PathBuf::join` to exercise the OS-native separator —
                // `/` on Unix, `\` on Windows. Path strings flow through
                // `to_string_lossy()` and `py_string` escapes any `\`.
                PathBuf::from("lorem").join("bar.baml"),
                "function foo() -> int { 1 }\n".to_string(),
            ),
        ];
        let out = to_source_code(&pool, &files, NamingConvention::PreserveCase);

        let inl = &out[&PathBuf::from("_inlinedbaml.py")];
        assert!(inl.starts_with(HEADER));
        assert!(inl.contains("from ._baml_sources import FILES"));

        let sources = &out[&PathBuf::from("_baml_sources.py")];
        assert!(sources.starts_with(HEADER));
        assert!(sources.contains("FILES: dict[str, str] = {"));
        // On Windows the path renders `lorem\bar.baml`, which `py_string`
        // escapes to `lorem\\bar.baml` in the emitted Python literal.
        #[cfg(windows)]
        let nested_key = "lorem\\\\bar.baml";
        #[cfg(not(windows))]
        let nested_key = "lorem/bar.baml";
        let lo = sources.find(nested_key).unwrap();
        let mo = sources.find("main.baml").unwrap();
        assert!(lo < mo);
        assert!(sources.contains("\"class Foo {}\\n\""));
    }

    #[test]
    fn bytecode_payload_initializes_runtime_from_bytecode() {
        let pool: SymbolPool = HashMap::new();
        let bytecode = b"\x00BAML\"\n\xff";
        let out = to_source_code_with_bytecode(&pool, bytecode, NamingConvention::PreserveCase);

        let root = &out[&PathBuf::from("__init__.py")];
        assert!(
            root.contains("BamlRuntime.initialize_runtime_from_bytecode(_inlinedbaml.BYTECODE, _inlinedbaml.EMBEDDED_BAML_TOML)")
        );
        assert!(!root.contains("BamlRuntime.initialize_runtime("));
        assert!(root.contains("def get_baml_source_files() -> dict[str, str]:"));

        let inl = &out[&PathBuf::from("_inlinedbaml.py")];
        assert!(inl.starts_with(HEADER));
        assert!(inl.contains("BYTECODE: bytes = ("));
        assert!(inl.contains("b\"\\x00BAML\\\"\\x0a\\xff\""));
        assert!(!inl.contains("FILES: dict[str, str]"));

        let sources = &out[&PathBuf::from("_baml_sources.py")];
        assert!(sources.contains("FILES: dict[str, str] = {\n}"));
    }

    #[test]
    fn bytecode_payload_retains_sorted_user_sources() {
        let pool: SymbolPool = HashMap::new();
        let files = vec![
            (
                PathBuf::from("z.baml"),
                "function z() -> int { 1 }\n".to_string(),
            ),
            (
                PathBuf::from("nested").join("a.baml"),
                "function a() -> int { 2 }\n".to_string(),
            ),
        ];
        let out = to_source_code_with_bytecode_and_metadata_and_source_files(
            &pool,
            b"bytecode",
            "[package]\nname = \"test\"\n",
            &files,
            NamingConvention::PreserveCase,
        );

        let inlined = &out[&PathBuf::from("_inlinedbaml.py")];
        assert!(inlined.contains("BYTECODE: bytes = ("));
        assert!(inlined.contains("EMBEDDED_BAML_TOML: str | None = "));
        assert!(!inlined.contains("function a()"));

        let sources = &out[&PathBuf::from("_baml_sources.py")];
        #[cfg(windows)]
        let nested_key = "nested\\\\a.baml";
        #[cfg(not(windows))]
        let nested_key = "nested/a.baml";
        let nested = sources.find(nested_key).unwrap();
        let root = sources.find("z.baml").unwrap();
        assert!(nested < root);
        assert!(sources.contains("function a() -> int { 2 }\\n"));

        let root_stub = &out[&PathBuf::from("__init__.pyi")];
        assert!(root_stub.contains("def get_baml_source_files() -> dict[str, str]: ..."));
    }

    #[test]
    fn py_string_escapes() {
        assert_eq!(py_string("hello"), "\"hello\"");
        assert_eq!(py_string("a\\b"), "\"a\\\\b\"");
        assert_eq!(py_string("a\"b"), "\"a\\\"b\"");
        assert_eq!(py_string("a\nb"), "\"a\\nb\"");
    }

    fn class_with_props(name: Name, props: Vec<(&str, Ty)>, file: &str, span: u32) -> Symbol {
        Symbol::Class(Class {
            generic_params: Vec::new(),
            name,
            docstring: None,
            properties: props
                .into_iter()
                .map(|(n, ty)| ClassProperty {
                    name: BaseName::new(n),
                    docstring: None,
                    ty,
                })
                .collect(),
            static_methods: vec![],
            instance_methods: vec![],
            origin: origin(file, span),
        })
    }

    fn alias_full(name: Name, resolves_to: Ty, recursive: bool, file: &str, span: u32) -> Symbol {
        Symbol::TypeAlias(TypeAlias {
            name,
            resolves_to,
            recursive,
            origin: origin(file, span),
        })
    }

    #[test]
    fn class_renders_mixed_property_types() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Resume");
        pool.insert(
            n.clone(),
            class_with_props(
                n,
                vec![
                    (
                        "name",
                        Ty::String {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                    ),
                    (
                        "email",
                        union(vec![
                            Ty::String {
                                attr: baml_base::TyAttr::EMPTY,
                            },
                            Ty::Null {
                                attr: baml_base::TyAttr::EMPTY,
                            },
                        ]),
                    ),
                    (
                        "tags",
                        list(Box::new(Ty::String {
                            attr: baml_base::TyAttr::EMPTY,
                        })),
                    ),
                ],
                "x.baml",
                0,
            ),
        );
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        let expected = "class Resume(pydantic.BaseModel):\n\
                        \x20   model_config = pydantic.ConfigDict(\n\
                        \x20       arbitrary_types_allowed=True,\n\
                        \x20       extra=\"ignore\",\n\
                        \x20       populate_by_name=True,\n\
                        \x20   )\n\
                        \x20   name: str\n\
                        \x20   email: typing.Optional[str] = None\n\
                        \x20   tags: typing.List[str]\n";
        assert!(leaf.contains(expected), "leaf missing class body:\n{leaf}");
    }

    #[test]
    fn nullable_fields_default_none_without_changing_function_parameters() {
        let mut pool: SymbolPool = HashMap::new();
        let nullable_string = union(vec![
            Ty::String {
                attr: baml_base::TyAttr::EMPTY,
            },
            Ty::Null {
                attr: baml_base::TyAttr::EMPTY,
            },
        ]);
        let nullable_alias = cg_name("user", &["lorem"], "NullableText");
        pool.insert(
            nullable_alias.clone(),
            alias_full(
                nullable_alias.clone(),
                nullable_string.clone(),
                false,
                "x.baml",
                0,
            ),
        );
        let model = cg_name("user", &["lorem"], "Payload");
        pool.insert(
            model.clone(),
            class_with_props(
                model,
                vec![
                    (
                        "required",
                        Ty::String {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                    ),
                    ("nullable", nullable_string.clone()),
                    (
                        "nullable_union",
                        union(vec![
                            Ty::Int {
                                attr: baml_base::TyAttr::EMPTY,
                            },
                            Ty::String {
                                attr: baml_base::TyAttr::EMPTY,
                            },
                            Ty::Null {
                                attr: baml_base::TyAttr::EMPTY,
                            },
                        ]),
                    ),
                    ("nullable_alias", alias_ty(nullable_alias)),
                    ("nullable_items", list(Box::new(nullable_string.clone()))),
                ],
                "x.baml",
                10,
            ),
        );
        let function_name = cg_name("user", &["lorem"], "accept_nullable");
        let mut function = make_func("accept_nullable", &["value"], "x.baml", 20);
        function.arguments[0].ty = nullable_string;
        pool.insert(function_name, Symbol::Function(function));

        let output = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        for path in ["lorem/__init__.py", "lorem/__init__.pyi"] {
            let leaf = &output[&PathBuf::from(path)];
            assert!(leaf.contains("    required: str\n"), "{path}:\n{leaf}");
            assert!(
                leaf.contains("    nullable: typing.Optional[str] = None\n"),
                "{path}:\n{leaf}"
            );
            assert!(
                leaf.contains("    nullable_union: typing.Union[int, str, None] = None\n"),
                "{path}:\n{leaf}"
            );
            assert!(
                leaf.contains("    nullable_alias: NullableText = None\n"),
                "{path}:\n{leaf}"
            );
            assert!(
                leaf.contains("    nullable_items: typing.List[typing.Optional[str]]\n"),
                "{path}:\n{leaf}"
            );
        }
        let pyi = &output[&PathBuf::from("lorem/__init__.pyi")];
        assert!(pyi.contains("def accept_nullable(value: typing.Optional[str]) -> int: ...\n"));
        assert!(!pyi.contains("def accept_nullable(value: typing.Optional[str] = None)"));
    }

    #[test]
    fn zero_property_class_emits_only_model_config() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Empty");
        pool.insert(n.clone(), class_with_props(n, vec![], "x.baml", 0));
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        let expected = "class Empty(pydantic.BaseModel):\n\
                        \x20   model_config = pydantic.ConfigDict(\n\
                        \x20       arbitrary_types_allowed=True,\n\
                        \x20       extra=\"ignore\",\n\
                        \x20       populate_by_name=True,\n\
                        \x20   )\n";
        assert!(leaf.contains(expected));
    }

    #[test]
    fn multi_variant_enum_renders_each_variant() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["ipsum"], "Sentiment");
        pool.insert(
            n.clone(),
            Symbol::Enum(Enum {
                name: n,
                docstring: None,
                variants: vec![
                    EnumVariant {
                        name: BaseName::new("POSITIVE"),
                        docstring: None,
                        value: "POSITIVE".to_string(),
                    },
                    EnumVariant {
                        name: BaseName::new("NEGATIVE"),
                        docstring: None,
                        value: "NEGATIVE".to_string(),
                    },
                    EnumVariant {
                        name: BaseName::new("NEUTRAL"),
                        docstring: None,
                        value: "NEUTRAL".to_string(),
                    },
                ],
                origin: origin("x.baml", 0),
            }),
        );
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("ipsum/__init__.py")];
        let expected = "class Sentiment(str, enum.Enum):\n\
                        \x20   POSITIVE = \"POSITIVE\"\n\
                        \x20   NEGATIVE = \"NEGATIVE\"\n\
                        \x20   NEUTRAL = \"NEUTRAL\"\n";
        assert!(leaf.contains(expected), "leaf missing enum body:\n{leaf}");
    }

    #[test]
    fn empty_enum_emits_defensive_pass() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Nothing");
        pool.insert(
            n.clone(),
            Symbol::Enum(Enum {
                name: n,
                docstring: None,
                variants: vec![],
                origin: origin("x.baml", 0),
            }),
        );
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        assert!(leaf.contains("class Nothing(str, enum.Enum):\n    pass\n"));
    }

    #[test]
    fn recursive_type_alias_emits_type_alias_type() {
        // type JsonValue = int | str | List<JsonValue>  (recursive).
        // Per 18c, recursive aliases render via
        // `typing_extensions.TypeAliasType` with self-references quoted
        // as forward-refs, so a `BaseModel` field annotated with
        // `JsonValue` no longer infinite-recurses during Pydantic
        // schema build.
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["tree"], "JsonValue");
        let rhs = union(vec![
            Ty::Int {
                attr: baml_base::TyAttr::EMPTY,
            },
            Ty::String {
                attr: baml_base::TyAttr::EMPTY,
            },
            list(Box::new(alias_ty(n.clone()))),
        ]);
        pool.insert(n.clone(), alias_full(n, rhs, true, "tree.baml", 0));
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("tree/__init__.py")];
        assert!(leaf.contains("import typing_extensions\n"));
        assert!(leaf.contains(
            "JsonValue = typing_extensions.TypeAliasType(\"JsonValue\", typing.Union[int, str, typing.List[\"JsonValue\"]])\n"
        ));
    }

    #[test]
    fn recursive_alias_quotes_cross_leaf_and_root_refs() {
        // A recursive alias body referencing names from other leaves
        // and from the root has to emit them as forward-ref strings.
        // The RHS of `TypeAliasType(...)` evaluates eagerly at module
        // load; cross-leaf imports are TYPE_CHECKING-guarded and root
        // names are also imported under TYPE_CHECKING from non-root
        // leaves — bare references would `NameError` at line eval.
        let mut pool: SymbolPool = HashMap::new();
        let foo = cg_name("user", &[], "Foo"); // root-routed
        let bar = cg_name("user", &["util"], "Bar"); // cross-leaf
        let alias = cg_name("user", &["lorem"], "Mixed"); // recursive in lorem
        pool.insert(foo.clone(), class(foo.clone()));
        pool.insert(bar.clone(), class(bar.clone()));
        pool.insert(
            alias.clone(),
            alias_full(
                alias.clone(),
                union(vec![
                    class_ty(foo, vec![]),
                    class_ty(bar, vec![]),
                    list(Box::new(alias_ty(alias))),
                ]),
                true,
                "lorem.baml",
                0,
            ),
        );
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        assert!(
            leaf.contains(
                "Mixed = typing_extensions.TypeAliasType(\"Mixed\", typing.Union[\"Foo\", \"util.Bar\", typing.List[\"Mixed\"]])\n"
            ),
            "lorem leaf missing properly-quoted recursive alias body:\n{leaf}"
        );
    }

    #[test]
    fn non_recursive_alias_referencing_recursive_one_is_unquoted() {
        // type Bar = List<JsonValue>  (non-recursive).
        let mut pool: SymbolPool = HashMap::new();
        let json = cg_name("user", &["tree"], "JsonValue");
        let bar = cg_name("user", &["tree"], "Bar");
        pool.insert(
            json.clone(),
            alias_full(
                json.clone(),
                union(vec![
                    Ty::Int {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                    alias_ty(json.clone()),
                ]),
                true,
                "tree.baml",
                0,
            ),
        );
        pool.insert(
            bar.clone(),
            alias_full(bar, list(Box::new(alias_ty(json))), false, "tree.baml", 100),
        );
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("tree/__init__.py")];
        assert!(leaf.contains("Bar: typing.TypeAlias = typing.List[JsonValue]\n"));
    }

    #[test]
    fn stream_companion_resolves_non_stream_sibling_by_fqn() {
        // $stream companion with a field typed as the non-stream sibling.
        let mut pool: SymbolPool = HashMap::new();
        let non_stream = cg_name("user", &["lorem"], "Resume");
        let stream = cg_name("user", &["lorem"], "Resume$stream");
        pool.insert(
            non_stream.clone(),
            class_with_props(
                non_stream.clone(),
                vec![(
                    "name",
                    Ty::String {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                )],
                "x.baml",
                0,
            ),
        );
        pool.insert(
            stream.clone(),
            class_with_props(
                stream,
                vec![
                    (
                        "summary",
                        union(vec![
                            Ty::String {
                                attr: baml_base::TyAttr::EMPTY,
                            },
                            Ty::Null {
                                attr: baml_base::TyAttr::EMPTY,
                            },
                        ]),
                    ),
                    // Non-stream FQN -> resolves to baml_sdk.lorem.Resume
                    ("origin", class_ty(non_stream, vec![])),
                ],
                "x.baml",
                0,
            ),
        );
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);

        // Non-stream leaf has the sibling.
        let non_stream_leaf = &out[&PathBuf::from("lorem/__init__.py")];
        assert!(non_stream_leaf.contains("class Resume(pydantic.BaseModel):\n"));

        // Stream leaf has the companion; the cross-stream reference to
        // the non-stream sibling should render as `lorem.Resume` (G3's
        // cross-leaf FQN form).
        let stream_leaf = &out[&PathBuf::from("stream_types/lorem/__init__.py")];
        let expected = "class Resume(pydantic.BaseModel):\n\
                        \x20   model_config = pydantic.ConfigDict(\n\
                        \x20       arbitrary_types_allowed=True,\n\
                        \x20       extra=\"ignore\",\n\
                        \x20       populate_by_name=True,\n\
                        \x20   )\n\
                        \x20   summary: typing.Optional[str] = None\n\
                        \x20   origin: lorem.Resume\n";
        assert!(
            stream_leaf.contains(expected),
            "stream leaf missing body:\n{stream_leaf}"
        );
        // 25b2 Phase 4: cross-leaf Pydantic field-edge import lifted out
        // of TYPE_CHECKING. Different first segments so root-anchored
        // (three dots from depth-2 stream leaf).
        assert!(
            stream_leaf.contains("\nfrom ... import lorem\n"),
            "stream leaf missing unconditional three-dot lorem import:\n{stream_leaf}"
        );
        assert!(
            !stream_leaf.contains("if typing.TYPE_CHECKING:\n    from ... import lorem"),
            "lorem import should not be under TYPE_CHECKING:\n{stream_leaf}"
        );
    }

    #[test]
    fn cross_leaf_class_reference_uses_routed_fqn() {
        // class Envelope { sentiment: Sentiment }  across leaves.
        let mut pool: SymbolPool = HashMap::new();
        let sentiment = cg_name("user", &["ipsum"], "Sentiment");
        let envelope = cg_name("user", &["lorem"], "Envelope");
        pool.insert(
            sentiment.clone(),
            Symbol::Enum(Enum {
                name: sentiment.clone(),
                docstring: None,
                variants: vec![EnumVariant {
                    name: BaseName::new("POSITIVE"),
                    docstring: None,
                    value: "POSITIVE".to_string(),
                }],
                origin: origin("ipsum.baml", 0),
            }),
        );
        pool.insert(
            envelope.clone(),
            class_with_props(
                envelope,
                vec![("sentiment", enum_ty(sentiment))],
                "lorem.baml",
                0,
            ),
        );
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let lorem_leaf = &out[&PathBuf::from("lorem/__init__.py")];
        assert!(lorem_leaf.contains("    sentiment: ipsum.Sentiment\n"));
        // 25b2 Phase 4: cross-leaf Pydantic field-edge import is now
        // unconditional (not under TYPE_CHECKING).
        assert!(
            lorem_leaf.contains("\nfrom .. import ipsum\n"),
            "lorem missing unconditional ipsum import:\n{lorem_leaf}"
        );
        assert!(
            !lorem_leaf.contains("if typing.TYPE_CHECKING:\n    from .. import ipsum"),
            "ipsum import should not be under TYPE_CHECKING:\n{lorem_leaf}"
        );
    }

    fn make_func(bare: &str, args: &[&str], file: &str, span: u32) -> Function {
        Function {
            generic_params: Vec::new(),
            name: BaseName::new(bare),
            docstring: None,
            arguments: args
                .iter()
                .map(|n| FunctionArgument {
                    injected: false,
                    name: BaseName::new(*n),
                    docstring: None,
                    ty: Ty::String {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                    default: None,
                })
                .collect(),
            return_type: Ty::Int {
                attr: baml_base::TyAttr::EMPTY,
            },
            throws: None,
            watchers: vec![],
            origin: origin(file, span),
        }
    }

    /// Insert a parent function that takes args, no companions.
    fn insert_parent_only(
        pool: &mut SymbolPool,
        pkg: &str,
        ns: &[&str],
        bare: &str,
        args: &[&str],
        file: &str,
        span: u32,
    ) {
        let key = cg_name(pkg, ns, bare);
        pool.insert(key, Symbol::Function(make_func(bare, args, file, span)));
    }

    #[test]
    fn function_zero_args_renders_empty_param_list() {
        let mut pool: SymbolPool = HashMap::new();
        insert_parent_only(&mut pool, "user", &["lorem"], "ping", &[], "x.baml", 0);
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        assert!(leaf.contains("ping       = _define_function(\"user.lorem.ping\", \"sync\",  []"));
        assert!(leaf.contains("ping_async = _define_function(\"user.lorem.ping\", \"async\", []"));
    }

    #[test]
    fn function_multi_arg_param_names_in_order() {
        let mut pool: SymbolPool = HashMap::new();
        insert_parent_only(
            &mut pool,
            "user",
            &["lorem"],
            "make",
            &["a", "b", "c"],
            "x.baml",
            0,
        );
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        assert!(leaf.contains(
            "make       = _define_function(\"user.lorem.make\", \"sync\",  [\"a\", \"b\", \"c\"]"
        ));
        assert!(leaf.contains(
            "make_async = _define_function(\"user.lorem.make\", \"async\", [\"a\", \"b\", \"c\"]"
        ));
    }

    #[test]
    fn function_defaults_render_keyword_only_signature_and_positional_limit() {
        let mut pool: SymbolPool = HashMap::new();
        let key = cg_name("user", &["lorem"], "search");
        pool.insert(
            key,
            Symbol::Function(Function {
                generic_params: Vec::new(),
                name: BaseName::new("search"),
                docstring: None,
                arguments: vec![
                    FunctionArgument {
                        injected: false,
                        name: BaseName::new("query"),
                        docstring: None,
                        ty: Ty::String {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                        default: None,
                    },
                    FunctionArgument {
                        injected: false,
                        name: BaseName::new("max_results"),
                        docstring: None,
                        ty: Ty::Int {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                        default: Some(FunctionArgumentDefault::Literal(DefaultLiteral::Scalar(
                            baml_base::Literal::Int(10),
                        ))),
                    },
                    FunctionArgument {
                        injected: false,
                        name: BaseName::new("filter"),
                        docstring: None,
                        ty: Ty::String {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                        default: Some(FunctionArgumentDefault::Expression {
                            source: Some("default_filter()".to_string()),
                        }),
                    },
                    FunctionArgument {
                        injected: false,
                        name: BaseName::new("tags"),
                        docstring: None,
                        ty: list(Box::new(Ty::String {
                            attr: baml_base::TyAttr::EMPTY,
                        })),
                        default: Some(FunctionArgumentDefault::Literal(DefaultLiteral::EmptyList)),
                    },
                    FunctionArgument {
                        injected: false,
                        name: BaseName::new("metadata"),
                        docstring: None,
                        ty: Ty::Map {
                            key: Box::new(Ty::String {
                                attr: baml_base::TyAttr::EMPTY,
                            }),
                            value: Box::new(Ty::String {
                                attr: baml_base::TyAttr::EMPTY,
                            }),
                            attr: baml_base::TyAttr::EMPTY,
                        },
                        default: Some(FunctionArgumentDefault::Literal(DefaultLiteral::EmptyMap)),
                    },
                    FunctionArgument {
                        injected: false,
                        name: BaseName::new("fallback"),
                        docstring: None,
                        ty: union(vec![
                            Ty::String {
                                attr: baml_base::TyAttr::EMPTY,
                            },
                            Ty::Null {
                                attr: baml_base::TyAttr::EMPTY,
                            },
                        ]),
                        default: Some(FunctionArgumentDefault::Null),
                    },
                ],
                return_type: Ty::String {
                    attr: baml_base::TyAttr::EMPTY,
                },
                throws: None,
                watchers: vec![],
                origin: origin("x.baml", 0),
            }),
        );

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let py = &out[&PathBuf::from("lorem/__init__.py")];
        assert!(py.contains(
            "search       = _define_function(\"user.lorem.search\", \"sync\",  [\"query\"], [\"max_results\", \"filter\", \"tags\", \"metadata\", \"fallback\"]"
        ));
        assert!(py.contains(
            "search_async = _define_function(\"user.lorem.search\", \"async\", [\"query\"], [\"max_results\", \"filter\", \"tags\", \"metadata\", \"fallback\"]"
        ));

        let pyi = &out[&PathBuf::from("lorem/__init__.pyi")];
        assert!(pyi.contains(
            "def search(query: str, *, max_results: typing.Union[int, UNSET] = 10, filter: typing.Union[str, UNSET] = UNSET, tags: typing.Union[typing.List[str], UNSET] = [], metadata: typing.Union[typing.Dict[str, str], UNSET] = {}, fallback: typing.Union[str, None, UNSET] = None) -> str: ...\n"
        ));
        assert!(pyi.contains(
            "async def search_async(query: str, *, max_results: typing.Union[int, UNSET] = 10, filter: typing.Union[str, UNSET] = UNSET, tags: typing.Union[typing.List[str], UNSET] = [], metadata: typing.Union[typing.Dict[str, str], UNSET] = {}, fallback: typing.Union[str, None, UNSET] = None) -> str: ...\n"
        ));
        // The sentinel is imported as a bare name so it can appear in the
        // `typing.Union[..., UNSET]` type expressions above (type checkers
        // reject `baml.UNSET` member access in a type position).
        assert!(pyi.contains("    from ..baml import UNSET as UNSET\n"));
    }

    #[test]
    fn empty_collection_defaults_do_not_require_unset_import_in_pyi() {
        let mut pool: SymbolPool = HashMap::new();
        let key = cg_name("user", &["lorem"], "defaults");
        pool.insert(
            key,
            Symbol::Function(Function {
                generic_params: Vec::new(),
                name: BaseName::new("defaults"),
                docstring: None,
                arguments: vec![
                    FunctionArgument {
                        injected: false,
                        name: BaseName::new("tags"),
                        docstring: None,
                        ty: list(Box::new(Ty::String {
                            attr: baml_base::TyAttr::EMPTY,
                        })),
                        default: Some(FunctionArgumentDefault::Literal(DefaultLiteral::EmptyList)),
                    },
                    FunctionArgument {
                        injected: false,
                        name: BaseName::new("metadata"),
                        docstring: None,
                        ty: Ty::Map {
                            key: Box::new(Ty::String {
                                attr: baml_base::TyAttr::EMPTY,
                            }),
                            value: Box::new(Ty::Int {
                                attr: baml_base::TyAttr::EMPTY,
                            }),
                            attr: baml_base::TyAttr::EMPTY,
                        },
                        default: Some(FunctionArgumentDefault::Literal(DefaultLiteral::EmptyMap)),
                    },
                    FunctionArgument {
                        injected: false,
                        name: BaseName::new("fallback"),
                        docstring: None,
                        ty: union(vec![
                            Ty::String {
                                attr: baml_base::TyAttr::EMPTY,
                            },
                            Ty::Null {
                                attr: baml_base::TyAttr::EMPTY,
                            },
                        ]),
                        default: Some(FunctionArgumentDefault::Null),
                    },
                ],
                return_type: Ty::String {
                    attr: baml_base::TyAttr::EMPTY,
                },
                throws: None,
                watchers: vec![],
                origin: origin("x.baml", 0),
            }),
        );

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let pyi = &out[&PathBuf::from("lorem/__init__.pyi")];

        assert!(pyi.contains(
            "def defaults(*, tags: typing.Union[typing.List[str], UNSET] = [], metadata: typing.Union[typing.Dict[str, int], UNSET] = {}, fallback: typing.Union[str, None, UNSET] = None) -> str: ...\n"
        ));
        assert!(pyi.contains(
            "async def defaults_async(*, tags: typing.Union[typing.List[str], UNSET] = [], metadata: typing.Union[typing.Dict[str, int], UNSET] = {}, fallback: typing.Union[str, None, UNSET] = None) -> str: ...\n"
        ));
    }

    #[test]
    fn vendor_function_fqn_uses_vendor_pkg() {
        let mut pool: SymbolPool = HashMap::new();
        insert_parent_only(&mut pool, "aws", &["s3"], "create_bucket", &[], "x.baml", 0);
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("vendor/aws/s3/__init__.py")];
        assert!(leaf.contains(
            "create_bucket       = _define_function(\"aws.s3.create_bucket\", \"sync\",  []"
        ));
    }

    #[test]
    fn baml_pkg_function_fqn_keeps_baml_prefix() {
        let mut pool: SymbolPool = HashMap::new();
        insert_parent_only(&mut pool, "baml", &["http"], "fetch", &["url"], "x.baml", 0);
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("baml/http/__init__.py")];
        assert!(
            leaf.contains(
                "fetch       = _define_function(\"baml.http.fetch\", \"sync\",  [\"url\"]"
            )
        );
    }

    #[test]
    fn root_no_namespace_function_fqn_drops_segment() {
        let mut pool: SymbolPool = HashMap::new();
        insert_parent_only(&mut pool, "user", &[], "ping", &[], "x.baml", 0);
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let root = &out[&PathBuf::from("__init__.py")];
        assert!(
            root.contains("ping       = _define_function(\"user.ping\", \"sync\",  []"),
            "missing root binding in:\n{root}"
        );
    }

    #[test]
    fn determinism_repeated_runs_produce_identical_output() {
        let mut pool: SymbolPool = HashMap::new();
        let a = cg_name("user", &["lorem"], "Alpha");
        let b = cg_name("user", &["lorem"], "Beta");
        pool.insert(
            a.clone(),
            class_with_props(
                a,
                vec![(
                    "x",
                    Ty::Int {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                )],
                "a.baml",
                0,
            ),
        );
        pool.insert(
            b.clone(),
            class_with_props(
                b,
                vec![(
                    "y",
                    Ty::String {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                )],
                "b.baml",
                0,
            ),
        );
        let out1 = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let out2 = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        // Same keys + same contents on every path.
        let mut k1: Vec<_> = out1.keys().collect();
        let mut k2: Vec<_> = out2.keys().collect();
        k1.sort();
        k2.sort();
        assert_eq!(k1, k2);
        for (p, c) in &out1 {
            assert_eq!(&out2[p], c, "mismatch at {}", p.display());
        }
    }

    fn class_with_methods(
        name: Name,
        static_methods: Vec<Function>,
        instance_methods: Vec<Function>,
        file: &str,
        span: u32,
    ) -> Symbol {
        Symbol::Class(Class {
            generic_params: Vec::new(),
            name,
            docstring: None,
            properties: vec![],
            static_methods,
            instance_methods,
            origin: origin(file, span),
        })
    }

    fn method_func(bare: &str, args: &[&str], file: &str, span: u32) -> Function {
        Function {
            generic_params: Vec::new(),
            name: BaseName::new(bare),
            docstring: None,
            arguments: args
                .iter()
                .map(|n| FunctionArgument {
                    injected: false,
                    name: BaseName::new(*n),
                    docstring: None,
                    ty: Ty::Int {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                    default: None,
                })
                .collect(),
            return_type: Ty::Int {
                attr: baml_base::TyAttr::EMPTY,
            },
            throws: None,
            watchers: vec![],
            origin: origin(file, span),
        }
    }

    #[test]
    fn class_static_method_wraps_in_staticmethod() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Counter");
        pool.insert(
            n.clone(),
            class_with_methods(
                n,
                vec![method_func("zero", &[], "x.baml", 100)],
                vec![],
                "x.baml",
                0,
            ),
        );

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        assert!(
            leaf.contains("define_function as _define_function"),
            "missing factory import:\n{leaf}"
        );
        assert!(
            leaf.contains(
                "    zero       = staticmethod(_define_function(\"user.lorem.Counter.zero\", \"sync\",  []"
            ),
            "missing static method sync line in:\n{leaf}"
        );
        assert!(
            leaf.contains(
                "    zero_async = staticmethod(_define_function(\"user.lorem.Counter.zero\", \"async\", []"
            ),
            "missing static method async line in:\n{leaf}"
        );
    }

    #[test]
    fn class_instance_method_no_wrap_self_prepended() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Counter");
        // The pool's `Function` for an instance method does not carry
        // the `self` parameter — the receiver is prepended at render
        // time. Pass `["by"]` and confirm the rendered RHS has
        // `["self", "by"]`.
        pool.insert(
            n.clone(),
            class_with_methods(
                n,
                vec![],
                vec![method_func("bump", &["by"], "x.baml", 100)],
                "x.baml",
                0,
            ),
        );

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        assert!(
            leaf.contains("define_function as _define_function"),
            "missing factory import:\n{leaf}"
        );
        assert!(
            leaf.contains(
                "    bump       = _define_function(\"user.lorem.Counter.bump\", \"sync\",  [\"self\", \"by\"]"
            ),
            "missing instance method sync line in:\n{leaf}"
        );
        assert!(
            leaf.contains(
                "    bump_async = _define_function(\"user.lorem.Counter.bump\", \"async\", [\"self\", \"by\"]"
            ),
            "missing instance method async line in:\n{leaf}"
        );
        // Critical: instance methods must NOT be wrapped in
        // `staticmethod(...)` — that breaks descriptor-protocol binding.
        assert!(
            !leaf.contains("staticmethod("),
            "instance method must not be wrapped:\n{leaf}"
        );
    }

    #[test]
    fn class_with_both_method_kinds_emits_single_factory_import() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Mixed");
        pool.insert(
            n.clone(),
            class_with_methods(
                n,
                vec![method_func("make", &["x"], "x.baml", 100)],
                vec![method_func("describe", &[], "x.baml", 200)],
                "x.baml",
                0,
            ),
        );

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        // Both static and instance methods route through the same
        // `define_function` factory, so the import collapses to one
        // line — no parenthesized form needed.
        assert!(
            leaf.contains("from baml_bridge import define_function as _define_function\n"),
            "missing single-line factory import:\n{leaf}"
        );
    }

    #[test]
    fn property_only_class_unchanged_by_method_renderer() {
        // Class with only properties (no methods) should render
        // byte-identical to G5 — no method block, no factory import.
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Resume");
        pool.insert(n.clone(), class(n));

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        assert!(
            !leaf.contains("_define_function"),
            "property-only class must not import _define_function:\n{leaf}"
        );
        assert!(
            !leaf.contains("staticmethod("),
            "property-only class must not contain staticmethod wrap:\n{leaf}"
        );
    }

    #[test]
    fn no_legacy_output_paths() {
        let pool: SymbolPool = HashMap::new();
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        for path in out.keys() {
            let s = path.to_string_lossy();
            assert!(!s.starts_with("baml_types/"));
            assert!(!s.starts_with("baml_stream_types/"));
            assert!(!s.starts_with("baml_sync/"));
            assert!(!s.starts_with("baml_async/"));
            // 25b2 Phase 2: `_inlinedbaml.py` lives at the SDK root.
            assert!(!s.contains("inlinedbaml.py") || s == "_inlinedbaml.py");
            assert_ne!(s, "runtime.py");
            assert_ne!(s, "config.py");
            assert_ne!(s, "globals.py");
            assert_ne!(s, "tracing.py");
        }
    }

    // ----- 12d `.pyi` stub generation -----

    #[test]
    fn pyi_emitted_for_every_init_py() {
        // Every emitted `__init__.py` (and only those) gets a sibling
        // `__init__.pyi`. `_inlinedbaml.py` and `_typemap.py` (data
        // modules at the SDK root) are the documented exceptions
        // (12d §6, 25b2 Phase 2).
        let mut pool: SymbolPool = HashMap::new();
        let resume = cg_name("user", &["lorem"], "Resume");
        pool.insert(resume.clone(), class(resume));
        let stream = cg_name("user", &["lorem"], "Resume$stream");
        pool.insert(stream.clone(), class(stream));
        let bucket = cg_name("aws", &["s3"], "Bucket");
        pool.insert(bucket.clone(), class(bucket));

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);

        for path in out.keys() {
            let s = path.to_string_lossy();
            if s.ends_with("__init__.py") {
                let pyi: String = s.replace("__init__.py", "__init__.pyi");
                assert!(
                    out.contains_key(&PathBuf::from(&pyi)),
                    "missing .pyi sibling for {s}"
                );
            }
        }
        assert!(!out.contains_key(&PathBuf::from("_inlinedbaml.pyi")));
        assert!(!out.contains_key(&PathBuf::from("_typemap.pyi")));
    }

    #[test]
    fn pyi_class_renders_typed_field_declarations() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Resume");
        pool.insert(
            n.clone(),
            class_with_props(
                n,
                vec![
                    (
                        "name",
                        Ty::String {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                    ),
                    (
                        "email",
                        union(vec![
                            Ty::String {
                                attr: baml_base::TyAttr::EMPTY,
                            },
                            Ty::Null {
                                attr: baml_base::TyAttr::EMPTY,
                            },
                        ]),
                    ),
                ],
                "x.baml",
                0,
            ),
        );

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("lorem/__init__.pyi")];

        // Field declarations are mirrored into the stub so type
        // checkers can see the public Pydantic surface.
        let expected = "class Resume(pydantic.BaseModel):\n\
                        \x20   name: str\n\
                        \x20   email: typing.Optional[str] = None\n";
        assert!(leaf.contains(expected), "pyi missing class body:\n{leaf}");
        // The collapsed `class Foo(...): ...` form must not appear here.
        assert!(!leaf.contains("class Resume(pydantic.BaseModel): ..."));
        // `model_config` is a runtime concern and stays out of the stub.
        assert!(!leaf.contains("model_config"));
        assert!(leaf.contains("import pydantic"));
        // Property type uses `typing.Optional`, so typing is required.
        assert!(leaf.contains("import typing"));
    }

    #[test]
    fn pyi_enum_renders_each_variant() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["ipsum"], "Sentiment");
        pool.insert(
            n.clone(),
            Symbol::Enum(Enum {
                name: n,
                docstring: None,
                variants: vec![
                    EnumVariant {
                        name: BaseName::new("POSITIVE"),
                        docstring: None,
                        value: "POSITIVE".to_string(),
                    },
                    EnumVariant {
                        name: BaseName::new("NEGATIVE"),
                        docstring: None,
                        value: "NEGATIVE".to_string(),
                    },
                ],
                origin: origin("x.baml", 0),
            }),
        );

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("ipsum/__init__.pyi")];

        let expected = "class Sentiment(str, enum.Enum):\n\
                        \x20   POSITIVE = \"POSITIVE\"\n\
                        \x20   NEGATIVE = \"NEGATIVE\"\n";
        assert!(leaf.contains(expected), "pyi missing enum body:\n{leaf}");
        assert!(leaf.contains("import enum"));
    }

    #[test]
    fn pyi_function_signature_typed_sync_and_async() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "extract_resume");
        // Single-arg function returning a class; signature must reflect
        // both ends of the typed surface (12d §3.4).
        let resume = cg_name("user", &["lorem"], "Resume");
        pool.insert(resume.clone(), class(resume.clone()));
        pool.insert(
            n,
            Symbol::Function(Function {
                generic_params: Vec::new(),
                name: BaseName::new("extract_resume"),
                docstring: None,
                arguments: vec![FunctionArgument {
                    injected: false,
                    name: BaseName::new("text"),
                    docstring: None,
                    ty: Ty::String {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                    default: None,
                }],
                return_type: class_ty(resume, vec![]),
                throws: None,
                watchers: vec![],
                origin: origin("x.baml", 100),
            }),
        );

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("lorem/__init__.pyi")];

        assert!(
            leaf.contains("def extract_resume(text: str) -> Resume: ...\n"),
            "missing sync sig in:\n{leaf}"
        );
        assert!(
            leaf.contains("async def extract_resume_async(text: str) -> Resume: ...\n"),
            "missing async sig in:\n{leaf}"
        );
        // No factory call, no `_define_function`.
        assert!(!leaf.contains("_define_function"));
        assert!(!leaf.contains("baml_bridge"));
    }

    #[test]
    fn pyi_type_alias_mirrors_py_shape() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["util"], "Foo");
        pool.insert(n.clone(), alias(n, "x.baml", 0));

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("util/__init__.pyi")];
        // 12d §3.3: type-alias body is identical between `.py` and `.pyi`.
        assert!(leaf.contains("Foo: typing.TypeAlias = int\n"));
    }

    #[test]
    fn pyi_recursive_type_alias_emits_type_alias_type() {
        // `.pyi` mirrors `.py`: recursive aliases render via
        // `typing_extensions.TypeAliasType` (18c).
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["tree"], "JsonValue");
        let rhs = union(vec![
            Ty::Int {
                attr: baml_base::TyAttr::EMPTY,
            },
            Ty::String {
                attr: baml_base::TyAttr::EMPTY,
            },
            list(Box::new(alias_ty(n.clone()))),
        ]);
        pool.insert(n.clone(), alias_full(n, rhs, true, "tree.baml", 0));

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("tree/__init__.pyi")];
        assert!(leaf.contains("import typing_extensions\n"));
        assert!(leaf.contains(
            "JsonValue = typing_extensions.TypeAliasType(\"JsonValue\", typing.Union[int, str, typing.List[\"JsonValue\"]])\n"
        ));
    }

    #[test]
    fn pyi_static_method_includes_decorator_and_typed_signature() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Counter");
        pool.insert(
            n.clone(),
            class_with_methods(
                n,
                vec![method_func("zero", &[], "x.baml", 100)],
                vec![],
                "x.baml",
                0,
            ),
        );

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("lorem/__init__.pyi")];

        // 12b method-bearing class: body is the method block, not `...`.
        assert!(leaf.contains("class Counter(pydantic.BaseModel):\n"));
        assert!(!leaf.contains("class Counter(pydantic.BaseModel): ..."));
        // Each fan-out gets its own @staticmethod decorator.
        assert!(
            leaf.contains("    @staticmethod\n    def zero() -> int: ...\n"),
            "missing static sync sig in:\n{leaf}"
        );
        assert!(
            leaf.contains("    @staticmethod\n    async def zero_async() -> int: ...\n"),
            "missing static async sig in:\n{leaf}"
        );
    }

    #[test]
    fn pyi_instance_method_prepends_self_no_annotation() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Counter");
        pool.insert(
            n.clone(),
            class_with_methods(
                n,
                vec![],
                vec![method_func("bump", &["by"], "x.baml", 100)],
                "x.baml",
                0,
            ),
        );

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("lorem/__init__.pyi")];

        // Instance methods: `self` (no annotation) + typed remaining params.
        assert!(
            leaf.contains("    def bump(self, by: int) -> int: ...\n"),
            "missing instance sync sig in:\n{leaf}"
        );
        assert!(
            leaf.contains("    async def bump_async(self, by: int) -> int: ...\n"),
            "missing instance async sig in:\n{leaf}"
        );
        // No `@staticmethod` decorator on instance methods.
        assert!(!leaf.contains("@staticmethod\n    def bump"));
    }

    #[test]
    fn pyi_property_only_class_has_no_method_block() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Resume");
        pool.insert(n.clone(), class(n));

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("lorem/__init__.pyi")];

        // Field declaration is mirrored, but no method block exists.
        assert!(leaf.contains("class Resume(pydantic.BaseModel):\n    a: int\n"));
        assert!(!leaf.contains("def "));
    }

    #[test]
    fn pyi_all_mirrors_py_all() {
        let mut pool: SymbolPool = HashMap::new();
        let c = cg_name("user", &["lorem"], "Resume");
        let e = cg_name("user", &["lorem"], "Sentiment");
        pool.insert(c.clone(), class_at(c, "x.baml", 0));
        pool.insert(e.clone(), enum_(e, "x.baml", 50));

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let py = &out[&PathBuf::from("lorem/__init__.py")];
        let pyi = &out[&PathBuf::from("lorem/__init__.pyi")];
        assert!(py.contains("__all__ = [\n    \"Resume\",\n    \"Sentiment\",\n]"));
        assert!(pyi.contains("__all__ = [\n    \"Resume\",\n    \"Sentiment\",\n]"));
    }

    #[test]
    fn pyi_typing_imported_for_class_or_signature() {
        // Any class (now that fields are mirrored), function, or alias
        // pulls `import typing` into the `.pyi`. Mirrors the `.py`
        // "be generous" rule so field types like `typing.Optional[…]`
        // and the `typing.Generic[…]` base resolve.
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Resume");
        pool.insert(n.clone(), class(n));

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("lorem/__init__.pyi")];
        assert!(leaf.contains("import typing"), "typing expected:\n{leaf}");

        // Adding a function still results in typing being imported.
        let f = cg_name("user", &["lorem"], "ping");
        pool.insert(f, func_sym("ping", "x.baml", 100));
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("lorem/__init__.pyi")];
        assert!(leaf.contains("import typing"));
    }

    #[test]
    fn pyi_no_factory_imports_anywhere() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "extract_resume");
        pool.insert(n, func_sym("extract_resume", "x.baml", 100));

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        for (path, content) in &out {
            if !path.to_string_lossy().ends_with(".pyi") {
                continue;
            }
            assert!(
                !content.contains("baml_bridge"),
                "{} must not import baml_bridge",
                path.display()
            );
            assert!(
                !content.contains("_define_function"),
                "{} must not reference _define_function",
                path.display()
            );
        }
    }

    // ----- 12f cross-leaf relative-import block -----

    #[test]
    fn cross_leaf_user_user() {
        // lorem leaf references ipsum.Sentiment (sibling user-package
        // leaf). Both `.py` and `.pyi` carry a guarded `from .. import ipsum`.
        let mut pool: SymbolPool = HashMap::new();
        let sentiment = cg_name("user", &["ipsum"], "Sentiment");
        let envelope = cg_name("user", &["lorem"], "Envelope");
        pool.insert(
            sentiment.clone(),
            Symbol::Enum(Enum {
                name: sentiment.clone(),
                docstring: None,
                variants: vec![EnumVariant {
                    name: BaseName::new("POSITIVE"),
                    docstring: None,
                    value: "POSITIVE".to_string(),
                }],
                origin: origin("ipsum.baml", 0),
            }),
        );
        pool.insert(
            envelope.clone(),
            class_with_props(
                envelope,
                vec![("sentiment", enum_ty(sentiment))],
                "lorem.baml",
                0,
            ),
        );

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let py = &out[&PathBuf::from("lorem/__init__.py")];
        // 25b2 Phase 4: cross-leaf Pydantic field-edge import is now
        // unconditional in `.py` (the typemap installs sentinel-resolved
        // classes lazily; field annotations need the name at runtime).
        assert!(
            py.contains("\nfrom .. import ipsum\n"),
            "py missing unconditional ipsum import:\n{py}"
        );
        assert!(
            !py.contains("if typing.TYPE_CHECKING:\n    from .. import ipsum"),
            "py ipsum import should not be under TYPE_CHECKING:\n{py}"
        );
        // The `.pyi` still wraps cross-leaf imports under TYPE_CHECKING
        // — stubs never run, so the guard is harmless and matches the
        // legacy stub shape.
        let pyi = &out[&PathBuf::from("lorem/__init__.pyi")];
        assert!(
            pyi.contains("if typing.TYPE_CHECKING:\n    from .. import ipsum\n"),
            "pyi missing guarded ipsum import:\n{pyi}"
        );
        assert!(pyi.contains("    sentiment: ipsum.Sentiment\n"));
    }

    #[test]
    fn cross_leaf_user_root() {
        // Non-root leaf (`lorem`) references a root-namespace user type
        // (`Foo` declared at `root.baml`). The translator emits the
        // bare name `Foo` thanks to the empty-segment routing
        // shortcut, so the leaf needs `from .. import Foo` to bring
        // the name into scope. Both `.py` and `.pyi` carry a guarded
        // import.
        let mut pool: SymbolPool = HashMap::new();
        let foo = cg_name("user", &[], "Foo");
        let envelope = cg_name("user", &["lorem"], "Envelope");
        pool.insert(foo.clone(), class(foo.clone()));
        pool.insert(
            envelope.clone(),
            class_with_props(
                envelope,
                vec![("inner", class_ty(foo, vec![]))],
                "lorem.baml",
                0,
            ),
        );
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let py = &out[&PathBuf::from("lorem/__init__.py")];
        // 25b2 Phase 4: lifted out of TYPE_CHECKING in `.py`.
        assert!(
            py.contains("\nfrom .. import Foo\n"),
            "py missing unconditional root Foo import:\n{py}"
        );
        assert!(
            !py.contains("if typing.TYPE_CHECKING:\n    from .. import Foo"),
            "py Foo import should not be under TYPE_CHECKING:\n{py}"
        );
        let pyi = &out[&PathBuf::from("lorem/__init__.pyi")];
        assert!(
            pyi.contains("if typing.TYPE_CHECKING:\n    from .. import Foo\n"),
            "pyi missing guarded root Foo import:\n{pyi}"
        );
        assert!(pyi.contains("    inner: Foo\n"));
    }

    #[test]
    fn root_leaf_does_not_self_import_root_types() {
        // Same `Foo` referenced from a same-leaf class on the root
        // leaf — no import should be emitted (it's locally defined).
        let mut pool: SymbolPool = HashMap::new();
        let foo = cg_name("user", &[], "Foo");
        let consumer = cg_name("user", &[], "FooConsumer");
        pool.insert(foo.clone(), class(foo.clone()));
        pool.insert(
            consumer.clone(),
            class_with_props(
                consumer,
                vec![("inner", class_ty(foo, vec![]))],
                "root.baml",
                0,
            ),
        );
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let py = &out[&PathBuf::from("__init__.py")];
        assert!(
            !py.contains("from . import Foo") && !py.contains("from .. import Foo"),
            "root leaf should not import its own Foo:\n{py}"
        );
    }

    #[test]
    fn cross_leaf_user_baml() {
        // lorem leaf references baml.http.Response — needs a runtime
        // `from .. import baml` so the `baml.http.Response` annotation
        // resolves at Pydantic-validation time. The per-package cascade
        // in `baml/__init__.py` binds `http` as an attribute of `baml`.
        let mut pool: SymbolPool = HashMap::new();
        let response = cg_name("baml", &["http"], "Response");
        let envelope = cg_name("user", &["lorem"], "Envelope");
        pool.insert(response.clone(), class(response.clone()));
        pool.insert(
            envelope.clone(),
            class_with_props(
                envelope,
                vec![("resp", class_ty(response, vec![]))],
                "x.baml",
                0,
            ),
        );
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let py = &out[&PathBuf::from("lorem/__init__.py")];
        // Import the top-level segment (`baml`) from the SDK root; the
        // annotation uses the fully-qualified `baml.http.Response`.
        assert!(
            py.contains("\nfrom .. import baml\n"),
            "py missing runtime .. import baml:\n{py}"
        );
        assert!(
            !py.contains("if typing.TYPE_CHECKING:\n    from .. import baml"),
            "baml import should not be under TYPE_CHECKING:\n{py}"
        );
        assert!(py.contains("    resp: baml.http.Response\n"));
    }

    #[test]
    fn cross_leaf_user_vendor() {
        // lorem leaf references aws.s3.Bucket — routes to vendor/aws/s3,
        // first segment is `vendor`. Cascades in `vendor/__init__.py`
        // and `vendor/aws/__init__.py` bind the deeper segments.
        let mut pool: SymbolPool = HashMap::new();
        let bucket = cg_name("aws", &["s3"], "Bucket");
        let envelope = cg_name("user", &["lorem"], "Envelope");
        pool.insert(bucket.clone(), class(bucket.clone()));
        pool.insert(
            envelope.clone(),
            class_with_props(envelope, vec![("b", class_ty(bucket, vec![]))], "x.baml", 0),
        );
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let py = &out[&PathBuf::from("lorem/__init__.py")];
        // Import the top-level segment `vendor` from the SDK root;
        // annotation uses `vendor.aws.s3.Bucket`.
        assert!(
            py.contains("\nfrom .. import vendor\n"),
            "py missing unconditional .. import vendor:\n{py}"
        );
        assert!(
            !py.contains("if typing.TYPE_CHECKING:\n    from .. import vendor"),
            "py vendor import should not be under TYPE_CHECKING:\n{py}"
        );
        assert!(py.contains("    b: vendor.aws.s3.Bucket\n"));
    }

    #[test]
    fn cross_leaf_stream_to_nonstream() {
        // stream_types/lorem leaf references the non-stream Resume —
        // depth 2, three dots: `from ... import lorem`.
        let mut pool: SymbolPool = HashMap::new();
        let non_stream = cg_name("user", &["lorem"], "Resume");
        let stream = cg_name("user", &["lorem"], "Resume$stream");
        pool.insert(
            non_stream.clone(),
            class_with_props(
                non_stream.clone(),
                vec![(
                    "name",
                    Ty::String {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                )],
                "x.baml",
                0,
            ),
        );
        pool.insert(
            stream.clone(),
            class_with_props(
                stream,
                vec![("origin", class_ty(non_stream, vec![]))],
                "x.baml",
                0,
            ),
        );
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let py = &out[&PathBuf::from("stream_types/lorem/__init__.py")];
        // 25b2 Phase 4: lifted out of TYPE_CHECKING in `.py`. Different
        // first segments (stream_types vs lorem) so import stays root-
        // anchored — three dots from depth-2 stream leaf.
        assert!(
            py.contains("\nfrom ... import lorem\n"),
            "py missing unconditional lorem import from stream leaf:\n{py}"
        );
        assert!(
            !py.contains("if typing.TYPE_CHECKING:\n    from ... import lorem"),
            "py lorem import should not be under TYPE_CHECKING:\n{py}"
        );
    }

    #[test]
    fn cross_leaf_deep_stream_vendor() {
        // stream_types/vendor/aws/s3 leaf (depth 4) referencing
        // baml.http.Response. Always anchor at the SDK root and import
        // the top-level segment `baml` — five dots escape the depth-4
        // leaf to the SDK root.
        let mut pool: SymbolPool = HashMap::new();
        let response = cg_name("baml", &["http"], "Response");
        let stream_bucket = cg_name("aws", &["s3"], "Bucket$stream");
        pool.insert(response.clone(), class(response.clone()));
        pool.insert(
            stream_bucket.clone(),
            class_with_props(
                stream_bucket,
                vec![("resp", class_ty(response, vec![]))],
                "x.baml",
                0,
            ),
        );
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let py = &out[&PathBuf::from("stream_types/vendor/aws/s3/__init__.py")];
        assert!(
            py.contains("\nfrom ..... import baml\n"),
            "py missing five-dot import of baml:\n{py}"
        );
        assert!(
            !py.contains("if typing.TYPE_CHECKING:\n    from ..... import baml"),
            "baml should not be under TYPE_CHECKING:\n{py}"
        );
        assert!(py.contains("    resp: baml.http.Response\n"));
    }

    #[test]
    fn import_block_one_line_per_segment() {
        // A leaf with multiple cross-leaf first-segments emits one
        // `from <dots> import <name>` line per top-level segment,
        // never the comma-joined form.
        let mut pool: SymbolPool = HashMap::new();
        let resume = cg_name("user", &["lorem"], "Resume");
        let sentiment = cg_name("user", &["ipsum"], "Sentiment");
        let bucket = cg_name("aws", &["s3"], "Bucket");
        let response = cg_name("baml", &["http"], "Response");
        pool.insert(
            sentiment.clone(),
            Symbol::Enum(Enum {
                name: sentiment.clone(),
                docstring: None,
                variants: vec![EnumVariant {
                    name: BaseName::new("X"),
                    docstring: None,
                    value: "X".to_string(),
                }],
                origin: origin("x.baml", 0),
            }),
        );
        pool.insert(bucket.clone(), class(bucket.clone()));
        pool.insert(response.clone(), class(response.clone()));
        pool.insert(
            resume.clone(),
            class_with_props(
                resume,
                vec![
                    ("s", enum_ty(sentiment)),
                    ("b", class_ty(bucket, vec![])),
                    ("r", class_ty(response, vec![])),
                ],
                "x.baml",
                0,
            ),
        );
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let py = &out[&PathBuf::from("lorem/__init__.py")];
        // Every cross-leaf field-edge import lifts to a runtime
        // `from <root_dots> import <top_seg>` line — no TYPE_CHECKING
        // wrapping. Each top-level segment renders on its own line.
        assert!(
            py.contains("\nfrom .. import baml\n"),
            "py missing runtime .. import baml:\n{py}"
        );
        assert!(
            py.contains("\nfrom .. import ipsum\n"),
            "py missing runtime .. import ipsum:\n{py}"
        );
        assert!(
            py.contains("\nfrom .. import vendor\n"),
            "py missing runtime .. import vendor:\n{py}"
        );
        assert!(
            !py.contains("if typing.TYPE_CHECKING:"),
            "py field-edge imports should not be under TYPE_CHECKING:\n{py}"
        );
        // Never comma-joined.
        assert!(!py.contains("from .. import baml,"));
        assert!(!py.contains("from .. import ipsum, vendor"));
    }

    #[test]
    fn import_block_dedups_within_segment() {
        // Two refs whose routed leaves share the top-level segment
        // `vendor` collapse to a single `from .. import vendor` line.
        // The dotted form `vendor.aws.s3.Bucket` / `vendor.gcp.gcs.Object`
        // distinguishes them in the annotation.
        let mut pool: SymbolPool = HashMap::new();
        let s3_bucket = cg_name("aws", &["s3"], "Bucket");
        let gcs_object = cg_name("gcp", &["gcs"], "Object");
        let resume = cg_name("user", &["lorem"], "Resume");
        pool.insert(s3_bucket.clone(), class(s3_bucket.clone()));
        pool.insert(gcs_object.clone(), class(gcs_object.clone()));
        pool.insert(
            resume.clone(),
            class_with_props(
                resume,
                vec![
                    ("a", class_ty(s3_bucket, vec![])),
                    ("b", class_ty(gcs_object, vec![])),
                ],
                "x.baml",
                0,
            ),
        );
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let py = &out[&PathBuf::from("lorem/__init__.py")];
        assert_eq!(
            py.matches("from .. import vendor").count(),
            1,
            "expected exactly one `from .. import vendor`:\n{py}"
        );
        assert!(py.contains("    a: vendor.aws.s3.Bucket\n"));
        assert!(py.contains("    b: vendor.gcp.gcs.Object\n"));
    }

    #[test]
    fn same_leaf_reference_emits_no_import() {
        // A class field of type `Resume` in the same leaf doesn't trigger
        // any cross-leaf import or TYPE_CHECKING block.
        let mut pool: SymbolPool = HashMap::new();
        let resume = cg_name("user", &["lorem"], "Resume");
        let other = cg_name("user", &["lorem"], "Other");
        pool.insert(resume.clone(), class(resume.clone()));
        pool.insert(
            other.clone(),
            class_with_props(other, vec![("r", class_ty(resume, vec![]))], "x.baml", 100),
        );
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let py = &out[&PathBuf::from("lorem/__init__.py")];
        assert!(!py.contains("if typing.TYPE_CHECKING:"));
        assert!(!py.contains("from .."));
        let pyi = &out[&PathBuf::from("lorem/__init__.pyi")];
        assert!(!pyi.contains("if typing.TYPE_CHECKING:"));
    }

    #[test]
    fn cross_leaf_via_function_signature_in_pyi() {
        // Leaf with only a function whose param/return crosses leaves —
        // .pyi must carry the TYPE_CHECKING block (signatures render
        // types). The .py side gets the same block defensively.
        let mut pool: SymbolPool = HashMap::new();
        let sentiment = cg_name("user", &["ipsum"], "Sentiment");
        let func = cg_name("user", &["lorem"], "classify");
        pool.insert(
            sentiment.clone(),
            Symbol::Enum(Enum {
                name: sentiment.clone(),
                docstring: None,
                variants: vec![EnumVariant {
                    name: BaseName::new("X"),
                    docstring: None,
                    value: "X".to_string(),
                }],
                origin: origin("x.baml", 0),
            }),
        );
        pool.insert(
            func,
            Symbol::Function(Function {
                generic_params: Vec::new(),
                name: BaseName::new("classify"),
                docstring: None,
                arguments: vec![FunctionArgument {
                    injected: false,
                    name: BaseName::new("text"),
                    docstring: None,
                    ty: Ty::String {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                    default: None,
                }],
                return_type: enum_ty(sentiment),
                throws: None,
                watchers: vec![],
                origin: origin("x.baml", 100),
            }),
        );
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let pyi = &out[&PathBuf::from("lorem/__init__.pyi")];
        assert!(
            pyi.contains("if typing.TYPE_CHECKING:\n    from .. import ipsum\n"),
            "pyi missing guarded ipsum import:\n{pyi}"
        );
        assert!(pyi.contains("def classify(text: str) -> ipsum.Sentiment: ..."));

        // .py for a function-only leaf — typing must still be imported
        // (the TYPE_CHECKING block needs it).
        let py = &out[&PathBuf::from("lorem/__init__.py")];
        assert!(py.contains("import typing\n"));
        assert!(py.contains("if typing.TYPE_CHECKING:\n    from .. import ipsum\n"));
    }

    #[test]
    fn generic_class_emits_typevar_and_generic_base() {
        let mut pool: SymbolPool = HashMap::new();
        let box_name = cg_name("user", &["lorem"], "Box");
        let crate_name = cg_name("user", &["lorem"], "Crate");

        pool.insert(
            box_name.clone(),
            Symbol::Class(Class {
                name: box_name,
                generic_params: vec![BaseName::new("T")],
                docstring: None,
                properties: vec![ClassProperty {
                    name: BaseName::new("item"),
                    docstring: None,
                    ty: type_var(BaseName::new("T")),
                }],
                static_methods: vec![],
                instance_methods: vec![],
                origin: origin("box.baml", 0),
            }),
        );
        pool.insert(
            crate_name.clone(),
            Symbol::Class(Class {
                name: crate_name,
                generic_params: vec![BaseName::new("T")],
                docstring: None,
                properties: vec![ClassProperty {
                    name: BaseName::new("contents"),
                    docstring: None,
                    ty: list(Box::new(class_ty(
                        cg_name("user", &["lorem"], "Box"),
                        vec![type_var(BaseName::new("T"))],
                    ))),
                }],
                static_methods: vec![],
                instance_methods: vec![],
                origin: origin("box.baml", 100),
            }),
        );

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        assert!(
            leaf.contains("T = typing.TypeVar(\"T\")"),
            "missing TypeVar declaration:\n{leaf}",
        );
        assert!(
            leaf.contains("class Box(pydantic.BaseModel, typing.Generic[T]):"),
            "missing Generic[T] base on Box:\n{leaf}",
        );
        assert!(
            leaf.contains("    item: T"),
            "missing T-typed field on Box:\n{leaf}",
        );
        assert!(
            leaf.contains("class Crate(pydantic.BaseModel, typing.Generic[T]):"),
            "missing Generic[T] base on Crate:\n{leaf}",
        );
        assert!(
            leaf.contains("    contents: typing.List[Box[T]]"),
            "missing nested generic ref Box[T]:\n{leaf}",
        );
        assert_eq!(
            leaf.matches(
                "    model_config = pydantic.ConfigDict(\n        arbitrary_types_allowed=True,\n        extra=\"ignore\",\n        populate_by_name=True,\n    )"
            )
            .count(),
            2,
            "every generated generic model should ignore extra fields:\n{leaf}",
        );
        assert!(
            !leaf.contains("extra=\"forbid\""),
            "generated generic models must not forbid extra fields:\n{leaf}",
        );

        // TypeVar declaration appears once.
        assert_eq!(
            leaf.matches("T = typing.TypeVar(\"T\")").count(),
            1,
            "TypeVar should be declared exactly once:\n{leaf}"
        );

        // The .pyi mirrors the same generic shape, including the
        // typed field declaration.
        let pyi = &out[&PathBuf::from("lorem/__init__.pyi")];
        assert!(
            pyi.contains("T = typing.TypeVar(\"T\")"),
            "pyi missing TypeVar:\n{pyi}",
        );
        assert!(
            pyi.contains("class Box(pydantic.BaseModel, typing.Generic[T]):\n    item: T\n"),
            "pyi missing Generic[T] on Box:\n{pyi}",
        );
        assert!(
            pyi.contains(
                "class Crate(pydantic.BaseModel, typing.Generic[T]):\n    contents: typing.List[Box[T]]\n",
            ),
            "pyi missing typed Crate body:\n{pyi}",
        );
        assert!(
            !pyi.contains("model_config"),
            "runtime Pydantic configuration should not be redeclared in stubs:\n{pyi}",
        );
    }

    /// 13a §4.4 — a generic free function emits its `TypeVar`s at the leaf
    /// and the .pyi signature uses bare `TypeVar` identifiers.
    #[test]
    fn generic_function_emits_typevar_at_leaf() {
        let mut pool: SymbolPool = HashMap::new();
        let key = cg_name("user", &["lorem"], "echo");
        pool.insert(
            key,
            Symbol::Function(Function {
                name: BaseName::new("echo"),
                generic_params: vec![BaseName::new("T")],
                docstring: None,
                arguments: vec![FunctionArgument {
                    injected: false,
                    name: BaseName::new("value"),
                    docstring: None,
                    ty: type_var(BaseName::new("T")),
                    default: None,
                }],
                return_type: type_var(BaseName::new("T")),
                throws: None,
                watchers: vec![],
                origin: origin("echo.baml", 0),
            }),
        );
        pool.insert(
            cg_name("user", &["lorem"], "one_type_arg"),
            Symbol::Function(Function {
                name: BaseName::new("one_type_arg"),
                generic_params: vec![BaseName::new("T")],
                docstring: None,
                arguments: vec![],
                return_type: Ty::String {
                    attr: baml_base::TyAttr::EMPTY,
                },
                throws: None,
                watchers: vec![],
                origin: origin("echo.baml", 100),
            }),
        );
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let pyi = &out[&PathBuf::from("lorem/__init__.pyi")];
        assert!(
            pyi.contains("T = typing.TypeVar(\"T\")"),
            "pyi missing TypeVar:\n{pyi}",
        );
        // Required value arguments make `_types=` optional.
        assert!(
            pyi.contains(
                "def echo(value: T, *, _types: dict[str, typing.Any] | None = None) -> T: ..."
            ),
            "pyi missing typed echo signature:\n{pyi}",
        );
        assert!(
            pyi.contains(
                "async def echo_async(value: T, *, _types: dict[str, typing.Any] | None = None) -> T: ..."
            ),
            "pyi missing async echo signature:\n{pyi}",
        );
        // A body-only TypeVar has no inference source, so `_types=` stays
        // statically required on both host modes.
        assert!(
            pyi.contains("def one_type_arg(*, _types: dict[str, typing.Any]) -> str: ..."),
            "pyi should require body-only TypeVars:\n{pyi}",
        );
        assert!(
            pyi.contains(
                "async def one_type_arg_async(*, _types: dict[str, typing.Any]) -> str: ..."
            ),
            "pyi should require async body-only TypeVars:\n{pyi}",
        );
    }

    #[test]
    fn generic_function_types_kwarg_tracks_engine_inference_sources() {
        let mut pool: SymbolPool = HashMap::new();
        let default_label = || FunctionArgument {
            injected: false,
            name: BaseName::new("label"),
            docstring: None,
            ty: Ty::String {
                attr: baml_base::TyAttr::EMPTY,
            },
            default: Some(FunctionArgumentDefault::Literal(DefaultLiteral::Scalar(
                baml_base::Literal::String("default".to_string()),
            ))),
        };

        pool.insert(
            cg_name("user", &["lorem"], "identity_with_default"),
            Symbol::Function(Function {
                name: BaseName::new("identity_with_default"),
                generic_params: vec![BaseName::new("T")],
                docstring: None,
                arguments: vec![
                    FunctionArgument {
                        injected: false,
                        name: BaseName::new("value"),
                        docstring: None,
                        ty: type_var(BaseName::new("T")),
                        default: None,
                    },
                    default_label(),
                ],
                return_type: type_var(BaseName::new("T")),
                throws: None,
                watchers: vec![],
                origin: origin("generic.baml", 0),
            }),
        );
        pool.insert(
            cg_name("user", &["lorem"], "optional_only"),
            Symbol::Function(Function {
                name: BaseName::new("optional_only"),
                generic_params: vec![BaseName::new("T")],
                docstring: None,
                arguments: vec![FunctionArgument {
                    injected: false,
                    name: BaseName::new("value"),
                    docstring: None,
                    ty: union(vec![
                        type_var(BaseName::new("T")),
                        Ty::Null {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                    ]),
                    default: Some(FunctionArgumentDefault::Null),
                }],
                return_type: type_var(BaseName::new("T")),
                throws: None,
                watchers: vec![],
                origin: origin("generic.baml", 100),
            }),
        );
        pool.insert(
            cg_name("user", &["lorem"], "apply"),
            Symbol::Function(Function {
                name: BaseName::new("apply"),
                generic_params: vec![BaseName::new("T"), BaseName::new("R")],
                docstring: None,
                arguments: vec![
                    FunctionArgument {
                        injected: false,
                        name: BaseName::new("callback"),
                        docstring: None,
                        ty: Ty::Function {
                            params: Box::new([baml_codegen_types::CallableParam::required(
                                None,
                                type_var(BaseName::new("T")),
                            )]),
                            ret: Box::new(type_var(BaseName::new("R"))),
                            throws: Box::new(Ty::Never {
                                attr: baml_base::TyAttr::EMPTY,
                            }),
                            attr: baml_base::TyAttr::EMPTY,
                        },
                        default: None,
                    },
                    FunctionArgument {
                        injected: false,
                        name: BaseName::new("value"),
                        docstring: None,
                        ty: type_var(BaseName::new("T")),
                        default: None,
                    },
                ],
                return_type: type_var(BaseName::new("R")),
                throws: None,
                watchers: vec![],
                origin: origin("generic.baml", 200),
            }),
        );
        pool.insert(
            cg_name("user", &["lorem"], "ambiguous"),
            Symbol::Function(Function {
                name: BaseName::new("ambiguous"),
                generic_params: vec![BaseName::new("T"), BaseName::new("U")],
                docstring: None,
                arguments: vec![FunctionArgument {
                    injected: false,
                    name: BaseName::new("value"),
                    docstring: None,
                    ty: union(vec![
                        type_var(BaseName::new("T")),
                        type_var(BaseName::new("U")),
                        Ty::Int {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                    ]),
                    default: None,
                }],
                return_type: Ty::String {
                    attr: baml_base::TyAttr::EMPTY,
                },
                throws: None,
                watchers: vec![],
                origin: origin("generic.baml", 300),
            }),
        );
        pool.insert(
            cg_name("user", &["lorem"], "ambiguous_with_values"),
            Symbol::Function(Function {
                name: BaseName::new("ambiguous_with_values"),
                generic_params: vec![BaseName::new("T"), BaseName::new("U")],
                docstring: None,
                arguments: vec![
                    FunctionArgument {
                        injected: false,
                        name: BaseName::new("ambiguous"),
                        docstring: None,
                        ty: union(vec![
                            type_var(BaseName::new("T")),
                            type_var(BaseName::new("U")),
                            Ty::Int {
                                attr: baml_base::TyAttr::EMPTY,
                            },
                        ]),
                        default: None,
                    },
                    FunctionArgument {
                        injected: false,
                        name: BaseName::new("left"),
                        docstring: None,
                        ty: type_var(BaseName::new("T")),
                        default: None,
                    },
                    FunctionArgument {
                        injected: false,
                        name: BaseName::new("right"),
                        docstring: None,
                        ty: type_var(BaseName::new("U")),
                        default: None,
                    },
                ],
                return_type: Ty::String {
                    attr: baml_base::TyAttr::EMPTY,
                },
                throws: None,
                watchers: vec![],
                origin: origin("generic.baml", 400),
            }),
        );

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let pyi = &out[&PathBuf::from("lorem/__init__.pyi")];
        let inferred = pyi
            .lines()
            .find(|line| line.starts_with("def identity_with_default("))
            .expect("identity_with_default stub");
        assert!(
            inferred.contains("_types: dict[str, typing.Any] | None = None"),
            "a required value should infer T after defaulted args: {inferred}"
        );
        assert!(
            inferred.find("label:").unwrap() < inferred.find("_types:").unwrap(),
            "`_types` must follow existing keyword-only defaults: {inferred}"
        );

        let optional_only = pyi
            .lines()
            .find(|line| line.starts_with("def optional_only("))
            .expect("optional_only stub");
        assert!(
            optional_only.contains("_types: dict[str, typing.Any] | None = None"),
            "a defaulted value position should infer T via Rule 4: {optional_only}"
        );
        assert!(
            optional_only.find("value:").unwrap() < optional_only.find("_types:").unwrap(),
            "`_types` must follow the defaulted value parameter: {optional_only}"
        );

        let rich = pyi
            .lines()
            .find(|line| line.starts_with("def ambiguous_with_values("))
            .expect("ambiguous_with_values stub");
        assert!(
            rich.contains("_types: dict[str, typing.Any] | None = None"),
            "separate ordinary value positions should bind both union vars: {rich}"
        );

        for name in ["apply", "ambiguous"] {
            let prefix = format!("def {name}(");
            let line = pyi
                .lines()
                .find(|line| line.starts_with(&prefix))
                .unwrap_or_else(|| panic!("missing {name} stub:\n{pyi}"));
            assert!(
                line.contains("_types: dict[str, typing.Any]"),
                "{name} should expose `_types`: {line}"
            );
            assert!(
                !line.contains("_types: dict[str, typing.Any] | None"),
                "{name} must require `_types`: {line}"
            );
        }
    }

    #[test]
    fn generic_method_types_kwarg_is_optional_in_stub() {
        let mut pool: SymbolPool = HashMap::new();
        let box_name = cg_name("user", &["lorem"], "Box");
        let pair_method = Function {
            name: BaseName::new("pair_with"),
            generic_params: vec![BaseName::new("U")],
            docstring: None,
            arguments: vec![FunctionArgument {
                injected: false,
                name: BaseName::new("other"),
                docstring: None,
                ty: type_var(BaseName::new("U")),
                default: None,
            }],
            return_type: type_var(BaseName::new("U")),
            throws: None,
            watchers: vec![],
            origin: origin("box.baml", 10),
        };
        let pair_with_default = Function {
            name: BaseName::new("pair_with_default"),
            generic_params: vec![BaseName::new("U")],
            docstring: None,
            arguments: vec![
                FunctionArgument {
                    injected: false,
                    name: BaseName::new("other"),
                    docstring: None,
                    ty: type_var(BaseName::new("U")),
                    default: None,
                },
                FunctionArgument {
                    injected: false,
                    name: BaseName::new("label"),
                    docstring: None,
                    ty: Ty::String {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                    default: Some(FunctionArgumentDefault::Literal(DefaultLiteral::Scalar(
                        baml_base::Literal::String("default".to_string()),
                    ))),
                },
            ],
            return_type: type_var(BaseName::new("U")),
            throws: None,
            watchers: vec![],
            origin: origin("box.baml", 20),
        };
        let static_type_name = Function {
            name: BaseName::new("static_type_name"),
            generic_params: vec![BaseName::new("V")],
            docstring: None,
            arguments: vec![],
            return_type: Ty::String {
                attr: baml_base::TyAttr::EMPTY,
            },
            throws: None,
            watchers: vec![],
            origin: origin("box.baml", 30),
        };
        let static_with_default = Function {
            name: BaseName::new("static_with_default"),
            generic_params: vec![BaseName::new("V")],
            docstring: None,
            arguments: vec![
                FunctionArgument {
                    injected: false,
                    name: BaseName::new("value"),
                    docstring: None,
                    ty: type_var(BaseName::new("V")),
                    default: None,
                },
                FunctionArgument {
                    injected: false,
                    name: BaseName::new("label"),
                    docstring: None,
                    ty: Ty::String {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                    default: Some(FunctionArgumentDefault::Literal(DefaultLiteral::Scalar(
                        baml_base::Literal::String("default".to_string()),
                    ))),
                },
            ],
            return_type: type_var(BaseName::new("V")),
            throws: None,
            watchers: vec![],
            origin: origin("box.baml", 40),
        };
        pool.insert(
            box_name.clone(),
            Symbol::Class(Class {
                name: box_name,
                generic_params: vec![BaseName::new("T")],
                docstring: None,
                properties: vec![ClassProperty {
                    name: BaseName::new("item"),
                    docstring: None,
                    ty: type_var(BaseName::new("T")),
                }],
                static_methods: vec![static_type_name, static_with_default],
                instance_methods: vec![pair_method, pair_with_default],
                origin: origin("box.baml", 0),
            }),
        );

        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let pyi = &out[&PathBuf::from("lorem/__init__.pyi")];
        assert!(
            pyi.contains(
                "def pair_with(self, other: U, *, _types: dict[str, typing.Any] | None = None) -> U: ..."
            ),
            "pyi should allow inferred method TypeVars:\n{pyi}",
        );
        assert!(
            pyi.contains(
                "async def pair_with_async(self, other: U, *, _types: dict[str, typing.Any] | None = None) -> U: ..."
            ),
            "pyi should allow inferred async method TypeVars:\n{pyi}",
        );
        assert!(
            pyi.contains(
                "def pair_with_default(self, other: U, *, label: typing.Union[str, UNSET] = \"default\", _types: dict[str, typing.Any] | None = None) -> U: ..."
            ),
            "pyi should keep inferred `_types` after instance defaults:\n{pyi}",
        );
        assert!(
            pyi.contains(
                "def static_with_default(value: V, *, label: typing.Union[str, UNSET] = \"default\", _types: dict[str, typing.Any] | None = None) -> V: ..."
            ),
            "pyi should keep inferred `_types` after static defaults:\n{pyi}",
        );
        assert!(
            pyi.contains("def static_type_name(*, _types: dict[str, typing.Any]) -> str: ..."),
            "zero-arg static own generics should require `_types`:\n{pyi}",
        );
        assert!(
            pyi.contains(
                "async def static_type_name_async(*, _types: dict[str, typing.Any]) -> str: ..."
            ),
            "zero-arg async static own generics should require `_types`:\n{pyi}",
        );
        assert!(
            !pyi.contains("static_type_name(,"),
            "zero-arg static signatures must not start with a comma:\n{pyi}",
        );
    }

    #[test]
    fn public_interface_type_emits_erased_runtime_token() {
        let interface = cg_name("user", &[], "Named");
        let mut function = bare_func("read_name", "main.baml", 0);
        function.arguments[0].ty = Ty::Interface(
            interface,
            Box::new([]),
            Box::new([]),
            baml_base::TyAttr::EMPTY,
        );
        let mut pool = SymbolPool::new();
        pool.insert(
            cg_name("user", &[], "read_name"),
            Symbol::Function(function),
        );
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let py = &out[&PathBuf::from("__init__.py")];
        let pyi = &out[&PathBuf::from("__init__.pyi")];
        assert!(py.contains("class Named:"));
        assert!(py.contains("__baml_interface_fqn__ = \"user.Named\""));
        assert!(py.contains("__all__.extend([\"Named\"])"));
        assert!(py.contains("BAML interface tokens cannot be instantiated"));
        assert!(pyi.contains("class Named:"));
        assert!(pyi.contains("def __class_getitem__"));
    }

    // ── 25b Phase 2: ClassVar + _register_* trailer emission ──────────────

    #[test]
    fn root_init_no_longer_passes_sdk_root() {
        // sdk_root is deleted in Phase 5; codegen drops the kwarg now (no
        // runtime consumer, no diagnostic value worth a separate argument).
        let pool: SymbolPool = HashMap::new();
        let out = to_source_code(&pool, &[], NamingConvention::PreserveCase);
        let root = &out[&PathBuf::from("__init__.py")];
        assert!(
            root.contains(
                "BamlRuntime.initialize_runtime(\n    \
                 \"baml_src\", _inlinedbaml.FILES\n)"
            ),
            "root was:\n{root}"
        );
        assert!(
            !root.contains("sdk_root="),
            "root still passes sdk_root: {root}"
        );
    }

    #[test]
    fn python_identifier_projection_preserves_wire_names_and_reports_once() {
        let keyword_name = cg_name("user", &[], "None");
        let collision_name = cg_name("user", &[], "None_");
        let mut pool = SymbolPool::new();
        pool.insert(
            keyword_name.clone(),
            Symbol::Class(Class {
                name: keyword_name.clone(),
                generic_params: Vec::new(),
                docstring: None,
                properties: [
                    "None",
                    "None_",
                    "foo-bar",
                    "foo_bar",
                    "_secret",
                    "field_secret",
                    "model_config",
                ]
                .into_iter()
                .map(|name| ClassProperty {
                    name: BaseName::new(name),
                    docstring: None,
                    ty: Ty::Int {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                })
                .collect(),
                static_methods: Vec::new(),
                instance_methods: Vec::new(),
                origin: origin("keywords.baml", 0),
            }),
        );
        pool.insert(
            collision_name.clone(),
            Symbol::Class(Class {
                name: collision_name,
                generic_params: Vec::new(),
                docstring: None,
                properties: Vec::new(),
                static_methods: Vec::new(),
                instance_methods: Vec::new(),
                origin: origin("keywords.baml", 1),
            }),
        );

        let enum_name = cg_name("user", &[], "Choice");
        pool.insert(
            enum_name.clone(),
            Symbol::Enum(Enum {
                name: enum_name,
                docstring: None,
                variants: vec![
                    EnumVariant {
                        name: BaseName::new("None"),
                        docstring: None,
                        value: "None".to_string(),
                    },
                    EnumVariant {
                        name: BaseName::new("None_"),
                        docstring: None,
                        value: "None_".to_string(),
                    },
                ],
                origin: origin("keywords.baml", 2),
            }),
        );

        let mut function = bare_func("from", "keywords.baml", 3);
        function.arguments = ["class", "class_", "_types"]
            .into_iter()
            .map(|name| FunctionArgument {
                injected: false,
                name: BaseName::new(name),
                docstring: None,
                ty: class_ty(keyword_name.clone(), Vec::new()),
                default: None,
            })
            .collect();
        function.return_type = class_ty(keyword_name, Vec::new());
        pool.insert(cg_name("user", &[], "from"), Symbol::Function(function));

        let generated = to_source_code_internal(
            &pool,
            RuntimePayload::SourceFiles(&[]),
            NamingConvention::PreserveCase,
        );
        let py = &generated.files[&PathBuf::from("__init__.py")];
        let pyi = &generated.files[&PathBuf::from("__init__.pyi")];

        assert!(py.contains("class None__(pydantic.BaseModel):"), "{py}");
        assert!(py.contains("class None_(pydantic.BaseModel):"), "{py}");
        assert!(
            py.contains("None__: int = pydantic.Field(alias=\"None\")"),
            "{py}"
        );
        assert!(
            py.contains("foo_bar_: int = pydantic.Field(alias=\"foo-bar\")"),
            "{py}"
        );
        assert!(
            py.contains("field_secret_: int = pydantic.Field(alias=\"_secret\")"),
            "{py}"
        );
        assert!(py.contains("None__ = \"None\""), "{py}");
        assert!(py.contains("from_       = _define_function"), "{py}");
        assert!(
            py.contains(
                "binding_name=\"from_\", binding_qualname=\"from_\", binding_module=__name__"
            ),
            "{py}"
        );
        assert!(
            py.contains("param_aliases={\"class__\": \"class\", \"_types_\": \"_types\"}"),
            "{py}"
        );
        assert!(
            pyi.contains("def from_(class__: None__, class_: None__, _types_: None__)")
                && pyi.contains("-> None__"),
            "{pyi}"
        );

        let logical: BTreeSet<_> = generated
            .renames
            .iter()
            .map(|rename| (&rename.kind, &rename.fqn))
            .collect();
        assert_eq!(logical.len(), generated.renames.len());
        assert!(generated.renames.iter().any(|rename| {
            rename.kind == "class"
                && rename.original == "None"
                && rename.generated == "None__"
                && rename.reason == IdentifierRenameReason::Collision
        }));
        assert!(generated.renames.iter().any(|rename| {
            rename.kind == "enum variant"
                && rename.original == "None"
                && rename.generated == "None__"
        }));
    }
}
