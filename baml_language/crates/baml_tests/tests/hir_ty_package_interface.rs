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

enum Status {
    Active
    Retired
}

type Score = int

function choose<T extends View<int>>(value: T) -> T throws never {
    value
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
        .map(|diagnostic| format!("[{}] {}", diagnostic.code(), diagnostic.message))
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
