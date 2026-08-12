//! MIR/emitter invariant verifier.
//!
//! This module validates assumptions shared by analysis and emission so
//! regressions fail loudly during development/testing.

use std::collections::HashSet;

use baml_compiler2_mir::{BlockId, MirFunctionBody, Terminator};

use crate::analysis::{self, AnalysisResult};

/// Verify MIR + analysis invariants required by bytecode emission.
///
/// Intended for debug builds to catch invariant drift between MIR lowering,
/// analysis, and emission.
pub(crate) fn verify_mir_emit_invariants(
    body: &MirFunctionBody,
    arity: usize,
    analysis: &AnalysisResult,
) {
    let _ = arity; // available for error messages if needed
    let block_ids: HashSet<BlockId> = body.blocks.iter().map(|b| b.id).collect();

    // Block IDs must be dense and match indexing assumptions used by MirFunctionBody::block().
    for (idx, block) in body.blocks.iter().enumerate() {
        assert!(
            block.id == BlockId(idx),
            "block id/index mismatch: block.id={:?}, index=bb{}",
            block.id,
            idx
        );
    }

    // Redirect map must only contain known blocks and must resolve to a final non-source target.
    for (&src, &dst) in &analysis.redirect_targets {
        assert!(
            block_ids.contains(&src),
            "redirect source {src:?} missing in MIR body",
        );
        assert!(
            block_ids.contains(&dst),
            "redirect target {dst:?} missing in MIR body",
        );
        assert!(src != dst, "self-redirect for {src:?} in MIR body");

        let src_block = body.block(src);
        let is_threadable =
            analysis::threadable_goto_target(src_block, &analysis.classifications).is_some();
        assert!(
            is_threadable,
            "non-threadable redirect source {src:?} in MIR body",
        );

        let resolved = analysis.resolve_jump_target(src);
        assert!(
            !analysis.redirect_targets.contains_key(&resolved),
            "redirect chain did not converge for {src:?} -> {resolved:?} in MIR body",
        );
    }

    // Exhaustive switches rely on an unreachable default path. If this regresses,
    // if-else chain emission can become unsound.
    for block in &body.blocks {
        if let Some(Terminator::Switch {
            otherwise,
            exhaustive,
            ..
        }) = &block.terminator
            && *exhaustive
        {
            let otherwise_block = body.block(*otherwise);
            assert!(
                analysis::is_dead_unreachable_block(otherwise_block),
                "exhaustive switch in {:?} has non-unreachable default block {:?}",
                block.id,
                otherwise
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use baml_compiler2_mir::{
        BasicBlock, Constant, Local, LocalDecl, MirFunctionBody, Operand, Place, Rvalue, Statement,
        StatementKind,
    };
    use baml_type::RuntimeTy;

    use super::*;
    use crate::analysis::AnalysisResult;

    fn local(name: &str) -> LocalDecl {
        LocalDecl {
            name: Some(baml_base::Name::new(name)),
            ty: RuntimeTy::Int {
                attr: baml_type::TyAttr::default(),
            },
            span: None,
            scope_span: None,
            is_captured: false,
        }
    }

    fn stmt_assign(local: Local, value: i64) -> Statement {
        Statement {
            kind: StatementKind::Assign {
                destination: Place::Local(local),
                value: Rvalue::Use(Operand::Constant(Constant::Int(value))),
            },
            span: None,
        }
    }

    #[test]
    fn verifier_allows_exhaustive_switch_with_unreachable_default() {
        let mut body = MirFunctionBody {
            catch_regions: vec![],
            blocks: vec![
                BasicBlock {
                    id: BlockId(0),
                    statements: vec![],
                    terminator: Some(Terminator::Switch {
                        discriminant: Operand::Constant(Constant::Int(0)),
                        arms: vec![(0, BlockId(1))],
                        otherwise: BlockId(2),
                        exhaustive: true,
                        arm_names: vec![],
                    }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(1),
                    statements: vec![stmt_assign(Local(0), 1)],
                    terminator: Some(Terminator::Return),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(2),
                    statements: vec![],
                    terminator: Some(Terminator::Unreachable),
                    span: None,
                    terminator_span: None,
                },
            ],
            entry: BlockId(0),
            locals: vec![local("ret")],
            viz_nodes: vec![],
        };
        // Ensure IDs/indexes stay coherent for this synthetic MIR.
        for (i, block) in body.blocks.iter_mut().enumerate() {
            block.id = BlockId(i);
        }
        let arity = 0usize;
        let analysis = AnalysisResult::analyze(&body, arity, crate::analysis::OptLevel::One);
        verify_mir_emit_invariants(&body, arity, &analysis);
    }

    #[test]
    #[should_panic(expected = "exhaustive switch")]
    fn verifier_rejects_exhaustive_switch_with_reachable_default() {
        let mut body = MirFunctionBody {
            catch_regions: vec![],
            blocks: vec![
                BasicBlock {
                    id: BlockId(0),
                    statements: vec![],
                    terminator: Some(Terminator::Switch {
                        discriminant: Operand::Constant(Constant::Int(0)),
                        arms: vec![(0, BlockId(1))],
                        otherwise: BlockId(2),
                        exhaustive: true,
                        arm_names: vec![],
                    }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(1),
                    statements: vec![stmt_assign(Local(0), 1)],
                    terminator: Some(Terminator::Return),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(2),
                    statements: vec![],
                    terminator: Some(Terminator::Goto { target: BlockId(1) }),
                    span: None,
                    terminator_span: None,
                },
            ],
            entry: BlockId(0),
            locals: vec![local("ret")],
            viz_nodes: vec![],
        };
        for (i, block) in body.blocks.iter_mut().enumerate() {
            block.id = BlockId(i);
        }
        let arity = 0usize;
        let analysis = AnalysisResult::analyze(&body, arity, crate::analysis::OptLevel::One);
        verify_mir_emit_invariants(&body, arity, &analysis);
    }
}
