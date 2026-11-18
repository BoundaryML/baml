//! Decides whether a MIR function is in the subset native code is generated
//! for, and names every construct that keeps it out.

use baml_compiler2_hir::loc::{DeclRef, FunctionLoc};
use baml_compiler2_mir::{
    AggregateKind, BoundFrame, Constant, Local, MirFunction, MirFunctionBody, MirFunctionKind,
    Operand, OptLevel, Place, RuntimeTy, Rvalue, ShortCircuitKind, StatementKind, SwitchKey,
    Terminator, function_display_name,
};

use crate::{
    Blocker,
    loops::{ArrayLoops, is_done_test, operand_local},
    narrow::TestKind,
    program::{Entry, FnKey, Needed},
    types::{
        Body, NativeTy, Signature, assignable, binary_typing, class_layout, declared_ty, describe,
        local_ty, narrowed, operand_ty, place_local, place_ty, unary_typing,
    },
};

/// The optimization level every function of a native program is lowered at.
pub(crate) const OPT: OptLevel = OptLevel::Two;

/// A standard library function implemented by the interpreter that native
/// code implements itself, inline or in its runtime.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Intrinsic {
    /// `array.push(item)`, returning the new length.
    ArrayPush,
    /// The number of characters in a string.
    StringCharCount,
    /// The text of an `int`, `float`, `bool`, `null` or `string`: what a
    /// template hole and `to_string()` produce for them.
    ToString,
    /// `==` and `!=` between two variants of an enum.
    EnumEq,
    EnumNe,
}

impl Intrinsic {
    fn named(name: &str) -> Option<Self> {
        match name {
            "baml.Array.push" => Some(Self::ArrayPush),
            // `length` is declared as `char_count`.
            "baml.String.char_count" | "baml.String.length" => Some(Self::StringCharCount),
            "baml._to_string_default" | "baml._to_string_primitive" => Some(Self::ToString),
            "baml.ops.equals_equals" => Some(Self::EnumEq),
            "baml.ops.not_equals" => Some(Self::EnumNe),
            _ => None,
        }
    }

    /// The result type for value arguments of these types, when they fit.
    pub(crate) fn result<'db>(self, arguments: &[NativeTy<'db>]) -> Option<NativeTy<'db>> {
        match (self, arguments) {
            (Self::ArrayPush, [NativeTy::Array(element), item]) if assignable(element, item) => {
                Some(NativeTy::Int)
            }
            (Self::StringCharCount, [NativeTy::String]) => Some(NativeTy::Int),
            (Self::EnumEq | Self::EnumNe, [NativeTy::Enum(left), NativeTy::Enum(right)])
                if left == right =>
            {
                Some(NativeTy::Bool)
            }
            (
                Self::ToString,
                [
                    NativeTy::Int
                    | NativeTy::Float
                    | NativeTy::Bool
                    | NativeTy::Null
                    | NativeTy::Enum(_)
                    | NativeTy::String,
                ],
            ) => Some(NativeTy::String),
            _ => None,
        }
    }
}

/// What a call terminator calls.
pub(crate) enum Callee<'db> {
    Function(FunctionLoc<'db>),
    Intrinsic(Intrinsic),
    /// The function value an operand holds.
    Value,
}

/// The body of a function that has one in MIR.
pub(crate) fn body_of<'a, 'db>(
    mir: &'a MirFunction<'db>,
) -> Result<&'a MirFunctionBody<'db>, Blocker> {
    match &mir.kind {
        MirFunctionKind::Bytecode(body) => Ok(body),
        MirFunctionKind::Builtin(kind) => Err(Blocker::new(
            "runtime builtin",
            format!("{kind:?}").to_lowercase(),
        )),
    }
}

/// Local `_0` is the result and `_1..=arity` are the parameters.
pub(crate) fn signature<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    mir: &MirFunction<'_>,
    body: &MirFunctionBody<'_>,
) -> Result<Signature<'db>, Blocker> {
    let rename = |blocker: Blocker, what: &str| {
        Blocker::new(format!("{what} is a {}", blocker.construct), blocker.detail)
    };
    let result = declared_ty(db, body, Local(0)).map_err(|blocker| rename(blocker, "result"))?;
    let params = (1..=mir.arity)
        .map(|index| match declared_ty(db, body, Local(index)) {
            // A captured parameter arrives as its value; the function puts
            // it in a cell.
            Ok(NativeTy::Cell(content)) => Ok(*content),
            Ok(ty) => Ok(ty),
            Err(blocker) => Err(rename(blocker, "parameter")),
        })
        .collect::<Result<_, _>>()?;
    Ok(Signature { params, result })
}

/// The signature of a function declared in source.
pub(crate) fn function_signature<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    function: FunctionLoc<'db>,
) -> Result<Signature<'db>, Blocker> {
    let mir = FnKey::new(function, Entry::Direct).mir(db)?;
    signature(db, mir, body_of(mir)?)
}

/// The signature of the function value `function` bound to a receiver is:
/// the function's, less its first parameter, which is returned beside it.
fn bound_signature<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    function: FunctionLoc<'db>,
) -> Result<(NativeTy<'db>, Signature<'db>), Blocker> {
    let mut signature = function_signature(db, function)?;
    if signature.params.is_empty() {
        return Err(Blocker::new("bound method value", "without a receiver"));
    }
    let receiver = signature.params.remove(0);
    Ok((receiver, signature))
}

/// What a call names directly: a function written in BAML source, or a
/// builtin native code has its own implementation of.
pub(crate) fn direct_callee<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    callee: &Operand<'db>,
) -> Result<Callee<'db>, Blocker> {
    match callee {
        Operand::Constant(Constant::Function(function)) => {
            if let Some(intrinsic) = Intrinsic::named(&function_display_name(db, *function)) {
                return Ok(Callee::Intrinsic(intrinsic));
            }
            match function {
                DeclRef::Source(function) => Ok(Callee::Function(*function)),
                DeclRef::External(_) => Err(Blocker::new(
                    "call of a function without source",
                    function_display_name(db, *function),
                )),
            }
        }
        Operand::Constant(Constant::GenericFunction { func, .. }) => Err(Blocker::new(
            "call of a generic function",
            function_display_name(db, *func),
        )),
        Operand::Constant(_) => Err(Blocker::new("call of a non-function constant", "")),
        Operand::Copy(_) | Operand::Move(_) => Ok(Callee::Value),
    }
}

/// What a function needs from the rest of the program, and what keeps it
/// from being compiled.
pub(crate) struct Checked<'db> {
    /// Every unsupported construct, in block order, without repeats.
    pub(crate) blockers: Vec<Blocker>,
    /// The functions it calls directly or makes values of.
    pub(crate) needed: Vec<Needed<'db>>,
}

pub(crate) fn check_function<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    key: &FnKey<'db>,
    mir: &MirFunction<'db>,
    captures: &[NativeTy<'db>],
) -> Checked<'db> {
    let mut checker = Checker {
        db,
        key,
        blockers: Vec::new(),
        needed: Vec::new(),
    };
    match body_of(mir) {
        Ok(body) => {
            checker.note(signature(db, mir, body).map(drop));
            if key.entry == Entry::Bound {
                checker.note(bound_signature(db, key.loc).map(drop));
            }
            let body = &Body::new(db, mir, body, captures);
            let loops = ArrayLoops::of(db, body);
            for (index, block) in body.blocks.iter().enumerate() {
                if !body.is_reachable(index) {
                    continue;
                }
                body.enter(index);
                if block.unwind.is_some() || block.landing.is_some() {
                    checker.note(Err(Blocker::new(
                        "catch or defer handler",
                        block.id.to_string(),
                    )));
                }
                for statement in &block.statements {
                    checker.statement_needs(body, &statement.kind);
                    checker.note(statement_supported(db, body, &loops, &statement.kind));
                }
                match &block.terminator {
                    Some(terminator) => checker.terminator(body, &loops, terminator),
                    None => checker.note(Err(Blocker::new(
                        "unterminated block",
                        block.id.to_string(),
                    ))),
                }
            }
        }
        Err(blocker) => checker.note(Err(blocker)),
    }
    Checked {
        blockers: checker.blockers,
        needed: checker.needed,
    }
}

struct Checker<'a, 'db> {
    db: &'db dyn baml_compiler2_mir::Db,
    key: &'a FnKey<'db>,
    blockers: Vec<Blocker>,
    needed: Vec<Needed<'db>>,
}

impl<'db> Checker<'_, 'db> {
    fn need(&mut self, key: FnKey<'db>, captures: Vec<NativeTy<'db>>) {
        if !self
            .needed
            .iter()
            .any(|needed| needed.key == key && needed.captures == captures)
        {
            self.needed.push(Needed { key, captures });
        }
    }

    /// A function named where a value is expected is needed as one.
    fn operand_needs(&mut self, operand: &Operand<'db>) {
        if let Operand::Constant(Constant::Function(DeclRef::Source(function))) = operand {
            self.need(FnKey::new(*function, Entry::Value), Vec::new());
        }
    }

    /// The functions a statement makes values of.
    fn statement_needs(&mut self, body: &Body<'_, 'db>, statement: &StatementKind<'db>) {
        let StatementKind::Assign { value, .. } = statement else {
            return;
        };
        match value {
            Rvalue::Use(operand) => self.operand_needs(operand),
            Rvalue::Array(_, operands)
            | Rvalue::Aggregate {
                fields: operands, ..
            } => {
                for operand in operands {
                    self.operand_needs(operand);
                }
            }
            Rvalue::MakeClosure {
                lambda_idx,
                captures,
                ..
            } => {
                // A capture that is not a cell is reported by the statement's
                // own check.
                let captures = captures
                    .iter()
                    .map(|capture| match operand_ty(self.db, body, capture) {
                        Ok(NativeTy::Cell(content)) => Some(*content),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>();
                if let Some(captures) = captures {
                    self.need(self.key.lambda(*lambda_idx), captures);
                }
            }
            Rvalue::MakeBoundMethod {
                func: DeclRef::Source(function),
                ..
            } => self.need(FnKey::new(*function, Entry::Bound), Vec::new()),
            _ => {}
        }
    }

    fn note(&mut self, outcome: Result<(), Blocker>) {
        if let Err(blocker) = outcome
            && !self
                .blockers
                .iter()
                .any(|seen| seen.construct == blocker.construct)
        {
            self.blockers.push(blocker);
        }
    }

    fn terminator(
        &mut self,
        body: &Body<'_, 'db>,
        loops: &ArrayLoops<'db>,
        terminator: &Terminator<'db>,
    ) {
        if let Terminator::Call { callee, args, .. } = terminator {
            if let Ok(Callee::Function(function)) = direct_callee(self.db, callee) {
                self.need(FnKey::new(function, Entry::Direct), Vec::new());
            }
            for argument in args {
                self.operand_needs(argument);
            }
        }
        let outcome = if loops.start(terminator).is_some() || loops.step(terminator).is_some() {
            Ok(())
        } else {
            terminator_supported(self.db, body, terminator)
        };
        self.note(outcome);
    }
}

fn rvalue_name(value: &Rvalue<'_>) -> &'static str {
    match value {
        Rvalue::Use(_) => "use",
        Rvalue::BinaryOp { .. } => "binary operator",
        Rvalue::UnaryOp { .. } => "unary operator",
        Rvalue::TraceHookSettings { .. } => "trace hook settings",
        Rvalue::ConcatStrings(_) => "string concatenation",
        Rvalue::Array(..)
        | Rvalue::Aggregate {
            kind: AggregateKind::Array,
            ..
        } => "array construction",
        Rvalue::Uint8Array(_) => "byte array construction",
        Rvalue::Map(..) => "map construction",
        Rvalue::Aggregate {
            kind: AggregateKind::Class { .. },
            ..
        } => "class instance construction",
        Rvalue::Aggregate {
            kind: AggregateKind::Tuple { .. },
            ..
        } => "tuple construction",
        Rvalue::Discriminant(_) => "enum discriminant read",
        Rvalue::TypeTag(_) => "type tag read",
        Rvalue::Len(_) => "length",
        Rvalue::IsType { .. } | Rvalue::IsTypeTag { .. } => "type test",
        Rvalue::SatisfiesBound { .. } => "interface bound test",
        Rvalue::MakeClosure { .. } => "closure construction",
        Rvalue::MakeBoundMethod { .. } | Rvalue::MakeVirtualBoundMethod { .. } => {
            "bound method value"
        }
        Rvalue::MakeVirtualFunction { .. }
        | Rvalue::MakeGenericFunction { .. }
        | Rvalue::MakeGenericFunctionFromValue { .. } => "function value",
        Rvalue::VirtualFieldAccess { .. } => "interface field access",
        Rvalue::LoadType(_) => "type value",
        Rvalue::CurrentPackage => "package value",
    }
}

/// Each operand can be stored where `expected` is declared.
fn all_assignable<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    body: &Body<'_, 'db>,
    operands: &[Operand<'db>],
    mut expected: impl FnMut(usize) -> Option<NativeTy<'db>>,
    what: &str,
) -> Result<(), Blocker> {
    for (index, operand) in operands.iter().enumerate() {
        let found = operand_ty(db, body, operand)?;
        let fits = expected(index).is_some_and(|expected| assignable(&expected, &found));
        if !fits {
            return Err(Blocker::new(
                format!("{what} of an unexpected type"),
                describe(&found),
            ));
        }
    }
    Ok(())
}

/// The type an rvalue evaluates to when it is stored where `expected` is
/// declared. Only an array literal needs the expectation: its element type
/// is the destination's.
pub(crate) fn rvalue_ty<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    body: &Body<'_, 'db>,
    loops: &ArrayLoops<'db>,
    value: &Rvalue<'db>,
    expected: &NativeTy<'db>,
) -> Result<NativeTy<'db>, Blocker> {
    // What `next()` answered is read in two ways: as the element, and
    // through the test for the end of the iteration.
    match value {
        Rvalue::Use(operand) => {
            if let Some(element) = operand_local(operand).and_then(|local| loops.item(local)) {
                return Ok(element.clone());
            }
        }
        Rvalue::IsType { operand, test }
            if operand_local(operand).is_some_and(|local| loops.item(local).is_some())
                && is_done_test(db, test) =>
        {
            return Ok(NativeTy::Bool);
        }
        _ => {}
    }
    match value {
        Rvalue::Use(operand) => operand_ty(db, body, operand),
        Rvalue::BinaryOp { op, left, right } => {
            let left = operand_ty(db, body, left)?;
            let right = operand_ty(db, body, right)?;
            binary_typing(*op, &left, &right).map(|(_, result)| result)
        }
        Rvalue::UnaryOp { op, operand } => unary_typing(*op, &operand_ty(db, body, operand)?),
        Rvalue::ConcatStrings(parts) => {
            all_assignable(db, body, parts, |_| Some(NativeTy::String), "string part")?;
            Ok(NativeTy::String)
        }
        // An enum value is its variant's position already.
        Rvalue::Discriminant(place) => match narrowed(place_ty(db, body, place)?) {
            NativeTy::Enum(_) => Ok(NativeTy::Int),
            other => Err(Blocker::new(
                "discriminant of a non-enum value",
                describe(&other),
            )),
        },
        Rvalue::Len(place) => match narrowed(place_ty(db, body, place)?) {
            NativeTy::Array(_) => Ok(NativeTy::Int),
            other => Err(Blocker::new(
                "length of a non-array value",
                describe(&other),
            )),
        },
        Rvalue::Array(_, elements)
        | Rvalue::Aggregate {
            kind: AggregateKind::Array,
            fields: elements,
        } => {
            let array = match expected {
                NativeTy::Optional(inner) => inner,
                other => other,
            };
            let NativeTy::Array(element) = array else {
                return Err(Blocker::new(
                    "array construction where no array is expected",
                    describe(expected),
                ));
            };
            all_assignable(
                db,
                body,
                elements,
                |_| Some((**element).clone()),
                "array element",
            )?;
            Ok(array.clone())
        }
        Rvalue::Aggregate {
            kind:
                AggregateKind::Class {
                    class,
                    type_arg_templates,
                    finalizable,
                },
            fields,
        } => {
            if !type_arg_templates.is_empty() {
                return Err(Blocker::new("generic class instance construction", ""));
            }
            if *finalizable {
                return Err(Blocker::new("construction of a class with a finalizer", ""));
            }
            let layout = class_layout(db, *class)?;
            if layout.len() != fields.len() {
                return Err(Blocker::new(
                    "class instance construction that does not match the layout",
                    "",
                ));
            }
            all_assignable(
                db,
                body,
                fields,
                |index| layout.get(index).cloned(),
                "field value",
            )?;
            Ok(NativeTy::Class(*class))
        }
        Rvalue::IsType { operand, test } => {
            let operand = operand_ty(db, body, operand)?;
            type_test_plan(db, &operand, TestKind::of(db, test), false)?;
            Ok(NativeTy::Bool)
        }
        Rvalue::IsTypeTag { operand, tag } => {
            let operand = operand_ty(db, body, operand)?;
            type_test_plan(db, &operand, TestKind::of_tag(*tag), true)?;
            Ok(NativeTy::Bool)
        }
        Rvalue::MakeClosure {
            lambda_idx,
            captures,
            type_arg_templates,
        } => {
            if !type_arg_templates.is_empty() {
                return Err(Blocker::new("closure that captures type arguments", ""));
            }
            for capture in captures {
                if !matches!(operand_ty(db, body, capture)?, NativeTy::Cell(_)) {
                    return Err(Blocker::new("closure capture that is not a cell", ""));
                }
            }
            let lambda = body
                .mir
                .lambdas
                .get(*lambda_idx)
                .ok_or_else(|| Blocker::new("closure of a lambda that does not exist", ""))?;
            // A lambda outside the subset is reported as its own blocked
            // function.
            let signature = signature(db, lambda, body_of(lambda)?)
                .map_err(|_| Blocker::new("closure of a lambda outside the subset", ""))?;
            Ok(NativeTy::Function(Box::new(signature)))
        }
        Rvalue::MakeBoundMethod {
            func,
            receiver,
            frame,
        } => {
            let DeclRef::Source(function) = func else {
                return Err(Blocker::new(
                    "bound method value",
                    function_display_name(db, *func),
                ));
            };
            if !matches!(frame, BoundFrame::Receiver) {
                return Err(Blocker::new("bound method value", "with type arguments"));
            }
            let (expected, signature) = bound_signature(db, *function)?;
            let found = operand_ty(db, body, receiver)?;
            if !found.is_ref() || !assignable(&expected, &found) {
                return Err(Blocker::new("bound method value", describe(&found)));
            }
            Ok(NativeTy::Function(Box::new(signature)))
        }
        other => Err(Blocker::new(rvalue_name(other), "")),
    }
}

/// A local of type `type` holds a type argument. Native code is compiled per
/// concrete type, so the value is never needed: the local is supported only
/// as an argument of an intrinsic, which ignores it.
fn is_type_value(body: &Body<'_, '_>, place: &Place) -> bool {
    matches!(place, Place::Local(local) if matches!(body.local(*local).ty, RuntimeTy::Type))
}

fn statement_supported<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    body: &Body<'_, 'db>,
    loops: &ArrayLoops<'db>,
    statement: &StatementKind<'db>,
) -> Result<(), Blocker> {
    match statement {
        StatementKind::Nop => Ok(()),
        StatementKind::Assign {
            destination,
            value: Rvalue::LoadType(_),
        } if is_type_value(body, destination) => Ok(()),
        StatementKind::Assign { destination, value } => {
            let destination = place_ty(db, body, destination)?;
            let value = rvalue_ty(db, body, loops, value, &destination)?;
            if assignable(&destination, &value) {
                Ok(())
            } else {
                Err(Blocker::new(
                    "assignment between different types",
                    format!("{} to {}", describe(&value), describe(&destination)),
                ))
            }
        }
        // Nothing is freed eagerly: the collector reclaims what is dropped.
        StatementKind::Drop(place) => {
            let loop_local = matches!(place, Place::Local(local)
                if loops.iterator(*local).is_some() || loops.item(*local).is_some());
            if loop_local || is_type_value(body, place) {
                Ok(())
            } else {
                place_ty(db, body, place).map(drop)
            }
        }
        StatementKind::FreshCell { local, .. } => match local_ty(db, body, *local)? {
            NativeTy::Cell(_) => Ok(()),
            _ => Err(Blocker::new("fresh cell for a local that has none", "")),
        },
        StatementKind::Intrinsic { op, .. } => {
            Err(Blocker::new("compiler intrinsic", format!("{op:?}")))
        }
        StatementKind::VirtualFieldStore { .. } => Err(Blocker::new("interface field access", "")),
    }
}

fn expect_bool(found: &NativeTy<'_>, what: &str) -> Result<(), Blocker> {
    if *found == NativeTy::Bool {
        Ok(())
    } else {
        Err(Blocker::new(
            format!("{what} that is not a bool"),
            describe(found),
        ))
    }
}

/// How a type test of a value of a known static type is decided.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum TypeTestPlan<'db> {
    /// By the static type alone.
    Always(bool),
    /// By whether an optional holds a value (`true`) or `null` (`false`).
    Present(bool),
    /// By the type id in the header of the object a union's reference
    /// names; `null` fails.
    Header(TestKind<'db>),
}

/// How `test` is decided for an operand of type `operand`. `coarse` says the
/// test asks for any array rather than an array of one element type.
pub(crate) fn type_test_plan<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    operand: &NativeTy<'db>,
    test: Option<TestKind<'db>>,
    coarse: bool,
) -> Result<TypeTestPlan<'db>, Blocker> {
    let test = test.ok_or_else(|| Blocker::new("type test", "of a type without one type id"))?;
    Ok(match operand {
        NativeTy::Union(_) | NativeTy::Unknown => {
            match &test {
                TestKind::Exactly(NativeTy::Null) => return Ok(TypeTestPlan::Present(false)),
                TestKind::Exactly(NativeTy::Class(class)) => {
                    class_layout(db, *class)?;
                }
                TestKind::Exactly(_) => {}
                // Arrays of different element types share their type ids.
                TestKind::AnyArray => {
                    let arrays = match operand {
                        NativeTy::Union(members) => members
                            .iter()
                            .filter(|member| matches!(member, NativeTy::Array(_)))
                            .count(),
                        _ => usize::MAX,
                    };
                    if !coarse && arrays > 1 {
                        return Err(Blocker::new("type test", "between array types"));
                    }
                }
            }
            TypeTestPlan::Header(test)
        }
        NativeTy::Optional(inner) => {
            if test == TestKind::Exactly(NativeTy::Null) {
                TypeTestPlan::Present(false)
            } else if test.matches(inner) {
                TypeTestPlan::Present(true)
            } else {
                TypeTestPlan::Always(false)
            }
        }
        other => TypeTestPlan::Always(test.matches(other)),
    })
}

/// The type of the value a narrowing bind stores where its test passes.
pub(crate) fn narrow_bound<'db>(source: &NativeTy<'db>, plan: &TypeTestPlan<'db>) -> NativeTy<'db> {
    match (source, plan) {
        (NativeTy::Optional(inner), TypeTestPlan::Present(true)) => (**inner).clone(),
        (_, TypeTestPlan::Present(false)) => NativeTy::Null,
        (_, TypeTestPlan::Header(TestKind::Exactly(ty))) => ty.clone(),
        _ => source.clone(),
    }
}

fn terminator_supported<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    body: &Body<'_, 'db>,
    terminator: &Terminator<'db>,
) -> Result<(), Blocker> {
    match terminator {
        Terminator::Goto { .. } | Terminator::Return | Terminator::Unreachable => Ok(()),
        Terminator::Branch { condition, .. } => {
            expect_bool(&operand_ty(db, body, condition)?, "branch condition")
        }
        Terminator::Switch {
            discriminant, arms, ..
        } => {
            if operand_ty(db, body, discriminant)? != NativeTy::Int {
                return Err(Blocker::new("switch on a non-int value", ""));
            }
            if arms
                .iter()
                .any(|(key, _)| matches!(key, SwitchKey::Class(_)))
            {
                return Err(Blocker::new("switch on a class tag", ""));
            }
            Ok(())
        }
        Terminator::NarrowBind {
            source,
            test,
            destination,
            ..
        } => {
            let source = operand_ty(db, body, source)?;
            let plan = type_test_plan(db, &source, TestKind::of(db, test), false)?;
            let destination = local_ty(db, body, *destination)?;
            if assignable(&destination, &narrow_bound(&source, &plan)) {
                Ok(())
            } else {
                Err(Blocker::new("type test", "narrowing bind to another type"))
            }
        }
        Terminator::Call {
            has_trace,
            callee,
            args,
            ntypeargs,
            destination,
            unwind,
            ..
        } => {
            let called = direct_callee(db, callee)?;
            if *has_trace {
                return Err(Blocker::new("call with a trace attachment", ""));
            }
            if unwind.is_some() {
                return Err(Blocker::new("catch or defer handler", "call"));
            }
            let result = local_ty(db, body, place_local(destination)?)?;
            match called {
                Callee::Value => {
                    let NativeTy::Function(expected) = narrowed(operand_ty(db, body, callee)?)
                    else {
                        return Err(Blocker::new("call of a value that is not a function", ""));
                    };
                    if *ntypeargs != 0 {
                        return Err(Blocker::new("call with type arguments", "function value"));
                    }
                    let fits = expected.params.len() == args.len()
                        && assignable(&result, &expected.result);
                    if !fits {
                        return Err(Blocker::new(
                            "call that does not match its callee's layout",
                            "function value",
                        ));
                    }
                    all_assignable(
                        db,
                        body,
                        args,
                        |index| expected.params.get(index).cloned(),
                        "argument",
                    )
                }
                Callee::Intrinsic(intrinsic) => {
                    // Leading type arguments carry nothing native code needs.
                    let found = args
                        .iter()
                        .skip(*ntypeargs)
                        .map(|argument| operand_ty(db, body, argument))
                        .collect::<Result<Vec<_>, _>>()?;
                    match intrinsic.result(&found) {
                        Some(produced) if assignable(&result, &produced) => Ok(()),
                        _ => Err(Blocker::new(
                            "builtin call on these argument types",
                            format!("{intrinsic:?}"),
                        )),
                    }
                }
                Callee::Function(function) => {
                    let name = || function_display_name(db, DeclRef::Source(function));
                    if *ntypeargs != 0 {
                        return Err(Blocker::new("call with type arguments", name()));
                    }
                    let found = args
                        .iter()
                        .map(|argument| operand_ty(db, body, argument))
                        .collect::<Result<Vec<_>, _>>()?;
                    // A callee outside the subset is reported as its own
                    // blocked function; only a callee that will be compiled
                    // constrains the call.
                    if let Ok(expected) = function_signature(db, function) {
                        let fits = expected.params.len() == found.len()
                            && expected
                                .params
                                .iter()
                                .zip(&found)
                                .all(|(expected, found)| assignable(expected, found))
                            && assignable(&result, &expected.result);
                        if !fits {
                            return Err(Blocker::new(
                                "call that does not match its callee's layout",
                                name(),
                            ));
                        }
                    }
                    Ok(())
                }
            }
        }
        Terminator::VirtualCall { method, .. } => {
            Err(Blocker::new("interface method call", method.clone()))
        }
        Terminator::SysOp { callee, .. } => Err(Blocker::new(
            "system operation",
            match callee {
                Operand::Constant(Constant::Function(function)) => {
                    function_display_name(db, *function)
                }
                _ => String::new(),
            },
        )),
        Terminator::Spawn { .. } => Err(Blocker::new("spawn", "")),
        Terminator::Await { .. } | Terminator::AwaitAny { .. } => Err(Blocker::new("await", "")),
        Terminator::Throw { .. } | Terminator::Rethrow { .. } => Err(Blocker::new("throw", "")),
        Terminator::ShortCircuit {
            operand,
            kind,
            destination,
            ..
        } => {
            if *kind == ShortCircuitKind::Coalesce {
                // `a ?? b`: where `a` is not null it is the result.
                let NativeTy::Optional(inner) = operand_ty(db, body, operand)? else {
                    return Err(Blocker::new("null coalescing of a non-optional value", ""));
                };
                let destination = place_ty(db, body, destination)?;
                return if assignable(&destination, &inner) {
                    Ok(())
                } else {
                    Err(Blocker::new("null coalescing into another type", ""))
                };
            }
            expect_bool(&operand_ty(db, body, operand)?, "short-circuit operand")?;
            expect_bool(&place_ty(db, body, destination)?, "short-circuit result")
        }
    }
}
