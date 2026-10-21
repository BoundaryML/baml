//! The emitter's session mode: a submission's unit holds the submission's
//! own declarations and reaches the session package's earlier ones as
//! imports at `DepSlot::SELF`; its `let`s are recorded initializers rather
//! than an `$init`.

use baml_compiler2_emit::{OptLevel, emit_session_submission};
use baml_db::ProjectDatabase;
use baml_linker_types::{ImportEntry, SessionInitializer};
use baml_tests::engine::TestDbExt;
use bex_vm_types::{DeclPath, FnPath, types::LocalName};

fn local(name: &str) -> LocalName {
    LocalName {
        namespace: Vec::new(),
        name: baml_base::Name::new(name),
    }
}

fn self_imports(entries: &[ImportEntry]) -> Vec<DeclPath> {
    entries
        .iter()
        .filter(|entry| entry.key.dep.is_self())
        .map(|entry| entry.key.path.clone())
        .collect()
}

#[test]
fn a_submission_owns_its_file_and_imports_earlier_submissions_at_self() {
    let mut db = ProjectDatabase::new();
    let root = db.workspace(std::path::Path::new("/session-emit"));
    db.add_session_file_in(
        root,
        "$submission_0.baml",
        "class Earlier { v int }\nfunction earlier() -> int { 1 }\nlet base = 2\n",
    );
    let submission = db.add_session_file_in(
        root,
        "$submission_1.baml",
        "class Later { e Earlier }\nfunction later() -> int { earlier() + base }\nlet x = later()\nlet y = x + 1\n",
    );
    baml_db::testing::assert_no_diagnostic_errors(&db);

    let emitted = emit_session_submission(&db, root, submission, OptLevel::One)
        .expect("the submission emits");
    let unit = &emitted.package.unit;

    // Own declarations: the submission's file only.
    let exported: Vec<&DeclPath> = unit.exports.objects.iter().map(|(path, _)| path).collect();
    assert!(
        exported.contains(&&DeclPath::Class(local("Later"))),
        "{exported:?}"
    );
    assert!(
        !exported.contains(&&DeclPath::Class(local("Earlier"))),
        "{exported:?}"
    );
    let slotted: Vec<&DeclPath> = unit.exports.globals.iter().map(|(path, _)| path).collect();
    assert!(
        slotted.contains(&&DeclPath::Function(FnPath::Free(local("later")))),
        "{slotted:?}"
    );
    assert!(slotted.contains(&&DeclPath::Let(local("x"))), "{slotted:?}");
    assert!(
        !slotted.contains(&&DeclPath::Let(local("base"))),
        "{slotted:?}"
    );

    // The earlier submission's declarations are imports at SELF.
    let objects = self_imports(&unit.object_imports);
    assert!(
        objects.contains(&DeclPath::Class(local("Earlier"))),
        "{objects:?}"
    );
    let globals = self_imports(&unit.global_imports);
    assert!(
        globals.contains(&DeclPath::Function(FnPath::Free(local("earlier")))),
        "{globals:?}"
    );
    assert!(
        globals.contains(&DeclPath::Let(local("base"))),
        "{globals:?}"
    );

    // The tail: one helper per own `let` in dependency order, no `$init`,
    // and the steps recorded for the session to run.
    let tail = emitted
        .package
        .tail
        .as_ref()
        .expect("the submission's lets make a tail");
    assert!(
        tail.init.is_none(),
        "a submission's tail carries no `$init`"
    );
    assert_eq!(tail.slot_objects.len(), 2);
    assert_eq!(
        emitted.initializers,
        vec![
            SessionInitializer {
                helper: 0,
                target: local("x"),
            },
            SessionInitializer {
                helper: 1,
                target: local("y"),
            },
        ]
    );
}
