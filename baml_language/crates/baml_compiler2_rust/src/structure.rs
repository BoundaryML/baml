//! Stackifier control-flow structuring for reducible graphs, including unwind edges.
//! Each reachable block is emitted once, within its immediate dominator's region.

use std::{cell::RefCell, collections::BTreeSet, fmt};

/// Terminator edges; computations belong to the block's leaf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Flow {
    Goto(usize),
    Branch {
        then_block: usize,
        else_block: usize,
    },
    Switch {
        arms: Vec<usize>,
        otherwise: usize,
    },
    /// The function ends here (return or unreachable).
    Exit,
}

impl Flow {
    fn successors(&self) -> Vec<usize> {
        match self {
            Self::Goto(target) => vec![*target],
            Self::Branch {
                then_block,
                else_block,
            } => vec![*then_block, *else_block],
            Self::Switch { arms, otherwise } => arms.iter().copied().chain([*otherwise]).collect(),
            Self::Exit => Vec::new(),
        }
    }
}

/// A control-flow graph by shape: `flows[i]` is block `i`'s terminator and
/// `unwinds[i]` the handler a throw anywhere in block `i` lands in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Cfg {
    pub entry: usize,
    pub flows: Vec<Flow>,
    pub unwinds: Vec<Option<usize>>,
}

impl Cfg {
    /// Successors in terminator order, then the unwind edge, so a depth-first
    /// search lays the normal path out before the handler.
    fn successors(&self, block: usize) -> Vec<usize> {
        let mut out = self.flows[block].successors();
        out.extend(self.unwinds[block]);
        out
    }

    pub(crate) fn loop_headers(&self) -> BTreeSet<usize> {
        self.traverse().1
    }

    /// Terminator-order DFS, collecting postorder and back-edge targets.
    fn traverse(&self) -> (Vec<usize>, BTreeSet<usize>) {
        let mut postorder = Vec::with_capacity(self.flows.len());
        let mut headers = BTreeSet::new();
        let mut on_path = vec![false; self.flows.len()];
        let mut seen = vec![false; self.flows.len()];
        let mut stack = vec![(self.entry, self.successors(self.entry).into_iter())];
        seen[self.entry] = true;
        on_path[self.entry] = true;
        while let Some((block, successors)) = stack.last_mut() {
            match successors.next() {
                Some(next) if on_path[next] => {
                    headers.insert(next);
                }
                Some(next) if !seen[next] => {
                    seen[next] = true;
                    on_path[next] = true;
                    stack.push((next, self.successors(next).into_iter()));
                }
                Some(_) => {}
                None => {
                    on_path[*block] = false;
                    postorder.push(*block);
                    stack.pop();
                }
            }
        }
        (postorder, headers)
    }

    fn distinct_successors(&self, block: usize) -> Vec<usize> {
        let mut out = self.successors(block);
        let mut seen = Vec::with_capacity(out.len());
        out.retain(|block| {
            let fresh = !seen.contains(block);
            seen.push(*block);
            fresh
        });
        out
    }
}

/// The structured program, with the jump each block takes to its handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Structured {
    pub stmts: Vec<Stmt>,
    /// Per block, the statements that leave for its unwind handler (a
    /// `break` to the handler's label); empty for a block without one or an
    /// unreachable block.
    pub unwind_jumps: Vec<Vec<Stmt>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Stmt {
    /// The straight-line body of a block, including whatever its terminator
    /// computes before transferring control.
    Leaf(usize),
    /// A labeled block whose body leaves it with [`Stmt::Break`] to `label`;
    /// block `label`'s own code follows it in the enclosing sequence.
    Block {
        label: usize,
        body: Vec<Stmt>,
    },
    Loop {
        label: usize,
        body: Vec<Stmt>,
    },
    If {
        block: usize,
        then_branch: Vec<Stmt>,
        else_branch: Vec<Stmt>,
    },
    /// The multi-way branch that ends `block`. Each arm group lists the
    /// indices (into the terminator's arm list) that share one target.
    Switch {
        block: usize,
        arms: Vec<(Vec<usize>, Vec<Stmt>)>,
        otherwise: Vec<Stmt>,
    },
    /// Leave the labeled block `label` (a forward jump to a merge node).
    Break(usize),
    /// Leave the loop headed by `label`, continuing with the code after it (a
    /// forward jump to the loop's natural exit).
    BreakLoop(usize),
    /// Re-enter the labeled loop `label` (a back edge).
    Continue(usize),
    /// Leave the function as `block`'s terminator says.
    Exit(usize),
}

/// Violated MIR invariants: lowering must produce reducible, well-formed graphs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StructureError {
    MissingEntry(usize),
    UnknownBlock {
        from: usize,
        to: usize,
    },
    /// A retreating edge whose target does not dominate its source.
    Irreducible {
        from: usize,
        to: usize,
    },
}

impl fmt::Display for StructureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingEntry(entry) => write!(f, "entry block bb{entry} does not exist"),
            Self::UnknownBlock { from, to } => {
                write!(f, "bb{from} jumps to missing block bb{to}")
            }
            Self::Irreducible { from, to } => {
                write!(f, "irreducible control flow: bb{from} -> bb{to}")
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Label {
    Block(usize),
    Loop(usize),
}

struct Analysis<'a> {
    cfg: &'a Cfg,
    /// Reverse postorder number of each reachable block.
    rpo: Vec<Option<usize>>,
    /// Dominator-tree children, each list sorted by descending reverse
    /// postorder so the outermost merge block comes first.
    children: Vec<Vec<usize>>,
    is_merge: Vec<bool>,
    /// For a loop header, which blocks its natural loop contains.
    loop_members: Vec<Option<Vec<bool>>>,
    /// For a block that is the natural exit of a loop, that loop's header.
    natural_exit_of: Vec<Option<usize>>,
    /// Filled as each leaf is emitted: the jump to its unwind handler.
    unwind_jumps: RefCell<Vec<Vec<Stmt>>>,
}

/// Structure `cfg` into a statement sequence. Unreachable blocks are dropped.
pub(crate) fn structurize(cfg: &Cfg) -> Result<Structured, StructureError> {
    let analysis = Analysis::new(cfg)?;
    let mut context = Vec::new();
    let stmts = analysis.do_tree(cfg.entry, &mut context)?;
    Ok(Structured {
        stmts,
        unwind_jumps: analysis.unwind_jumps.into_inner(),
    })
}

impl<'a> Analysis<'a> {
    fn new(cfg: &'a Cfg) -> Result<Self, StructureError> {
        let n = cfg.flows.len();
        if cfg.entry >= n {
            return Err(StructureError::MissingEntry(cfg.entry));
        }
        debug_assert_eq!(cfg.unwinds.len(), n, "one unwind slot per block");
        for from in 0..n {
            if let Some(to) = cfg.successors(from).into_iter().find(|&to| to >= n) {
                return Err(StructureError::UnknownBlock { from, to });
            }
        }

        let (postorder, loop_headers) = cfg.traverse();
        let reached = postorder.len();
        let mut rpo = vec![None; n];
        let mut by_rpo = vec![0; reached];
        for (post_index, &block) in postorder.iter().enumerate() {
            let number = reached - 1 - post_index;
            rpo[block] = Some(number);
            by_rpo[number] = block;
        }

        let mut preds: Vec<Vec<usize>> = vec![Vec::new(); n];
        for &block in &by_rpo {
            for succ in cfg.successors(block) {
                preds[succ].push(block);
            }
        }

        // Cooper-Harvey-Kennedy iterative dominators over reverse postorder.
        let mut idom: Vec<Option<usize>> = vec![None; n];
        idom[cfg.entry] = Some(cfg.entry);
        let intersect = |idom: &[Option<usize>], mut a: usize, mut b: usize| {
            while a != b {
                while rpo[a] > rpo[b] {
                    a = idom[a].expect("processed blocks have a dominator");
                }
                while rpo[b] > rpo[a] {
                    b = idom[b].expect("processed blocks have a dominator");
                }
            }
            a
        };
        let mut changed = true;
        while changed {
            changed = false;
            for &block in by_rpo.iter().skip(1) {
                let mut new_idom = None;
                for &pred in &preds[block] {
                    if idom[pred].is_none() {
                        continue;
                    }
                    new_idom = Some(match new_idom {
                        None => pred,
                        Some(current) => intersect(&idom, pred, current),
                    });
                }
                if new_idom.is_some() && idom[block] != new_idom {
                    idom[block] = new_idom;
                    changed = true;
                }
            }
        }

        // Every retreating edge must be a back edge to a dominating header.
        let dominates = |a: usize, mut b: usize| loop {
            if a == b {
                return true;
            }
            if b == cfg.entry {
                return false;
            }
            b = idom[b].expect("reachable blocks have a dominator");
        };
        let mut is_merge = vec![false; n];
        let mut forward_in = vec![0usize; n];
        for &block in &by_rpo {
            for succ in cfg.distinct_successors(block) {
                if rpo[succ] <= rpo[block] {
                    if !dominates(succ, block) {
                        return Err(StructureError::Irreducible {
                            from: block,
                            to: succ,
                        });
                    }
                } else {
                    forward_in[succ] += 1;
                }
            }
        }
        for block in 0..n {
            is_merge[block] = forward_in[block] >= 2;
        }
        // Handlers need labels for unwinding from the middle of a block.
        for &block in &by_rpo {
            if let Some(handler) = cfg.unwinds[block] {
                is_merge[handler] = true;
            }
        }

        let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
        for &block in by_rpo.iter().skip(1) {
            children[idom[block].expect("reachable")].push(block);
        }
        for list in &mut children {
            list.sort_by(|a, b| rpo[*b].cmp(&rpo[*a]));
        }

        // The header must dominate every member of its natural loop.
        let mut loop_members: Vec<Option<Vec<bool>>> = vec![None; n];
        let mut natural_exit_of: Vec<Option<usize>> = vec![None; n];
        for &header in &by_rpo {
            if !loop_headers.contains(&header) {
                continue;
            }
            let mut members = vec![false; n];
            members[header] = true;
            let mut work: Vec<usize> = preds[header]
                .iter()
                .copied()
                .filter(|&pred| rpo[pred] >= rpo[header])
                .collect();
            while let Some(block) = work.pop() {
                if members[block] {
                    continue;
                }
                if !dominates(header, block) {
                    return Err(StructureError::Irreducible {
                        from: block,
                        to: header,
                    });
                }
                members[block] = true;
                work.extend(preds[block].iter().copied());
            }
            // The first dominated block outside the loop follows it in the output.
            let natural = children[header]
                .iter()
                .copied()
                .filter(|&child| !members[child])
                .min_by_key(|&child| rpo[child]);
            if let Some(exit) = natural {
                natural_exit_of[exit] = Some(header);
            }
            loop_members[header] = Some(members);
        }

        Ok(Self {
            cfg,
            rpo,
            children,
            is_merge,
            loop_members,
            natural_exit_of,
            unwind_jumps: RefCell::new(vec![Vec::new(); n]),
        })
    }

    fn do_tree(&self, block: usize, context: &mut Vec<Label>) -> Result<Vec<Stmt>, StructureError> {
        let Some(members) = &self.loop_members[block] else {
            let merge_children: Vec<usize> = self.children[block]
                .iter()
                .copied()
                .filter(|&child| self.is_merge[child])
                .collect();
            return self.node_within(block, &merge_children, context);
        };
        // Merge exits wrap the loop; internal merges stay inside it.
        let (inside, outside): (Vec<usize>, Vec<usize>) = self.children[block]
            .iter()
            .copied()
            .filter(|&child| self.is_merge[child] && self.natural_exit_of[child] != Some(block))
            .partition(|&child| members[child]);
        self.loop_within(block, &inside, &outside, context)
    }

    /// `outside` lists merge exits in outermost-first order.
    fn loop_within(
        &self,
        header: usize,
        inside: &[usize],
        outside: &[usize],
        context: &mut Vec<Label>,
    ) -> Result<Vec<Stmt>, StructureError> {
        if let Some((&outer, rest)) = outside.split_first() {
            context.push(Label::Block(outer));
            let body = self.loop_within(header, inside, rest, context)?;
            context.pop();
            let mut out = vec![Stmt::Block { label: outer, body }];
            out.extend(self.do_tree(outer, context)?);
            return Ok(out);
        }
        context.push(Label::Loop(header));
        let body = self.node_within(header, inside, context)?;
        context.pop();
        let mut out = vec![Stmt::Loop {
            label: header,
            body,
        }];
        let natural = self.children[header]
            .iter()
            .copied()
            .find(|&child| self.natural_exit_of[child] == Some(header));
        if let Some(exit) = natural {
            out.extend(self.do_tree(exit, context)?);
        }
        Ok(out)
    }

    fn node_within(
        &self,
        block: usize,
        merge_children: &[usize],
        context: &mut Vec<Label>,
    ) -> Result<Vec<Stmt>, StructureError> {
        if let Some((&outer, rest)) = merge_children.split_first() {
            context.push(Label::Block(outer));
            let body = self.node_within(block, rest, context)?;
            context.pop();
            let mut out = vec![Stmt::Block { label: outer, body }];
            out.extend(self.do_tree(outer, context)?);
            return Ok(out);
        }
        let mut out = vec![Stmt::Leaf(block)];
        if let Some(handler) = self.cfg.unwinds[block] {
            let jump = self.do_branch(block, handler, context)?;
            self.unwind_jumps.borrow_mut()[block] = jump;
        }
        match &self.cfg.flows[block] {
            Flow::Goto(target) => out.extend(self.do_branch(block, *target, context)?),
            Flow::Branch {
                then_block,
                else_block,
            } if then_block == else_block => {
                // The leaf has already evaluated the condition.
                out.extend(self.do_branch(block, *then_block, context)?);
            }
            Flow::Branch {
                then_block,
                else_block,
            } => out.push(Stmt::If {
                block,
                then_branch: self.do_branch(block, *then_block, context)?,
                else_branch: self.do_branch(block, *else_block, context)?,
            }),
            Flow::Switch { arms, otherwise } => {
                if arms.iter().all(|target| target == otherwise) {
                    out.extend(self.do_branch(block, *otherwise, context)?);
                    return Ok(out);
                }
                // Group shared targets in first-appearance order to avoid duplication.
                let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
                for (index, &target) in arms.iter().enumerate() {
                    if target == *otherwise {
                        continue;
                    }
                    match groups.iter_mut().find(|(t, _)| *t == target) {
                        Some((_, indices)) => indices.push(index),
                        None => groups.push((target, vec![index])),
                    }
                }
                let mut structured = Vec::with_capacity(groups.len());
                for (target, indices) in groups {
                    structured.push((indices, self.do_branch(block, target, context)?));
                }
                out.push(Stmt::Switch {
                    block,
                    arms: structured,
                    otherwise: self.do_branch(block, *otherwise, context)?,
                });
            }
            Flow::Exit => out.push(Stmt::Exit(block)),
        }
        Ok(out)
    }

    fn do_branch(
        &self,
        source: usize,
        target: usize,
        context: &mut Vec<Label>,
    ) -> Result<Vec<Stmt>, StructureError> {
        let (label, jump) = if self.rpo[target] <= self.rpo[source] {
            (Label::Loop(target), Stmt::Continue(target))
        } else if let Some(header) = self.natural_exit_of[target] {
            (Label::Loop(header), Stmt::BreakLoop(header))
        } else if self.is_merge[target] {
            (Label::Block(target), Stmt::Break(target))
        } else {
            return self.do_tree(target, context);
        };
        if !context.contains(&label) {
            return Err(StructureError::Irreducible {
                from: source,
                to: target,
            });
        }
        Ok(vec![jump])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch(then_block: usize, else_block: usize) -> Flow {
        Flow::Branch {
            then_block,
            else_block,
        }
    }

    fn switch(arms: &[usize], otherwise: usize) -> Flow {
        Flow::Switch {
            arms: arms.to_vec(),
            otherwise,
        }
    }

    fn shape(stmts: &[Stmt]) -> String {
        stmts
            .iter()
            .map(|stmt| match stmt {
                Stmt::Leaf(b) => format!("leaf{b}"),
                Stmt::Exit(b) => format!("exit{b}"),
                Stmt::Break(b) => format!("break{b}"),
                Stmt::BreakLoop(b) => format!("leave{b}"),
                Stmt::Continue(b) => format!("continue{b}"),
                Stmt::Block { label, body } => format!("block{label}[{}]", shape(body)),
                Stmt::Loop { label, body } => format!("loop{label}[{}]", shape(body)),
                Stmt::If {
                    block,
                    then_branch,
                    else_branch,
                } => format!("if{block}[{}][{}]", shape(then_branch), shape(else_branch)),
                Stmt::Switch {
                    block,
                    arms,
                    otherwise,
                } => {
                    let arms = arms
                        .iter()
                        .map(|(indices, body)| format!("{indices:?}[{}]", shape(body)))
                        .collect::<Vec<_>>()
                        .join(";");
                    format!("switch{block}[{arms}][{}]", shape(otherwise))
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn cfg_shapes() {
        use Flow::{Exit as E, Goto as G};
        for (name, flows, unwind, expected) in [
            ("straight_line", vec![G(1), E], None, "leaf0 leaf1 exit1"),
            (
                "diamond",
                vec![branch(1, 2), G(3), G(3), E],
                None,
                "block3[leaf0 if0[leaf1 break3][leaf2 break3]] leaf3 exit3",
            ),
            (
                "break_and_continue",
                vec![
                    G(1),
                    branch(2, 6),
                    branch(3, 4),
                    G(1),
                    branch(6, 5),
                    G(1),
                    E,
                ],
                None,
                "leaf0 loop1[leaf1 if1[leaf2 if2[leaf3 continue1][leaf4 if4[leave1][leaf5 continue1]]][leave1]] leaf6 exit6",
            ),
            (
                "nested_loops",
                vec![
                    G(1),
                    branch(2, 7),
                    branch(3, 5),
                    branch(4, 2),
                    G(5),
                    branch(6, 1),
                    G(1),
                    E,
                ],
                None,
                "leaf0 loop1[leaf1 if1[loop2[leaf2 if2[leaf3 if3[leaf4 leave2][continue2]][leave2]] leaf5 if5[leaf6 continue1][continue1]][leave1]] leaf7 exit7",
            ),
            (
                "early_return",
                vec![G(1), branch(2, 4), branch(3, 1), E, E],
                None,
                "leaf0 loop1[leaf1 if1[leaf2 if2[leaf3 exit3][continue1]][leave1]] leaf4 exit4",
            ),
            (
                "shared_switch_targets",
                vec![switch(&[1, 2, 2], 3), G(3), G(3), E],
                None,
                "block3[leaf0 switch0[[0][leaf1 break3];[1, 2][leaf2 break3]][break3]] leaf3 exit3",
            ),
            (
                "switch_default_arm",
                vec![switch(&[2, 1], 2), G(3), G(3), E],
                None,
                "block3[leaf0 switch0[[1][leaf1 break3]][leaf2 break3]] leaf3 exit3",
            ),
            (
                "switch_fallthrough",
                vec![switch(&[1, 1], 1), E],
                None,
                "leaf0 leaf1 exit1",
            ),
            (
                "distinct_loop_exits",
                vec![G(1), branch(2, 4), branch(5, 3), G(1), G(6), G(6), E],
                None,
                "leaf0 block6[loop1[leaf1 if1[leaf2 if2[leaf5 break6][leaf3 continue1]][leave1]] leaf4 break6] leaf6 exit6",
            ),
            (
                "loop_header_merge",
                vec![branch(1, 2), G(3), G(3), branch(4, 5), G(3), E],
                None,
                "block3[leaf0 if0[leaf1 break3][leaf2 break3]] loop3[leaf3 if3[leaf4 continue3][leave3]] leaf5 exit5",
            ),
            (
                "branch_fallthrough",
                vec![branch(1, 1), E],
                None,
                "leaf0 leaf1 exit1",
            ),
            (
                "unwind",
                vec![G(1), G(3), branch(3, 4), E, E],
                Some((0, 2)),
                "block3[block2[leaf0 leaf1 break3] leaf2 if2[break3][leaf4 exit4]] leaf3 exit3",
            ),
            (
                "unwind_inside_loop",
                vec![G(1), branch(2, 5), G(4), G(1), G(1), E],
                Some((2, 3)),
                "leaf0 loop1[leaf1 if1[block3[leaf2 leaf4 continue1] leaf3 continue1][leave1]] leaf5 exit5",
            ),
            (
                "unwind_only_loop",
                vec![E, branch(2, 3), G(1), E],
                Some((0, 1)),
                "block1[leaf0 exit0] loop1[leaf1 if1[leaf2 continue1][leave1]] leaf3 exit3",
            ),
            ("unreachable", vec![E, G(0)], None, "leaf0 exit0"),
        ] {
            let mut cfg = Cfg {
                entry: 0,
                unwinds: vec![None; flows.len()],
                flows,
            };
            if let Some((from, to)) = unwind {
                cfg.unwinds[from] = Some(to);
            }
            let structured = structurize(&cfg).unwrap();
            assert_eq!(shape(&structured.stmts), expected, "{name}");
            for (block, jumps) in structured.unwind_jumps.iter().enumerate() {
                let expected: Vec<_> = cfg.unwinds[block].map(Stmt::Break).into_iter().collect();
                assert_eq!(*jumps, expected, "{name}: unwind from {block}");
            }
            if name == "unwind_only_loop" {
                assert_eq!(cfg.loop_headers(), BTreeSet::from([1]));
            }
        }
    }

    #[test]
    fn invalid_cfgs() {
        for (entry, flows, error) in [
            (
                0,
                vec![Flow::Goto(7)],
                StructureError::UnknownBlock { from: 0, to: 7 },
            ),
            (3, vec![Flow::Exit], StructureError::MissingEntry(3)),
            (
                0,
                vec![branch(1, 2), Flow::Goto(2), Flow::Goto(1)],
                StructureError::Irreducible { from: 2, to: 1 },
            ),
        ] {
            let cfg = Cfg {
                entry,
                unwinds: vec![None; flows.len()],
                flows,
            };
            assert_eq!(structurize(&cfg), Err(error));
        }
    }
}
