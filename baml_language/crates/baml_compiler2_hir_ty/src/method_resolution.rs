//! Method resolution: receiver type -> method candidate, the
//! rust-analyzer `method_resolution.rs` analog (S11). BAML's version has
//! no autoderef/autoref chains: a receiver resolves to exactly one owning
//! class - its own for nominal receivers, the language's builtin class for
//! structural ones (`int[]` methods live on `class baml.Array<T>`,
//! `string`'s on `class baml.String`, and so on) - and the receiver's
//! structure supplies the class generic arguments.
//!
//! Not yet resolved here (later slices): methods via `implements` blocks
//! and interface-existential/type-var receivers (the I cluster), union
//! receivers, and `$stream` companions.

use baml_compiler2_hir::{contributions::Definition, loc::ClassLoc, loc::FunctionLoc};
use baml_type::{
    Literal, MediaKind, Name, TypeName,
    interned::{Ty, TyKind},
    normalize::TypeContext as _,
};

use crate::facts::Facts;

/// One resolved method: the function plus the receiver-driven
/// instantiation of its owning class's generic params (the frame prefix
/// that `function_generic_frame` prepends for methods).
pub struct MethodCandidate<'db> {
    pub method: FunctionLoc<'db>,
    pub class_args: Vec<Ty>,
}

/// Finds `name` among the methods of `receiver`'s owning class. The
/// receiver must already be structurally resolved (no top-level inference
/// var); aliases expand through the fact oracle.
pub fn lookup_method<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    facts: &Facts<'db>,
    receiver: &Ty,
    name: &Name,
) -> Option<MethodCandidate<'db>> {
    let (class, class_args) = receiver_class(facts, receiver, 8)?;
    let method = baml_compiler2_ppir::item_data::class_data(db, class)
        .methods
        .iter()
        .copied()
        .find(|&method| {
            baml_compiler2_ppir::item_data::function_data(db, method).name == *name
        })?;
    Some(MethodCandidate { method, class_args })
}

/// The class whose declaration owns `receiver`'s methods, with the generic
/// arguments the receiver pins. This table IS the language's builtin-class
/// correspondence (TIR: `resolve_builtin_member` call sites), one row per
/// structural kind; literals defer to their base primitive's class.
fn receiver_class<'db>(
    facts: &Facts<'db>,
    receiver: &Ty,
    fuel: u32,
) -> Option<(ClassLoc<'db>, Vec<Ty>)> {
    let builtin = |namespace: &[&str], name: &str, args: Vec<Ty>| {
        let qtn = TypeName::new(
            Name::new("baml"),
            namespace.iter().map(Name::new).collect(),
            Name::new(name),
        );
        match facts.definition_of(&qtn) {
            Some(Definition::Class(class)) => Some((class, args)),
            _ => None,
        }
    };
    match receiver.kind() {
        TyKind::Class(qtn, args, _) => match facts.definition_of(qtn) {
            Some(Definition::Class(class)) => Some((class, args.to_vec())),
            _ => None,
        },
        TyKind::List(element, _) => builtin(&[], "Array", vec![element.clone()]),
        TyKind::Map { key, value, .. } => builtin(&[], "Map", vec![key.clone(), value.clone()]),
        TyKind::Future(value, error, _) => {
            builtin(&["future"], "Future", vec![value.clone(), error.clone()])
        }
        TyKind::String { .. } | TyKind::Literal(Literal::String(_), _, _) => {
            builtin(&[], "String", Vec::new())
        }
        TyKind::Int { .. } | TyKind::Literal(Literal::Int(_), _, _) => {
            builtin(&[], "Int", Vec::new())
        }
        TyKind::Bigint { .. } | TyKind::Literal(Literal::Bigint(_), _, _) => {
            builtin(&[], "Bigint", Vec::new())
        }
        TyKind::Float { .. } | TyKind::Literal(Literal::Float(_), _, _) => {
            builtin(&[], "Float", Vec::new())
        }
        TyKind::Bool { .. } | TyKind::Literal(Literal::Bool(_), _, _) => {
            builtin(&[], "Bool", Vec::new())
        }
        TyKind::Uint8Array { .. } => builtin(&[], "Uint8Array", Vec::new()),
        TyKind::Media(kind, _) => {
            let class = match kind {
                MediaKind::Image => "Image",
                MediaKind::Audio => "Audio",
                MediaKind::Video => "Video",
                MediaKind::Pdf => "Pdf",
                // Generic media (`media`, any subtype) has no single class.
                MediaKind::Generic => return None,
            };
            builtin(&["media"], class, Vec::new())
        }
        // Aliases are transparent: expand through the oracle (fuel-bounded
        // like every alias walk) and resolve on the expansion.
        TyKind::TypeAlias(qtn, _) => {
            let expanded = facts.alias_def(qtn)?;
            let fuel = fuel.checked_sub(1)?;
            receiver_class(facts, &Ty::from_plain(&expanded), fuel)
        }
        _ => None,
    }
}
