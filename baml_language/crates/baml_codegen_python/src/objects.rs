use crate::{
    docstring::{DocString, PyString},
    ty::{Name, Namespace, RenderCtx, Ty},
};

macro_rules! impl_eq_ord_by_name {
    ($($ty:ty),* $(,)?) => {
        $(
            impl PartialEq for $ty {
                fn eq(&self, other: &Self) -> bool {
                    self.name == other.name
                }
            }
            impl Eq for $ty {}
            impl PartialOrd for $ty {
                fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                    Some(self.cmp(other))
                }
            }
            impl Ord for $ty {
                fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                    self.name.cmp(&other.name)
                }
            }
        )*
    };
}

impl_eq_ord_by_name!(Class, Enum, TypeAlias, Function);

/// Ordering: Enum < Class < `TypeAlias` (enums first, type aliases last)
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Object {
    // Enums first, then classes, then type aliases
    // This is important for the code-gen to be able to generate the correct imports
    Enum(Enum),
    Class(Class),
    TypeAlias(TypeAlias),
}

pub(crate) struct Class {
    name: Name,
    docstring: Option<DocString>,
    properties: Vec<ClassProperty>,
}

pub(crate) struct ClassProperty {
    name: baml_base::Name,
    docstring: Option<DocString>,
    ty: Ty,
}

impl Class {
    pub(crate) fn from_codegen_types(class: &baml_codegen_types::Class) -> Self {
        Self {
            name: Name::from_codegen_types(&class.name),
            docstring: class.docstring.as_ref().map(DocString::new),
            properties: class
                .properties
                .iter()
                .map(ClassProperty::from_codegen_types)
                .collect(),
        }
    }
}

impl ClassProperty {
    pub(crate) fn from_codegen_types(property: &baml_codegen_types::ClassProperty) -> Self {
        Self {
            name: property.name.clone(),
            docstring: property.docstring.as_ref().map(DocString::new),
            ty: Ty::from_codegen_types(&property.ty),
        }
    }
}

pub(crate) struct Enum {
    name: Name,
    docstring: Option<DocString>,
    variants: Vec<EnumVariant>,
}

impl Enum {
    pub(crate) fn from_codegen_types(enum_: &baml_codegen_types::Enum) -> Self {
        Self {
            name: Name::from_codegen_types(&enum_.name),
            docstring: enum_.docstring.as_ref().map(DocString::new),
            variants: enum_
                .variants
                .iter()
                .map(EnumVariant::from_codegen_types)
                .collect(),
        }
    }
}

pub(crate) struct EnumVariant {
    name: baml_base::Name,
    docstring: Option<DocString>,
    value: PyString,
}

impl EnumVariant {
    pub(crate) fn from_codegen_types(variant: &baml_codegen_types::EnumVariant) -> Self {
        Self {
            name: variant.name.clone(),
            docstring: variant.docstring.as_ref().map(DocString::new),
            value: PyString::new(&variant.value),
        }
    }
}

pub(crate) struct TypeAlias {
    name: Name,
    resolves_to: Ty,
    recursive: bool,
}

impl TypeAlias {
    pub(crate) fn from_codegen_types(type_alias: &baml_codegen_types::TypeAlias) -> Self {
        Self {
            name: Name::from_codegen_types(&type_alias.name),
            resolves_to: Ty::from_codegen_types(&type_alias.resolves_to),
            recursive: type_alias.recursive,
        }
    }

    /// Returns true if this is a recursive type alias.
    pub(crate) fn is_recursive(&self) -> bool {
        self.recursive
    }

    /// Render the alias name (LHS).
    pub(crate) fn render_name(&self, ns: Namespace) -> String {
        self.name.render(ns)
    }

    /// Render the RHS of the TypeAlias definition.
    ///
    /// Self-references are quoted to avoid `NameError` at module load time.
    pub(crate) fn render_rhs(&self, ns: Namespace) -> String {
        let alias_rendered = self.name.render(ns);
        let ctx = RenderCtx::for_type_alias(ns, alias_rendered);
        self.resolves_to.render_with_ctx(&ctx)
    }
}

/// Discriminates how a namespace path maps to the output directory tree.
///
/// - `Root`: no path — objects go in the root `baml_types/__init__.py`.
/// - `Baml`: path starts with `baml` — objects go in `baml_types/baml/<rest>/`.
/// - `Vendor`: any other path — objects go in `baml_types/<path>/`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum NsLayout {
    /// No path — objects belong in the root `__init__.py`.
    Root,
    /// Path starts with `baml` — `baml_types/baml/<rest>/`.
    Baml,
    /// Any other named path — `baml_types/<path>/`.
    Vendor,
}

/// A group of objects that share the same output directory.
pub(crate) struct NamespaceGroup {
    /// The filesystem path of the directory under `baml_types/` (or `baml_stream_types/`).
    /// For root objects this is empty (they go in the package root `__init__.py`).
    pub(crate) fs_path: Vec<String>,
    /// The Python import path under `baml_types` / `baml_stream_types`.
    /// Reserved for future phases (e.g. Phase 4 function routing).
    #[allow(dead_code)]
    pub(crate) public_path: Vec<String>,
    /// The objects belonging to this group.
    pub(crate) objects: Vec<Object>,
    /// Whether this group is from the stream-types namespace.
    pub(crate) is_stream: bool,
    /// Layout discriminator. Reserved for future phases.
    #[allow(dead_code)]
    pub(crate) layout: NsLayout,
}

impl Object {
    /// Partition all type-namespace objects into namespace groups.
    ///
    /// Each distinct path (from `Namespace::Types { path }`) becomes a `NamespaceGroup`.
    /// Objects with empty paths go into a `Root` group.
    pub(crate) fn load_type_groups(
        objects: &baml_codegen_types::ObjectPool,
        is_stream: bool,
    ) -> Vec<NamespaceGroup> {
        use std::collections::BTreeMap;

        // Collect objects into per-path buckets.
        let mut buckets: BTreeMap<Vec<String>, Vec<Object>> = BTreeMap::new();

        let iter = objects.iter().filter(|(name, _)| {
            if is_stream {
                matches!(
                    name.namespace,
                    baml_codegen_types::Namespace::StreamTypes { .. }
                )
            } else {
                matches!(name.namespace, baml_codegen_types::Namespace::Types { .. })
            }
        });

        for (_, object) in iter {
            let converted = match object {
                baml_codegen_types::Object::Function(_) => continue,
                baml_codegen_types::Object::Class(class) => {
                    Object::Class(Class::from_codegen_types(class))
                }
                baml_codegen_types::Object::Enum(enum_) => {
                    Object::Enum(Enum::from_codegen_types(enum_))
                }
                baml_codegen_types::Object::TypeAlias(type_alias) => {
                    Object::TypeAlias(TypeAlias::from_codegen_types(type_alias))
                }
            };

            // Extract the path from the namespace.
            let path: Vec<String> = match object {
                baml_codegen_types::Object::Class(c) => match &c.name.namespace {
                    baml_codegen_types::Namespace::Types { path } => {
                        path.iter().map(|s| s.to_string()).collect()
                    }
                    baml_codegen_types::Namespace::StreamTypes { path } => {
                        path.iter().map(|s| s.to_string()).collect()
                    }
                },
                baml_codegen_types::Object::Enum(e) => match &e.name.namespace {
                    baml_codegen_types::Namespace::Types { path } => {
                        path.iter().map(|s| s.to_string()).collect()
                    }
                    baml_codegen_types::Namespace::StreamTypes { path } => {
                        path.iter().map(|s| s.to_string()).collect()
                    }
                },
                baml_codegen_types::Object::TypeAlias(ta) => match &ta.name.namespace {
                    baml_codegen_types::Namespace::Types { path } => {
                        path.iter().map(|s| s.to_string()).collect()
                    }
                    baml_codegen_types::Namespace::StreamTypes { path } => {
                        path.iter().map(|s| s.to_string()).collect()
                    }
                },
                baml_codegen_types::Object::Function(_) => unreachable!(),
            };

            buckets.entry(path).or_default().push(converted);
        }

        // Sort each bucket's objects.
        for objects in buckets.values_mut() {
            objects.sort();
        }

        // Convert buckets to NamespaceGroups.
        buckets
            .into_iter()
            .map(|(path, mut objects)| {
                objects.sort();
                let layout = if path.is_empty() {
                    NsLayout::Root
                } else if path.first().map(|s| s.as_str()) == Some("baml") {
                    NsLayout::Baml
                } else {
                    NsLayout::Vendor
                };

                NamespaceGroup {
                    fs_path: path.clone(),
                    public_path: path,
                    objects,
                    is_stream,
                    layout,
                }
            })
            .collect()
    }
}

pub(crate) struct Function {
    /// functions in python have no namespace
    name: baml_base::Name,
    /// The "wire name" used when calling the runtime — usually the same as `name`,
    /// but for companion functions this is the companion's bare name.
    wire_name: String,
    assembed_docstring: DocString,
    arguments: Vec<FunctionArgument>,
    return_type: Ty,
    /// Companion functions attached to this function (e.g. `build_request`, `parse`).
    companions: Vec<Function>,
}

/// A group of functions that share the same namespace path.
pub(crate) struct FunctionGroup {
    /// Namespace path segments (empty = root).
    pub(crate) path: Vec<String>,
    /// Functions in this group.
    pub(crate) functions: Vec<Function>,
}

pub(crate) struct FunctionArgument {
    name: baml_base::Name,
    ty: Ty,
    default_value: Option<String>,
}

impl Function {
    /// Partition all Types-namespace functions into `FunctionGroup`s by path.
    ///
    /// Functions with empty paths go into a root group; functions with non-empty
    /// paths go into per-path groups. Companion functions are NOT inserted as
    /// top-level entries — they live on their parent `Function.companions`.
    pub(crate) fn load_function_groups(
        objects: &baml_codegen_types::ObjectPool,
    ) -> Vec<FunctionGroup> {
        use std::collections::BTreeMap;

        let mut buckets: BTreeMap<Vec<String>, Vec<Function>> = BTreeMap::new();

        let iter = objects
            .iter()
            .filter(|(name, _)| {
                matches!(name.namespace, baml_codegen_types::Namespace::Types { .. })
            })
            .filter_map(|(name, object)| match object {
                baml_codegen_types::Object::Function(function) => {
                    let path: Vec<String> = match &name.namespace {
                        baml_codegen_types::Namespace::Types { path } => {
                            path.iter().map(|s| s.to_string()).collect()
                        }
                        _ => vec![],
                    };
                    let f = Function::from_codegen_types(
                        function,
                        Ty::from_codegen_types(&function.return_type),
                    );
                    Some((path, f))
                }
                _ => None,
            });

        for (path, f) in iter {
            buckets.entry(path).or_default().push(f);
        }

        for fns in buckets.values_mut() {
            fns.sort();
        }

        buckets
            .into_iter()
            .map(|(path, functions)| FunctionGroup { path, functions })
            .collect()
    }

    pub(crate) fn load_functions(objects: &baml_codegen_types::ObjectPool) -> Vec<Function> {
        let mut objects = objects
            .iter()
            .filter(|(name, _)| {
                matches!(name.namespace, baml_codegen_types::Namespace::Types { .. })
            })
            .filter_map(|(_, object)| match object {
                baml_codegen_types::Object::Function(function) => {
                    Some(Function::from_codegen_types(
                        function,
                        Ty::from_codegen_types(&function.return_type),
                    ))
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        objects.sort();

        objects
    }

    pub(crate) fn load_stream_functions(objects: &baml_codegen_types::ObjectPool) -> Vec<Function> {
        let mut objects = objects
            .iter()
            .filter(|(name, _)| {
                matches!(
                    name.namespace,
                    baml_codegen_types::Namespace::StreamTypes { .. }
                )
            })
            .filter_map(|(_, object)| match object {
                baml_codegen_types::Object::Function(function) => {
                    function.stream_return_type.as_ref().map(|return_type| {
                        Function::from_codegen_types(
                            function,
                            Ty::Stream {
                                stream_type: Box::new(Ty::from_codegen_types(
                                    &function.return_type,
                                )),
                                return_type: Box::new(Ty::from_codegen_types(return_type)),
                            },
                        )
                    })
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        objects.sort();

        objects
    }

    fn from_codegen_types(function: &baml_codegen_types::Function, return_type: Ty) -> Self {
        /// ```askama
        /// {% if let Some(function) = function %}
        /// {{ function }}
        /// {% endif %}
        /// Args:
        /// {%- for (name, docstring) in arguments %}
        ///   {{ name }}: {{ docstring|indent(4) }}
        /// {%- endfor %}
        /// ```
        #[derive(Debug, askama::Template)]
        #[template(in_doc = true, ext = "txt")]
        struct FunctionDocString<'a> {
            function: Option<&'a str>,
            arguments: Vec<(baml_base::Name, &'a str)>,
        }

        let baml_options_arg = baml_codegen_types::FunctionArgument {
            name: "baml_options".into(),
            docstring: Some("See `baml.Options` for more information".into()),
            ty: baml_codegen_types::Ty::Optional(Box::new(baml_codegen_types::Ty::BamlOptions)),
        };

        let docstring = FunctionDocString {
            function: function.docstring.as_deref(),
            arguments: function
                .arguments
                .iter()
                .chain(Some(&baml_options_arg))
                .map(|arg| (arg.name.clone(), arg.docstring.as_deref().unwrap_or("none")))
                .collect(),
        };

        let arguments: Vec<FunctionArgument> = function
            .arguments
            .iter()
            .chain(Some(&baml_options_arg))
            .rev()
            .fold(Vec::new(), |mut acc, arg| {
                acc.insert(
                    0,
                    FunctionArgument::from_codegen_types(
                        arg,
                        acc.first()
                            .map(|first| first.default_value.is_some())
                            .unwrap_or(true),
                    ),
                );
                acc
            });

        // Build companion functions.
        let companions: Vec<Function> = function
            .companions
            .iter()
            .map(|(_, companion)| {
                Function::from_codegen_types(
                    companion,
                    Ty::from_codegen_types(&companion.return_type),
                )
            })
            .collect();

        Self {
            name: function.name.clone(),
            wire_name: function.name.to_string(),
            assembed_docstring: DocString::new(docstring.to_string()),
            arguments,
            return_type,
            companions,
        }
    }
}

impl FunctionArgument {
    pub(crate) fn from_codegen_types(
        arg: &baml_codegen_types::FunctionArgument,
        allow_default_value: bool,
    ) -> Self {
        Self {
            name: arg.name.clone(),
            ty: Ty::from_codegen_types(&arg.ty),
            default_value: if allow_default_value {
                arg.ty
                    .default_value()
                    .and_then(|value| value.to_py_string())
            } else {
                None
            },
        }
    }
}

trait ToPyString {
    fn to_py_string(&self) -> Option<String>;
}

impl ToPyString for baml_codegen_types::DefaultValue {
    fn to_py_string(&self) -> Option<String> {
        use baml_codegen_types::DefaultValue;

        match self {
            DefaultValue::Null => Some("None".to_string()),
            DefaultValue::Literal(lit) => match lit {
                baml_base::Literal::Int(v) => Some(v.to_string()),
                baml_base::Literal::Float(s) => Some(s.clone()),
                baml_base::Literal::String(v) => Some(PyString::new(v).to_string()),
                baml_base::Literal::Bool(true) => Some("True".to_string()),
                baml_base::Literal::Bool(false) => Some("False".to_string()),
            },
        }
    }
}

#[allow(unreachable_pub)]
mod render {
    use super::{Function, FunctionGroup, Namespace, NamespaceGroup, Object};
    mod class;
    mod r#enum;
    mod function;
    mod type_alias;

    impl Object {
        fn print(&self, namespace: Namespace) -> String {
            match self {
                Object::Class(class) => class::print(class, namespace),
                Object::Enum(r#enum) => r#enum::print(r#enum),
                Object::TypeAlias(type_alias) => type_alias::print(type_alias, namespace),
            }
        }
    }

    impl Function {
        fn print_signature(&self) -> String {
            function::print_signature(self, Namespace::Other)
        }

        fn print_sync_impl(&self) -> String {
            function::print_sync_impl(self, Namespace::Other)
        }

        fn print_async_impl(&self) -> String {
            function::print_async_impl(self, Namespace::Other)
        }

        fn print_sync_module_fn(&self) -> String {
            function::print_sync_module_fn(self, Namespace::Other)
        }

        fn print_async_module_fn(&self) -> String {
            function::print_async_module_fn(self, Namespace::Other)
        }

        fn print_module_fn_pyi(&self) -> String {
            function::print_module_fn_pyi(self, Namespace::Other)
        }
    }

    baml_codegen_types::render_fn! {
        /// ```askama
        /// from __future__ import annotations
        ///
        /// import pydantic
        /// import typing
        /// import typing_extensions
        /// from enum import Enum
        ///
        /// try:
        ///     import baml
        /// except ImportError:
        ///     # baml is not installed; provide stubs so the module can be imported
        ///     # for type-checking or testing without the native extension.
        ///     import types as _types
        ///     baml = _types.ModuleType("baml")  # type: ignore[assignment]
        ///     for _n in ("Image", "Audio", "Video", "Pdf", "Collector", "AbortController", "Options"):
        ///         setattr(baml, _n, type(_n, (), {}))
        ///
        /// {% for object in objects %}
        /// {{ object.print(Namespace::Types) }}
        /// {% endfor %}
        /// ```
        pub fn get_types_py(objects: &Vec<Object>) -> String;
    }

    baml_codegen_types::render_fn! {
        /// ```askama
        /// from __future__ import annotations
        ///
        /// import pydantic
        /// import typing
        /// import typing_extensions
        /// from enum import Enum
        ///
        /// try:
        ///     import baml
        /// except ImportError:
        ///     import types as _types
        ///     baml = _types.ModuleType("baml")  # type: ignore[assignment]
        ///     for _n in ("Image", "Audio", "Video", "Pdf", "Collector", "AbortController", "Options"):
        ///         setattr(baml, _n, type(_n, (), {}))
        ///
        /// from . import baml_types
        ///
        /// {% for object in objects %}
        /// {{ object.print(Namespace::StreamTypes) }}
        ///
        /// {% endfor %}
        /// ```
        pub fn get_stream_types_py(objects: &Vec<Object>) -> String;
    }

    baml_codegen_types::render_fn! {
        /// ```askama
        /// {% for fn_ in fns %}
        /// {{ fn_.print_signature() }}
        /// {% endfor %}
        /// ```
        pub fn get_functions_pyi(fns: &Vec<Function>) -> String;
    }

    baml_codegen_types::render_fn! {
        /// ```askama
        /// # This file was generated by BAML: do not edit it.
        /// # Instead, edit the BAML files and re-generate this code.
        ///
        /// import typing
        /// import typing_extensions
        ///
        /// try:
        ///     import baml
        /// except ImportError:
        ///     import types as _types
        ///     baml = _types.ModuleType("baml")  # type: ignore[assignment]
        ///     for _n in ("Image", "Audio", "Video", "Pdf", "Collector", "AbortController", "Options"):
        ///         setattr(baml, _n, type(_n, (), {}))
        ///
        /// from . import baml_types
        /// from . import baml_stream_types
        /// from .runtime import DoNotUseDirectlyCallManager, BamlCallOptions
        ///
        ///
        /// class BamlSyncClient:
        ///     __options: DoNotUseDirectlyCallManager
        ///
        ///     def __init__(self, options: DoNotUseDirectlyCallManager):
        ///         self.__options = options
        ///
        ///     def with_options(
        ///         self,
        ///         collector: typing.Optional[typing.Union[baml.Collector, typing.List[baml.Collector]]] = None,
        ///         abort_controller: typing.Optional[baml.AbortController] = None,
        ///     ) -> "BamlSyncClient":
        ///         options: BamlCallOptions = {}
        ///         if collector is not None:
        ///             options["collector"] = collector
        ///         if abort_controller is not None:
        ///             options["abort_controller"] = abort_controller
        ///         return BamlSyncClient(self.__options.merge_options(options))
        ///
        /// {% for fn_ in fns %}
        ///     {{ fn_.print_sync_impl()|indent(4) }}
        ///
        /// {% endfor %}
        ///
        /// b = BamlSyncClient(DoNotUseDirectlyCallManager({}))
        /// ```
        pub fn get_sync_client_py(fns: &Vec<Function>) -> String;
    }

    baml_codegen_types::render_fn! {
        /// ```askama
        /// # This file was generated by BAML: do not edit it.
        /// # Instead, edit the BAML files and re-generate this code.
        ///
        /// import typing
        /// import typing_extensions
        ///
        /// try:
        ///     import baml
        /// except ImportError:
        ///     import types as _types
        ///     baml = _types.ModuleType("baml")  # type: ignore[assignment]
        ///     for _n in ("Image", "Audio", "Video", "Pdf", "Collector", "AbortController", "Options"):
        ///         setattr(baml, _n, type(_n, (), {}))
        ///
        /// from . import baml_types
        /// from . import baml_stream_types
        /// from .runtime import DoNotUseDirectlyCallManager, BamlCallOptions
        ///
        ///
        /// class BamlAsyncClient:
        ///     __options: DoNotUseDirectlyCallManager
        ///
        ///     def __init__(self, options: DoNotUseDirectlyCallManager):
        ///         self.__options = options
        ///
        ///     def with_options(
        ///         self,
        ///         collector: typing.Optional[typing.Union[baml.Collector, typing.List[baml.Collector]]] = None,
        ///         abort_controller: typing.Optional[baml.AbortController] = None,
        ///     ) -> "BamlAsyncClient":
        ///         options: BamlCallOptions = {}
        ///         if collector is not None:
        ///             options["collector"] = collector
        ///         if abort_controller is not None:
        ///             options["abort_controller"] = abort_controller
        ///         return BamlAsyncClient(self.__options.merge_options(options))
        ///
        /// {% for fn_ in fns %}
        ///     {{ fn_.print_async_impl()|indent(4) }}
        ///
        /// {% endfor %}
        ///
        /// b = BamlAsyncClient(DoNotUseDirectlyCallManager({}))
        /// ```
        pub fn get_async_client_py(fns: &Vec<Function>) -> String;
    }

    /// Common Python header for type files (imports needed by dataclasses, enums, etc.).
    fn type_file_header(ns: Namespace) -> String {
        let _ = ns; // namespace unused in header but kept for future use
        r#"from __future__ import annotations

import pydantic
import typing
import typing_extensions
from enum import Enum

try:
    import baml
except ImportError:
    # baml is not installed; provide stubs so the module can be imported
    # for type-checking or testing without the native extension.
    import types as _types
    baml = _types.ModuleType("baml")  # type: ignore[assignment]
    for _n in ("Image", "Audio", "Video", "Pdf", "Collector", "AbortController", "Options"):
        setattr(baml, _n, type(_n, (), {}))"#
            .to_string()
    }

    /// Render a per-namespace `__init__.py` for one `NamespaceGroup`.
    ///
    /// The file contains the Python header plus all object definitions rendered
    /// in the context of the group's namespace.
    pub fn get_types_namespace_init_py(group: &NamespaceGroup) -> String {
        let ns = if group.is_stream {
            Namespace::StreamTypes
        } else {
            Namespace::Types
        };

        let header = type_file_header(ns);

        let mut parts: Vec<String> = vec![header];

        for object in &group.objects {
            parts.push(object.print(ns));
        }

        parts.join("\n\n") + "\n"
    }

    /// Render the root `baml_types/__init__.py` (or `baml_stream_types/__init__.py`).
    ///
    /// This file:
    /// 1. Defines all root-namespace objects directly (objects with empty path).
    /// 2. Imports every sub-namespace as a submodule so attribute access works
    ///    (e.g. `baml_types.foo.Sentiment`).
    ///
    /// Submodule imports are placed BEFORE object definitions to satisfy ruff E402.
    pub fn get_types_root_init_py(
        root_group: Option<&NamespaceGroup>,
        all_groups: &[NamespaceGroup],
        is_stream: bool,
    ) -> String {
        let ns = if is_stream {
            Namespace::StreamTypes
        } else {
            Namespace::Types
        };

        // Collect top-level submodule names that need to be imported.
        // e.g. for group with path ["foo"], emit `from . import foo`
        //      for group with path ["other", "foo"], emit `from . import other`
        let imported_tops: std::collections::BTreeSet<String> = all_groups
            .iter()
            .filter(|g| !g.fs_path.is_empty())
            .filter_map(|g| g.fs_path.first().cloned())
            .collect();

        // Build the file header. If there's a subpackage named "baml", we skip
        // the runtime `import baml` stub because the subpackage would shadow it.
        let has_baml_subpackage = imported_tops.contains("baml");
        let header = if has_baml_subpackage {
            // No `import baml` runtime stub — the baml subpackage takes precedence.
            r#"from __future__ import annotations

import pydantic
import typing
import typing_extensions
from enum import Enum"#
                .to_string()
        } else {
            type_file_header(ns)
        };

        let mut lines: Vec<String> = vec![header];

        // Submodule imports come next — before any class/enum/alias definitions
        // so ruff E402 (module-level import not at top) is not triggered.
        if !imported_tops.is_empty() {
            let import_lines: Vec<String> = imported_tops
                .iter()
                .map(|top| format!("from . import {top}"))
                .collect();
            lines.push(import_lines.join("\n"));
        }

        // Render root objects after the imports.
        if let Some(root) = root_group {
            for object in &root.objects {
                lines.push(object.print(ns));
            }
        }

        lines.join("\n\n") + "\n"
    }

    /// Render the root `baml_client/__init__.py`.
    ///
    /// This file re-exposes the four top-level modules so that
    /// `import baml_client` gives access to `.baml_types`, `.baml_stream_types`,
    /// `.baml_sync`, and `.baml_async` as attributes.
    pub fn get_client_init_py() -> String {
        r#"# This file was generated by BAML: do not edit it.
# Instead, edit the BAML files and re-generate this code.

from . import baml_types
from . import baml_stream_types
from . import baml_sync
from . import baml_async
"#
        .to_string()
    }

    /// Common header for baml_sync / baml_async __init__.py files.
    fn function_file_header() -> &'static str {
        r#"# This file was generated by BAML: do not edit it.
# Instead, edit the BAML files and re-generate this code.

from __future__ import annotations

import typing
import typing_extensions

try:
    import baml
except ImportError:
    import types as _types
    baml = _types.ModuleType("baml")  # type: ignore[assignment]
    for _n in ("Image", "Audio", "Video", "Pdf", "Collector", "AbortController", "Options"):
        setattr(baml, _n, type(_n, (), {}))"#
    }

    /// Render `baml_sync/__init__.py` (or a sub-namespace equivalent).
    ///
    /// - `is_root`: if true, defines `_runtime` + `_get_runtime()`; otherwise imports them.
    /// - `depth`: number of namespace segments within baml_sync (0 = root, 1 = foo/, etc.).
    /// - `sub_namespaces`: top-level sub-namespace names to import (e.g. `["foo"]`).
    ///   These are placed at the top to satisfy ruff E402.
    pub fn get_baml_sync_init_py(
        group: &FunctionGroup,
        is_root: bool,
        depth: usize,
        sub_namespaces: &[String],
    ) -> String {
        // Number of dots to reach baml_client from this __init__.py:
        //   baml_sync/__init__.py (depth=0)     → ".." (2 dots = 1 level up from baml_sync)
        //   baml_sync/foo/__init__.py (depth=1)  → "..." (3 dots = 2 levels up)
        // Formula: depth + 2 dots (depth levels up within baml_sync + 1 to reach baml_client)
        let client_dots = ".".repeat(depth + 2);

        let mut lines: Vec<String> = vec![function_file_header().to_string()];

        // All imports come first (ruff E402).
        let mut import_block: Vec<String> = Vec::new();

        import_block.push(format!("from {client_dots} import baml_types"));
        import_block.push(format!(
            "from {client_dots}runtime import DoNotUseDirectlyCallManager, BamlCallOptions"
        ));

        if is_root {
            // Sub-namespace submodule imports at top.
            for ns in sub_namespaces {
                import_block.push(format!("from . import {ns}"));
            }
        }
        // Non-root: _get_runtime is defined as a lazy function below (not imported at top)
        // to avoid circular imports when the root loads sub-packages.

        lines.push(import_block.join("\n"));

        // Runtime singleton (root only).
        if is_root {
            lines.push(
                r#"_runtime = DoNotUseDirectlyCallManager({})

def _get_runtime() -> DoNotUseDirectlyCallManager:
    return _runtime"#
                    .to_string(),
            );
        } else {
            // Lazy accessor to avoid circular import (root loads sub-package before _get_runtime is defined).
            // We import from the root package on each call.
            let sync_root_dots = if depth <= 1 {
                "..".to_string()
            } else {
                ".".repeat(depth + 1)
            };
            lines.push(format!(
                r#"def _get_runtime() -> "DoNotUseDirectlyCallManager":
    from {sync_root_dots} import _runtime
    return _runtime"#
            ));
        }

        // Render each function.
        for fn_ in &group.functions {
            // Main function definition.
            lines.push(fn_.print_sync_module_fn());

            // Companion attribute assignments.
            for companion in &fn_.companions {
                // Define the companion as a private helper function.
                let private_name = format!(
                    "_{fn_name}_{cname}",
                    fn_name = fn_.name,
                    cname = companion.name
                );
                let companion_def = {
                    let sig_line = format!(
                        "def {private_name}{args} -> {ret}:",
                        args = companion.render_args_str(Namespace::Other),
                        ret = companion.return_type.render(Namespace::Other),
                    );
                    let body_line = format!(
                        "    __result__ = _get_runtime().merge_options(baml_options or {{}}).call_function_sync(\n        function_name=\"{wire}\",\n        args={args_dict},\n    )\n    return {coerce}",
                        wire = companion.wire_name,
                        args_dict = companion.render_args_dict_str(),
                        coerce = companion.render_coerce_result_str(Namespace::Other),
                    );
                    format!("{sig_line}\n{body_line}")
                };
                lines.push(companion_def);
                // Attach companion as attribute on the parent function.
                lines.push(format!(
                    "{fn_name}.{cname} = {private_name}  # type: ignore[attr-defined]",
                    fn_name = fn_.name,
                    cname = companion.name,
                    private_name = private_name,
                ));
            }
        }

        lines.join("\n\n") + "\n"
    }

    /// Render `baml_async/__init__.py` (or a sub-namespace equivalent).
    pub fn get_baml_async_init_py(
        group: &FunctionGroup,
        is_root: bool,
        depth: usize,
        sub_namespaces: &[String],
    ) -> String {
        let client_dots = ".".repeat(depth + 2);

        let mut lines: Vec<String> = vec![function_file_header().to_string()];

        let mut import_block: Vec<String> = Vec::new();

        import_block.push(format!("from {client_dots} import baml_types"));
        import_block.push(format!(
            "from {client_dots}runtime import DoNotUseDirectlyCallManager, BamlCallOptions"
        ));

        if is_root {
            for ns in sub_namespaces {
                import_block.push(format!("from . import {ns}"));
            }
        }
        // Non-root: _get_runtime defined as lazy function below.

        lines.push(import_block.join("\n"));

        if is_root {
            lines.push(
                r#"_runtime = DoNotUseDirectlyCallManager({})

def _get_runtime() -> DoNotUseDirectlyCallManager:
    return _runtime"#
                    .to_string(),
            );
        } else {
            let async_root_dots = if depth <= 1 {
                "..".to_string()
            } else {
                ".".repeat(depth + 1)
            };
            lines.push(format!(
                r#"def _get_runtime() -> "DoNotUseDirectlyCallManager":
    from {async_root_dots} import _runtime
    return _runtime"#
            ));
        }

        // Render each function.
        for fn_ in &group.functions {
            lines.push(fn_.print_async_module_fn());

            for companion in &fn_.companions {
                let private_name = format!(
                    "_{fn_name}_{cname}",
                    fn_name = fn_.name,
                    cname = companion.name
                );
                let companion_def = {
                    let sig_line = format!(
                        "async def {private_name}{args} -> {ret}:",
                        args = companion.render_args_str(Namespace::Other),
                        ret = companion.return_type.render(Namespace::Other),
                    );
                    let body_line = format!(
                        "    __result__ = await _get_runtime().merge_options(baml_options or {{}}).call_function_async(\n        function_name=\"{wire}\",\n        args={args_dict},\n    )\n    return {coerce}",
                        wire = companion.wire_name,
                        args_dict = companion.render_args_dict_str(),
                        coerce = companion.render_coerce_result_str(Namespace::Other),
                    );
                    format!("{sig_line}\n{body_line}")
                };
                lines.push(companion_def);
                lines.push(format!(
                    "{fn_name}.{cname} = {private_name}  # type: ignore[attr-defined]",
                    fn_name = fn_.name,
                    cname = companion.name,
                    private_name = private_name,
                ));
            }
        }

        lines.join("\n\n") + "\n"
    }

    /// Render `baml_sync/__init__.pyi` — type stubs for IDE support.
    pub fn get_baml_sync_init_pyi(group: &FunctionGroup) -> String {
        let mut parts: Vec<String> = vec![
            r#"# This file was generated by BAML: do not edit it.

from __future__ import annotations

import typing
import typing_extensions"#
                .to_string(),
        ];

        for fn_ in &group.functions {
            parts.push(fn_.print_module_fn_pyi());
            // Stubs for companion attributes (as bare function stubs).
            for companion in &fn_.companions {
                parts.push(format!(
                    "def {cname}{args} -> {ret}: ...",
                    cname = companion.name,
                    args = companion.render_args_str(Namespace::Other),
                    ret = companion.return_type.render(Namespace::Other),
                ));
            }
        }

        parts.join("\n\n") + "\n"
    }

    /// Emit the full baml_sync (and baml_async) function trees into `out`.
    ///
    /// Produces:
    ///   `baml_sync/__init__.py` + `__init__.pyi`
    ///   `baml_sync/<path>/__init__.py` (for each non-root group)
    ///   `baml_async/__init__.py`
    ///   `baml_async/<path>/__init__.py` (for each non-root group)
    pub fn emit_function_tree(
        out: &mut std::collections::HashMap<std::path::PathBuf, String>,
        groups: Vec<FunctionGroup>,
    ) {
        // Separate root group (empty path) from sub-groups.
        let (root_groups, sub_groups): (Vec<_>, Vec<_>) =
            groups.into_iter().partition(|g| g.path.is_empty());

        let root_group = root_groups.into_iter().next().unwrap_or(FunctionGroup {
            path: vec![],
            functions: vec![],
        });

        // Collect sorted top-level sub-namespace names.
        let top_level_submodules: Vec<String> = {
            let set: std::collections::BTreeSet<String> = sub_groups
                .iter()
                .filter_map(|g| g.path.first().cloned())
                .collect();
            set.into_iter().collect()
        };

        // Render root sync __init__.py (sub-namespace imports are passed in, placed at top).
        let sync_root = get_baml_sync_init_py(&root_group, true, 0, &top_level_submodules);
        out.insert(
            std::path::PathBuf::from("baml_sync").join("__init__.py"),
            sync_root,
        );

        // Render root sync __init__.pyi.
        let sync_pyi = get_baml_sync_init_pyi(&root_group);
        out.insert(
            std::path::PathBuf::from("baml_sync").join("__init__.pyi"),
            sync_pyi,
        );

        // Render root async __init__.py.
        let async_root = get_baml_async_init_py(&root_group, true, 0, &top_level_submodules);
        out.insert(
            std::path::PathBuf::from("baml_async").join("__init__.py"),
            async_root,
        );

        // Render sub-namespace groups.
        for group in &sub_groups {
            let depth = group.path.len(); // depth within baml_sync (1 = foo/, 2 = foo/bar/, etc.)
            let sync_content = get_baml_sync_init_py(group, false, depth, &[]);
            let async_content = get_baml_async_init_py(group, false, depth, &[]);

            let mut sync_path = std::path::PathBuf::from("baml_sync");
            let mut async_path = std::path::PathBuf::from("baml_async");
            for seg in &group.path {
                sync_path = sync_path.join(seg);
                async_path = async_path.join(seg);
            }
            out.insert(sync_path.join("__init__.py"), sync_content);
            out.insert(async_path.join("__init__.py"), async_content);

            // Write intermediate empty __init__.py files for prefix paths.
            for prefix_len in 1..group.path.len() {
                let prefix = &group.path[..prefix_len];
                let mut sync_prefix_path = std::path::PathBuf::from("baml_sync");
                let mut async_prefix_path = std::path::PathBuf::from("baml_async");
                for seg in prefix {
                    sync_prefix_path = sync_prefix_path.join(seg);
                    async_prefix_path = async_prefix_path.join(seg);
                }
                // Only write if not already covered by a sub_group.
                let prefix_vec: Vec<String> = prefix.to_vec();
                if !sub_groups.iter().any(|g| g.path == prefix_vec) {
                    out.entry(sync_prefix_path.join("__init__.py"))
                        .or_insert_with(String::new);
                    out.entry(async_prefix_path.join("__init__.py"))
                        .or_insert_with(String::new);
                }
            }
        }
    }

    /// Emit the full type tree into `out` as `(PathBuf, String)` pairs.
    ///
    /// - `root_prefix` is `"baml_types"` or `"baml_stream_types"`.
    /// - One `__init__.py` is written for each `NamespaceGroup`.
    /// - Intermediate directories (prefix paths) get empty `__init__.py` files.
    /// - The package root gets a combined `__init__.py` (root objects + submodule imports).
    pub fn emit_type_tree(
        out: &mut std::collections::HashMap<std::path::PathBuf, String>,
        root_prefix: &str,
        groups: Vec<NamespaceGroup>,
    ) {
        let is_stream = groups.first().map(|g| g.is_stream).unwrap_or(false);

        // Separate root group from subgroups.
        let (root_groups, sub_groups): (Vec<_>, Vec<_>) =
            groups.into_iter().partition(|g| g.fs_path.is_empty());

        let root_group = root_groups.into_iter().next();

        // Write root __init__.py
        let root_init = get_types_root_init_py(root_group.as_ref(), &sub_groups, is_stream);
        out.insert(
            std::path::PathBuf::from(root_prefix).join("__init__.py"),
            root_init,
        );

        // Write per-namespace __init__.py files + intermediate empty __init__.py files.
        let mut intermediate_dirs: std::collections::BTreeSet<Vec<String>> =
            std::collections::BTreeSet::new();

        for group in &sub_groups {
            // Collect all prefix paths for intermediates.
            for prefix_len in 1..group.fs_path.len() {
                intermediate_dirs.insert(group.fs_path[..prefix_len].to_vec());
            }

            // Write the actual __init__.py for this group.
            let content = get_types_namespace_init_py(group);
            let mut file_path = std::path::PathBuf::from(root_prefix);
            for seg in &group.fs_path {
                file_path = file_path.join(seg);
            }
            file_path = file_path.join("__init__.py");
            out.insert(file_path, content);
        }

        // Write intermediate empty __init__.py files for prefix dirs that don't already
        // have a group (so Python can traverse the package namespace chain).
        let sub_paths: std::collections::BTreeSet<&Vec<String>> =
            sub_groups.iter().map(|g| &g.fs_path).collect();

        for dir in intermediate_dirs {
            if sub_paths.contains(&dir) {
                // Already written above.
                continue;
            }
            let mut file_path = std::path::PathBuf::from(root_prefix);
            for seg in &dir {
                file_path = file_path.join(seg);
            }
            file_path = file_path.join("__init__.py");
            // Write an intermediate __init__.py that imports its sub-packages.
            // Collect which immediate children this dir has.
            let mut children: std::collections::BTreeSet<String> =
                std::collections::BTreeSet::new();
            for g in &sub_groups {
                if g.fs_path.len() > dir.len() && g.fs_path.starts_with(dir.as_slice()) {
                    if let Some(child) = g.fs_path.get(dir.len()) {
                        children.insert(child.clone());
                    }
                }
            }
            // Also add children from other intermediate dirs.
            for other_dir in sub_groups.iter().map(|g| &g.fs_path) {
                if other_dir.len() > dir.len() && other_dir.starts_with(dir.as_slice()) {
                    if let Some(child) = other_dir.get(dir.len()) {
                        children.insert(child.clone());
                    }
                }
            }

            let import_lines: Vec<String> = children
                .iter()
                .map(|c| format!("from . import {c}"))
                .collect();

            let content = if import_lines.is_empty() {
                String::new()
            } else {
                import_lines.join("\n") + "\n"
            };
            out.insert(file_path, content);
        }
    }
}

pub(crate) use render::*;
