//! MIR/emitter invariant verifier.
//!
//! This module validates assumptions shared by analysis and emission so
//! regressions fail loudly during development/testing.

use std::collections::HashSet;

use baml_compiler_mir::{BlockId, Local, MirFunction, StatementKind, Terminator};

use crate::analysis::{self, AnalysisResult, LocalClassification};

/// Verify MIR + analysis invariants required by bytecode emission.
///
/// Intended for debug builds to catch invariant drift between MIR lowering,
/// analysis, and emission.
pub(crate) fn verify_mir_emit_invariants(mir: &MirFunction, analysis: &AnalysisResult) {
    let block_ids: HashSet<BlockId> = mir.blocks.iter().map(|b| b.id).collect();

    // Block IDs must be dense and match indexing assumptions used by MirFunction::block().
    for (idx, block) in mir.blocks.iter().enumerate() {
        assert!(
            block.id == BlockId(idx),
            "block id/index mismatch in {}: block.id={:?}, index=bb{}",
            mir.name,
            block.id,
            idx
        );
    }

    // Redirect map must only contain known blocks and must resolve to a final non-source target.
    for (&src, &dst) in &analysis.redirect_targets {
        assert!(
            block_ids.contains(&src),
            "redirect source {:?} missing in MIR function {}",
            src,
            mir.name
        );
        assert!(
            block_ids.contains(&dst),
            "redirect target {:?} missing in MIR function {}",
            dst,
            mir.name
        );
        assert!(
            src != dst,
            "self-redirect for {:?} in MIR function {}",
            src,
            mir.name
        );

        let src_block = mir.block(src);
        let is_threadable =
            analysis::threadable_goto_target(src_block, &analysis.classifications).is_some();
        assert!(
            is_threadable,
            "non-threadable redirect source {:?} in MIR function {}",
            src, mir.name
        );

        let resolved = analysis.resolve_jump_target(src);
        assert!(
            !analysis.redirect_targets.contains_key(&resolved),
            "redirect chain did not converge for {:?} -> {:?} in MIR function {}",
            src,
            resolved,
            mir.name
        );
    }

    // Exhaustive switches rely on an unreachable default path. If this regresses,
    // if-else chain emission can become unsound.
    for block in &mir.blocks {
        if let Some(Terminator::Switch {
            otherwise,
            exhaustive,
            ..
        }) = &block.terminator
            && *exhaustive
        {
            let otherwise_block = mir.block(*otherwise);
            assert!(
                analysis::is_dead_unreachable_block(otherwise_block),
                "exhaustive switch in {:?} has non-unreachable default block {:?}",
                block.id,
                otherwise
            );
        }
    }

    // Watched locals must always be real so Watch/Unwatch have stable slots.
    for (idx, decl) in mir.locals.iter().enumerate() {
        if decl.is_watched {
            let local = Local(idx);
            assert!(
                decl.name.is_some(),
                "watched local {} must have a user-visible name in MIR function {}",
                local,
                mir.name
            );
            let class = analysis
                .classifications
                .get(&local)
                .copied()
                .unwrap_or_else(|| {
                    panic!(
                        "missing classification for watched local {} in MIR function {}",
                        local, mir.name
                    )
                });
            assert!(
                class == LocalClassification::Real,
                "watched local {} classified as {:?} (expected Real) in MIR function {}",
                local,
                class,
                mir.name
            );
        }
    }

    // Watch-manipulation statements must only reference watched locals.
    for block in &mir.blocks {
        for stmt in &block.statements {
            let Some(local) = (match &stmt.kind {
                StatementKind::Unwatch(local)
                | StatementKind::WatchNotify(local)
                | StatementKind::WatchOptions { local, .. } => Some(*local),
                _ => None,
            }) else {
                continue;
            };

            let decl = mir.local(local);
            assert!(
                decl.is_watched,
                "watch statement references non-watched local {} in MIR function {}",
                local, mir.name
            );
            assert!(
                decl.name.is_some(),
                "watch statement references unnamed watched local {} in MIR function {}",
                local,
                mir.name
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use baml_base::Name;
    use baml_compiler_mir::{BasicBlock, Constant, LocalDecl, Operand, Place, Rvalue, Statement};
    use baml_type::Ty;

    use super::*;
    use crate::analysis::AnalysisResult;

    fn local(name: &str) -> LocalDecl {
        LocalDecl {
            name: Some(Name::new(name)),
            ty: Ty::Int {
                attr: baml_type::TyAttr::default(),
            },
            span: None,
            scope_span: None,
            is_watched: false,
        }
    }

    fn local_watched(name: &str) -> LocalDecl {
        LocalDecl {
            name: Some(Name::new(name)),
            ty: Ty::Int {
                attr: baml_type::TyAttr::default(),
            },
            span: None,
            scope_span: None,
            is_watched: true,
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
        let mut mir = MirFunction {
            name: Name::new("f"),
            arity: 0,
            unwind_error_locals: std::collections::HashMap::new(),
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
            span: None,
            viz_nodes: vec![],
        };
        // Ensure IDs/indexes stay coherent for this synthetic MIR.
        for (i, block) in mir.blocks.iter_mut().enumerate() {
            block.id = BlockId(i);
        }
        let analysis = AnalysisResult::analyze(&mir, crate::analysis::OptLevel::One);
        verify_mir_emit_invariants(&mir, &analysis);
    }

    #[test]
    #[should_panic(expected = "exhaustive switch")]
    fn verifier_rejects_exhaustive_switch_with_reachable_default() {
        let mut mir = MirFunction {
            name: Name::new("f"),
            arity: 0,
            unwind_error_locals: std::collections::HashMap::new(),
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
            span: None,
            viz_nodes: vec![],
        };
        for (i, block) in mir.blocks.iter_mut().enumerate() {
            block.id = BlockId(i);
        }
        let analysis = AnalysisResult::analyze(&mir, crate::analysis::OptLevel::One);
        verify_mir_emit_invariants(&mir, &analysis);
    }

    #[test]
    fn verifier_accepts_watched_locals_classified_real() {
        let mut mir = MirFunction {
            name: Name::new("f"),
            arity: 0,
            unwind_error_locals: std::collections::HashMap::new(),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                statements: vec![],
                terminator: Some(Terminator::Return),
                span: None,
                terminator_span: None,
            }],
            entry: BlockId(0),
            locals: vec![local("ret"), local_watched("x")],
            span: None,
            viz_nodes: vec![],
        };
        for (i, block) in mir.blocks.iter_mut().enumerate() {
            block.id = BlockId(i);
        }
        let analysis = AnalysisResult::analyze(&mir, crate::analysis::OptLevel::One);
        verify_mir_emit_invariants(&mir, &analysis);
    }
}
