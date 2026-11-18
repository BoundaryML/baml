//! The types native code can hold today, how values of them are represented,
//! and the typing rules of the operators over them.
//!
//! Both the support check and code generation go through these functions, so
//! a construct is accepted exactly when code can be generated for it.

use std::cell::Cell;

use baml_compiler2_hir::loc::DeclRef;
use baml_compiler2_hir_ty::{
    extern_loc::{ClassRef, EnumRef},
    layout::{class_fields, class_generic_params, class_head, class_ref_of, enum_ref_of},
};
use baml_compiler2_mir::{
    BinOp, CellId, Constant, IndexKind, Local, MirFunction, MirFunctionBody, Operand, Place,
    RuntimeTy, Rvalue, StatementKind, UnaryOp, resolved_aliases_for_package,
};
use baml_type::Literal;

use crate::{Blocker, check::function_signature, flow::terminator_destination, loops::ArrayLoops};

/// Parameter and result types of a function with a native calling convention.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct Signature<'db> {
    pub(crate) params: Vec<NativeTy<'db>>,
    pub(crate) result: NativeTy<'db>,
}

/// What a value is to the machine: which register class it travels in and
/// whether the collector follows it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Repr {
    Int,
    Float,
    Byte,
    Ref,
    Pair,
}

/// A type whose values native code knows the representation of. There are no
/// tags: the static type alone says what the bits mean.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum NativeTy<'db> {
    /// A signed 64-bit integer.
    Int,
    Float,
    Bool,
    /// The one value of the unit type. It carries no information, so it is
    /// never kept in a variable.
    Null,
    /// A variant of this enum: its position in the declaration, as an int.
    Enum(EnumRef<'db>),
    /// A reference to an immutable string object.
    String,
    /// A reference to an instance of a class without type parameters.
    Class(ClassRef<'db>),
    /// A reference to a growable array of the element type.
    Array(Box<NativeTy<'db>>),
    /// A reference to a closure: a function value. Calling it passes the
    /// closure and then arguments of these types.
    Function(Box<Signature<'db>>),
    /// A reference to the cell holding a local that a closure captures.
    /// Never the type of a source value: only of the local itself.
    Cell(Box<NativeTy<'db>>),
    /// One of these types, none of them a union and at most one `null`: a
    /// reference, zero for `null`, whose object's header says which. An
    /// `int`, `float`, `bool` or enum variant is in a box.
    Union(Vec<NativeTy<'db>>),
    /// Any value at all, in the representation of a union.
    Unknown,
    /// A type or `null`. For a reference type it is the same reference,
    /// with zero for `null`. For an `int`, `float` or `bool` it is a pair of
    /// words: the value's bits, and whether there is a value at all.
    Optional(Box<NativeTy<'db>>),
}

impl NativeTy<'_> {
    /// Whether a value is a pointer the collector must know about.
    pub(crate) fn is_ref(&self) -> bool {
        match self {
            Self::String
            | Self::Class(_)
            | Self::Array(_)
            | Self::Function(_)
            | Self::Cell(_)
            | Self::Union(_)
            | Self::Unknown => true,
            Self::Optional(inner) => inner.is_ref(),
            Self::Int | Self::Float | Self::Bool | Self::Null | Self::Enum(_) => false,
        }
    }

    /// Whether a value says what it is in its object's header: a union or
    /// `unknown`.
    pub(crate) fn is_boxed(&self) -> bool {
        matches!(self, Self::Union(_) | Self::Unknown)
    }

    /// Whether a value is an optional scalar: a pair of words.
    pub(crate) fn is_pair(&self) -> bool {
        matches!(self, Self::Optional(inner) if !inner.is_ref())
    }

    pub(crate) fn repr(&self) -> Repr {
        match self {
            Self::Int | Self::Enum(_) => Repr::Int,
            Self::Float => Repr::Float,
            Self::Bool | Self::Null => Repr::Byte,
            _ if self.is_pair() => Repr::Pair,
            _ => Repr::Ref,
        }
    }

    /// How many slots a value takes in an object.
    pub(crate) fn slots(&self) -> usize {
        if self.is_pair() { 2 } else { 1 }
    }
}

pub(crate) fn native_ty<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    ty: &RuntimeTy,
) -> Option<NativeTy<'db>> {
    Some(match ty {
        RuntimeTy::Int | RuntimeTy::Literal(Literal::Int(_), _) => NativeTy::Int,
        RuntimeTy::Float | RuntimeTy::Literal(Literal::Float(_), _) => NativeTy::Float,
        RuntimeTy::Bool | RuntimeTy::Literal(Literal::Bool(_), _) => NativeTy::Bool,
        RuntimeTy::Null => NativeTy::Null,
        RuntimeTy::Enum(head) | RuntimeTy::EnumVariant(head, _) => {
            NativeTy::Enum(enum_ref_of(db, head)?)
        }
        RuntimeTy::Unknown => NativeTy::Unknown,
        RuntimeTy::String | RuntimeTy::Literal(Literal::String(_), _) => NativeTy::String,
        RuntimeTy::Class(head, arguments) if arguments.is_empty() => {
            let class = class_ref_of(db, head)?;
            // A class with type parameters has no single layout.
            if !class_generic_params(db, class).is_empty() {
                return None;
            }
            NativeTy::Class(class)
        }
        RuntimeTy::List(element) => {
            let element = native_ty(db, element)?;
            // Elements are one slot each so far.
            if element.is_pair() {
                return None;
            }
            NativeTy::Array(Box::new(element))
        }
        RuntimeTy::Function { params, ret, .. } => {
            let params = params
                .iter()
                .map(|param| native_ty(db, &param.ty))
                .collect::<Option<_>>()?;
            let result = native_ty(db, ret)?;
            NativeTy::Function(Box::new(Signature { params, result }))
        }
        RuntimeTy::Union(members) => {
            let mut flat = Vec::new();
            for member in members {
                let members = match native_ty(db, member)? {
                    NativeTy::Unknown => return Some(NativeTy::Unknown),
                    NativeTy::Union(members) => members,
                    NativeTy::Optional(inner) => vec![*inner, NativeTy::Null],
                    other => vec![other],
                };
                for member in members {
                    if !flat.contains(&member) {
                        flat.push(member);
                    }
                }
            }
            match &flat[..] {
                [only] => only.clone(),
                // One type or `null` needs no header to tell which.
                [NativeTy::Null, inner] | [inner, NativeTy::Null] => {
                    NativeTy::Optional(Box::new(inner.clone()))
                }
                _ => NativeTy::Union(flat),
            }
        }
        _ => return None,
    })
}

/// A coarse name for a type, for grouping what blocks compilation.
pub(crate) fn type_kind(ty: &RuntimeTy) -> &'static str {
    match ty {
        RuntimeTy::Int => "int",
        RuntimeTy::Float => "float",
        RuntimeTy::Bool => "bool",
        RuntimeTy::Null => "null",
        RuntimeTy::Bigint | RuntimeTy::Literal(Literal::Bigint(_), _) => "bigint",
        RuntimeTy::String | RuntimeTy::Literal(Literal::String(_), _) => "string",
        RuntimeTy::Media(..) => "media",
        RuntimeTy::Literal(..) => "literal",
        RuntimeTy::Class(_, arguments) if arguments.is_empty() => "class",
        RuntimeTy::Class(..) => "generic class",
        RuntimeTy::Interface(..) => "interface",
        RuntimeTy::Enum(..) | RuntimeTy::EnumVariant(..) => "enum",
        RuntimeTy::List(..) => "list of unsupported elements",
        RuntimeTy::Map { .. } => "map",
        RuntimeTy::Union(members)
            if members
                .iter()
                .any(|member| matches!(member, RuntimeTy::Null)) =>
        {
            "optional scalar or union"
        }
        RuntimeTy::Union(..) => "union",
        RuntimeTy::Function { .. } => "function",
        RuntimeTy::Future(..) => "future",
        RuntimeTy::TypeVar(..) => "type variable",
        RuntimeTy::Never => "never",
        RuntimeTy::Unknown => "unknown",
        _ => "other",
    }
}

fn unsupported_type(ty: &RuntimeTy, detail: impl Into<String>) -> Blocker {
    Blocker::new(format!("{} value", type_kind(ty)), detail)
}

/// The slot each field of a layout starts at: fields take their slots in
/// declaration order.
pub(crate) fn field_slots(layout: &[NativeTy<'_>]) -> Vec<usize> {
    layout
        .iter()
        .scan(0, |next, field| {
            let slot = *next;
            *next += field.slots();
            Some(slot)
        })
        .collect()
}

/// How many slots an instance with this layout has.
pub(crate) fn layout_slots(layout: &[NativeTy<'_>]) -> usize {
    layout.iter().map(NativeTy::slots).sum()
}

/// The types of the fields of a class, in declaration order.
pub(crate) fn class_layout<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    class: ClassRef<'db>,
) -> Result<Vec<NativeTy<'db>>, Blocker> {
    let head = class_head(db, class);
    let aliases = resolved_aliases_for_package(db, head.root());
    class_fields(db, class)
        .iter()
        .map(|(name, ty)| {
            let ty = aliases.0.convert(ty);
            native_ty(db, &ty).ok_or_else(|| {
                Blocker::new(
                    format!(
                        "class field that is a {}",
                        unsupported_type(&ty, "").construct
                    ),
                    name.to_string(),
                )
            })
        })
        .collect()
}

/// A function body together with the types of the locals whose declarations
/// do not give one.
///
/// After the optimizer has replaced an object by its fields, a temporary can
/// be left declared with the type of something it no longer holds, or as
/// `unknown`. Such a local is only ever a copy of other values, so its type
/// is theirs wherever its declaration could not hold them.
pub(crate) struct Body<'a, 'db> {
    /// The function the body belongs to: closures name its lambdas.
    pub(crate) mir: &'a MirFunction<'db>,
    inner: &'a MirFunctionBody<'db>,
    /// What the cells a lambda captures hold, in capture order.
    captures: &'a [NativeTy<'db>],
    inferred: Vec<Option<NativeTy<'db>>>,
    /// The locals an array loop keeps as an array and a position; see
    /// [`crate::loops`].
    loop_locals: Vec<bool>,
    /// For each block, the member each local of a union type is known to
    /// hold throughout it; see [`crate::narrow`].
    narrowed: Vec<Vec<Option<NativeTy<'db>>>>,
    /// Whether any run reaches each block.
    reachable: Vec<bool>,
    /// The block whose code is being typed, if any is.
    at: Cell<Option<usize>>,
}

impl<'db> std::ops::Deref for Body<'_, 'db> {
    type Target = MirFunctionBody<'db>;

    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

impl<'a, 'db> Body<'a, 'db> {
    pub(crate) fn new(
        db: &'db dyn baml_compiler2_mir::Db,
        mir: &'a MirFunction<'db>,
        body: &'a MirFunctionBody<'db>,
        captures: &'a [NativeTy<'db>],
    ) -> Self {
        let mut typed = Self {
            mir,
            inner: body,
            captures,
            inferred: vec![None; body.locals.len()],
            loop_locals: vec![false; body.locals.len()],
            narrowed: Vec::new(),
            reachable: vec![true; body.blocks.len()],
            at: Cell::new(None),
        };
        // A local written any other way than by copying has the type it is
        // declared with.
        let mut copied_only = vec![true; body.locals.len()];
        for parameter in copied_only.iter_mut().take(mir.arity + 1) {
            *parameter = false;
        }
        for block in &body.blocks {
            for statement in &block.statements {
                match &statement.kind {
                    StatementKind::Assign {
                        destination: Place::Local(local),
                        value,
                    } if !matches!(value, Rvalue::Use(_)) => copied_only[local.0] = false,
                    StatementKind::FreshCell { local, .. } => copied_only[local.0] = false,
                    _ => {}
                }
            }
            if let Some(local) = block.terminator.as_ref().and_then(terminator_destination) {
                copied_only[local.0] = false;
            }
        }
        // A local assigned values of two different types has no type.
        let mut conflicting = vec![false; body.locals.len()];
        loop {
            let mut changed = false;
            for statement in body.blocks.iter().flat_map(|block| &block.statements) {
                let StatementKind::Assign {
                    destination: Place::Local(local),
                    value: Rvalue::Use(operand),
                } = &statement.kind
                else {
                    continue;
                };
                let declaration = body.local(*local);
                if declaration.is_captured || conflicting[local.0] || !copied_only[local.0] {
                    continue;
                }
                let Ok(found) = operand_ty(db, &typed, operand) else {
                    continue;
                };
                // A declaration that can hold what is copied stands.
                if native_ty(db, &declaration.ty)
                    .is_some_and(|declared| assignable(&declared, &found))
                {
                    continue;
                }
                match &typed.inferred[local.0] {
                    None => {
                        typed.inferred[local.0] = Some(found);
                        changed = true;
                    }
                    Some(known) if *known != found => {
                        typed.inferred[local.0] = None;
                        conflicting[local.0] = true;
                        changed = true;
                    }
                    Some(_) => {}
                }
            }
            if !changed {
                break;
            }
        }
        let loops = ArrayLoops::of(db, &typed);
        for (local, is_loop_local) in typed.loop_locals.iter_mut().enumerate() {
            *is_loop_local =
                loops.iterator(Local(local)).is_some() || loops.item(Local(local)).is_some();
        }
        let found = crate::narrow::block_types(db, &typed);
        typed.narrowed = found.narrowed;
        typed.reachable = found.reachable;
        typed
    }

    /// Whether any run reaches `block`. One that none does is not compiled.
    pub(crate) fn is_reachable(&self, block: usize) -> bool {
        self.reachable[block]
    }

    /// Type what follows as the code of `block`.
    pub(crate) fn enter(&self, block: usize) {
        self.at.set(Some(block));
    }
}

/// The type a local is declared with, when it is a native one. A local
/// that a closure captures is a cell of its declared type.
pub(crate) fn declared_ty<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    body: &MirFunctionBody<'_>,
    local: Local,
) -> Result<NativeTy<'db>, Blocker> {
    let declaration = body.local(local);
    let ty = native_ty(db, &declaration.ty)
        .ok_or_else(|| unsupported_type(&declaration.ty, local.to_string()))?;
    Ok(if declaration.is_captured {
        NativeTy::Cell(Box::new(ty))
    } else {
        ty
    })
}

/// What the cell a place names holds.
fn cell_content<'db>(ty: NativeTy<'db>, place: &Place) -> Result<NativeTy<'db>, Blocker> {
    match ty {
        NativeTy::Cell(content) => Ok(*content),
        _ => Err(Blocker::new(
            "closure cell access outside a cell",
            place.to_string(),
        )),
    }
}

/// The type of the value a local holds in the block being typed: for a local
/// of a union type, the member it is known to hold there, if one is.
pub(crate) fn local_ty<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    body: &Body<'_, 'db>,
    local: Local,
) -> Result<NativeTy<'db>, Blocker> {
    if let Some(block) = body.at.get()
        && let Some(Some(narrowed)) = body
            .narrowed
            .get(block)
            .map(|locals| locals[local.0].as_ref())
    {
        return Ok(narrowed.clone());
    }
    storage_ty(db, body, local)
}

/// The type of the variable a local is kept in.
pub(crate) fn storage_ty<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    body: &Body<'_, 'db>,
    local: Local,
) -> Result<NativeTy<'db>, Blocker> {
    if body.loop_locals[local.0] {
        return Err(Blocker::new(
            "array loop local used outside its loop",
            local.to_string(),
        ));
    }
    match &body.inferred[local.0] {
        Some(inferred) if !body.local(local).is_captured => Ok(inferred.clone()),
        _ => declared_ty(db, body, local),
    }
}

/// The local a place names, when it is a plain local.
pub(crate) fn place_local(place: &Place) -> Result<Local, Blocker> {
    match place {
        Place::Local(local) => Ok(*local),
        other => Err(Blocker::new(
            "call result stored outside a local",
            other.to_string(),
        )),
    }
}

/// The reference type behind an optional. MIR projects through an optional
/// only where the checker proved it is not null.
pub(crate) fn narrowed(ty: NativeTy<'_>) -> NativeTy<'_> {
    match ty {
        NativeTy::Optional(inner) => *inner,
        other => other,
    }
}

/// The type of the value a place holds.
pub(crate) fn place_ty<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    body: &Body<'_, 'db>,
    place: &Place,
) -> Result<NativeTy<'db>, Blocker> {
    match place {
        Place::Local(local) => local_ty(db, body, *local),
        Place::Field { base, field } => match narrowed(place_ty(db, body, base)?) {
            NativeTy::Class(class) => class_layout(db, class)?
                .into_iter()
                .nth(*field)
                .ok_or_else(|| Blocker::new("field outside its class layout", place.to_string())),
            _ => Err(Blocker::new(
                "field access on a non-class value",
                place.to_string(),
            )),
        },
        Place::Index {
            base,
            index,
            kind: IndexKind::Array,
        } => {
            if local_ty(db, body, *index)? != NativeTy::Int {
                return Err(Blocker::new(
                    "array index that is not an int",
                    place.to_string(),
                ));
            }
            match narrowed(place_ty(db, body, base)?) {
                NativeTy::Array(element) => Ok(*element),
                _ => Err(Blocker::new(
                    "index access on a non-array value",
                    place.to_string(),
                )),
            }
        }
        Place::Index {
            kind: IndexKind::Map,
            ..
        } => Err(Blocker::new("map index access", place.to_string())),
        Place::Index {
            kind: IndexKind::Tuple,
            ..
        } => Err(Blocker::new("tuple index access", place.to_string())),
        Place::Capture(index) => body
            .captures
            .get(*index)
            .map(|content| NativeTy::Cell(Box::new(content.clone())))
            .ok_or_else(|| Blocker::new("capture outside its closure", place.to_string())),
        Place::Deref(CellId::Local(local)) => cell_content(local_ty(db, body, *local)?, place),
        Place::Deref(CellId::Capture(index)) => {
            cell_content(place_ty(db, body, &Place::Capture(*index))?, place)
        }
    }
}

pub(crate) fn constant_ty<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    constant: &Constant<'db>,
) -> Result<NativeTy<'db>, Blocker> {
    match constant {
        Constant::Int(_) => Ok(NativeTy::Int),
        Constant::Float(_) => Ok(NativeTy::Float),
        Constant::Bool(_) => Ok(NativeTy::Bool),
        Constant::Null => Ok(NativeTy::Null),
        Constant::String(_) => Ok(NativeTy::String),
        Constant::Bigint(_) => Err(Blocker::new("bigint value", "constant")),
        Constant::OmittedArg => Err(Blocker::new("defaulted argument", "")),
        Constant::Function(DeclRef::Source(function)) => Ok(NativeTy::Function(Box::new(
            function_signature(db, *function)?,
        ))),
        Constant::Function(DeclRef::External(_)) => {
            Err(Blocker::new("function value", "a function without source"))
        }
        Constant::GenericFunction { .. } => Err(Blocker::new("function value", "generic")),
        Constant::GlobalItem(_) => Err(Blocker::new("global read", "")),
        Constant::EnumVariant { enum_ref, .. } => Ok(NativeTy::Enum(*enum_ref)),
    }
}

pub(crate) fn operand_ty<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    body: &Body<'_, 'db>,
    operand: &Operand<'db>,
) -> Result<NativeTy<'db>, Blocker> {
    match operand {
        Operand::Copy(place) | Operand::Move(place) => place_ty(db, body, place),
        Operand::Constant(constant) => constant_ty(db, constant),
    }
}

/// Whether a value of type `found` can be stored where `expected` is
/// declared: the same type, a reference and its optional either way round,
/// or `null` where an optional is expected.
pub(crate) fn assignable(expected: &NativeTy<'_>, found: &NativeTy<'_>) -> bool {
    expected == found
        || match (expected, found) {
            // A union takes whatever the type checker let through, and
            // gives it back where flow analysis proved what it holds.
            (expected, found) if expected.is_boxed() || found.is_boxed() => {
                !matches!(expected, NativeTy::Cell(_)) && !matches!(found, NativeTy::Cell(_))
            }
            (NativeTy::Optional(inner), found) => {
                *found == NativeTy::Null || assignable(inner, found)
            }
            // Flow narrowing: MIR reads an optional as its reference where
            // the checker proved it is not null.
            (expected, NativeTy::Optional(inner)) => assignable(expected, inner),
            // The type checker has decided the two function types agree;
            // what is left to ask is that a call passes the same registers.
            (NativeTy::Function(expected), NativeTy::Function(found)) => {
                expected.params.len() == found.params.len()
                    && expected.result.repr() == found.result.repr()
                    && expected
                        .params
                        .iter()
                        .zip(&found.params)
                        .all(|(expected, found)| expected.repr() == found.repr())
            }
            _ => false,
        }
}

/// How a binary operator is evaluated for a pair of operand types.
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
    /// Both operands are strings: `+` and the comparisons.
    String,
    /// An optional against `null`; only `==` and `!=`. The flag says the
    /// optional is the left operand.
    NullTest { optional_on_left: bool },
}

pub(crate) fn is_comparison(op: BinOp) -> bool {
    matches!(
        op,
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
    )
}

/// The evaluation mode and result type of `left op right`.
pub(crate) fn binary_typing<'db>(
    op: BinOp,
    left: &NativeTy<'db>,
    right: &NativeTy<'db>,
) -> Result<(BinaryMode, NativeTy<'db>), Blocker> {
    let unsupported = || {
        Blocker::new(
            "binary operator on these operand types",
            format!("{} {op} {}", describe(left), describe(right)),
        )
    };
    let equality = matches!(op, BinOp::Eq | BinOp::Ne);
    let mode = match (left, right) {
        (NativeTy::Int, NativeTy::Int) => BinaryMode::Int,
        (NativeTy::Float, NativeTy::Float | NativeTy::Int) | (NativeTy::Int, NativeTy::Float) => {
            if matches!(
                op,
                BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr
            ) {
                return Err(unsupported());
            }
            BinaryMode::Float
        }
        (NativeTy::Bool, NativeTy::Bool) if equality => BinaryMode::Bool,
        (NativeTy::Null, NativeTy::Null) if equality => BinaryMode::Null,
        (NativeTy::String, NativeTy::String) if is_comparison(op) || op == BinOp::Add => {
            BinaryMode::String
        }
        (NativeTy::Optional(_) | NativeTy::Union(_) | NativeTy::Unknown, NativeTy::Null)
            if equality =>
        {
            BinaryMode::NullTest {
                optional_on_left: true,
            }
        }
        (NativeTy::Null, NativeTy::Optional(_) | NativeTy::Union(_) | NativeTy::Unknown)
            if equality =>
        {
            BinaryMode::NullTest {
                optional_on_left: false,
            }
        }
        _ => return Err(unsupported()),
    };
    let result = if is_comparison(op) {
        NativeTy::Bool
    } else {
        match mode {
            BinaryMode::Int => NativeTy::Int,
            BinaryMode::Float => NativeTy::Float,
            BinaryMode::String => NativeTy::String,
            BinaryMode::Bool | BinaryMode::Null | BinaryMode::NullTest { .. } => {
                return Err(unsupported());
            }
        }
    };
    Ok((mode, result))
}

/// A short name for a type in a blocker's detail.
pub(crate) fn describe(ty: &NativeTy<'_>) -> &'static str {
    match ty {
        NativeTy::Int => "int",
        NativeTy::Float => "float",
        NativeTy::Bool => "bool",
        NativeTy::Null => "null",
        NativeTy::Enum(_) => "enum",
        NativeTy::Union(_) => "union",
        NativeTy::Unknown => "unknown",
        NativeTy::String => "string",
        NativeTy::Class(_) => "class",
        NativeTy::Array(_) => "array",
        NativeTy::Function(_) => "function",
        NativeTy::Cell(_) => "cell",
        NativeTy::Optional(_) => "optional",
    }
}

/// The result type of `op operand`.
pub(crate) fn unary_typing<'db>(
    op: UnaryOp,
    operand: &NativeTy<'db>,
) -> Result<NativeTy<'db>, Blocker> {
    let unsupported = || {
        Blocker::new(
            "unary operator on this operand type",
            format!("{op}{}", describe(operand)),
        )
    };
    match (op, operand) {
        (
            UnaryOp::Not | UnaryOp::Truthy,
            NativeTy::Int | NativeTy::Float | NativeTy::Bool | NativeTy::Null,
        ) => Ok(NativeTy::Bool),
        (UnaryOp::Neg, NativeTy::Int) => Ok(NativeTy::Int),
        (UnaryOp::Neg, NativeTy::Float) => Ok(NativeTy::Float),
        _ => Err(unsupported()),
    }
}
