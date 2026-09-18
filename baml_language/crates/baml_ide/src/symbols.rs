//! Symbol listing and lookup for BAML projects.
//!
//! This module provides APIs for listing symbols (functions, classes, enums, etc.)
//! in a BAML project.

use baml_compiler2_hir::{
    contributions::Definition,
    item_data::{function_data, function_llm_meta, function_source_map},
    package::package_items,
};
use baml_compiler2_hir_ty::package_interface::package_interface;
use baml_db::{Name, ProjectDatabase};

use crate::{
    param_schema,
    param_schema::{ParamSchema, TypeSchema},
};

/// What a declaration is, as far as any enumeration of the language surface
/// is concerned.
///
/// TOTAL: every declaration is exactly one of these, so a view states its
/// policy as an exhaustive `match` rather than a stack of predicates. That
/// is the point. Four views enumerate this surface — describe's listings,
/// describe's search, completion's qualifier arm, completion's bare-name
/// arms — and they disagreed about three of these categories with nothing
/// in the code saying so. A new category now breaks all four until each one
/// decides what to do with it.
///
/// Variants are ordered by how far off-surface they are, strongest claim
/// first, because a declaration can satisfy several and the strongest wins:
/// `$init_test` is language-internal *and* synthetic, and calling it
/// language-internal is the more useful answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Surface {
    /// Outside the language entirely (`$init_test`, the shim test lowering
    /// mints). No source position can name one, and `is_language_internal`
    /// documents that no language-surface view lists them.
    LanguageInternal,
    /// Spelled with `$`. The lexer takes `$` both leading (the form
    /// `$rust_function` uses) and infix, so these are ordinary identifiers:
    /// `testing.$invoke_collector` compiles, and a user-written `Doc$partial`
    /// is a real type (`expected int, found Doc$partial`).
    ///
    /// A NAME heuristic, not a provenance fact — and a weak one. Corpus-wide
    /// it has exactly one inhabitant, `testing.$invoke_collector`, which is
    /// hand-written stdlib with `UserDefined` origin: the compiler's own
    /// provenance says it is ordinary, and only its spelling says otherwise.
    Synthetic,
    /// Genuinely compiler-generated: the `@` companions (`summarize@spec`),
    /// which carry `FunctionOrigin::Companion`, so this one IS provenance.
    /// Real source references them — the test corpus writes `OaiEcho@spec`
    /// and friends by the dozen.
    Companion,
    /// A companion carrier an ALIAS already reaches: `baml.Int` is written
    /// `int`, so its own path teaches a spelling nobody uses. Carriers with
    /// no alias (`baml.Array`, `baml.Map`, `reflect.Type`) are
    /// [`Surface::Public`] — they are the only handle on their own members.
    AliasedCarrier,
    /// Marked internal by the stdlib's `_` convention. Whether a view shows
    /// one is [`Internals`]' business, not this classification's.
    StdlibInternal,
    /// An ordinary declaration: every view offers it.
    Public,
}

/// Classify `def` for every surface at once.
///
/// The checks run in [`Surface`]'s own order, so a declaration that is
/// several things is reported as the strongest.
pub(crate) fn surface_of(
    db: &dyn baml_compiler2_hir::Db,
    name: &Name,
    def: Definition<'_>,
) -> Surface {
    // Read function metadata through the item-data firewall.
    let metadata = match def {
        Definition::Function(func) => Some(function_data(db, func).metadata),
        Definition::Class(_)
        | Definition::Enum(_)
        | Definition::Interface(_)
        | Definition::TypeAlias(_)
        | Definition::Let(_) => None,
    };
    if metadata.is_some_and(|metadata| metadata.is_language_internal) {
        return Surface::LanguageInternal;
    }
    if name.as_str().contains('$') {
        return Surface::Synthetic;
    }
    if let Some(metadata) = metadata {
        use baml_compiler2_ast::ast::FunctionOrigin;
        match metadata.origin {
            FunctionOrigin::UserDefined => {}
            // Companions and auto-derives carry the docstring of the
            // declaration they shadow, so listing them turns every original
            // into several near-duplicate rows.
            FunctionOrigin::Companion | FunctionOrigin::Internal | FunctionOrigin::AutoDerive => {
                return Surface::Companion;
            }
        }
    }
    if let Definition::Class(class) = def
        && is_aliased_carrier(db, class)
    {
        return Surface::AliasedCarrier;
    }
    if is_stdlib_internal(db, name.as_str(), def.file(db)) {
        return Surface::StdlibInternal;
    }
    Surface::Public
}

/// Whether `class` is a builtin's companion carrier that an alias already
/// reaches, so its own path is a spelling nobody writes.
fn is_aliased_carrier(
    db: &dyn baml_compiler2_hir::Db,
    class: baml_compiler2_hir::loc::ClassLoc<'_>,
) -> bool {
    let data = baml_compiler2_hir::item_data::class_data(db, class);
    let pkg = baml_compiler2_hir::file_package::file_package(db, class.file(db));
    let decl = baml_type::DeclName::in_root(pkg.root, pkg.namespace_path, data.name.clone());
    baml_type::type_kind::builtin_companion_of_decl(
        baml_compiler2_hir::package::lang_roots(db),
        &decl,
    )
    .is_some_and(|companion| companion.members_reachable_without_carrier)
}

/// What completion offers, stated once for all three of its arms.
///
/// A completion list is a MENU of what to write, so it drops everything a
/// reader cannot write — and, unlike describe, everything a better spelling
/// already reaches: `baml.Int` is real documentation but `int` is how one
/// writes it, so the menu offers the alias and the reference lists the
/// carrier.
///
/// [`Surface::StdlibInternal`] is deliberately NOT decided here. Completion
/// also offers dot members, which have no [`Definition`] to classify, so its
/// accumulator applies [`Internals`] to declarations and members alike in
/// one place.
pub(crate) fn offered_in_completion(
    db: &dyn baml_compiler2_hir::Db,
    name: &Name,
    def: Definition<'_>,
) -> bool {
    match surface_of(db, name, def) {
        Surface::LanguageInternal
        | Surface::Synthetic
        | Surface::Companion
        | Surface::AliasedCarrier => false,
        Surface::StdlibInternal | Surface::Public => true,
    }
}

/// The mark the stdlib uses for a helper that is its own business: until
/// BAML has `public`/`private`, a leading `_` is the whole convention.
const INTERNAL_PREFIX: &str = "_";

/// Whether an enumeration of the language surface includes the stdlib's
/// `_`-marked internal helpers.
///
/// The stdlib prefixes hundreds of helpers with `_`, and every surface that
/// SUGGESTS what to reach for — completion, `baml describe`'s search, its
/// listings, its did-you-mean — would otherwise bury the answer in them. So
/// the policy is a parameter those enumerations take rather than a rule each
/// caller remembers to apply: a new consumer has to say which it wants.
///
/// Narrow by design. It is the STDLIB's convention, so [`Internals::hides`]
/// reads stdlib roots and nothing else; a name in the reader's own source is
/// theirs, however it is spelled. And it only ever hides a SUGGESTION —
/// never a resolution, and never an answer to a question that named the
/// symbol, which is what [`Internals::for_query`] is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Internals {
    Hide,
    Show,
}

impl Internals {
    /// What the reader wrote decides: a leading `_` on any word of it asks
    /// for the internals.
    ///
    /// Words split on whitespace and `.`, because a dotted path is how BAML
    /// addresses a symbol — `baml.time._tz_offset_at` names one as plainly
    /// as `_tz_offset_at` does.
    pub fn for_query(query: &str) -> Self {
        if query
            .split(|c: char| c.is_whitespace() || c == '.')
            .any(|word| word.starts_with(INTERNAL_PREFIX))
        {
            Self::Show
        } else {
            Self::Hide
        }
    }

    /// Whether this policy keeps `name`, declared in `declared_in`, out.
    ///
    /// For a declaration prefer `surface_of`, which answers this as
    /// `Surface::StdlibInternal` along with everything else; this is for
    /// the member path, where there is no [`Definition`] to classify.
    /// (Both are crate-private, so they are named rather than linked.)
    pub fn hides(
        self,
        db: &dyn baml_compiler2_hir::Db,
        name: &str,
        declared_in: baml_base::SourceFile,
    ) -> bool {
        self == Self::Hide && is_stdlib_internal(db, name, declared_in)
    }
}

/// The stdlib's `_` convention, in one place: both [`surface_of`] and
/// [`Internals::hides`] read it, so a declaration and a member can never
/// disagree about what the mark means.
fn is_stdlib_internal(
    db: &dyn baml_compiler2_hir::Db,
    name: &str,
    declared_in: baml_base::SourceFile,
) -> bool {
    name.starts_with(INTERNAL_PREFIX)
        && declared_in.source_root(db).kind(db) == baml_base::SourceRootKind::Stdlib
}

/// Extended function metadata for the playground.
#[derive(Debug, Clone)]
pub struct FunctionSymbol {
    pub name: String,
    /// Source-like declaration including name, generic parameters, parameters,
    /// return type, and throws clause.
    pub signature: String,
    /// One-based source location of the function name.
    pub source_position: FunctionSourcePosition,
    /// Whether the function came directly from user source, a companion, or compiler lowering.
    pub origin: FunctionOrigin,
    /// Whether this is an LLM function (has `client`/`prompt` declarative body).
    pub is_llm: bool,
    /// The LLM client name (if LLM function).
    pub client_name: Option<String>,
    /// Whether this function is compiler-generated (`render_prompt`, `build_request`, `resolve`).
    pub is_sub_function: bool,
    /// Parameter schemas for the playground args form. Named types inside are
    /// [`crate::FieldSchema::Ref`]s into [`FunctionListing::types`]. `None`
    /// means no schema was extracted (function missing from the package
    /// interface mid-edit, or extraction skipped for companions/internal
    /// functions); `Some(vec![])` means the function takes no arguments.
    pub params: Option<Vec<ParamSchema>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSourcePosition {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

/// Playground function metadata plus the shared type table their param
/// schemas reference into.
#[derive(Debug, Clone)]
pub struct FunctionListing {
    pub functions: Vec<FunctionSymbol>,
    /// Every named type referenced from any function's params, defined exactly
    /// once and keyed by canonical dotted FQN (`user.shapes.Foo`).
    pub types: std::collections::BTreeMap<String, TypeSchema>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionOrigin {
    UserDefined,
    Companion,
    Internal,
    AutoDerive,
}

impl From<baml_compiler2_ast::ast::FunctionOrigin> for FunctionOrigin {
    fn from(origin: baml_compiler2_ast::ast::FunctionOrigin) -> Self {
        match origin {
            baml_compiler2_ast::ast::FunctionOrigin::UserDefined => Self::UserDefined,
            baml_compiler2_ast::ast::FunctionOrigin::Companion => Self::Companion,
            baml_compiler2_ast::ast::FunctionOrigin::Internal => Self::Internal,
            baml_compiler2_ast::ast::FunctionOrigin::AutoDerive => Self::AutoDerive,
        }
    }
}

/// List the user-facing functions of `package` with metadata for the
/// playground, along with the shared type table their param schemas
/// reference.
///
/// Extracts LLM metadata (client name, `is_llm`) from `declarative_meta` on the
/// compiler2 [`Function`](baml_compiler2_hir::item_tree::Function) item tree entry.
pub fn list_functions_with_metadata(
    db: &ProjectDatabase,
    package: baml_base::SourceRoot,
) -> FunctionListing {
    let pkg = package_items(db, package);
    let iface = package_interface(db, package);
    let mut functions = Vec::new();
    let mut types = std::collections::BTreeMap::new();
    for (namespace_path, ns_items) in &pkg.namespaces {
        for (name, defn) in &ns_items.values {
            if let Definition::Function(func_loc) = defn {
                let llm_meta = function_llm_meta(db, *func_loc);
                let is_llm = llm_meta.is_some();
                let client_name = llm_meta
                    .as_ref()
                    .and_then(|meta| meta.client_name.as_ref())
                    .map(std::string::ToString::to_string);

                // Callable companions have names with `@` (e.g. `MyFunc@render_prompt`).
                let is_sub_function = name.as_str().contains('@');

                let function = function_data(db, *func_loc);
                let origin: FunctionOrigin = function.metadata.origin.into();
                // Companions clone parent params verbatim and non-userDefined
                // functions are hidden by default — extracting schemas for
                // them only duplicates payload. The UI degrades to raw mode.
                let params = if is_sub_function || origin != FunctionOrigin::UserDefined {
                    None
                } else {
                    param_schema::function_param_schemas(
                        db,
                        *func_loc,
                        iface,
                        namespace_path,
                        name,
                        is_llm,
                        &mut types,
                    )
                };

                functions.push(FunctionSymbol {
                    name: playground_function_name(namespace_path, name),
                    signature: render_function_signature(function),
                    source_position: function_source_position(db, *func_loc),
                    origin,
                    is_llm,
                    client_name,
                    is_sub_function,
                    params,
                });
            }
        }
    }
    functions.sort_by(|a, b| a.name.cmp(&b.name));
    FunctionListing { functions, types }
}

fn render_function_signature(function: &baml_compiler2_hir::item_data::FunctionData) -> String {
    let generic_params = function
        .generic_params
        .iter()
        .map(|param| {
            if param.bounds.is_empty() {
                return param.name.to_string();
            }
            let bounds = param
                .bounds
                .iter()
                .map(|&id| function.type_refs.display(id).to_string())
                .collect::<Vec<_>>()
                .join(" & ");
            format!("{} extends {bounds}", param.name)
        })
        .collect::<Vec<_>>();
    let generics = if generic_params.is_empty() {
        String::new()
    } else {
        format!("<{}>", generic_params.join(", "))
    };
    let params = function
        .params
        .iter()
        .map(|param| {
            let optional = if param.has_default { "?" } else { "" };
            match param.type_ref {
                Some(id) => format!(
                    "{}{optional}: {}",
                    param.name,
                    function.type_refs.display(id)
                ),
                None => param.name.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let return_type = function
        .return_type
        .map(|id| format!(" -> {}", function.type_refs.display(id)))
        .unwrap_or_default();
    let throws = function
        .throws
        .map(|id| format!(" throws {}", function.type_refs.display(id)))
        .unwrap_or_else(|| " throws never".to_string());
    format!(
        "function {}{generics}({params}){return_type}{throws}",
        function.name
    )
}

fn function_source_position(
    db: &ProjectDatabase,
    function: baml_compiler2_hir::loc::FunctionLoc<'_>,
) -> FunctionSourcePosition {
    let source_file = function.file(db);
    let source_map = function_source_map(db, function);
    let offset: u32 = if source_map.name_span.is_empty() {
        source_map.span.start()
    } else {
        source_map.name_span.start()
    }
    .into();
    let (line, character) = crate::line_index::LineIndex::new(source_file.text(db))
        .offset_to_position(offset)
        .unwrap_or_default();
    let path = source_file.path(db);
    let root = source_file.source_root(db);
    let relative_path = match root.kind(db) {
        baml_base::SourceRootKind::Workspace => path.strip_prefix(root.path(db)).unwrap_or(&path),
        baml_base::SourceRootKind::Stdlib
        | baml_base::SourceRootKind::Dependency
        | baml_base::SourceRootKind::Dynamic => &path,
    };

    FunctionSourcePosition {
        file: relative_path.to_string_lossy().into_owned(),
        line: line.saturating_add(1),
        column: character.saturating_add(1),
    }
}

/// [`playground_function_name`] with the namespace read from the file's
/// package info — the spelling every playground surface (CFG, cursor
/// context, run targets) uses for a function declared in `source_file`.
pub(crate) fn playground_function_name_for_file(
    db: &dyn baml_compiler2_hir::Db,
    source_file: baml_base::SourceFile,
    name: &Name,
) -> String {
    let package_info = baml_compiler2_hir::file_package::file_package(db, source_file);
    playground_function_name(&package_info.namespace_path, name)
}

/// Whether a declared function name answers to `target_name` in playground
/// addressing: the bare name, or the namespace-qualified playground name.
pub(crate) fn function_name_matches_source_name(
    db: &dyn baml_compiler2_hir::Db,
    source_file: baml_base::SourceFile,
    name: &Name,
    target_name: &str,
) -> bool {
    name.as_str() == target_name
        || playground_function_name_for_file(db, source_file, name) == target_name
}

/// Function names exposed to the playground preserve source namespaces so the
/// UI can group them. Root-level functions keep their historical bare names.
pub(crate) fn playground_function_name(namespace_path: &[Name], name: &Name) -> String {
    if namespace_path.is_empty() {
        return name.to_string();
    }

    let mut parts = Vec::with_capacity(namespace_path.len() + 1);
    parts.extend(namespace_path.iter().map(ToString::to_string));
    parts.push(name.to_string());
    parts.join(".")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDbExt;

    fn make_db() -> (ProjectDatabase, baml_base::SourceRoot) {
        let mut db = ProjectDatabase::new();
        let package = db.workspace(std::path::Path::new("/tmp"));
        (db, package)
    }

    #[test]
    fn playground_function_metadata_preserves_namespace_paths() {
        let (mut db, package) = make_db();
        db.file(
            std::path::Path::new("/tmp/main.baml"),
            "function root_main() -> int { 1 }",
        );
        db.file(
            std::path::Path::new("/tmp/ns_demo/demo.baml"),
            "function demo_func() -> int { 2 }",
        );
        db.file(
            std::path::Path::new("/tmp/ns_demo/ns_inner/inner.baml"),
            "function inner_func() -> int { 3 }",
        );

        let names = list_functions_with_metadata(&db, package)
            .functions
            .into_iter()
            .map(|function| function.name)
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "demo.demo_func".to_string(),
                "demo.inner.inner_func".to_string(),
                "root_main".to_string(),
            ]
        );
    }

    #[test]
    fn playground_function_metadata_includes_signature_and_source_position() {
        let (mut db, package) = make_db();
        let root = std::path::Path::new("/tmp")
            .canonicalize()
            .unwrap_or_else(|_| "/tmp".into());
        db.file(
            root.join("ns_demo/main.baml"),
            "\n\nfunction transform<T extends string>(value: T, count: int) -> T throws Error {\n  value\n}",
        );

        let functions = list_functions_with_metadata(&db, package).functions;
        let function = functions
            .iter()
            .find(|function| function.name == "demo.transform")
            .expect("transform should be listed");

        assert_eq!(
            function.signature,
            "function transform<T extends string>(value: T, count: int) -> T throws Error"
        );
        assert_eq!(
            function.source_position,
            FunctionSourcePosition {
                file: "ns_demo/main.baml".to_string(),
                line: 3,
                column: 10,
            }
        );
    }
}
