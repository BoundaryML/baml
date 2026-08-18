//! BEP-066 package-interface schema and source-less resolution checkpoints.

use baml_base::Name;
use baml_compiler2_hir::package::PackageId;
use baml_compiler2_hir_ty::{
    callable::{ExternalCallTarget, ExternalLinkability},
    package_interface::{
        ExportedType, ResolvedValue, package_interface, package_resolution_context,
    },
};
use baml_project::{ProjectDatabase, collect_diagnostics, testing::assert_no_diagnostic_errors};

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
    let mut db = ProjectDatabase::new();
    db.set_project_root(std::path::Path::new("/hir-ty-package-interface-library"));
    db.add_compiler2_virtual_file("<builtin>/app/lib.baml", LIBRARY);
    db
}

fn library_blob() -> Vec<u8> {
    let db = library_db();
    assert_no_diagnostic_errors(&db);
    borsh::to_vec(package_interface(
        &db,
        PackageId::new(&db, Name::new("app")),
    ))
    .expect("package interface serializes")
}

#[test]
fn enriched_interface_is_symbolic_loc_free_and_borsh_stable() {
    let db = library_db();
    assert_no_diagnostic_errors(&db);
    let interface = package_interface(&db, PackageId::new(&db, Name::new("app")));
    let bytes = borsh::to_vec(interface).expect("serialize");
    let decoded = borsh::from_slice(&bytes).expect("deserialize");
    assert_eq!(interface, &decoded);

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
    db.set_project_root(std::path::Path::new("/hir-ty-package-interface-consumer"));
    db.set_mounted_packages([("app".to_owned(), library_blob())].into());
    db.add_file("main.baml", "function main() -> int throws never { 0 }");
    let context = package_resolution_context(&db, PackageId::new(&db, Name::new("user")));

    let (_, ty) = context
        .resolve_type(&db, &[Name::new("app"), Name::new("View")], &[])
        .expect("mounted interface type resolves");
    assert!(matches!(ty, baml_type::Ty::Interface(ref qtn, ..) if qtn.package().as_str() == "app"));
    let baml_type::Ty::Interface(qtn, args, pins, _) = ty else {
        unreachable!()
    };
    let root = baml_type::interned::InterfaceRef::new(
        qtn,
        if args.is_empty() {
            vec![baml_type::interned::Ty::int()].into_boxed_slice()
        } else {
            args.iter()
                .map(baml_type::interned::Ty::from_plain)
                .collect()
        },
        pins.iter()
            .map(|(name, ty)| (name.clone(), baml_type::interned::Ty::from_plain(ty)))
            .collect(),
    );
    let inherited =
        baml_compiler2_hir_ty::impls::direct_requires_closure(&db, &root, &root.existential(), 64);
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
            generics: root.generics.iter().map(|ty| ty.to_plain()).collect(),
            associated_types: Vec::new(),
        }],
    )]
    .into_iter()
    .collect();
    let projection = baml_compiler2_hir_ty::interfaces::lower_projection(
        &db,
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
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );

    let ResolvedValue::Exported(function) = context
        .resolve_value(&db, &[Name::new("app"), Name::new("choose")], &[])
        .expect("mounted function resolves")
    else {
        panic!("mounted result must not contain a FunctionLoc");
    };
    let external = function.external.expect("loc-free callable facts");
    assert!(matches!(external.target, ExternalCallTarget::Free { .. }));
    assert_eq!(external.user_generic_params().count(), 1);
}

fn error_messages(source: &str) -> Vec<String> {
    let mut db = ProjectDatabase::new();
    db.set_project_root(std::path::Path::new("/hir-ty-package-interface-errors"));
    db.set_mounted_packages([("app".to_owned(), library_blob())].into());
    db.add_file("ns_reflect/local.baml", "class Type<T> { value T }");
    db.add_file("main.baml", source);
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
fn mounted_type_validation_and_shorthand_shadowing_are_fail_closed() {
    assert!(
        error_messages(
            r#"
function ok(
    local: reflect.Type<int>,
    view: app.View<int>,
    status: app.Status,
    score: app.Score,
) -> int throws never { 0 }

function type_shorthand() -> type throws never {
    type.of<int>()
}

function reflect_shorthand() -> type throws never {
    reflect.literal.new(1).as_type()
}

function json_shorthand() -> string throws never {
    json.stringify(null)
}
"#,
        )
        .is_empty()
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
fn reflect_type_shorthand_requires_baml_package_access() {
    let mut db = ProjectDatabase::new();
    db.set_project_root(std::path::Path::new(
        "/hir-ty-reflect-shorthand-package-access",
    ));
    let boundary_file = db.add_compiler2_virtual_file(
        "<builtin>/boundary/reflect_probe.baml",
        "type ForbiddenReflect = reflect.Signature\n",
    );
    let user_file = db.add_file(
        "allowed_reflect.baml",
        "type AllowedReflect = reflect.Signature\n",
    );

    let boundary_alias = *baml_compiler2_ppir::item_data::file_type_aliases(&db, boundary_file)
        .first()
        .expect("boundary alias");
    let boundary_errors =
        baml_compiler2_hir_ty::lower::type_alias_lowering_diagnostics(&db, boundary_alias);
    assert!(
        !boundary_errors.is_empty(),
        "boundary has no baml dependency, so reflect.Signature must be unresolved"
    );

    let user_alias = *baml_compiler2_ppir::item_data::file_type_aliases(&db, user_file)
        .first()
        .expect("user alias");
    let user_errors =
        baml_compiler2_hir_ty::lower::type_alias_lowering_diagnostics(&db, user_alias);
    assert!(user_errors.is_empty(), "{user_errors:?}");
    assert_eq!(
        baml_compiler2_hir_ty::lower::type_alias_value(&db, user_alias)
            .to_plain()
            .render_canonical(),
        "baml.reflect.Signature"
    );
}

#[test]
fn keyword_shorthands_follow_the_exported_baml_surface() {
    let mut db = ProjectDatabase::new();
    db.set_project_root(std::path::Path::new(
        "/hir-ty-reflect-shorthand-export-surface",
    ));
    db.add_compiler2_virtual_file(
        "<builtin>/baml/ns_reflect/raw_only.baml",
        "interface RawOnly {}\nclient raw_only = openai.ResponsesClient.new(model = \"gpt-4\");\n",
    );
    db.add_file(
        "main.baml",
        r#"
type ExportedShorthandType = reflect.RawOnly

function exported_shorthand_value() -> type throws never {
    reflect.literal.new(1).as_type()
}

function raw_only_value_must_stay_hidden() -> string throws never {
    reflect.raw_only
}
"#,
    );

    let user_pkg = PackageId::new(&db, Name::new("user"));
    let context = package_resolution_context(&db, user_pkg);
    let baml_items =
        baml_compiler2_ppir::package_items(&db, PackageId::new(&db, Name::new("baml")));
    assert!(
        baml_items
            .lookup_type(&[Name::new("reflect")], &Name::new("RawOnly"))
            .is_some()
    );
    assert!(
        baml_items
            .lookup_value(&[Name::new("reflect")], &Name::new("raw_only"))
            .is_some()
    );
    let exported_baml = context
        .dep_interfaces
        .iter()
        .find(|(name, _)| name.as_str() == "baml")
        .map(|(_, interface)| interface)
        .expect("user package can access baml");
    assert!(
        exported_baml
            .lookup_type(&[Name::new("reflect")], &Name::new("RawOnly"))
            .is_some()
    );
    assert!(
        exported_baml
            .lookup_function(&[Name::new("reflect")], &Name::new("raw_only"))
            .is_none()
    );

    let errors: Vec<_> = collect_diagnostics(&db)
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == baml_compiler_diagnostics::Severity::Error)
        .map(|diagnostic| diagnostic.message)
        .collect();
    let shorthand_errors: Vec<_> = errors
        .iter()
        .filter(|message| message.contains("reflect.raw_only"))
        .collect();
    assert_eq!(
        shorthand_errors.len(),
        1,
        "the raw-only value must not leak through shorthand: {errors:#?}"
    );
}

#[test]
fn mounted_witnesses_members_defaults_and_symbolic_calls_type_check_source_less() {
    let errors = error_messages(WITNESS_CONSUMER);
    assert!(errors.is_empty(), "{errors:#?}");

    let mut mounted = ProjectDatabase::new();
    mounted.set_project_root(std::path::Path::new(
        "/hir-ty-package-interface-parity-mounted",
    ));
    mounted.set_mounted_packages([("app".to_owned(), library_blob())].into());
    mounted.add_file("main.baml", WITNESS_CONSUMER);
    let mut local = library_db();
    local.add_file("main.baml", WITNESS_CONSUMER);
    assert_no_diagnostic_errors(&mounted);
    assert_no_diagnostic_errors(&local);

    let inspect = |db: &ProjectDatabase| {
        let items = baml_compiler2_ppir::package_items(db, PackageId::new(db, Name::new("user")));
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
        let root_ty = inference.type_of_expr[&root].to_plain();
        let targets = inference
            .member_resolutions
            .values()
            .filter_map(|resolution| match resolution {
                baml_compiler2_hir_ty::infer::MemberResolution::External(callable) => {
                    Some(callable.target.clone())
                }
                _ => None,
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
        ExternalCallTarget::Free { name, .. } if name.as_str() == "choose"
    )));
    assert!(mounted_targets.iter().any(|target| matches!(
        target,
        ExternalCallTarget::Method { class, name, .. }
            if class.as_str() == "Box" && name.as_str() == "get_value"
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
    library.set_project_root(std::path::Path::new("/hir-ty-package-interface-native"));
    library.add_compiler2_virtual_file(
        "<builtin>/native/native.baml",
        r#"
function value() -> int throws never {
    $rust_function
}
"#,
    );
    assert_no_diagnostic_errors(&library);
    let blob = borsh::to_vec(package_interface(
        &library,
        PackageId::new(&library, Name::new("native")),
    ))
    .expect("native package interface serializes");

    let mut db = ProjectDatabase::new();
    db.set_project_root(std::path::Path::new(
        "/hir-ty-package-interface-native-consumer",
    ));
    db.set_mounted_packages([("native".to_owned(), blob)].into());
    db.add_file(
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
