//! The value kinds native code can hold today, and the typing rules of the
//! operators over them.
//!
//! Both the support check and code generation go through these functions, so
//! a construct is accepted exactly when code can be generated for it.

use baml_compiler2_mir::{BinOp, Constant, MirFunctionBody, Operand, Place, RuntimeTy, UnaryOp};
use baml_type::Literal;

use crate::Blocker;

/// A value that lives in a machine register with no heap part.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Scalar {
    /// A 63-bit signed integer, sign-extended in a 64-bit register.
    Int,
    Float,
    Bool,
    /// The one value of the unit type. It carries no information, so it is
    /// never kept in a variable.
    Null,
}

pub(crate) fn scalar_of(ty: &RuntimeTy) -> Option<Scalar> {
    match ty {
        RuntimeTy::Int | RuntimeTy::Literal(Literal::Int(_), _) => Some(Scalar::Int),
        RuntimeTy::Float | RuntimeTy::Literal(Literal::Float(_), _) => Some(Scalar::Float),
        RuntimeTy::Bool | RuntimeTy::Literal(Literal::Bool(_), _) => Some(Scalar::Bool),
        RuntimeTy::Null => Some(Scalar::Null),
        _ => None,
    }
}

/// A coarse name for a type, for grouping what blocks compilation.
pub(crate) fn type_kind(ty: &RuntimeTy) -> &'static str {
    match ty {
        RuntimeTy::Int => "int",
        RuntimeTy::Float => "float",
        RuntimeTy::Bool => "bool",
        RuntimeTy::Null => "null",
        RuntimeTy::Bigint => "bigint",
        RuntimeTy::String => "string",
        RuntimeTy::Media(..) => "media",
        RuntimeTy::Literal(Literal::String(_), _) => "string",
        RuntimeTy::Literal(Literal::Bigint(_), _) => "bigint",
        RuntimeTy::Literal(..) => "literal",
        RuntimeTy::Class(..) => "class",
        RuntimeTy::Interface(..) => "interface",
        RuntimeTy::Enum(..) | RuntimeTy::EnumVariant(..) => "enum",
        RuntimeTy::List(..) => "list",
        RuntimeTy::Map { .. } => "map",
        RuntimeTy::Union(..) => "union",
        RuntimeTy::Function { .. } => "function",
        RuntimeTy::Future(..) => "future",
        RuntimeTy::TypeVar(..) => "type variable",
        RuntimeTy::Never => "never",
        RuntimeTy::Unknown => "unknown",
        _ => "other",
    }
}

pub(crate) fn local_scalar(
    body: &MirFunctionBody<'_>,
    local: baml_compiler2_mir::Local,
) -> Result<Scalar, Blocker> {
    let decl = body.local(local);
    if decl.is_captured {
        return Err(Blocker::new("closure-captured local", local.to_string()));
    }
    scalar_of(&decl.ty)
        .ok_or_else(|| Blocker::new(format!("{} value", type_kind(&decl.ty)), local.to_string()))
}

/// The local a place names, when it is a plain local.
pub(crate) fn place_local(place: &Place) -> Result<baml_compiler2_mir::Local, Blocker> {
    match place {
        Place::Local(local) => Ok(*local),
        Place::Field { .. } => Err(Blocker::new("field access", place.to_string())),
        Place::Index { .. } => Err(Blocker::new("index access", place.to_string())),
        Place::Capture(_) | Place::Deref(_) => {
            Err(Blocker::new("closure cell access", place.to_string()))
        }
    }
}

pub(crate) fn place_scalar(body: &MirFunctionBody<'_>, place: &Place) -> Result<Scalar, Blocker> {
    local_scalar(body, place_local(place)?)
}

pub(crate) fn constant_scalar(constant: &Constant<'_>) -> Result<Scalar, Blocker> {
    match constant {
        Constant::Int(_) => Ok(Scalar::Int),
        Constant::Float(_) => Ok(Scalar::Float),
        Constant::Bool(_) => Ok(Scalar::Bool),
        Constant::Null => Ok(Scalar::Null),
        Constant::Bigint(_) => Err(Blocker::new("bigint value", "constant")),
        Constant::String(_) => Err(Blocker::new("string value", "constant")),
        Constant::OmittedArg => Err(Blocker::new("defaulted argument", "")),
        Constant::Function(_) | Constant::GenericFunction { .. } => {
            Err(Blocker::new("function value", "constant"))
        }
        Constant::GlobalItem(_) => Err(Blocker::new("global read", "")),
        Constant::EnumVariant { .. } => Err(Blocker::new("enum value", "constant")),
    }
}

pub(crate) fn operand_scalar(
    body: &MirFunctionBody<'_>,
    operand: &Operand<'_>,
) -> Result<Scalar, Blocker> {
    match operand {
        Operand::Copy(place) | Operand::Move(place) => place_scalar(body, place),
        Operand::Constant(constant) => constant_scalar(constant),
    }
}

/// How a binary operator is evaluated for a pair of operand kinds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum BinaryMode {
    /// Both operands are ints.
    Int,
    /// At least one operand is a float; an int operand is converted first.
    Float,
    /// Both operands are bools; only `==` and `!=`.
    Bool,
    /// Both operands are null; only `==` and `!=`.
    Null,
}

pub(crate) fn is_comparison(op: BinOp) -> bool {
    matches!(
        op,
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
    )
}

/// The evaluation mode and result kind of `left op right`.
pub(crate) fn binary_typing(
    op: BinOp,
    left: Scalar,
    right: Scalar,
) -> Result<(BinaryMode, Scalar), Blocker> {
    let unsupported = || {
        Blocker::new(
            "binary operator on these operand types",
            format!("{left:?} {op} {right:?}"),
        )
    };
    let equality = matches!(op, BinOp::Eq | BinOp::Ne);
    let mode = match (left, right) {
        (Scalar::Int, Scalar::Int) => BinaryMode::Int,
        (Scalar::Float, Scalar::Float | Scalar::Int) | (Scalar::Int, Scalar::Float) => {
            if matches!(
                op,
                BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr
            ) {
                return Err(unsupported());
            }
            BinaryMode::Float
        }
        (Scalar::Bool, Scalar::Bool) if equality => BinaryMode::Bool,
        (Scalar::Null, Scalar::Null) if equality => BinaryMode::Null,
        _ => return Err(unsupported()),
    };
    let result = if is_comparison(op) {
        Scalar::Bool
    } else {
        match mode {
            BinaryMode::Int => Scalar::Int,
            BinaryMode::Float => Scalar::Float,
            BinaryMode::Bool | BinaryMode::Null => return Err(unsupported()),
        }
    };
    Ok((mode, result))
}

/// The result kind of `op operand`.
pub(crate) fn unary_typing(op: UnaryOp, operand: Scalar) -> Result<Scalar, Blocker> {
    match (op, operand) {
        (UnaryOp::Not | UnaryOp::Truthy, _) => Ok(Scalar::Bool),
        (UnaryOp::Neg, Scalar::Int) => Ok(Scalar::Int),
        (UnaryOp::Neg, Scalar::Float) => Ok(Scalar::Float),
        (UnaryOp::Neg, _) => Err(Blocker::new(
            "unary operator on this operand type",
            format!("{op}{operand:?}"),
        )),
    }
}
