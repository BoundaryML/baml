//! BEP-066 package-interface schema and source-less resolution checkpoints.

use baml_base::{Name, SourceRoot};
use baml_compiler2_hir::loc::{DeclRef, FunctionLoc};
use baml_compiler2_hir_ty::{
    callable::{
        ExternalCallTarget, ExternalLinkability, callable_signature, callable_takes_self,
        callable_throws_of, callable_user_generic_params, function_signature_ty,
    },
    extern_loc::{
        ExternFunctionLoc, ExternRowAddr, exported_impl_identity, extern_class_method,
        extern_function_named, extern_function_row, extern_impl_block, extern_impl_method,
        extern_interface_method, extern_signature_ty, impl_identity,
    },
    impls::{ResolvedImplFacts, impl_facts, package_impl_locs},
    package_interface::{
        ExportedFunction, ExportedImpl, ExportedType, ResolvedValue, export_interface,
        exported_takes_self, package_interface, package_resolution_context,
        reduce_ground_projections,
    },
};
use baml_db::{ProjectDatabase, collect_diagnostics, testing::assert_no_diagnostic_errors};
use baml_tests::engine::TestDbExt;
use baml_type::{DeclName, ParamTy};

const LIBRARY: &str = r#"
interface Parent {
    type Root = string

    function root(self) -> Self.Root throws never {
        "root"
    }
}

interface View<T> requires Parent {
    type Item = T
    label string @alias("lbl")

    function get(self) -> Self.Item throws never
    function twice(self) -> Self.Item[] throws never {
        [self.get(), self.get()]
    }
}

class Box<T extends View<int>> {
    value T @description("payload")

    function get_value(self) -> T throws never {
        self.value
    }
}

class Entry {
    label string
    value int

    implements Parent {}
    implements View<int> {
        function get(self) -> Self.Item throws never {
            self.value
        }
    }
}

enum Status {
    Active
    Retired
}

type Score = int

function choose<T extends View<int>>(value: T) -> T throws never {
    value
}
"#;

const WITNESS_CONSUMER: &str = r#"
function inspect(
    value: app.Entry,
    generic: app.Box<app.Entry>,
    view: app.View<int>,
) -> int[] throws never {
    let field: string = value.label
    let class_field: int = generic.get_value().value
    let bound: int = value.get()
    let defaulted: int[] = value.twice()
    let virtual: int[] = view.twice()
    let inherited_default = view.root()
    let unbound: int = app.Entry.get(value)
    let chosen: app.Entry = app.choose(value)
    let status: app.Status = app.Status.Active
    let constructed = app.Entry { label: field, value: chosen.value }
    return [bound, class_field, defaulted[0], virtual[0], unbound, constructed.value]
}
"#;

fn library_db() -> ProjectDatabase {
    library_db_and_file().0
}

fn library_db_and_file() -> (ProjectDatabase, baml_base::SourceFile) {
    let mut db = ProjectDatabase::new();
    db.workspace(std::path::Path::new("/hir-ty-package-interface-library"));
    db.dependency("app");
    let file = db.file("<builtin>/app/lib.baml", LIBRARY);
    (db, file)
}

fn library_blob() -> Vec<u8> {
    library_blob_with_format(baml_artifact::FORMAT_VERSION)
}

fn library_blob_with_format(artifact_format: u32) -> Vec<u8> {
    let db = library_db();
    assert_no_diagnostic_errors(&db);
    baml_artifact::encode_with_format_for_test(
        artifact_format,
        baml_artifact::ArtifactKind::PackageInterface,
        &export_interface(
            &db,
            baml_compiler2_hir::package::spelling(&db)
                .root(&Name::new("app"))
                .unwrap(),
        ),
    )
    .expect("package interface serializes")
}

#[test]
fn mounted_interface_skew_is_rejected_before_installation() {
    let blob = library_blob_with_format(baml_artifact::FORMAT_VERSION + 1);

    let mut db = ProjectDatabase::new();
    db.workspace(std::path::Path::new("/hir-ty-package-interface-skew"));
    let error = match db.try_mount("app", blob) {
        Err(baml_db::SourceRootError::InvalidInterface { message }) => message,
        other => panic!("a skewed interface must be refused as invalid, got {other:?}"),
    };
    assert_eq!(
        error,
        format!(
            "package interface: toolchain {} / format {}; this runtime: {} / format {} — recompile the package and runtime with the same BAML toolchain",
            baml_artifact::BUILD_FINGERPRINT,
            baml_artifact::FORMAT_VERSION + 1,
            baml_artifact::BUILD_FINGERPRINT,
            baml_artifact::FORMAT_VERSION,
        )
    );
}

#[test]
fn enriched_interface_is_symbolic_loc_free_and_borsh_stable() {
    let db = library_db();
    assert_no_diagnostic_errors(&db);
    let interface = export_interface(
        &db,
        baml_compiler2_hir::package::spelling(&db)
            .root(&Name::new("app"))
            .unwrap(),
    );
    let bytes = borsh::to_vec(&interface).expect("serialize");
    let decoded = borsh::from_slice(&bytes).expect("deserialize");
    assert_eq!(interface, decoded);

    let ExportedType::Interface {
        generic_params,
        requires,
        associated_types,
        fields,
        required_methods,
        default_methods,
        ..
    } = interface
        .lookup_type(&[], &Name::new("View"))
        .expect("View export")
    else {
        panic!("View must export as an interface");
    };
    assert_eq!(generic_params.len(), 1);
    assert!(
        requires
            .iter()
            .any(|required| required.name.name().as_str() == "Parent")
    );
    assert_eq!(associated_types[0].name.as_str(), "Item");
    assert!(
        associated_types
            .iter()
            .all(|associated| associated.name.as_str() != "Root"),
        "{associated_types:#?}"
    );
    assert!(associated_types[0].default.is_some());
    assert_eq!(fields[0].2.alias.as_deref(), Some("lbl"));
    assert_eq!(required_methods.len(), 1);
    assert_eq!(default_methods.len(), 1);
    assert!(matches!(
        required_methods[0].target,
        ExternalCallTarget::Interface { .. }
    ));

    let choose = interface
        .lookup_function(&[], &Name::new("choose"))
        .expect("choose export");
    assert_eq!(choose.generic_params.len(), 1);
    assert_eq!(choose.generic_param_bounds[0].len(), 1);
    assert_eq!(choose.linkability, ExternalLinkability::Linkable);
    assert!(matches!(choose.target, ExternalCallTarget::Free { .. }));
    assert_eq!(interface.impls.len(), 2);
    assert!(interface.impls.iter().any(|implementation| {
        implementation.interface.name.name().as_str() == "View"
            && implementation
                .methods
                .iter()
                .any(|method| method.name.as_str() == "get")
    }));
}

#[test]
fn mounted_lookup_returns_owned_exported_results_without_source_locs() {
    let mut db = ProjectDatabase::new();
    db.workspace(std::path::Path::new("/hir-ty-package-interface-consumer"));
    db.mount("app", library_blob());
    db.file("main.baml", "function main() -> int throws never { 0 }");
    let context = package_resolution_context(&db, db.workspace_root().unwrap());

    let (_, ty) = context
        .resolve_type(&db, &[Name::new("app"), Name::new("View")], &[])
        .expect("mounted interface type resolves");
    assert!(matches!(
        ty,
        baml_type::Ty::Interface(ref qtn, ..)
            if baml_compiler2_hir::package::spelling(&db).of(qtn.root()).as_str() == "app"
    ));
    let baml_type::Ty::Interface(qtn, args, pins, _) = ty else {
        unreachable!()
    };
    let root = baml_type::Interface::new(
        qtn,
        if args.is_empty() {
            Box::new([baml_type::Ty::Int {
                attr: baml_type::TyAttr::default(),
            }])
        } else {
            args.clone()
        },
        pins.clone(),
    );
    let inherited = baml_compiler2_hir_ty::impls::direct_requires_closure_plain(
        &db,
        &root,
        &root.to_ty(),
        baml_compiler2_hir_ty::impls::REQUIRES_CLOSURE_FUEL,
    );
    assert!(
        inherited
            .iter()
            .any(|required| required.name.name().as_str() == "Parent"),
        "root={root:#?}; inherited={inherited:#?}"
    );
    let param = baml_type::ParamTy::new(0, Name::new("T"));
    let bounds = [(
        param.clone(),
        vec![baml_type::Interface {
            name: root.name.clone(),
            generics: root.generics.clone(),
            associated_types: Box::new([]),
        }],
    )]
    .into_iter()
    .collect();
    let projection = baml_compiler2_hir_ty::interfaces::lower_projection(
        &db,
        db.workspace_root().expect("workspace root"),
        &bounds,
        baml_type::Ty::TypeVar(param, baml_type::TyAttr::default()),
        None,
        Name::new("Root"),
    );
    assert!(
        projection.diagnostics.is_empty(),
        "{:?}",
        projection
            .diagnostics
            .iter()
            .map(|diagnostic| {
                diagnostic.render(&baml_compiler2_hir_ty::render::Viewpoint::canonical(&db))
            })
            .collect::<Vec<_>>()
    );

    let ResolvedValue::External(function) = context
        .resolve_value(&db, &[Name::new("app"), Name::new("choose")], &[])
        .expect("mounted function resolves")
    else {
        panic!("mounted result must not contain a FunctionLoc");
    };
    assert!(matches!(
        function.slot(&db),
        ExternalCallTarget::Free { .. }
    ));
    assert_eq!(
        callable_user_generic_params(&db, DeclRef::External(function)).len(),
        1
    );
}

fn error_messages(source: &str) -> Vec<String> {
    let mut db = ProjectDatabase::new();
    db.workspace(std::path::Path::new("/hir-ty-package-interface-errors"));
    db.mount("app", library_blob());
    db.file("ns_reflect/local.baml", "class Type<T> { value T }");
    db.file("main.baml", source);
    collect_diagnostics(&db)
        .iter()
        .filter(|diagnostic| diagnostic.severity == baml_compiler_diagnostics::Severity::Error)
        .map(|diagnostic| {
            format!(
                "[{}] {} @ {:?}",
                diagnostic.code(),
                diagnostic.message,
                diagnostic
                    .annotations
                    .first()
                    .map(|annotation| annotation.span.range)
            )
        })
        .collect()
}

#[test]
fn bare_type_is_not_a_value_type_annotation() {
    let errors = error_messages("function removed(value: type) -> type { value }");
    assert!(
        errors
            .iter()
            .filter(|message| {
                message.contains("`type` no longer names a runtime type value")
                    && message.contains("write `reflect.Type` instead")
            })
            .count()
            >= 2,
        "bare `type` annotations must be rejected, and must name their replacement: {errors:#?}"
    );
}

#[test]
fn mounted_type_validation_and_package_shadowing_are_fail_closed() {
    let valid_errors = error_messages(
        r#"
function ok(
    local: root.reflect.Type<int>,
    view: app.View<int>,
    status: app.Status,
    score: app.Score,
) -> int throws never { 0 }

function json_shorthand() -> string throws never {
    json.stringify(null)
}
"#,
    );
    assert!(
        valid_errors.is_empty(),
        "the user reflect namespace and json shorthand should remain valid: {valid_errors:#?}"
    );

    let shadow_errors =
        error_messages("function shadowed() -> unknown throws never { reflect.Type.of<int>() }");
    assert!(
        shadow_errors
            .iter()
            .any(|message| message.contains("unresolved name: `of`")),
        "an ordinary package name should follow normal user-namespace shadowing: {shadow_errors:#?}"
    );

    let errors = error_messages(
        r#"
function bad(
    missing: app.View,
    extra: app.View<int, string>,
    unknown_pin: app.View<int, Nope = string>,
    enum_args: app.Status<int>,
    alias_args: app.Score<int>,
) -> int throws never { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .filter(|message| message.contains("type argument"))
            .count()
            >= 4,
        "{errors:#?}"
    );
    assert!(
        errors.iter().any(|message| message.contains("Nope")),
        "{errors:#?}"
    );
}

#[test]
fn reflect_resolves_as_an_ordinary_builtin_package() {
    let mut db = ProjectDatabase::new();
    db.workspace(std::path::Path::new(
        "/hir-ty-reflect-shorthand-package-access",
    ));
    // `assert` is a stdlib package whose manifest declares `reflect`: a file
    // joining its `Stdlib` root reaches `reflect` through that edge.
    let assert_file = db.file(
        "<builtin>/assert/reflect_probe.baml",
        "type DeclaredReflect = reflect.Signature\n",
    );
    // `boundary` declares no such edge: a package reaches only what its
    // manifest lists, and no leak across undeclared packages exists.
    let boundary_file = db.file(
        "<builtin>/boundary/reflect_probe.baml",
        "type UndeclaredReflect = reflect.Signature\n",
    );
    let user_file = db.file(
        "allowed_reflect.baml",
        "type AllowedReflect = reflect.Signature\n",
    );

    let assert_alias = *baml_compiler2_ppir::item_data::file_type_aliases(&db, assert_file)
        .first()
        .expect("assert alias");
    let assert_errors =
        baml_compiler2_hir_ty::lower::type_alias_lowering_diagnostics(&db, assert_alias);
    assert!(assert_errors.is_empty(), "{assert_errors:?}");

    let boundary_alias = *baml_compiler2_ppir::item_data::file_type_aliases(&db, boundary_file)
        .first()
        .expect("boundary alias");
    let boundary_errors =
        baml_compiler2_hir_ty::lower::type_alias_lowering_diagnostics(&db, boundary_alias);
    assert!(
        boundary_errors
            .iter()
            .any(|(_, error)| format!("{error:?}").contains("reflect.Signature")),
        "an undeclared package must not resolve: {boundary_errors:?}"
    );

    let user_alias = *baml_compiler2_ppir::item_data::file_type_aliases(&db, user_file)
        .first()
        .expect("user alias");
    let user_errors =
        baml_compiler2_hir_ty::lower::type_alias_lowering_diagnostics(&db, user_alias);
    assert!(user_errors.is_empty(), "{user_errors:?}");
    assert_eq!(
        baml_compiler2_hir_ty::lower::type_alias_value(&db, user_alias)
            .render_with(&baml_compiler2_hir_ty::render::Viewpoint::canonical(&db)),
        "reflect.Signature"
    );
}

#[test]
fn reflect_package_resolution_uses_ordinary_builtin_items() {
    let mut db = ProjectDatabase::new();
    db.workspace(std::path::Path::new(
        "/hir-ty-reflect-package-export-surface",
    ));
    // Joins the stdlib `reflect` root: reflect is its own root package.
    db.file(
        "<builtin>/reflect/raw_only.baml",
        "interface RawOnly {}\nclient raw_only = openai.ResponsesClient.new(model = \"gpt-4\");\n",
    );
    db.file(
        "main.baml",
        r#"
type ExportedShorthandType = reflect.RawOnly

function exported_shorthand_value() -> reflect.Type throws never {
    reflect.literal.new(1).as_type()
}

function raw_only_value_is_available() -> string throws never {
    reflect.raw_only
}
"#,
    );

    let user_pkg = db.workspace_root().unwrap();
    let context = package_resolution_context(&db, user_pkg);
    let reflect_items = baml_compiler2_ppir::package_items(
        &db,
        baml_compiler2_hir::package::spelling(&db)
            .root(&Name::new("reflect"))
            .unwrap(),
    );
    assert!(
        reflect_items
            .lookup_type(&[], &Name::new("RawOnly"))
            .is_some()
    );
    assert!(
        reflect_items
            .lookup_value(&[], &Name::new("raw_only"))
            .is_some()
    );
    let exported_reflect = context
        .dep_interfaces
        .iter()
        .find(|(name, _, _)| name.as_str() == "reflect")
        .map(|(_, _, interface)| interface)
        .expect("user package can access reflect");
    assert!(
        exported_reflect
            .lookup_type(&[], &Name::new("RawOnly"))
            .is_some()
    );
    assert!(
        exported_reflect
            .lookup_function(&[], &Name::new("raw_only"))
            .is_none()
    );

    let errors: Vec<_> = collect_diagnostics(&db)
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == baml_compiler_diagnostics::Severity::Error)
        .map(|diagnostic| diagnostic.message)
        .collect();
    let package_errors: Vec<_> = errors
        .iter()
        .filter(|message| message.contains("reflect.raw_only"))
        .collect();
    assert!(
        package_errors.is_empty(),
        "an in-tree builtin package resolves through its ordinary package items: {errors:#?}"
    );
}

#[test]
fn mounted_witnesses_members_defaults_and_symbolic_calls_type_check_source_less() {
    let errors = error_messages(WITNESS_CONSUMER);
    assert!(errors.is_empty(), "{errors:#?}");

    let mut mounted = ProjectDatabase::new();
    mounted.workspace(std::path::Path::new(
        "/hir-ty-package-interface-parity-mounted",
    ));
    mounted.mount("app", library_blob());
    mounted.file("main.baml", WITNESS_CONSUMER);
    let mut local = library_db();
    local.file("main.baml", WITNESS_CONSUMER);
    assert_no_diagnostic_errors(&mounted);
    assert_no_diagnostic_errors(&local);

    let inspect = |db: &ProjectDatabase| {
        let items = baml_compiler2_ppir::package_items(db, (db).workspace_root().unwrap());
        let Some(baml_compiler2_hir::contributions::Definition::Function(function)) =
            items.lookup_value(&[], &Name::new("inspect"))
        else {
            panic!("inspect function resolves")
        };
        let inference = baml_compiler2_hir_ty::infer::infer_body(
            db,
            baml_compiler2_hir::body::BodyOwnerId::Function(function),
        );
        let body = baml_compiler2_ppir::function_body(db, function);
        let baml_compiler2_hir::body::FunctionBody::Expr(body) = body.as_ref() else {
            panic!("inspect has an expression body")
        };
        let root = body.root_expr.expect("inspect root expression");
        let root_ty = inference.type_of_expr[&root].clone();
        let targets = inference
            .member_resolutions
            .values()
            .filter_map(|resolution| match resolution.callable(db)? {
                DeclRef::External(function) => Some(function.slot(db)),
                DeclRef::Source(_) => None,
            })
            .collect::<Vec<_>>();
        (root_ty, targets)
    };
    let (local_root, local_targets) = inspect(&local);
    let (mounted_root, mounted_targets) = inspect(&mounted);
    assert_eq!(local_root, mounted_root);
    assert!(local_targets.is_empty());
    assert!(mounted_targets.iter().any(|target| matches!(
        target,
        ExternalCallTarget::Free { function } if function.name().as_str() == "choose"
    )));
    assert!(mounted_targets.iter().any(|target| matches!(
        target,
        ExternalCallTarget::Method { class, name }
            if class.name().as_str() == "Box" && name.as_str() == "get_value"
    )));
    assert!(mounted_targets.iter().any(|target| matches!(
        target,
        ExternalCallTarget::Interface { interface, method }
            if interface.name().as_str() == "View" && method.as_str() == "twice"
    )));
}

#[test]
fn mounted_reserved_builtin_reports_normal_and_optional_calls() {
    let mut library = ProjectDatabase::new();
    library.workspace(std::path::Path::new("/hir-ty-package-interface-native"));
    library.dependency("native");
    library.file(
        "<builtin>/native/native.baml",
        r#"
function value() -> int throws never {
    $rust_function
}
"#,
    );
    assert_no_diagnostic_errors(&library);
    let blob = baml_artifact::encode(
        baml_artifact::ArtifactKind::PackageInterface,
        &export_interface(
            &library,
            baml_compiler2_hir::package::spelling(&library)
                .root(&Name::new("native"))
                .unwrap(),
        ),
    )
    .expect("native package interface serializes");

    let mut db = ProjectDatabase::new();
    db.workspace(std::path::Path::new(
        "/hir-ty-package-interface-native-consumer",
    ));
    db.mount("native", blob);
    db.file(
        "main.baml",
        r#"
function direct() -> int throws never {
    native.value()
}

function optional() -> int? throws never {
    native.value?.()
}
"#,
    );
    let errors: Vec<String> = collect_diagnostics(&db)
        .iter()
        .filter(|diagnostic| diagnostic.severity == baml_compiler_diagnostics::Severity::Error)
        .map(|diagnostic| format!("[{}] {}", diagnostic.code(), diagnostic.message))
        .collect();
    assert_eq!(
        errors
            .iter()
            .filter(|message| message.contains("E0158") && message.contains("native.value"))
            .count(),
        2,
        "{errors:#?}"
    );
}

// ── The External lane's substrate: locs minted from rows ─────────────────────

const GENERIC_IMPL_LIBRARY: &str = r#"
interface Marker<T> {
    function mark(self) -> T throws never
}

class Pair<A, B> {
    first A
    second B
}

implement<L, R extends Marker<int>> Marker<int> for Pair<L, R> {
    function mark(self) -> int throws never {
        self.second.mark()
    }
}
"#;

fn mounted_consumer() -> ProjectDatabase {
    let mut db = ProjectDatabase::new();
    db.workspace(std::path::Path::new("/hir-ty-package-interface-extern"));
    db.mount("app", library_blob());
    db.file("main.baml", "function main() -> int throws never { 0 }");
    db
}

fn dependency_db(source: &str) -> ProjectDatabase {
    let mut db = ProjectDatabase::new();
    db.workspace(std::path::Path::new("/hir-ty-package-interface-extern-dep"));
    db.dependency("app");
    db.file("<builtin>/app/lib.baml", source);
    assert_no_diagnostic_errors(&db);
    db
}

fn app_root(db: &ProjectDatabase) -> SourceRoot {
    baml_compiler2_hir::package::spelling(db)
        .root(&Name::new("app"))
        .expect("app is installed")
}

/// Every function row `root` exports, each with the loc minted from the key
/// it was found under.
fn minted_rows<'db>(
    db: &'db ProjectDatabase,
    root: SourceRoot,
) -> Vec<(ExternFunctionLoc<'db>, &'db ExportedFunction)> {
    let interface = package_interface(db, root);
    let mut rows = Vec::new();
    for (namespace, functions) in &interface.functions {
        for (name, row) in functions {
            let loc = extern_function_named(db, root, namespace, name)
                .expect("a declared free function mints");
            rows.push((loc, row));
        }
    }
    for (namespace, types) in &interface.types {
        for (name, ty) in types {
            let head = DeclName::in_root(root, namespace.clone(), name.clone());
            match ty {
                ExportedType::Class { methods, .. } => {
                    for row in methods {
                        let loc = extern_class_method(db, &head, &row.name)
                            .expect("a declared class method mints");
                        rows.push((loc, row));
                    }
                }
                ExportedType::Interface {
                    required_methods,
                    default_methods,
                    ..
                } => {
                    for row in required_methods.iter().chain(default_methods) {
                        let loc = extern_interface_method(db, &head, &row.name)
                            .expect("a declared interface method mints");
                        rows.push((loc, row));
                    }
                }
                ExportedType::Enum { .. } | ExportedType::TypeAlias { .. } => {}
            }
        }
    }
    for exported in &interface.impls {
        let block = extern_impl_block(db, root, exported_impl_identity(exported))
            .expect("an exported impl row mints");
        for row in &exported.methods {
            let loc =
                extern_impl_method(db, block, &row.name).expect("a provided impl method mints");
            rows.push((loc, row));
        }
    }
    rows
}

/// Every function the library file declares, paired with the extern loc its
/// export mints — `None` for a function the interface does not carry.
fn source_and_extern_locs<'db>(
    db: &'db ProjectDatabase,
    file: baml_base::SourceFile,
    app: SourceRoot,
) -> Vec<(FunctionLoc<'db>, Option<ExternFunctionLoc<'db>>)> {
    use baml_compiler2_ppir::item_data::{
        MethodOwner, file_functions, function_data, method_owner,
    };
    file_functions(db, file)
        .iter()
        .map(|&function| {
            let name = function_data(db, function).name.clone();
            let loc = match method_owner(db, function) {
                None => extern_function_named(db, app, &[], &name),
                Some(MethodOwner::Class(class)) => extern_class_method(
                    db,
                    &baml_compiler2_hir_ty::lower::class_qualified_name(db, class),
                    &name,
                ),
                Some(MethodOwner::Interface(interface)) => extern_interface_method(
                    db,
                    &baml_compiler2_hir_ty::lower::interface_qualified_name(db, interface),
                    &name,
                ),
                Some(MethodOwner::Impl(block)) => {
                    let facts = impl_facts(db, block)
                        .resolved()
                        .expect("the fixture's impl headers resolve");
                    extern_impl_block(db, app, impl_identity(&ResolvedImplFacts::Source(facts)))
                        .and_then(|block| extern_impl_method(db, block, &name))
                }
            };
            (function, loc)
        })
        .collect()
}

#[test]
fn extern_locs_are_interned_per_row_and_never_reach_across_families() {
    let db = mounted_consumer();
    let app = app_root(&db);
    let choose =
        extern_function_named(&db, app, &[], &Name::new("choose")).expect("choose is exported");
    assert_eq!(
        choose,
        extern_function_named(&db, app, &[], &Name::new("choose")).unwrap()
    );
    let r#box = DeclName::in_root(app, Vec::new(), Name::new("Box"));
    let get_value = extern_class_method(&db, &r#box, &Name::new("get_value"))
        .expect("Box.get_value is exported");
    assert_ne!(choose, get_value);
    // A type is not a function row, and a class row holds no interface method.
    assert!(extern_function_named(&db, app, &[], &Name::new("Box")).is_none());
    assert!(extern_class_method(&db, &r#box, &Name::new("twice")).is_none());
    let view = DeclName::in_root(app, Vec::new(), Name::new("View"));
    assert!(extern_interface_method(&db, &view, &Name::new("get")).is_some());
    assert!(extern_interface_method(&db, &view, &Name::new("twice")).is_some());
    // `root` is Parent's row: a `requires` closure does not make it View's.
    assert!(extern_interface_method(&db, &view, &Name::new("root")).is_none());
    let parent = DeclName::in_root(app, Vec::new(), Name::new("Parent"));
    assert!(extern_interface_method(&db, &parent, &Name::new("root")).is_some());
}

#[test]
fn extern_function_row_is_total_over_every_minted_row() {
    let db = mounted_consumer();
    let app = app_root(&db);
    let mut total = 0;
    for root in db.source_roots() {
        for (loc, row) in minted_rows(&db, root) {
            assert!(std::ptr::eq(extern_function_row(&db, loc), row));
            assert!(std::ptr::eq(
                callable_signature(&db, DeclRef::External(loc)),
                extern_signature_ty(&db, loc)
            ));
            total += 1;
        }
    }
    // choose, Box.get_value, Parent.root, View.get, View.twice, and the `get`
    // Entry's View impl provides.
    assert_eq!(minted_rows(&db, app).len(), 6);
    assert!(
        total > 6,
        "the stdlib packages contribute rows too: {total}"
    );
}

#[test]
fn a_declared_mint_address_equals_the_row_target() {
    let db = mounted_consumer();
    for root in db.source_roots() {
        for (loc, row) in minted_rows(&db, root) {
            match loc.addr(&db) {
                ExternRowAddr::Declared(target) => assert_eq!(&row.target, target, "{}", row.name),
                ExternRowAddr::ImplProvided { method, .. } => {
                    assert_eq!(*method, row.name);
                    assert!(
                        matches!(row.target, ExternalCallTarget::Interface { .. }),
                        "an impl-provided row's target is its interface's dispatch slot: {}",
                        row.name
                    );
                }
            }
        }
    }
}

#[test]
fn an_interface_method_mint_never_yields_an_impl_provided_row() {
    let db = mounted_consumer();
    let app = app_root(&db);
    let interface = package_interface(&db, app);
    let view = DeclName::in_root(app, Vec::new(), Name::new("View"));
    let Some(ExportedType::Interface {
        required_methods, ..
    }) = interface.lookup_type(&[], &Name::new("View"))
    else {
        panic!("View is exported as an interface")
    };
    let declared = extern_interface_method(&db, &view, &Name::new("get")).unwrap();
    assert!(std::ptr::eq(
        extern_function_row(&db, declared),
        &required_methods[0]
    ));
    let (exported, provided_row) = interface
        .impls
        .iter()
        .find_map(|row| {
            row.methods
                .iter()
                .find(|method| method.name.as_str() == "get")
                .map(|method| (row, method))
        })
        .expect("Entry's View impl provides get");
    let block = extern_impl_block(&db, app, exported_impl_identity(exported)).unwrap();
    let provided = extern_impl_method(&db, block, &Name::new("get")).unwrap();
    assert!(std::ptr::eq(
        extern_function_row(&db, provided),
        provided_row
    ));
    assert_ne!(declared, provided);
    // Same dispatch slot, distinct rows: the ADDRESS is the identity, not
    // the target.
    assert_eq!(
        extern_function_row(&db, declared).target,
        extern_function_row(&db, provided).target
    );
    assert_eq!(provided.impl_block(&db), Some(block));
    assert_eq!(declared.impl_block(&db), None);
}

#[test]
fn impl_identity_agrees_between_a_source_block_and_its_exported_row() {
    for source in [LIBRARY, GENERIC_IMPL_LIBRARY] {
        let db = dependency_db(source);
        let app = app_root(&db);
        let from_source: Vec<_> = package_impl_locs(&db, app)
            .iter()
            .map(|&block| {
                let facts = impl_facts(&db, block)
                    .resolved()
                    .expect("the fixture's impl headers resolve");
                impl_identity(&ResolvedImplFacts::Source(facts))
            })
            .collect();
        let from_rows: Vec<_> = package_interface(&db, app)
            .impls
            .iter()
            .map(exported_impl_identity)
            .collect();
        assert!(!from_source.is_empty());
        assert_eq!(from_source.len(), from_rows.len());
        for identity in &from_source {
            assert!(from_rows.contains(identity), "{identity:?}");
            assert!(extern_impl_block(&db, app, identity.clone()).is_some());
        }
    }
}

#[test]
fn impl_identity_ignores_written_parameter_names_but_not_bounds() {
    let db = dependency_db(GENERIC_IMPL_LIBRARY);
    let app = app_root(&db);
    let row = &package_interface(&db, app).impls[0];
    assert_eq!(row.generic_params.len(), 2, "the fixture impl is generic");
    let renamed = rename_impl_params(row);
    assert_ne!(renamed.generic_params, row.generic_params);
    assert_eq!(
        exported_impl_identity(&renamed),
        exported_impl_identity(row)
    );
    let mut unbounded = row.clone();
    unbounded.param_bounds = vec![Vec::new(); row.generic_params.len()];
    assert_ne!(
        exported_impl_identity(&unbounded),
        exported_impl_identity(row)
    );
}

fn rename_impl_params(row: &ExportedImpl) -> ExportedImpl {
    fn rename(param: &ParamTy) -> ParamTy {
        ParamTy::new(
            param.index(),
            Name::new(format!("Renamed{}", param.index())),
        )
    }
    fn rewrite(ty: &baml_type::Ty) -> baml_type::Ty {
        baml_type::unify::rewrite_ty(ty, &mut |ty| match ty {
            baml_type::Ty::TypeVar(param, attr) => {
                Some(baml_type::Ty::TypeVar(rename(param), attr.clone()))
            }
            _ => None,
        })
    }
    ExportedImpl {
        interface: row.interface.map_tys(rewrite),
        for_ty_pattern: rewrite(&row.for_ty_pattern),
        generic_params: row.generic_params.iter().map(rename).collect(),
        param_bounds: row
            .param_bounds
            .iter()
            .map(|bounds| bounds.iter().map(|bound| bound.map_tys(rewrite)).collect())
            .collect(),
        associated_types: row.associated_types.clone(),
        field_links: row.field_links.clone(),
        origin: row.origin.clone(),
        methods: row.methods.clone(),
    }
}

#[test]
fn extern_signature_is_the_source_signature_after_export_reduction() {
    let (db, file) = library_db_and_file();
    let app = app_root(&db);
    let mut compared = 0;
    for (function, loc) in source_and_extern_locs(&db, file, app) {
        let Some(loc) = loc else { continue };
        let source = function_signature_ty(&db, function);
        let exported = extern_signature_ty(&db, loc);
        let reduce = |ty: &baml_type::Ty| reduce_ground_projections(&db, ty, 8);
        assert_eq!(source.params.len(), exported.params.len());
        for (from_source, from_row) in source.params.iter().zip(&exported.params) {
            assert_eq!(from_source.name, from_row.name);
            assert_eq!(from_source.mode, from_row.mode);
            assert_eq!(reduce(&from_source.ty), from_row.ty);
        }
        assert_eq!(reduce(&source.return_type), exported.return_type);
        assert_eq!(source.generic_params, exported.generic_params);
        assert_eq!(source.builtin_kind, exported.builtin_kind);
        assert_eq!(
            callable_throws_of(&db, DeclRef::Source(function)),
            callable_throws_of(&db, DeclRef::External(loc))
        );
        compared += 1;
    }
    assert!(compared >= 6, "{compared}");
}

#[test]
fn callable_takes_self_agrees_across_lanes_and_with_the_export_predicate() {
    let (db, file) = library_db_and_file();
    let app = app_root(&db);
    let (mut seen_instance, mut seen_static) = (false, false);
    for (function, loc) in source_and_extern_locs(&db, file, app) {
        let Some(loc) = loc else { continue };
        let declared = baml_compiler2_ppir::item_data::function_data(&db, function)
            .params
            .first()
            .is_some_and(|param| param.name.as_str() == "self");
        let takes_self = callable_takes_self(&db, DeclRef::Source(function));
        assert_eq!(takes_self, declared);
        assert_eq!(takes_self, callable_takes_self(&db, DeclRef::External(loc)));
        assert_eq!(
            takes_self,
            exported_takes_self(extern_function_row(&db, loc))
        );
        if takes_self {
            seen_instance = true;
        } else {
            seen_static = true;
        }
    }
    assert!(seen_instance && seen_static);
}
