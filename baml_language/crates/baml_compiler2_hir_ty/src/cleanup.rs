//! BEP-042 `cleanup` finalizer recognition.
//!
//! `cleanup` is a reserved method name: a class's own method called
//! `cleanup` is its finalizer, and must have the finalizer's shape. The shape
//! is judged here, on the checked signature rather than on its spelling, so
//! `-> void`, `-> null` and an alias of either are one return type.

use baml_compiler2_ast::cleanup_guard::CLEANUP_METHOD;
use baml_compiler2_hir::{
    item_data::{MethodOwner, function_data, method_owner},
    loc::FunctionLoc,
};
use baml_type::{Ty, normalize::TypeContext};

use crate::{facts::Facts, lower::function_signature};

/// What a class's own method named `cleanup` is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupMethod {
    /// The class's finalizer: it runs at most once per instance, whether
    /// called, deferred, or run by the collector.
    Finalizer,
    /// Not shaped as a finalizer, which the reserved name requires (E0144).
    Malformed,
}

/// Whether `function` is a class's finalizer. `None` for anything but a
/// class's own method named `cleanup`: a method of an `implements` block or
/// an interface, and a free function, may use the name freely.
///
/// The finalizer takes `self` alone, with no default and no generic
/// parameters of its own, returns the unit type, and throws nothing: a
/// collected instance has no caller to hand an error to. A `throws` clause
/// may be left out, or written as one that admits no error.
pub fn cleanup_method<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    function: FunctionLoc<'db>,
) -> Option<CleanupMethod> {
    let data = function_data(db, function);
    if data.name.as_str() != CLEANUP_METHOD {
        return None;
    }
    let Some(MethodOwner::Class(_)) = method_owner(db, function) else {
        return None;
    };
    let takes_only_self = matches!(
        data.params.as_slice(),
        [receiver] if receiver.name.as_str() == "self" && !receiver.has_default
    );
    let signature = function_signature(db, function);
    let facts = Facts::new(db);
    let returns_unit = facts.equivalent(&signature.ret, &Ty::Null);
    let throws_nothing = data.throws.is_none() || facts.equivalent(&signature.throws, &Ty::Never);
    Some(
        if data.generic_params.is_empty() && takes_only_self && returns_unit && throws_nothing {
            CleanupMethod::Finalizer
        } else {
            CleanupMethod::Malformed
        },
    )
}
