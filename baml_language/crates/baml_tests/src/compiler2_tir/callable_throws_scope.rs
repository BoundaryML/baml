//! `callable_throws` lowers a written `throws` clause in the same scope as
//! the rest of the signature. `Self.Error` in an implements-block or free-impl
//! method resolves through the impl target's binding, and `T.Error` on a
//! bounded parameter resolves through `T`'s bound - a bare generic frame
//! cannot lower either, which used to leave the runtime throws metadata
//! wrong for exactly the methods that declare their error via an
//! associated type (e.g. `baml.iter.Iterator.collect`).

use baml_compiler2_ppir::item_data::MethodOwner;
use baml_db::ProjectDatabase;

use super::support::make_db;
use crate::engine::TestDbExt;

const SOURCE: &str = r#"
interface Fallible {
    type Error
    function run(self) -> int throws Self.Error
}

class Worker {
    implements Fallible {
        type Error = string
        function run(self) -> int throws Self.Error { throw "boom" }
    }
}

class Other {}

implement Fallible for Other {
    type Error = int
    function run(self) -> int throws Self.Error { throw 7 }
}

function drive<T extends Fallible>(w: T) -> int throws T.Error { w.run() }
"#;

/// The effective (`callable_throws`) error type of the function named `name`
/// whose owner's concrete `Self` renders as `owner_self` (`None` for a free
/// function), asserted equal to the signature road's `throws`, which lowers
/// in the full owner/bound scope. Types render as the test file's own package
/// sees them (its classes bare).
fn callable_throws(
    db: &ProjectDatabase,
    file: baml_base::SourceFile,
    name: &str,
    owner_self: Option<&str>,
) -> String {
    let viewer = baml_compiler2_hir::file_package::file_package(db, file).root;
    let vp = baml_compiler2_hir_ty::render::Viewpoint::user_facing(db, viewer);
    let loc = *baml_compiler2_ppir::item_data::file_functions(db, file)
        .iter()
        .find(|&&loc| {
            baml_compiler2_ppir::item_data::function_data(db, loc)
                .name
                .as_str()
                == name
                && baml_compiler2_ppir::item_data::method_owner(db, loc)
                    .as_ref()
                    .is_none_or(|owner| !matches!(owner, MethodOwner::Interface(_)))
                && baml_compiler2_hir_ty::lower::owner_self_ty(db, loc)
                    .map(|ty| ty.render_with(&vp))
                    .as_deref()
                    == owner_self
        })
        .unwrap_or_else(|| panic!("function `{name}` (self: {owner_self:?}) not found"));
    let effective = baml_compiler2_hir_ty::callable::callable_throws(db, loc)
        .0
        .render_with(&vp);
    let signature = baml_compiler2_hir_ty::lower::function_signature(db, loc)
        .throws
        .render_with(&vp);
    assert_eq!(
        effective, signature,
        "`{name}`: callable_throws must lower the clause in the signature's scope"
    );
    effective
}

#[test]
fn implements_block_method_throws_resolves_self_error_binding() {
    let mut db = make_db();
    let file = db.file("test.baml", SOURCE);
    assert_eq!(
        callable_throws(&db, file, "run", Some("Worker")),
        "(Worker as Fallible<Error = string>).Error"
    );
}

#[test]
fn free_impl_method_throws_resolves_self_error_binding() {
    let mut db = make_db();
    let file = db.file("test.baml", SOURCE);
    assert_eq!(
        callable_throws(&db, file, "run", Some("Other")),
        "(Other as Fallible<Error = int>).Error"
    );
}

#[test]
fn bounded_parameter_throws_resolves_through_its_bound() {
    let mut db = make_db();
    let file = db.file("test.baml", SOURCE);
    assert_eq!(
        callable_throws(&db, file, "drive", None),
        "(T as Fallible).Error"
    );
}
