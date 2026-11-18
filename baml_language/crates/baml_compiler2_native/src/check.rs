//! Decides whether a MIR function is in the subset native code is generated
//! for, and names every construct that keeps it out.

use baml_compiler2_hir::loc::{DeclRef, FunctionLoc};
use baml_compiler2_mir::{
    AggregateKind, Constant, Local, MirFunction, MirFunctionBody, MirFunctionKind, Operand,
    OptLevel, Place, RuntimeTy, Rvalue, ShortCircuitKind, StatementKind, SwitchKey, Terminator,
    TypeTest, function_display_name, lower_function,
};

use crate::{
    Blocker,
    loops::{ArrayLoops, is_done_test, operand_local},
    types::{
        Body, NativeTy, assignable, binary_typing, class_layout, declared_ty, describe, local_ty,
        narrowed, operand_ty, place_local, place_ty, unary_typing,
    },
};

/// The optimization level every function of a native program is lowered at.
pub(crate) const OPT: OptLevel = OptLevel::Two;

/// Parameter and result types of a function with a native calling convention.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct Signature<'db> {
    pub(crate) params: Vec<NativeTy<'db>>,
    pub(crate) result: NativeTy<'db>,
}

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
}

impl Intrinsic {
    fn named(name: &str) -> Option<Self> {
        match name {
            "baml.Array.push" => Some(Self::ArrayPush),
            // `length` is declared as `char_count`.
            "baml.String.char_count" | "baml.String.length" => Some(Self::StringCharCount),
            "baml._to_string_default" | "baml._to_string_primitive" => Some(Self::ToString),
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
            (
                Self::ToString,
                [
                    NativeTy::Int
                    | NativeTy::Float
                    | NativeTy::Bool
                    | NativeTy::Null
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
        .map(|index| {
            declared_ty(db, body, Local(index)).map_err(|blocker| rename(blocker, "parameter"))
        })
        .collect::<Result<_, _>>()?;
    Ok(Signature { params, result })
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
        Operand::Copy(place) | Operand::Move(place) => {
            Err(Blocker::new("indirect call", place.to_string()))
        }
    }
}

/// What a function needs from the rest of the program, and what keeps it
/// from being compiled.
pub(crate) struct Checked<'db> {
    /// Every unsupported construct, in block order, without repeats.
    pub(crate) blockers: Vec<Blocker>,
    /// The source functions it calls directly.
    pub(crate) callees: Vec<FunctionLoc<'db>>,
}

pub(crate) fn check_function<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    mir: &MirFunction<'db>,
) -> Checked<'db> {
    let mut checker = Checker {
        db,
        blockers: Vec::new(),
        callees: Vec::new(),
    };
    match body_of(mir) {
        Ok(body) => {
            checker.note(signature(db, mir, body).map(drop));
            let body = &Body::new(db, body);
            let loops = ArrayLoops::of(db, body);
            for block in &body.blocks {
                if block.unwind.is_some() || block.landing.is_some() {
                    checker.note(Err(Blocker::new(
                        "catch or defer handler",
                        block.id.to_string(),
                    )));
                }
                for statement in &block.statements {
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
        callees: checker.callees,
    }
}

struct Checker<'db> {
    db: &'db dyn baml_compiler2_mir::Db,
    blockers: Vec<Blocker>,
    callees: Vec<FunctionLoc<'db>>,
}

impl<'db> Checker<'db> {
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
        if let Terminator::Call { callee, .. } = terminator
            && let Ok(Callee::Function(function)) = direct_callee(self.db, callee)
            && !self.callees.contains(&function)
        {
            self.callees.push(function);
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
    operands: &[Operand<'_>],
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
        StatementKind::FreshCell { local, .. } => {
            Err(Blocker::new("closure-captured local", local.to_string()))
        }
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

/// Whether a narrowing test of an optional reference asks for the reference
/// (`true`) or for `null` (`false`).
pub(crate) fn narrow_test_wants_reference(
    source: &NativeTy<'_>,
    test: &TypeTest<'_>,
) -> Result<bool, Blocker> {
    let NativeTy::Optional(inner) = source else {
        return Err(Blocker::new("type test", describe(source)));
    };
    match (test, &**inner) {
        (TypeTest::Class { class, args }, NativeTy::Class(expected))
            if args.is_empty() && class == expected =>
        {
            Ok(true)
        }
        (TypeTest::Template(baml_compiler2_mir::TyTemplate::Null), _) => Ok(false),
        (TypeTest::Template(baml_compiler2_mir::TyTemplate::String), NativeTy::String) => Ok(true),
        (TypeTest::Template(baml_compiler2_mir::TyTemplate::List(_)), NativeTy::Array(_)) => {
            Ok(true)
        }
        _ => Err(Blocker::new("type test", "narrowing bind")),
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
            let wants_reference = narrow_test_wants_reference(&source, test)?;
            let destination = local_ty(db, body, *destination)?;
            let bound = match (&source, wants_reference) {
                (NativeTy::Optional(inner), true) => (**inner).clone(),
                _ => NativeTy::Null,
            };
            if assignable(&destination, &bound) {
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
            let callee = direct_callee(db, callee)?;
            if *has_trace {
                return Err(Blocker::new("call with a trace attachment", ""));
            }
            if unwind.is_some() {
                return Err(Blocker::new("catch or defer handler", "call"));
            }
            let result = local_ty(db, body, place_local(destination)?)?;
            match callee {
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
                    if let Ok(mir) = lower_function(db, function, OPT)
                        && let Ok(callee_body) = body_of(mir)
                        && let Ok(expected) = signature(db, mir, callee_body)
                    {
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
