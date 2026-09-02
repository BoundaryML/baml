//! Operator dispatch through interfaces (README decision 4): `a + b` IS
//! `Implements(lhs, baml.ops.Add<rhs>)` with the impl's `Output` as the
//! result. Since I1 this is a thin facade over the real impl registry
//! (`crate::impls`) - blanket impls, in-class implements blocks, and
//! user-defined operator impls all resolve through the same path as
//! every other interface fact. Rewriting primitive cases to single
//! instructions is MIR's job at lowering, invisible to inference.

use baml_type::{
    Name, TypeName,
    interned::{InterfaceRef, Ty, TyKind},
};

use crate::impls::{resolve_impl, resolved_pin};

/// One operator's dispatch contract: the `baml.ops` interface the operator
/// desugars to and the interface method the dispatch invokes. The single
/// source of the operator table — inference's interface arms and the IDE's
/// operator hover/definition both read this instead of re-spelling it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperatorDispatch {
    /// Short interface name under `baml.ops` (`Add`, `Compare`, `Index`).
    pub interface: &'static str,
    /// The interface method the operator dispatches to (`add`, `lt`, `index`).
    pub method: &'static str,
}

/// The dispatch contract of a binary operator, or `None` for the structural
/// operators (`&&`/`||` short-circuit control flow, `??` null algebra) —
/// type algebra, not dispatch. Each comparison names its own interface
/// method (`<=` is `Compare.le`, `!=` is `Equals.neq`): the defaulted
/// members compose `lt`/`eq` exactly as MIR's lowering does, so the named
/// method is the one the reader would override. `==`/`!=` type structurally
/// but runtime dispatch honours a custom `Equals`, so that is their
/// contract too.
pub fn binary_operator(op: baml_compiler2_ast::BinaryOp) -> Option<OperatorDispatch> {
    use baml_compiler2_ast::BinaryOp;
    let dispatch = |interface, method| Some(OperatorDispatch { interface, method });
    match op {
        BinaryOp::Add => dispatch("Add", "add"),
        BinaryOp::Sub => dispatch("Subtract", "sub"),
        BinaryOp::Mul => dispatch("Multiply", "mul"),
        BinaryOp::Div => dispatch("Divide", "div"),
        BinaryOp::Mod => dispatch("Remainder", "rem"),
        BinaryOp::BitAnd => dispatch("BitAnd", "bit_and"),
        BinaryOp::BitOr => dispatch("BitOr", "bit_or"),
        BinaryOp::BitXor => dispatch("BitXor", "bit_xor"),
        BinaryOp::Shl => dispatch("ShiftLeft", "shl"),
        BinaryOp::Shr => dispatch("ShiftRight", "shr"),
        BinaryOp::Lt => dispatch("Compare", "lt"),
        BinaryOp::Le => dispatch("Compare", "le"),
        BinaryOp::Gt => dispatch("Compare", "gt"),
        BinaryOp::Ge => dispatch("Compare", "ge"),
        BinaryOp::Eq => dispatch("Equals", "eq"),
        BinaryOp::Ne => dispatch("Equals", "neq"),
        BinaryOp::And | BinaryOp::Or | BinaryOp::NullCoalesce => None,
    }
}

/// The dispatch contract of a compound assignment (`+=` steps through the
/// same interface as `+`). Every `AssignOp` dispatches.
pub fn assign_operator(op: baml_compiler2_ast::AssignOp) -> OperatorDispatch {
    use baml_compiler2_ast::AssignOp;
    let dispatch = |interface, method| OperatorDispatch { interface, method };
    match op {
        AssignOp::Add => dispatch("Add", "add"),
        AssignOp::Sub => dispatch("Subtract", "sub"),
        AssignOp::Mul => dispatch("Multiply", "mul"),
        AssignOp::Div => dispatch("Divide", "div"),
        AssignOp::Mod => dispatch("Remainder", "rem"),
        AssignOp::BitAnd => dispatch("BitAnd", "bit_and"),
        AssignOp::BitOr => dispatch("BitOr", "bit_or"),
        AssignOp::BitXor => dispatch("BitXor", "bit_xor"),
        AssignOp::Shl => dispatch("ShiftLeft", "shl"),
        AssignOp::Shr => dispatch("ShiftRight", "shr"),
    }
}

/// The dispatch contract of a unary operator: `-` dispatches `Negate.neg`;
/// `!` is structural boolean algebra.
pub fn unary_operator(op: baml_compiler2_ast::UnaryOp) -> Option<OperatorDispatch> {
    use baml_compiler2_ast::UnaryOp;
    match op {
        UnaryOp::Neg => Some(OperatorDispatch {
            interface: "Negate",
            method: "neg",
        }),
        UnaryOp::Not => None,
    }
}

/// `base[idx]` dispatches through `baml.ops.Index.index`.
pub const INDEX_DISPATCH: OperatorDispatch = OperatorDispatch {
    interface: "Index",
    method: "index",
};

/// The `Output` of the unique impl matching `baml.ops.<interface><rhs>`
/// for `lhs`, or `None` when the operands do not support the operator.
/// Operands must be resolved and literal-widened by the caller.
pub fn operator_output(
    db: &dyn baml_compiler2_ppir::Db,
    interface: &str,
    lhs: &Ty,
    rhs: Option<&Ty>,
) -> Option<Ty> {
    let resolved = resolve_impl(db, lhs, &operator_goal(interface, rhs))?;
    // Binding-else-default through the shared `leaf_def` read, so an
    // operator interface with a defaulted `Output` resolves like any
    // other associated member.
    resolved_pin(db, &resolved, lhs, &Name::new("Output"))
}

/// The impl goal an operator application poses: `baml.ops.<interface>` with
/// the rhs operand filling the single generic slot when present.
fn operator_goal(interface: &str, rhs: Option<&Ty>) -> InterfaceRef {
    InterfaceRef::new(
        TypeName::new(
            Name::new("baml"),
            vec![Name::new("ops")],
            Name::new(interface),
        ),
        rhs.cloned()
            .into_iter()
            .collect::<Vec<_>>()
            .into_boxed_slice(),
        Vec::new(),
    )
}

/// The method declaration an operator application dispatches to, for the
/// reader: the method the matching source impl provides when the receiver
/// resolves to one, else the `baml.ops` interface's own method declaration
/// (the static truth when dispatch is dynamic — union/existential/unbound
/// receivers — or when the impl adopts the default). Operand literals
/// widen here (`1 + 2` navigates like `int + int`); `None` only when the
/// interface itself is not in the database.
pub fn operator_method<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    dispatch: OperatorDispatch,
    lhs: &Ty,
    rhs: Option<&Ty>,
) -> Option<baml_compiler2_hir::loc::FunctionLoc<'db>> {
    let widen = |ty: &Ty| match ty.kind() {
        TyKind::Literal(literal, _, attr) => {
            Ty::intern(crate::infer::literal_base(literal, attr.clone()))
        }
        _ => ty.clone(),
    };
    let lhs = widen(lhs);
    let rhs = rhs.map(widen);
    let method_name = Name::new(dispatch.method);

    if let Some(resolved) = resolve_impl(db, &lhs, &operator_goal(dispatch.interface, rhs.as_ref()))
        && let Some(crate::impls::ProvidedMethod::Source { func, .. }) =
            resolved.provided_method(db, &method_name)
    {
        return Some(func);
    }

    // The interface's own declaration: default body or required signature —
    // both are real function items on the interface.
    let package = baml_compiler2_hir::package::PackageId::new(db, Name::new("baml"));
    let Some(baml_compiler2_hir::contributions::Definition::Interface(iface)) =
        baml_compiler2_ppir::package_items(db, package)
            .lookup_type(&[Name::new("ops")], &Name::new(dispatch.interface))
    else {
        return None;
    };
    baml_compiler2_ppir::item_data::interface_data(db, iface)
        .methods
        .iter()
        .copied()
        .find(|&method| {
            baml_compiler2_ppir::item_data::function_data(db, method).name == method_name
        })
}
