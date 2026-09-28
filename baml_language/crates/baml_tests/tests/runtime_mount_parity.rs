//! The RUNTIME lane's twin of `mounted_package_parity`: a consumer compiled
//! through `reflect.Package.compile(.., packages = { "app": .. })` — the
//! `RuntimeCompiler` with a mounted dependency — must produce the SAME
//! consumer unit as the dependency compiled beside it from source.
//!
//! The runtime lane has no other oracle over the consumer unit it emits
//! (the corpus observes runtime VALUES; `snapshots/baml_src` holds host
//! diagnostics only), so this is what pins "emit produces the same unit from
//! the row identities": the wire contract per item kind — every dependency
//! slot and import key the consumer writes, every export it records — is a
//! pure function of the dependency's declarations and the consumer's edge
//! name, whichever lane serves the dependency.
//!
//! Two shapes:
//!
//! 1. TWIN — a dependency that source can express: the mount-lane consumer
//!    unit is byte-identical (after the debug `FileId` normalization
//!    `mounted_package_parity` applies) to the full-source compile's.
//! 2. GOLDENS — `with_types` mounts, which no source twin can express
//!    (minted rows, an export name that is not the item name, a witness of
//!    a stdlib interface): the consumer unit's wire contract is pinned as a
//!    snapshot, so a lane change that alters what the consumer writes is a
//!    reviewed diff, never a silent drift.

use std::path::Path;

use baml_base::Name;
use baml_compiler2_emit::{OptLevel, emit_package};
use baml_db::{ProjectDatabase, testing::assert_no_diagnostic_errors};
use baml_linker_types::CompilationUnit;
use baml_tests::engine::TestDbExt;
use bex_vm_types::{
    RuntimeCompileRequest, RuntimeMountEdge, RuntimeMountSurface, RuntimeMountedClass,
    RuntimeMountedEnum, RuntimeMountedFieldAttrs, RuntimeMountedImpl, RuntimeMountedVariantAttrs,
    RuntimePackageIdentity, RuntimePackageMount, RuntimeProjectedSurface, RuntimeReExport,
    RuntimeReExportKind, types::EdgeKind,
};

/// The runtime compile's opt level (`bex_project::precompiled_stdlib_config`):
/// the source twin must emit at the same one for bytes to compare.
const OPT: OptLevel = OptLevel::One;

/// A dependency exercising every declaration kind a consumer reaches by a
/// wire key: a class (fields, an inherent static and method), an enum, an
/// interface with a required and a default method, an in-body impl, a free
/// function, and a class-typed return.
const LIB: &str = r##"
interface Describable {
    function describe(self) -> string throws never;
    function shout(self) -> string throws never {
        self.describe() + "!"
    }
}

enum Level {
    Low,
    High,
}

class Widget {
    name: string,
    count: int,
    level: Level,

    function of(name: string, count: int) -> Widget throws never {
        Widget { name: name, count: count, level: Level.Low }
    }

    function bump(self) -> Widget throws never {
        Widget { name: self.name, count: self.count + 1, level: self.level }
    }

    implements Describable {
        function describe(self) -> string throws never {
            self.name + "#" + self.count.to_string()
        }
    }
}

function make(name: string) -> Widget throws never {
    Widget.of(name, 0)
}
"##;

/// Reaches every declaration of [`LIB`] through the `app` edge: construction,
/// field reads through a chain, an enum variant in a match, a static, an
/// inherent method, a free function, a required and a default interface
/// method, and a value reference to a free function.
const CONSUMER: &str = r#"
function level_name(level: app.Level) -> string throws never {
    match (level) {
        app.Level.Low => "low",
        app.Level.High => "high",
    }
}

function summary(name: string) -> string throws never {
    let made = app.make(name);
    let bumped = made.bump();
    let built = app.Widget { name: "built", count: 2, level: app.Level.High };
    let of = app.Widget.of("of", 3);
    let describe = bumped.describe();
    let shout = built.shout();
    let maker = app.make;
    let again = maker("again");
    describe + "|" + shout + "|" + of.name + "|" + level_name(built.level) + "|" + again.count.to_string()
}
"#;

/// A request over `packages` by identity, the consumer reaching each of
/// `aliases` under its name.
fn request(
    files: &[(&str, &str)],
    packages: Vec<(RuntimePackageIdentity, RuntimePackageMount)>,
    aliases: Vec<(&str, RuntimePackageIdentity)>,
) -> RuntimeCompileRequest {
    RuntimeCompileRequest {
        files: files
            .iter()
            .map(|(path, text)| ((*path).to_string(), (*text).to_string()))
            .collect(),
        packages: packages.into_iter().collect(),
        aliases: aliases
            .into_iter()
            .map(|(alias, identity)| (alias.to_string(), identity))
            .collect(),
        ..RuntimeCompileRequest::default()
    }
}

fn runtime_compile(request: RuntimeCompileRequest) -> bex_vm::RuntimeCompileArtifact {
    bex_project::runtime_compiler()
        .compile(request)
        .unwrap_or_else(|diagnostics| panic!("runtime compile failed: {diagnostics:#?}"))
}

/// A compiled package with no edges of its own.
fn compiled(interface_blob: Vec<u8>) -> RuntimePackageMount {
    RuntimePackageMount {
        edges: indexmap::IndexMap::new(),
        surface: RuntimeMountSurface::Compiled(interface_blob),
    }
}

/// The unit with every database-local debug `FileId` normalized: the two
/// lanes number `main.baml` differently, and nothing else about the
/// consumer's bytes may differ.
fn normalized_unit(mut unit: CompilationUnit) -> CompilationUnit {
    for object in &mut unit.code {
        let bex_vm_types::Object::Function(function) = object else {
            continue;
        };
        let normalized = baml_base::FileId::new(0);
        function.span.file_id = normalized;
        for entry in &mut function.bytecode.line_table {
            entry.span.file_id = normalized;
        }
        for local in &mut function.debug_locals {
            local.scope_span.file_id = normalized;
        }
    }
    unit
}

fn assert_bytes_identical(label: &str, left: &[u8], right: &[u8]) {
    if left == right {
        return;
    }
    let first_difference = left
        .iter()
        .zip(right)
        .position(|(left, right)| left != right)
        .unwrap_or_else(|| left.len().min(right.len()));
    panic!(
        "{label}: byte streams differ first at {first_difference} (left len {}, right len {})",
        left.len(),
        right.len()
    );
}

/// The wire contract a consumer unit writes: the packages it reaches, by
/// slot; what it imports, by slot and path; what it exports; its impl rules.
/// Rendered one fact per line so the golden reads as a table.
fn wire_contract(unit: &CompilationUnit) -> String {
    let mut lines = Vec::new();
    for (index, entry) in unit.dependencies.iter().enumerate() {
        lines.push(format!(
            "dep @{} {} via @{}",
            index + 1,
            entry.edge,
            entry.via.0
        ));
    }
    for entry in &unit.object_imports {
        lines.push(format!(
            "import object @{} {}",
            entry.key.dep.0, entry.key.path
        ));
    }
    for entry in &unit.global_imports {
        lines.push(format!(
            "import global @{} {}",
            entry.key.dep.0, entry.key.path
        ));
    }
    for (path, local) in &unit.exports.objects {
        lines.push(format!("export object {path} = {local:?}"));
    }
    for (path, slot) in &unit.exports.globals {
        lines.push(format!("export global {path} = {slot}"));
    }
    for rule in &unit.impl_rules {
        lines.push(format!(
            "impl rule for object {} with {} provided methods",
            rule.interface_head.raw(),
            rule.methods.len()
        ));
    }
    lines.join("\n") + "\n"
}

#[test]
fn runtime_mount_consumer_unit_is_byte_identical_to_the_source_dependency_compile() {
    // The mount lane: the dependency compiled to its interface blob, then the
    // consumer compiled against it as a mounted package.
    let library = runtime_compile(request(&[("lib.baml", LIB)], Vec::new(), Vec::new()));
    let app = RuntimePackageIdentity::synthetic(1);
    let mounted = runtime_compile(request(
        &[("main.baml", CONSUMER)],
        vec![(app, compiled(library.interface_blob))],
        vec![("app", app)],
    ));
    let mounted_unit = &mounted.emitted.unit;

    // The source lane: the same dependency as a source-bearing `app` root
    // beside the consumer, everything compiled at once.
    let root = Path::new("<runtime>");
    let mut db = ProjectDatabase::new();
    let workspace = db.workspace(root);
    db.dependency("app");
    db.file("<builtin>/app/lib.baml", LIB);
    db.file(root.join("main.baml"), CONSUMER);
    assert_no_diagnostic_errors(&db);
    let source = emit_package(&db, workspace, OPT).expect("source-lane emit");
    let source_unit = &source.unit;

    let app_slot = mounted_unit
        .dependencies
        .iter()
        .position(|entry| entry.edge.as_str() == "app")
        .map(baml_linker_types::DepSlot::of_dependency_index)
        .expect("the consumer reaches `app`");
    assert!(
        mounted_unit
            .object_imports
            .iter()
            .chain(&mounted_unit.global_imports)
            .any(|entry| entry.key.dep == app_slot),
        "the consumer must reach the dependency through imports at its slot"
    );
    assert_eq!(source_unit.dependencies, mounted_unit.dependencies);
    assert_eq!(source_unit.object_imports, mounted_unit.object_imports);
    assert_eq!(source_unit.global_imports, mounted_unit.global_imports);
    assert_eq!(source_unit.exports, mounted_unit.exports);
    assert_bytes_identical(
        "consumer unit after debug FileId normalization",
        &borsh::to_vec(&normalized_unit(source_unit.clone())).expect("serialize source unit"),
        &borsh::to_vec(&normalized_unit(mounted_unit.clone())).expect("serialize mounted unit"),
    );
    insta::assert_snapshot!("twin_consumer_wire_contract", wire_contract(mounted_unit));
}

/// The `with_types` shapes: a runtime class mounted under an export name
/// that is NOT its item name, a runtime enum under its own, and a witness
/// of a stdlib interface on the class. The consumer constructs the class
/// through the export name, reads a field, matches the enum, and dispatches
/// the witnessed method.
///
/// The world a `with_types` view presents: the view re-exports its base
/// (an empty compiled package) wholesale and adds the mounted declarations
/// by name; each minted declaration belongs to no package and is a mount of
/// its own — a root of one row — reached from the view under an anonymous
/// edge. The consumer imports each declaration from where it is DEFINED,
/// through the view's edge, never from the view.
#[test]
fn with_types_consumer_wire_contract_is_pinned() {
    use baml_compiler2_hir_ty::package_interface::PackageInterface;
    let empty = PackageInterface::<baml_type::TypeName> {
        types: indexmap::IndexMap::new(),
        functions: indexmap::IndexMap::new(),
        throw_sets: Default::default(),
        namespaces: Default::default(),
        impls: Vec::new(),
        reexports: indexmap::IndexMap::new(),
    };
    let interface_blob =
        baml_artifact::encode(baml_artifact::ArtifactKind::PackageInterface, &empty)
            .expect("an empty package interface encodes");

    let base = RuntimePackageIdentity::synthetic(1);
    let minted = RuntimePackageIdentity::synthetic(2);
    let level = RuntimePackageIdentity::synthetic(3);
    let view = RuntimePackageIdentity::synthetic(4);
    let local = |name: &str| baml_type::QualifiedTypeName::local(Name::new(name));
    let through = |edge: &str, name: &str| {
        baml_type::QualifiedTypeName::new(Name::new(edge), Vec::new(), Name::new(name))
    };
    let to_string = baml_type::Interface::<baml_type::TypeName>::new(
        baml_type::QualifiedTypeName::from_dotted_path("baml.ToString"),
        Box::new([]),
        Box::new([]),
    );
    let minted_mount = RuntimePackageMount {
        edges: indexmap::IndexMap::new(),
        surface: RuntimeMountSurface::Projected(RuntimeProjectedSurface {
            classes: vec![RuntimeMountedClass {
                name: Name::new("Minted"),
                docstring: Some("A minted class".to_string()),
                fields: vec![(
                    Name::new("value"),
                    baml_type::Ty::string(),
                    RuntimeMountedFieldAttrs::default(),
                )],
            }],
            enums: Vec::new(),
            aliases: Vec::new(),
            reexports: Vec::new(),
            impls: vec![RuntimeMountedImpl {
                interface: to_string,
                for_ty: baml_type::Ty::Class(local("Minted"), Box::new([])),
                field_links: Vec::new(),
            }],
        }),
    };
    let level_mount = RuntimePackageMount {
        edges: indexmap::IndexMap::new(),
        surface: RuntimeMountSurface::Projected(RuntimeProjectedSurface {
            classes: Vec::new(),
            enums: vec![RuntimeMountedEnum {
                name: Name::new("Level"),
                docstring: None,
                variants: vec![
                    (Name::new("Low"), RuntimeMountedVariantAttrs::default()),
                    (Name::new("High"), RuntimeMountedVariantAttrs::default()),
                ],
            }],
            aliases: Vec::new(),
            reexports: Vec::new(),
            impls: Vec::new(),
        }),
    };
    let reexport = |name: &str, edge: &str, of: &str, kind: RuntimeReExportKind| RuntimeReExport {
        name: bex_vm_types::types::LocalName {
            namespace: Vec::new(),
            name: Name::new(name),
        },
        of: through(edge, of),
        kind,
    };
    let view_mount = RuntimePackageMount {
        edges: [
            (
                Name::new("$base"),
                RuntimeMountEdge {
                    target: base,
                    kind: EdgeKind::ReExported,
                },
            ),
            (
                Name::new("$0"),
                RuntimeMountEdge {
                    target: minted,
                    kind: EdgeKind::Anonymous,
                },
            ),
            (
                Name::new("$1"),
                RuntimeMountEdge {
                    target: level,
                    kind: EdgeKind::Anonymous,
                },
            ),
        ]
        .into_iter()
        .collect(),
        surface: RuntimeMountSurface::Projected(RuntimeProjectedSurface {
            classes: Vec::new(),
            enums: Vec::new(),
            aliases: Vec::new(),
            reexports: vec![
                reexport("Exported", "$0", "Minted", RuntimeReExportKind::Class),
                reexport("Minted", "$0", "Minted", RuntimeReExportKind::Class),
                reexport("Level", "$1", "Level", RuntimeReExportKind::Enum),
            ],
            impls: Vec::new(),
        }),
    };
    let consumer = r#"
function level_name(level: app.Level) -> string throws never {
    match (level) {
        app.Level.Low => "low",
        app.Level.High => "high",
    }
}

function render(level: app.Level) -> string throws never {
    let minted = app.Exported { value: "x" };
    minted.value + "|" + minted.to_string() + "|" + level_name(level)
}
"#;
    let artifact = runtime_compile(request(
        &[("main.baml", consumer)],
        vec![
            (base, compiled(interface_blob)),
            (minted, minted_mount),
            (level, level_mount),
            (view, view_mount),
        ],
        vec![("app", view)],
    ));
    insta::assert_snapshot!(
        "with_types_consumer_wire_contract",
        wire_contract(&artifact.emitted.unit)
    );
}
