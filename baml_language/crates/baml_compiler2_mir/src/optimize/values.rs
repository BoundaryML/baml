use std::collections::HashMap;

use crate::{
    BinOp, Constant, MirFunctionBody, Operand, OptLevel, Place, Rvalue, ShortCircuitKind,
    Statement, StatementKind, Terminator, UnaryOp,
};

fn scalar(constant: &Constant<'_>) -> bool {
    matches!(
        constant,
        Constant::Int(_)
            | Constant::Float(_)
            | Constant::Bool(_)
            | Constant::String(_)
            | Constant::Null
    )
}

/// Propagate immutable scalar definitions, including named bindings. Keeping the
/// definitions until ordinary DCE preserves locals used in projection positions.
pub(super) fn fold_constants(body: &mut MirFunctionBody<'_>, arity: usize) {
    let mut defs = super::count_local_defs(body);
    for (_, local) in body.unwind_error_locals() {
        defs[local.0] += 1;
    }
    for region in &body.catch_regions {
        if let Some(local) = region.stack_trace_local {
            defs[local.0] += 1;
        }
    }
    let mut constants = HashMap::new();
    loop {
        let previous = constants.len();
        for block in &mut body.blocks {
            for statement in &mut block.statements {
                super::apply_subst_to_statement(statement, &constants);
                if let StatementKind::Assign { destination, value } = &mut statement.kind {
                    if let Some(constant) = fold(value) {
                        *value = Rvalue::Use(Operand::Constant(constant));
                    }
                    if let Place::Local(local) = destination
                        && local.0 > arity
                        && defs[local.0] == 1
                        && !body.locals[local.0].is_captured
                        && let Rvalue::Use(Operand::Constant(constant)) = value
                        && scalar(constant)
                    {
                        constants.insert(*local, Operand::Constant(constant.clone()));
                    }
                }
            }
            if let Some(term) = &mut block.terminator {
                super::apply_subst_to_terminator(term, &constants);
            }
        }
        if constants.len() == previous {
            break;
        }
    }

    for block in &mut body.blocks {
        let target = match &block.terminator {
            Some(Terminator::Branch {
                condition: Operand::Constant(Constant::Bool(value)),
                then_block,
                else_block,
            }) => Some(if *value { *then_block } else { *else_block }),
            Some(Terminator::ShortCircuit {
                operand: Operand::Constant(value),
                kind,
                destination,
                eval_rhs,
                join,
            }) if scalar(value) => {
                let taken = match (kind, value) {
                    (ShortCircuitKind::And, Constant::Bool(value)) => !value,
                    (ShortCircuitKind::Or, Constant::Bool(value)) => *value,
                    (ShortCircuitKind::Coalesce, value) => !matches!(value, Constant::Null),
                    _ => continue,
                };
                if taken {
                    block.statements.push(Statement {
                        kind: StatementKind::Assign {
                            destination: destination.clone(),
                            value: Rvalue::Use(Operand::Constant(value.clone())),
                        },
                        span: block.terminator_span,
                    });
                }
                Some(if taken { *join } else { *eval_rhs })
            }
            _ => None,
        };
        if let Some(target) = target {
            block.terminator = Some(Terminator::Goto { target });
        }
    }
    simplify_boolean_diamonds(body);
}

fn fold<'db>(value: &Rvalue<'db>) -> Option<Constant<'db>> {
    let int = |value| baml_type::Int63::new(value).map(|n| Constant::Int(n.get()));
    match value {
        Rvalue::UnaryOp {
            op,
            operand: Operand::Constant(arg),
        } => match (op, arg) {
            (UnaryOp::Not, Constant::Bool(value)) => Some(Constant::Bool(!value)),
            (UnaryOp::Neg, Constant::Int(value)) => int(value.checked_neg()?),
            (UnaryOp::Neg, Constant::Float(value)) if value.is_finite() => {
                Some(Constant::Float(-value))
            }
            (UnaryOp::Truthy, value) => Some(Constant::Bool(match value {
                Constant::Null => false,
                Constant::Bool(value) => *value,
                Constant::Int(value) => *value != 0,
                Constant::Float(value) => *value != 0.0,
                Constant::String(value) => !value.is_empty(),
                _ => return None,
            })),
            _ => None,
        },
        Rvalue::BinaryOp {
            op,
            left: Operand::Constant(left),
            right: Operand::Constant(right),
        } => match (left, right) {
            (Constant::Int(a), Constant::Int(b)) => match op {
                BinOp::Add => int(a.checked_add(*b)?),
                BinOp::Sub => int(a.checked_sub(*b)?),
                BinOp::Mul => int(a.checked_mul(*b)?),
                BinOp::Div => int(a.checked_div(*b)?),
                BinOp::Mod => int(a.checked_rem(*b)?),
                BinOp::BitAnd => int(a & b),
                BinOp::BitOr => int(a | b),
                BinOp::BitXor => int(a ^ b),
                BinOp::Shl => int(baml_type::Int63::new(*a)?.shift_left(*b).ok()?.get()),
                BinOp::Shr => int(baml_type::Int63::new(*a)?.shift_right(*b).ok()?.get()),
                _ => compare(*op, a, b).map(Constant::Bool),
            },
            (Constant::Float(a), Constant::Float(b)) if a.is_finite() && b.is_finite() => {
                if let Some(value) = compare(*op, a, b) {
                    return Some(Constant::Bool(value));
                }
                let value = match op {
                    BinOp::Add => a + b,
                    BinOp::Sub => a - b,
                    BinOp::Mul => a * b,
                    BinOp::Div if *b != 0.0 => a / b,
                    _ => return None,
                };
                value.is_finite().then_some(Constant::Float(value))
            }
            (Constant::Bool(a), Constant::Bool(b)) => compare(*op, a, b).map(Constant::Bool),
            (Constant::String(a), Constant::String(b)) => {
                if *op == BinOp::Add && a.len().saturating_add(b.len()) <= 4096 {
                    Some(Constant::String(format!("{a}{b}")))
                } else {
                    compare(*op, a, b).map(Constant::Bool)
                }
            }
            (Constant::Null, Constant::Null) => match op {
                BinOp::Eq => Some(Constant::Bool(true)),
                BinOp::Ne => Some(Constant::Bool(false)),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

fn compare<T: PartialOrd>(op: BinOp, a: &T, b: &T) -> Option<bool> {
    Some(match op {
        BinOp::Eq => a == b,
        BinOp::Ne => a != b,
        BinOp::Lt => a < b,
        BinOp::Le => a <= b,
        BinOp::Gt => a > b,
        BinOp::Ge => a >= b,
        _ => return None,
    })
}

fn simplify_boolean_diamonds(body: &mut MirFunctionBody<'_>) {
    for index in 0..body.blocks.len() {
        let Some(Terminator::Branch {
            condition,
            then_block,
            else_block,
        }) = body.blocks[index].terminator.clone()
        else {
            continue;
        };
        let then_block = &body.blocks[then_block.0];
        let else_block = &body.blocks[else_block.0];
        let (
            Some(Terminator::Goto { target: then_join }),
            Some(Terminator::Goto { target: else_join }),
        ) = (&then_block.terminator, &else_block.terminator)
        else {
            continue;
        };
        if then_join != else_join {
            continue;
        }
        let ([then_stmt], [else_stmt]) = (
            then_block.statements.as_slice(),
            else_block.statements.as_slice(),
        ) else {
            continue;
        };
        let (
            StatementKind::Assign {
                destination: Place::Local(then_local),
                value: Rvalue::Use(Operand::Constant(Constant::Bool(then_value))),
            },
            StatementKind::Assign {
                destination: Place::Local(else_local),
                value: Rvalue::Use(Operand::Constant(Constant::Bool(else_value))),
            },
        ) = (&then_stmt.kind, &else_stmt.kind)
        else {
            continue;
        };
        if then_local != else_local
            || then_value == else_value
            || body.locals[then_local.0].is_captured
        {
            continue;
        }
        let statement = Statement {
            kind: StatementKind::Assign {
                destination: Place::Local(*then_local),
                value: if *then_value {
                    Rvalue::Use(condition)
                } else {
                    Rvalue::UnaryOp {
                        op: UnaryOp::Not,
                        operand: condition,
                    }
                },
            },
            span: body.blocks[index].terminator_span,
        };
        let target = *then_join;
        body.blocks[index].statements.push(statement);
        body.blocks[index].terminator = Some(Terminator::Goto { target });
    }
}

/// Backwards liveness removes individual overwritten definitions, rather than
/// requiring the entire local to be unused. Potentially failing RHSs stay put.
pub(super) fn eliminate_dead_stores(body: &mut MirFunctionBody<'_>, arity: usize, opt: OptLevel) {
    let effects = super::effects::Analysis::new(body);
    let n = body.locals.len();
    let mut live_in = vec![vec![false; n]; body.blocks.len()];
    loop {
        let mut changed = false;
        for block in body.blocks.iter().rev() {
            let (mut live, exceptional) = live_out(body, block, &live_in);
            for statement in block.statements.iter().rev() {
                transfer(statement, &mut live, &exceptional);
            }
            if live != live_in[block.id.0] {
                live_in[block.id.0] = live;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    for index in 0..body.blocks.len() {
        let (mut live, exceptional) = live_out(body, &body.blocks[index], &live_in);
        let statements = &mut body.blocks[index].statements;
        for (statement_index, statement) in statements.iter_mut().enumerate().rev() {
            if let StatementKind::Assign {
                destination: Place::Local(local),
                ..
            } = &statement.kind
                && local.0 > arity
                && !body.locals[local.0].is_captured
                // Debugger-visible stores must survive at O0, even if overwritten.
                && (opt != OptLevel::Zero || body.locals[local.0].name.is_none())
                && !live[local.0]
                && effects.discardable[index][statement_index]
            {
                statement.kind = StatementKind::Nop;
            }
            transfer(statement, &mut live, &exceptional);
        }
        statements.retain(|statement| !matches!(statement.kind, StatementKind::Nop));
    }
}

fn live_out(
    body: &MirFunctionBody<'_>,
    block: &crate::BasicBlock<'_>,
    live_in: &[Vec<bool>],
) -> (Vec<bool>, Vec<bool>) {
    let mut live = vec![false; body.locals.len()];
    let mut exceptional = live.clone();
    for region in &body.catch_regions {
        if region.body_blocks.contains(&block.id) {
            union(&mut exceptional, &live_in[region.handler.0]);
        }
    }
    if let Some(term) = &block.terminator {
        for successor in term.successors() {
            union(&mut live, &live_in[successor.0]);
        }
        let mut uses = vec![0; live.len()];
        super::count_in_terminator(term, &mut uses);
        for (live, uses) in live.iter_mut().zip(uses) {
            *live |= uses != 0;
        }
        if matches!(term, Terminator::Return) {
            live[0] = true;
        }
    }
    union(&mut live, &exceptional);
    (live, exceptional)
}

fn transfer(statement: &Statement<'_>, live: &mut [bool], exceptional: &[bool]) {
    if let StatementKind::Assign {
        destination: Place::Local(local),
        ..
    } = &statement.kind
    {
        live[local.0] = false;
    }
    let mut uses = vec![0; live.len()];
    super::count_in_statement(statement, &mut uses);
    for (live, uses) in live.iter_mut().zip(uses) {
        *live |= uses != 0;
    }
    // A panic can enter the handler before this statement stores its result.
    union(live, exceptional);
}

fn union(into: &mut [bool], from: &[bool]) {
    for (into, from) in into.iter_mut().zip(from) {
        *into |= *from;
    }
}

/// Put infallible constants before the next call producing a consumer operand.
/// This exposes mixed prefixes like `[f(), 2, g()]` without moving any effectful
/// evaluation. Stackification may still reject carrying and inline the constant.
pub(super) fn materialize_operand_prefixes(body: &mut MirFunctionBody<'_>) {
    let defs = super::count_local_defs(body);
    let uses = super::count_local_uses(body);
    let mut calls = HashMap::new();
    for block in &body.blocks {
        if let Some(
            Terminator::Call {
                destination: Place::Local(local),
                ..
            }
            | Terminator::VirtualCall {
                destination: Place::Local(local),
                ..
            }
            | Terminator::SysOp {
                destination: Place::Local(local),
                ..
            }
            | Terminator::Await {
                destination: Place::Local(local),
                ..
            },
        ) = &block.terminator
            && defs[local.0] == 1
            && uses[local.0] == 1
        {
            calls.insert(*local, block.id);
        }
    }
    for block_index in 0..body.blocks.len() {
        let statements = body.blocks[block_index].statements.len();
        for consumer_index in 0..=statements {
            let operands = prefix_operands(&mut body.blocks[block_index], consumer_index)
                .into_iter()
                .map(|operand| operand.clone())
                .collect::<Vec<_>>();
            for (index, operand) in operands.iter().enumerate() {
                let Operand::Constant(constant) = operand else {
                    continue;
                };
                let Some(ty) = constant_type(constant) else {
                    continue;
                };
                let next = operands[index + 1..]
                    .iter()
                    .find(|operand| !matches!(operand, Operand::Constant(_)));
                let Some(Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local))) =
                    next
                else {
                    continue;
                };
                let Some(def_block) = calls.get(local) else {
                    continue;
                };
                if def_block.0 == block_index {
                    continue;
                }
                let temporary = crate::Local(body.locals.len());
                body.locals.push(crate::LocalDecl {
                    name: None,
                    ty,
                    span: None,
                    scope_span: None,
                    is_captured: false,
                });
                body.blocks[def_block.0].statements.push(Statement {
                    kind: StatementKind::Assign {
                        destination: Place::Local(temporary),
                        value: Rvalue::Use(Operand::Constant(constant.clone())),
                    },
                    span: None,
                });
                *prefix_operands(&mut body.blocks[block_index], consumer_index)[index] =
                    Operand::Copy(Place::Local(temporary));
            }
        }
    }
}

fn prefix_operands<'a, 'db>(
    block: &'a mut crate::BasicBlock<'db>,
    index: usize,
) -> Vec<&'a mut Operand<'db>> {
    if index == block.statements.len() {
        return match &mut block.terminator {
            Some(Terminator::Call { args, .. } | Terminator::VirtualCall { args, .. }) => {
                args.iter_mut().collect()
            }
            _ => Vec::new(),
        };
    }
    match &mut block.statements[index].kind {
        StatementKind::Assign { value, .. } => match value {
            Rvalue::BinaryOp { left, right, .. } => vec![left, right],
            Rvalue::Array(_, elements) => elements.iter_mut().collect(),
            Rvalue::Map(_, _, elements) => elements.iter_mut().map(|(_, value)| value).collect(),
            Rvalue::Aggregate { fields, .. }
                if !fields.iter().any(|field| {
                    matches!(
                        field,
                        Operand::Copy(Place::Field { .. }) | Operand::Move(Place::Field { .. })
                    )
                }) =>
            {
                fields.iter_mut().collect()
            }
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

fn constant_type(constant: &Constant<'_>) -> Option<baml_type::RuntimeTy> {
    use baml_type::RuntimeTy;
    let attr = baml_type::TyAttr::default();
    Some(match constant {
        Constant::Int(_) => RuntimeTy::Int { attr },
        Constant::Float(_) => RuntimeTy::Float { attr },
        Constant::Bool(_) => RuntimeTy::Bool { attr },
        Constant::String(_) => RuntimeTy::String { attr },
        Constant::Null => RuntimeTy::Null { attr },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use baml_type::RuntimeTy;

    use super::*;
    use crate::{BasicBlock, BlockId, Local, LocalDecl};

    fn binary(op: BinOp, left: Constant<'static>, right: Constant<'static>) -> Rvalue<'static> {
        Rvalue::BinaryOp {
            op,
            left: Operand::Constant(left),
            right: Operand::Constant(right),
        }
    }

    #[test]
    fn integer_folding_preserves_overflow_and_zero_divisor_panics() {
        let min = baml_type::Int63::MIN.get();
        let max = baml_type::Int63::MAX.get();
        for (op, left, right) in [
            (BinOp::Div, 1, 0),
            (BinOp::Mod, 1, 0),
            (BinOp::Add, max, 1),
            (BinOp::Add, i64::MAX, 1),
            (BinOp::Sub, min, 1),
            (BinOp::Mul, max, 2),
            (BinOp::Div, min, -1),
        ] {
            assert!(
                fold(&binary(op, Constant::Int(left), Constant::Int(right))).is_none(),
                "{left} {op:?} {right}"
            );
        }
        assert!(
            fold(&Rvalue::UnaryOp {
                op: UnaryOp::Neg,
                operand: Operand::Constant(Constant::Int(min)),
            })
            .is_none()
        );
        for (op, expected) in [
            (BinOp::Add, 8),
            (BinOp::Sub, 4),
            (BinOp::Mul, 12),
            (BinOp::Div, 3),
            (BinOp::Mod, 0),
            (BinOp::BitAnd, 2),
            (BinOp::BitOr, 6),
            (BinOp::BitXor, 4),
            (BinOp::Shl, 24),
            (BinOp::Shr, 1),
        ] {
            assert!(matches!(
                fold(&binary(op, Constant::Int(6), Constant::Int(2))),
                Some(Constant::Int(value)) if value == expected
            ));
        }
    }

    #[test]
    fn float_folding_rejects_zero_division_and_nonfinite_values() {
        for (op, left, right) in [
            (BinOp::Div, 1.0, 0.0),
            (BinOp::Div, 1.0, -0.0),
            (BinOp::Mul, f64::MAX, 2.0),
            (BinOp::Add, f64::INFINITY, 1.0),
            (BinOp::Add, f64::NAN, 1.0),
        ] {
            assert!(fold(&binary(op, Constant::Float(left), Constant::Float(right))).is_none());
        }
        for (op, expected) in [
            (BinOp::Add, 8.0_f64),
            (BinOp::Sub, 4.0),
            (BinOp::Mul, 12.0),
            (BinOp::Div, 3.0),
        ] {
            assert!(matches!(
                fold(&binary(op, Constant::Float(6.0), Constant::Float(2.0))),
                Some(Constant::Float(value)) if value.to_bits() == expected.to_bits()
            ));
        }
    }

    #[test]
    fn string_folding_obeys_the_allocation_limit() {
        assert!(matches!(
            fold(&binary(
                BinOp::Add,
                Constant::String("a".repeat(4095)),
                Constant::String("b".into()),
            )),
            Some(Constant::String(value)) if value.len() == 4096 && value.ends_with('b')
        ));
        assert!(
            fold(&binary(
                BinOp::Add,
                Constant::String("a".repeat(4096)),
                Constant::String("b".into()),
            ))
            .is_none()
        );
    }

    #[test]
    fn dead_stores_preserve_named_definitions_at_o0_but_optimize_temporaries() {
        let mut block = BasicBlock::new(BlockId(0));
        for (local, value) in [(1, 1), (2, 1), (1, 2), (2, 2)] {
            block.statements.push(Statement {
                kind: StatementKind::Assign {
                    destination: Place::Local(Local(local)),
                    value: Rvalue::Use(Operand::Constant(Constant::Int(value))),
                },
                span: None,
            });
        }
        block.statements.push(Statement {
            kind: StatementKind::Assign {
                destination: Place::Local(Local(0)),
                value: Rvalue::BinaryOp {
                    op: BinOp::Add,
                    left: Operand::copy_local(Local(1)),
                    right: Operand::copy_local(Local(2)),
                },
            },
            span: None,
        });
        block.terminator = Some(Terminator::Return);
        let original = MirFunctionBody {
            blocks: vec![block],
            entry: BlockId(0),
            locals: [None, Some("named"), None]
                .into_iter()
                .map(|name| LocalDecl {
                    name: name.map(baml_base::Name::new),
                    ty: RuntimeTy::int(),
                    span: None,
                    scope_span: None,
                    is_captured: false,
                })
                .collect(),
            catch_regions: vec![],
        };
        for opt in [OptLevel::Zero, OptLevel::One, OptLevel::Two] {
            let mut body = original.clone();
            eliminate_dead_stores(&mut body, 0, opt);
            let defs = super::super::count_local_defs(&body);
            assert_eq!(
                defs[1],
                if opt == OptLevel::Zero { 2 } else { 1 },
                "{opt:?}"
            );
            assert_eq!(defs[2], 1, "unnamed temporary at {opt:?}");
            if opt == OptLevel::Zero {
                super::super::optimize_body(&mut body, 0, opt);
                let named = body
                    .locals
                    .iter()
                    .position(|local| local.name.is_some())
                    .unwrap();
                assert_eq!(super::super::count_local_defs(&body)[named], 2);
            }
        }
    }
}
