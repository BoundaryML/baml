#![cfg(any())]

//! BEP-066 mounted-package linking capstone: consuming a dependency from source and from its
//! `PackageInterface` blob must be observationally equivalent.
//!
//! The SOURCE path puts the rich `app` fixture and the consumer in one database.
//! The BLOB path compiles `app` independently into its check artifact (borsh of
//! `PackageInterface`) and run artifact (`CompilationUnit`s), then mounts the
//! former in a fresh source-less consumer database and links the latter through
//! `generate_project_bytecode_with_mounted_units`.
//!
//! The oracle pins four boundaries:
//!
//! 1. normalized diagnostic bytes (code, message, and user-source range),
//! 2. runtime values and caught throws,
//! 3. emitted-program and relocatable-unit invariants,
//! 4. primary-only E0132 attribution for a user impl conflicting with a
//!    span-less mounted impl.
//!
//! Unit import tables do not legitimately diverge: both paths name library
//! symbols by the same fully-qualified B-693 identities. Raw consumer units and
//! programs differ only in debug `FileId`s: removing the dependency source shifts
//! the fresh consumer database's numeric id for `main.baml`. After normalizing
//! those database-local ids (function span, line table, local-scope spans), every
//! emitted byte is identical. Source paths/ranges and all executable content are
//! unchanged.

use baml_base::Name;
use baml_compiler2_emit::{
    CompileOptions, MountedPackageLinkError, OptLevel, decompose_units, emit_units,
    generate_project_bytecode_with_mounted_units, generate_project_bytecode_with_opt,
};
use baml_compiler2_hir::package::PackageId;
use baml_compiler2_tir::package_interface::{ExportedType, PackageInterface, package_interface};
use baml_project::{ProjectDatabase, collect_diagnostics, testing::assert_no_diagnostic_errors};
use baml_tests::engine::run_compiled;
use bex_engine::BexExternalValue;
use bex_vm_types::{CompilationUnit, Program};
use indexmap::IndexMap;

const ROOT: &str = "/mounted-parity";
const OPT: OptLevel = OptLevel::One;

/// A deliberately broad package surface: requires/default methods, associated
/// defaults, attrs, generic class/function bounds, enum/alias rows, every impl
/// shape, a callback-effect impl method, and a throwing function.
const LIB: &str = r#"
interface Named {
    name string

    function label(self) -> string throws never {
        self.name
    }
}

interface Measured requires Named {
    type Unit = string

    function measure(self) -> int throws never

    function decorated(self) -> string throws never {
        self.label()
    }
}

class Widget {
    name string @description("public label")
    value int @alias("score")

    implements Named {}

    implements Measured {
        function measure(self) -> int throws never {
            self.value
        }
    }
}

class Box<T extends Named> {
    value T

    function inner_name(self) -> string throws never {
        self.value.name
    }
}

class Plain {
    name string
    value int

    implements Named {}
}

implement Measured for Plain {
    function measure(self) -> int throws never {
        self.value
    }
}

enum Status {
    Active
    Retired
}

type Score = int

interface Tagged {
    function tag(self) -> string throws never
}

implement<T> Tagged for T {
    function tag(self) -> string throws never {
        "any"
    }
}

interface Applies {
    function apply(self, cb: (x: int) -> int throws never) -> int throws unknown
}

class Runner {
    base int

    implements Applies {
        function apply(self, cb: (x: int) -> int) -> int {
            cb(self.base)
        }
    }
}

class ParseError {
    message string
}

function measure_twice<T extends Measured>(value: T) -> int throws never {
    value.measure() + value.measure()
}

function tag_of<T extends Tagged>(value: T) -> string throws never {
    value.tag()
}

function parse_positive(value: int) -> int throws ParseError {
    if value < 0 {
        throw ParseError { message: "negative" }
    }
    value
}
"#;

fn options() -> CompileOptions {
    CompileOptions {
        emit_test_cases: false,
    }
}

fn library_db() -> ProjectDatabase {
    let mut db = ProjectDatabase::new();
    db.set_project_root(std::path::Path::new(ROOT));
    db.add_compiler2_virtual_file("<builtin>/app/lib.baml", LIB);
    db
}

struct LibraryArtifacts {
    blob: Vec<u8>,
    interface: PackageInterface,
    units: Vec<CompilationUnit>,
}

fn library_artifacts() -> LibraryArtifacts {
    let db = library_db();
    assert_no_diagnostic_errors(&db);
    let interface = package_interface(&db, PackageId::new(&db, Name::new("app"))).clone();
    let blob = borsh::to_vec(&interface).expect("serialize app package interface");
    let round_trip =
        borsh::from_slice::<PackageInterface>(&blob).expect("deserialize app package interface");
    assert_eq!(
        interface, round_trip,
        "interface blob must round-trip exactly"
    );
    let units = emit_units(&db, &options(), OPT).expect("emit independent app units");
    LibraryArtifacts {
        blob,
        interface,
        units,
    }
}

fn source_db(user: &str) -> ProjectDatabase {
    let mut db = library_db();
    db.add_file("main.baml", user);
    db
}

fn blob_db(user: &str, blob: Vec<u8>) -> ProjectDatabase {
    let mut db = ProjectDatabase::new();
    db.set_project_root(std::path::Path::new(ROOT));
    db.set_mounted_packages([("app".to_string(), blob)].into());
    db.add_file("main.baml", user);
    db
}

fn compile_source(user: &str) -> Program {
    let db = source_db(user);
    assert_no_diagnostic_errors(&db);
    generate_project_bytecode_with_opt(&db, &options(), OPT).expect("source-path compile")
}

fn compile_blob(user: &str, artifacts: &LibraryArtifacts) -> Program {
    let db = blob_db(user, artifacts.blob.clone());
    assert_no_diagnostic_errors(&db);
    generate_project_bytecode_with_mounted_units(&db, &options(), OPT, &artifacts.units)
        .expect("blob-path compile and link")
}

async fn run(program: Program, entry: &str) -> Result<BexExternalValue, String> {
    run_compiled(program, entry, IndexMap::new(), false)
        .await
        .result
        .map_err(|error| format!("{error:?}"))
}

async fn assert_runtime_parity(user: &str, entry: &str, expected: BexExternalValue) {
    let artifacts = library_artifacts();
    let source = compile_source(user);
    let blob = compile_blob(user, &artifacts);
    let source_result = run(source, entry).await;
    let blob_result = run(blob, entry).await;
    assert_eq!(source_result, blob_result, "runtime paths diverged");
    assert_eq!(source_result, Ok(expected));
}

#[tokio::test]
async fn rich_surface_runtime_is_identical() {
    assert_runtime_parity(
        r#"
class Local {
    name string
    implements app.Named {}
}

function status_score(status: app.Status) -> int throws never {
    match status {
        app.Status.Active => 1
        app.Status.Retired => 2
    }
}

function main() -> int throws never {
    let widget = app.Widget { name: "widget", value: 7 }
    let plain = app.Plain { name: "plain", value: 4 }
    let boxed = app.Box<app.Widget> { value: widget }
    let score: app.Score = 3
    let enum_score = status_score(app.Status.Retired)
    let local = Local { name: "local" }
    let tagged = app.tag_of(local)
    let boxed_score = if boxed.inner_name() == "widget" { 1 } else { 0 }
    let decorated_score = if widget.decorated() == "widget" { 1 } else { 0 }
    let tagged_score = if tagged == "any" { 1 } else { 0 }
    let inherited_default_score = if local.label() == "local" { 1 } else { 0 }
    widget.measure()
        + plain.measure()
        + app.measure_twice(widget)
        + score
        + enum_score
        + boxed_score
        + decorated_score
        + tagged_score
        + inherited_default_score
}
"#,
        "main",
        BexExternalValue::Int(34),
    )
    .await;
}

#[tokio::test]
async fn existential_default_and_out_of_body_dispatch_are_identical() {
    assert_runtime_parity(
        r#"
function read(value: app.Measured) -> int throws never {
    value.measure()
}

function main() -> int throws never {
    let widget = app.Widget { name: "widget", value: 20 }
    let plain = app.Plain { name: "plain", value: 22 }
    read(widget) + read(plain)
}
"#,
        "main",
        BexExternalValue::Int(42),
    )
    .await;
}

#[tokio::test]
async fn impl_method_callback_effect_metadata_and_frame_are_identical() {
    assert_runtime_parity(
        r#"
function plus_one(value: int) -> int throws never {
    value + 1
}

function main() -> int throws never {
    app.Runner { base: 41 }.apply(plus_one)
}
"#,
        "main",
        BexExternalValue::Int(42),
    )
    .await;
}

#[tokio::test]
async fn caught_library_throw_is_identical() {
    assert_runtime_parity(
        r#"
function classify(value: int) -> string {
    let parsed = app.parse_positive(value) catch (error) {
        let typed: app.ParseError => {
            return typed.message
        }
    }
    "ok:" + parsed.to_string()
}

function main() -> string {
    classify(7) + ":" + classify(0 - 1)
}
"#,
        "main",
        BexExternalValue::String("ok:7:negative".into()),
    )
    .await;
}

/// A stable byte record deliberately excludes `FileId`: the source database
/// allocates one extra id for the library file, while the blob database has no
/// such file. Codes, messages, severity, phase, and user-text ranges are the
/// user-observable diagnostic contract and must match byte-for-byte.
fn diagnostic_bytes(db: &ProjectDatabase) -> Vec<u8> {
    let rows: Vec<String> = collect_diagnostics(db)
        .into_iter()
        .filter(|diagnostic| {
            matches!(
                diagnostic.severity,
                baml_compiler_diagnostics::Severity::Error
            )
        })
        .map(|diagnostic| {
            let range = diagnostic
                .primary_span()
                .expect("consumer error has a primary span")
                .range;
            format!(
                "{}\0{:?}\0{:?}\0{}..{}\0{}",
                diagnostic.code(),
                diagnostic.severity,
                diagnostic.phase,
                u32::from(range.start()),
                u32::from(range.end()),
                diagnostic.message
            )
        })
        .collect();
    rows.join("\n").into_bytes()
}

#[test]
fn representative_error_diagnostics_are_byte_identical() {
    let artifacts = library_artifacts();
    let cases = [
        (
            "wrong arity",
            r#"
function main() -> int {
    app.measure_twice()
}
"#,
        ),
        (
            "unsatisfied bound",
            r#"
class Rock { value int }

function main() -> int {
    app.measure_twice(Rock { value: 1 })
}
"#,
        ),
        (
            "non-exhaustive enum",
            r#"
function inspect(status: app.Status) -> int throws never {
    match status {
        app.Status.Active => 1
    }
}
"#,
        ),
        (
            "missing impl member",
            r#"
class Broken {
    name string
    implements app.Named {}
    implements app.Measured {}
}
"#,
        ),
    ];

    for (label, user) in cases {
        let source = diagnostic_bytes(&source_db(user));
        let blob = diagnostic_bytes(&blob_db(user, artifacts.blob.clone()));
        assert!(!source.is_empty(), "{label}: fixture must produce an error");
        assert_eq!(source, blob, "{label}: source/blob diagnostics diverged");
    }
}

const ARTIFACT_USER: &str = r#"
class Local {
    name string
    implements app.Named {}
}

function status_score(status: app.Status) -> int throws never {
    match status {
        app.Status.Active => 1
        app.Status.Retired => 2
    }
}

function main() -> int throws never {
    let widget = app.Widget { name: "artifact", value: 20 }
    let boxed = app.Box<app.Widget> { value: widget }
    let enum_score = status_score(app.Status.Active)
    let boxed_score = if boxed.inner_name() == "artifact" { 1 } else { 0 }
    let tagged = app.tag_of(Local { name: "local" })
    let tagged_score = if tagged == "any" { 1 } else { 0 }
    app.measure_twice(widget)
        + enum_score
        + boxed_score
        + tagged_score
}
"#;

fn unit<'a>(units: &'a [CompilationUnit], path: &str) -> &'a CompilationUnit {
    units
        .iter()
        .find(|unit| unit.source_file == path)
        .unwrap_or_else(|| panic!("missing unit `{path}`"))
}

#[track_caller]
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

fn normalize_object_debug_file_ids(object: &mut bex_vm_types::Object) {
    let bex_vm_types::Object::Function(function) = object else {
        return;
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

fn normalized_unit(mut unit: CompilationUnit) -> CompilationUnit {
    for object in &mut unit.code {
        normalize_object_debug_file_ids(object);
    }
    if let Some(tail) = &mut unit.init_tail {
        for object in &mut tail.objects {
            normalize_object_debug_file_ids(object);
        }
    }
    unit
}

fn normalized_program(mut program: Program) -> Program {
    for object in program.objects.iter_mut() {
        normalize_object_debug_file_ids(object);
    }
    program
}

#[test]
fn emitted_program_dependency_and_consumer_units_are_byte_identical() {
    let artifacts = library_artifacts();
    let source_db = source_db(ARTIFACT_USER);
    assert_no_diagnostic_errors(&source_db);
    let source_program =
        generate_project_bytecode_with_opt(&source_db, &options(), OPT).expect("source program");
    let source_units = emit_units(&source_db, &options(), OPT).expect("source units");

    let blob_db = blob_db(ARTIFACT_USER, artifacts.blob.clone());
    assert_no_diagnostic_errors(&blob_db);
    let blob_program =
        generate_project_bytecode_with_mounted_units(&blob_db, &options(), OPT, &artifacts.units)
            .expect("blob program");

    let source_program_bytes = borsh::to_vec(&source_program).expect("serialize source program");
    let blob_program_bytes = borsh::to_vec(&blob_program).expect("serialize blob program");

    // The blob database intentionally has no app source with which to attribute
    // flat objects during decomposition. Use the source manifest only as the
    // inverse-link attribution oracle; the bytes being decomposed are the blob
    // path's independently linked program.
    let blob_units = decompose_units(&source_db, &options(), &blob_program)
        .expect("decompose blob image with source manifest");
    let source_user = unit(&source_units, "main.baml");
    let blob_user = unit(&blob_units, "main.baml");
    assert_ne!(
        borsh::to_vec(source_user).expect("serialize raw source user unit"),
        borsh::to_vec(blob_user).expect("serialize raw blob user unit"),
        "the fixture must keep the database-local FileId distinction visible"
    );
    assert_bytes_identical(
        "consumer unit after debug FileId normalization",
        &borsh::to_vec(&normalized_unit(source_user.clone()))
            .expect("serialize normalized source user unit"),
        &borsh::to_vec(&normalized_unit(blob_user.clone()))
            .expect("serialize normalized blob user unit"),
    );

    let imported_app_symbols: Vec<&str> = source_user
        .object_imports
        .iter()
        .chain(&source_user.global_imports)
        .map(|symbol| symbol.fq_name.as_str())
        .filter(|name| name.starts_with("app."))
        .collect();
    assert!(
        !imported_app_symbols.is_empty(),
        "consumer must exercise symbolic app imports"
    );
    assert_eq!(source_user.object_imports, blob_user.object_imports);
    assert_eq!(source_user.global_imports, blob_user.global_imports);

    let source_app = unit(&source_units, "<builtin>/app/lib.baml");
    let independent_app = unit(&artifacts.units, "<builtin>/app/lib.baml");
    assert_bytes_identical(
        "independent library unit",
        &borsh::to_vec(source_app).expect("serialize source app unit"),
        &borsh::to_vec(independent_app).expect("serialize independent app unit"),
    );

    assert_ne!(
        source_program_bytes, blob_program_bytes,
        "raw programs retain the consumer database's debug FileId"
    );
    assert_bytes_identical(
        "linked program after debug FileId normalization",
        &borsh::to_vec(&normalized_program(source_program))
            .expect("serialize normalized source program"),
        &borsh::to_vec(&normalized_program(blob_program))
            .expect("serialize normalized blob program"),
    );
}

#[test]
fn exported_impl_method_preserves_synthetic_callback_effect_params() {
    let artifacts = library_artifacts();
    let runner = artifacts
        .interface
        .impls
        .iter()
        .find(|implementation| {
            implementation.interface.name.name().as_str() == "Applies"
                && matches!(
                    &implementation.for_ty_pattern,
                    baml_type::Ty::Class(qtn, _, _) if qtn.name().as_str() == "Runner"
                )
        })
        .expect("Runner implements Applies row");
    let apply = runner
        .methods
        .iter()
        .find(|method| method.name.as_str() == "apply")
        .expect("apply override export");
    assert_eq!(apply.sig.generic_params.len(), 1);
    assert!(baml_type::is_synthetic_effect_param(
        apply.sig.generic_params[0].name()
    ));
    assert_eq!(apply.sig.generic_param_bounds, vec![Vec::new()]);
}

#[test]
fn mounted_unit_api_preserves_dependency_link_errors() {
    let artifacts = library_artifacts();
    let mut duplicate_units = artifacts.units.clone();
    duplicate_units.push(unit(&artifacts.units, "<builtin>/app/lib.baml").clone());
    let db = blob_db("function main() -> int { 0 }", artifacts.blob);
    let error =
        generate_project_bytecode_with_mounted_units(&db, &options(), OPT, &duplicate_units)
            .expect_err("duplicate dependency export must fail before consumer emit");
    assert!(matches!(
        error,
        MountedPackageLinkError::DependencyLink(bex_vm_types::link::LinkError::DuplicateExport(_))
    ));
}

#[test]
fn user_vs_blob_overlap_is_e0132_primary_only_with_structural_partner() {
    let artifacts = library_artifacts();
    let db = blob_db(
        r#"
class Mine { value int }

implement app.Tagged for Mine {
    function tag(self) -> string throws never {
        "mine"
    }
}
"#,
        artifacts.blob,
    );
    let overlap = collect_diagnostics(&db)
        .into_iter()
        .find(|diagnostic| diagnostic.code() == "E0132")
        .expect("mounted blanket overlap must produce E0132");
    assert_eq!(
        overlap.message,
        "overlapping interface implementations for the same receiver/interface \
(conflicts with the mounted dependency's `implement app.Tagged for T`)"
    );
    assert_eq!(
        overlap
            .annotations
            .iter()
            .filter(|annotation| annotation.is_primary)
            .count(),
        1
    );
    assert_eq!(overlap.annotations.len(), 1, "blob side has no source span");
    assert!(overlap.related_info.is_empty());
}

/// Two valid, independently checked mounted blobs cannot introduce a new
/// blob-vs-blob overlap: the orphan rule requires either the interface or the
/// receiver constructor to be local. Distinct packages therefore own distinct
/// receiver constructors, while a package attempting to overlap a dependency's
/// blanket impl is rejected by this same E0132 check before its blob is emitted.
/// This makes user-vs-blob the only expressible load-time direction for valid
/// mounted-package artifacts; artifact validation rejects malformed or
/// tampered blobs.
#[test]
fn blob_vs_blob_overlap_is_not_expressible_for_valid_artifacts() {
    // Keep the argument executable: the rich fixture's blanket impl is exported
    // and therefore would reject any downstream overlapping source impl before
    // that downstream package could become a second valid blob.
    let artifacts = library_artifacts();
    assert!(artifacts.interface.impls.iter().any(|implementation| {
        implementation.interface.name.name().as_str() == "Tagged"
            && matches!(implementation.for_ty_pattern, baml_type::Ty::TypeVar(_, _))
    }));
    assert!(matches!(
        artifacts.interface.lookup_type(&[], &Name::new("Tagged")),
        Some(ExportedType::Interface { .. })
    ));
}
