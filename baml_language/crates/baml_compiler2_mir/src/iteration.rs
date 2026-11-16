//! Recognition shared by escape analysis and native array iteration.

use baml_base::LangPackage;

use crate::{Operand, Place, Terminator};

/// A unary call to the installed standard library's `Iterable.iter` or
/// `Iterator.next`, as `(method, receiver local, destination local)`.
///
/// The interface head is a declaration identity (package root and path), not
/// its display spelling. An unrelated interface with an `iter` or `next`
/// method must retain ordinary virtual dispatch and its unknown effects.
/// Callers must additionally prove that the receiver is an array or its
/// builtin iterator before replacing the call with array operations.
pub fn unary_iteration_call<'a>(
    db: &dyn crate::Db,
    terminator: &'a Terminator<'_>,
) -> Option<(&'a str, usize, usize)> {
    let Terminator::VirtualCall {
        has_trace: false,
        argument_layout,
        iface,
        method,
        args,
        ntypeargs: 0,
        self_arg: 0,
        destination: Place::Local(destination),
        ..
    } = terminator
    else {
        return None;
    };
    let baml = baml_compiler2_hir::package::lang_roots(db).get(LangPackage::Baml)?;
    if iface.name.root() != baml
        || !matches!(iface.name.namespace().as_slice(), [namespace] if namespace == "iter")
        || !matches!(
            (iface.name.name().as_str(), method.as_str()),
            ("Iterable", "iter") | ("Iterator", "next")
        )
        || !iface.generics.is_empty()
        || argument_layout
            .as_ref()
            .is_some_and(|layout| layout.0.iter().any(Option::is_some))
    {
        return None;
    }
    let [Operand::Copy(Place::Local(receiver)) | Operand::Move(Place::Local(receiver))] =
        args.as_slice()
    else {
        return None;
    };
    Some((method, receiver.0, destination.0))
}
