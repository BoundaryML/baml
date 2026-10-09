//! Structured control flow from a reducible control-flow graph.
//!
//! This is the "Beyond Relooper" / Stackifier scheme: blocks are emitted in
//! reverse postorder inside their immediate dominator's region. A merge node
//! (two or more forward in-edges) opens a labeled block at its dominator that
//! its predecessors leave with `break`; a loop header becomes a labeled `loop`
//! that its back edges re-enter with `continue`. The first block after a loop
//! (the lowest-numbered exit the header dominates) follows the `loop` and is
//! reached by breaking out of it; further exits that merge paths open their
//! labeled blocks around the loop. With a reducible graph this never
//! duplicates a block and never needs a state variable: every reachable
//! block appears exactly once in the output.
//!
//! The structurizer sees only the graph's shape ([`Flow`]), so it can be unit
//! tested on hand-built graphs; the printer fills each [`Stmt::Leaf`] from
//! the MIR block it names.

use std::fmt;

/// How a block hands control to its successors. Everything the terminator
/// computes (a call, a short-circuit assignment) belongs to the block's leaf;
/// only the edge shape matters here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Flow {
    /// One successor.
    Goto(usize),
    /// Two-way branch.
    Branch {
        then_block: usize,
        else_block: usize,
    },
    /// Multi-way branch: one successor per arm, plus the default.
    Switch { arms: Vec<usize>, otherwise: usize },
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

    /// Successors without repeats: a branch or switch whose arms share a
    /// target hands control to that target once, whichever arm fires.
    fn distinct_successors(&self) -> Vec<usize> {
        let mut out = self.successors();
        let mut seen = Vec::with_capacity(out.len());
        out.retain(|block| {
            let fresh = !seen.contains(block);
            seen.push(*block);
            fresh
        });
        out
    }
}

/// A control-flow graph by shape: `flows[i]` is block `i`'s terminator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Cfg {
    pub entry: usize,
    pub flows: Vec<Flow>,
}

/// One statement of the structured program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Stmt {
    /// The straight-line body of a block, including whatever its terminator
    /// computes before transferring control.
    Leaf(usize),
    /// A labeled block whose body leaves it with [`Stmt::Break`] to `label`;
    /// block `label`'s own code follows it in the enclosing sequence.
    Block { label: usize, body: Vec<Stmt> },
    /// A labeled loop headed by block `label`, re-entered with
    /// [`Stmt::Continue`].
    Loop { label: usize, body: Vec<Stmt> },
    /// The two-way branch that ends `block`.
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

/// Why a graph could not be structured. Every variant is a violated MIR
/// invariant: lowering only produces reducible, well-formed graphs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StructureError {
    /// The entry block index is out of range.
    MissingEntry(usize),
    /// A block jumps to an index that is not a block.
    UnknownBlock { from: usize, to: usize },
    /// A retreating edge whose target does not dominate its source.
    Irreducible { from: usize, to: usize },
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
    /// Immediate dominator of each reachable block (the entry is its own).
    idom: Vec<Option<usize>>,
    /// Dominator-tree children, each list sorted by descending reverse
    /// postorder so the outermost merge block comes first.
    children: Vec<Vec<usize>>,
    is_merge: Vec<bool>,
    /// For a loop header, which blocks its natural loop contains.
    loop_members: Vec<Option<Vec<bool>>>,
    /// For a block that is the natural exit of a loop, that loop's header.
    natural_exit_of: Vec<Option<usize>>,
}

/// Structure `cfg` into a statement sequence. Unreachable blocks are dropped.
pub(crate) fn structurize(cfg: &Cfg) -> Result<Vec<Stmt>, StructureError> {
    let analysis = Analysis::new(cfg)?;
    let mut context = Vec::new();
    analysis.do_tree(cfg.entry, &mut context)
}

impl<'a> Analysis<'a> {
    fn new(cfg: &'a Cfg) -> Result<Self, StructureError> {
        let n = cfg.flows.len();
        if cfg.entry >= n {
            return Err(StructureError::MissingEntry(cfg.entry));
        }
        for (from, flow) in cfg.flows.iter().enumerate() {
            if let Some(to) = flow.successors().into_iter().find(|&to| to >= n) {
                return Err(StructureError::UnknownBlock { from, to });
            }
        }

        // Iterative depth-first search for a postorder, taking successors in
        // terminator order so the layout follows the source's branch order.
        let mut postorder = Vec::with_capacity(n);
        let mut visited = vec![false; n];
        let mut stack: Vec<(usize, std::vec::IntoIter<usize>)> = Vec::new();
        visited[cfg.entry] = true;
        stack.push((cfg.entry, cfg.flows[cfg.entry].successors().into_iter()));
        while let Some((block, successors)) = stack.last_mut() {
            match successors.next() {
                Some(next) if !visited[next] => {
                    visited[next] = true;
                    stack.push((next, cfg.flows[next].successors().into_iter()));
                }
                Some(_) => {}
                None => {
                    postorder.push(*block);
                    stack.pop();
                }
            }
        }
        let reached = postorder.len();
        let mut rpo = vec![None; n];
        let mut by_rpo = vec![0; reached];
        for (post_index, &block) in postorder.iter().enumerate() {
            let number = reached - 1 - post_index;
            rpo[block] = Some(number);
            by_rpo[number] = block;
        }

        // Predecessors of reachable blocks, with edge multiplicity.
        let mut preds: Vec<Vec<usize>> = vec![Vec::new(); n];
        for &block in &by_rpo {
            for succ in cfg.flows[block].successors() {
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
        let mut is_loop_header = vec![false; n];
        let mut forward_in = vec![0usize; n];
        for &block in &by_rpo {
            for succ in cfg.flows[block].distinct_successors() {
                if rpo[succ] <= rpo[block] {
                    if !dominates(succ, block) {
                        return Err(StructureError::Irreducible {
                            from: block,
                            to: succ,
                        });
                    }
                    is_loop_header[succ] = true;
                } else {
                    forward_in[succ] += 1;
                }
            }
        }
        for block in 0..n {
            is_merge[block] = forward_in[block] >= 2;
        }

        let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
        for &block in by_rpo.iter().skip(1) {
            children[idom[block].expect("reachable")].push(block);
        }
        for list in &mut children {
            list.sort_by(|a, b| rpo[*b].cmp(&rpo[*a]));
        }

        // A header's natural loop: its back-edge sources and everything that
        // reaches them without passing the header. Reducibility makes every
        // member dominated by the header.
        let mut loop_members: Vec<Option<Vec<bool>>> = vec![None; n];
        let mut natural_exit_of: Vec<Option<usize>> = vec![None; n];
        for &header in &by_rpo {
            if !is_loop_header[header] {
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
            // The exit emitted right after the loop: the first block outside
            // it that it dominates. Every jump to it comes from inside.
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
            idom,
            children,
            is_merge,
            loop_members,
            natural_exit_of,
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
        // Merge nodes inside the loop open their blocks inside it; merge
        // exits other than the natural one open theirs around it, so code
        // after the loop sits after the `loop`.
        let (inside, outside): (Vec<usize>, Vec<usize>) = self.children[block]
            .iter()
            .copied()
            .filter(|&child| self.is_merge[child] && self.natural_exit_of[child] != Some(block))
            .partition(|&child| members[child]);
        self.loop_within(block, &inside, &outside, context)
    }

    /// `outside` is the list of merge exits still to wrap around the loop,
    /// outermost first.
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
        match &self.cfg.flows[block] {
            Flow::Goto(target) => out.extend(self.do_branch(block, *target, context)?),
            Flow::Branch {
                then_block,
                else_block,
            } if then_block == else_block => {
                // Both arms agree, and the condition has no effect of its
                // own: fall through without a conditional.
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
                // An arm that jumps where `otherwise` does is the `_` arm;
                // structuring its target a second time would duplicate the
                // whole subtree (exponentially, for consecutive matches).
                if arms.iter().all(|target| target == otherwise) {
                    out.extend(self.do_branch(block, *otherwise, context)?);
                    return Ok(out);
                }
                // Arms sharing a target share one group, in first-appearance
                // order, so a target is emitted at most once.
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
        if self.rpo[target] <= self.rpo[source] {
            // A back edge: the header's loop is open, or the graph lied.
            if !context.contains(&Label::Loop(target)) {
                return Err(StructureError::Irreducible {
                    from: source,
                    to: target,
                });
            }
            return Ok(vec![Stmt::Continue(target)]);
        }
        if let Some(header) = self.natural_exit_of[target] {
            if !context.contains(&Label::Loop(header)) {
                return Err(StructureError::Irreducible {
                    from: source,
                    to: target,
                });
            }
            return Ok(vec![Stmt::BreakLoop(header)]);
        }
        if self.is_merge[target] {
            if !context.contains(&Label::Block(target)) {
                return Err(StructureError::Irreducible {
                    from: source,
                    to: target,
                });
            }
            return Ok(vec![Stmt::Break(target)]);
        }
        debug_assert_eq!(self.idom[target], Some(source));
        self.do_tree(target, context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(entry: usize, flows: Vec<Flow>) -> Cfg {
        Cfg { entry, flows }
    }

    fn branch(then_block: usize, else_block: usize) -> Flow {
        Flow::Branch {
            then_block,
            else_block,
        }
    }

    /// Every leaf reached, in emission order, and every jump.
    fn walk(stmts: &[Stmt], leaves: &mut Vec<usize>, jumps: &mut Vec<Stmt>) {
        for stmt in stmts {
            match stmt {
                Stmt::Leaf(block) => leaves.push(*block),
                Stmt::Block { body, .. } | Stmt::Loop { body, .. } => walk(body, leaves, jumps),
                Stmt::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    walk(then_branch, leaves, jumps);
                    walk(else_branch, leaves, jumps);
                }
                Stmt::Switch {
                    arms, otherwise, ..
                } => {
                    for (_, body) in arms {
                        walk(body, leaves, jumps);
                    }
                    walk(otherwise, leaves, jumps);
                }
                Stmt::Break(_) | Stmt::BreakLoop(_) | Stmt::Continue(_) | Stmt::Exit(_) => {
                    jumps.push(stmt.clone());
                }
            }
        }
    }

    /// Each reachable block appears exactly once: no duplication.
    fn assert_each_block_once(stmts: &[Stmt], reachable: &[usize]) {
        let mut leaves = Vec::new();
        let mut jumps = Vec::new();
        walk(stmts, &mut leaves, &mut jumps);
        let mut sorted = leaves.clone();
        sorted.sort_unstable();
        let mut expected = reachable.to_vec();
        expected.sort_unstable();
        assert_eq!(sorted, expected, "leaves {leaves:?}");
    }

    fn labels(stmts: &[Stmt], blocks: &mut Vec<usize>, loops: &mut Vec<usize>) {
        for stmt in stmts {
            match stmt {
                Stmt::Block { label, body } => {
                    blocks.push(*label);
                    labels(body, blocks, loops);
                }
                Stmt::Loop { label, body } => {
                    loops.push(*label);
                    labels(body, blocks, loops);
                }
                Stmt::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    labels(then_branch, blocks, loops);
                    labels(else_branch, blocks, loops);
                }
                Stmt::Switch {
                    arms, otherwise, ..
                } => {
                    for (_, body) in arms {
                        labels(body, blocks, loops);
                    }
                    labels(otherwise, blocks, loops);
                }
                Stmt::Leaf(_)
                | Stmt::Break(_)
                | Stmt::BreakLoop(_)
                | Stmt::Continue(_)
                | Stmt::Exit(_) => {}
            }
        }
    }

    #[test]
    fn straight_line() {
        let g = cfg(0, vec![Flow::Goto(1), Flow::Exit]);
        let stmts = structurize(&g).unwrap();
        assert_eq!(stmts, vec![Stmt::Leaf(0), Stmt::Leaf(1), Stmt::Exit(1)]);
    }

    #[test]
    fn diamond_opens_one_block_at_the_fork() {
        // 0 -> {1, 2} -> 3 -> exit
        let g = cfg(
            0,
            vec![branch(1, 2), Flow::Goto(3), Flow::Goto(3), Flow::Exit],
        );
        let stmts = structurize(&g).unwrap();
        assert_eq!(
            stmts,
            vec![
                Stmt::Block {
                    label: 3,
                    body: vec![
                        Stmt::Leaf(0),
                        Stmt::If {
                            block: 0,
                            then_branch: vec![Stmt::Leaf(1), Stmt::Break(3)],
                            else_branch: vec![Stmt::Leaf(2), Stmt::Break(3)],
                        },
                    ],
                },
                Stmt::Leaf(3),
                Stmt::Exit(3),
            ]
        );
    }

    #[test]
    fn while_loop_with_break_and_continue() {
        // 0: init -> 1 (header): branch 2 (body) / 6 (exit)
        // 2: branch 3 (continue path) / 4
        // 3: goto 1          (continue)
        // 4: branch 6 (break) / 5
        // 5: goto 1          (back edge)
        // 6: exit
        let g = cfg(
            0,
            vec![
                Flow::Goto(1),
                branch(2, 6),
                branch(3, 4),
                Flow::Goto(1),
                branch(6, 5),
                Flow::Goto(1),
                Flow::Exit,
            ],
        );
        let stmts = structurize(&g).unwrap();
        assert_each_block_once(&stmts, &[0, 1, 2, 3, 4, 5, 6]);
        let (mut blocks, mut loops) = (Vec::new(), Vec::new());
        labels(&stmts, &mut blocks, &mut loops);
        assert_eq!(loops, vec![1]);
        assert!(blocks.is_empty(), "the exit follows the loop: {blocks:?}");
        let mut leaves = Vec::new();
        let mut jumps = Vec::new();
        walk(&stmts, &mut leaves, &mut jumps);
        assert_eq!(jumps.iter().filter(|j| **j == Stmt::Continue(1)).count(), 2);
        assert_eq!(
            jumps.iter().filter(|j| **j == Stmt::BreakLoop(1)).count(),
            2,
            "both exits leave the loop itself"
        );
        assert!(jumps.contains(&Stmt::Exit(6)));
        // The exit's code comes right after the loop.
        assert!(matches!(stmts[1], Stmt::Loop { label: 1, .. }), "{stmts:?}");
        assert_eq!(stmts[2..], [Stmt::Leaf(6), Stmt::Exit(6)]);
    }

    #[test]
    fn nested_loops_with_inner_break_to_outer_body() {
        // 0 -> 1 (outer header): branch 2 / 7
        // 2 (inner header): branch 3 / 5
        // 3: branch 4 / 2      (inner back edge from 3)
        // 4: goto 5            (inner break)
        // 5: branch 6 / 1      (outer back edge from 5)
        // 6: goto 1            (continue outer)
        // 7: exit
        let g = cfg(
            0,
            vec![
                Flow::Goto(1),
                branch(2, 7),
                branch(3, 5),
                branch(4, 2),
                Flow::Goto(5),
                branch(6, 1),
                Flow::Goto(1),
                Flow::Exit,
            ],
        );
        let stmts = structurize(&g).unwrap();
        assert_each_block_once(&stmts, &[0, 1, 2, 3, 4, 5, 6, 7]);
        let (mut blocks, mut loops) = (Vec::new(), Vec::new());
        labels(&stmts, &mut blocks, &mut loops);
        assert_eq!(loops, vec![1, 2], "outer loop opened before inner");
        assert!(
            blocks.is_empty(),
            "5 is the inner loop's natural exit: {blocks:?}"
        );
        // The inner loop sits inside the outer one, and block 5 (where the
        // inner loop's exits meet) directly follows the inner `loop`.
        let outer = find_loop(&stmts, 1).expect("outer loop");
        find_loop(outer, 2).expect("inner loop nested in outer");
        let Stmt::If { then_branch, .. } = &outer[1] else {
            panic!("{outer:?}");
        };
        assert!(
            matches!(then_branch[0], Stmt::Loop { label: 2, .. }),
            "{then_branch:?}"
        );
        assert_eq!(then_branch[1], Stmt::Leaf(5));
        let mut leaves = Vec::new();
        let mut jumps = Vec::new();
        walk(&stmts, &mut leaves, &mut jumps);
        assert_eq!(
            jumps.iter().filter(|j| **j == Stmt::BreakLoop(2)).count(),
            2
        );
        assert_eq!(
            jumps.iter().filter(|j| **j == Stmt::BreakLoop(1)).count(),
            1
        );
    }

    fn find_loop(stmts: &[Stmt], wanted: usize) -> Option<&Vec<Stmt>> {
        for stmt in stmts {
            let found = match stmt {
                Stmt::Loop { label, body } if *label == wanted => Some(body),
                Stmt::Loop { body, .. } | Stmt::Block { body, .. } => find_loop(body, wanted),
                Stmt::If {
                    then_branch,
                    else_branch,
                    ..
                } => find_loop(then_branch, wanted).or_else(|| find_loop(else_branch, wanted)),
                Stmt::Switch {
                    arms, otherwise, ..
                } => arms
                    .iter()
                    .find_map(|(_, body)| find_loop(body, wanted))
                    .or_else(|| find_loop(otherwise, wanted)),
                Stmt::Leaf(_)
                | Stmt::Break(_)
                | Stmt::BreakLoop(_)
                | Stmt::Continue(_)
                | Stmt::Exit(_) => None,
            };
            if found.is_some() {
                return found;
            }
        }
        None
    }

    #[test]
    fn early_return_inside_loop() {
        // 0 -> 1 (header): branch 2 / 4; 2: branch 3 / 1; 3: exit (return); 4: exit
        let g = cfg(
            0,
            vec![
                Flow::Goto(1),
                branch(2, 4),
                branch(3, 1),
                Flow::Exit,
                Flow::Exit,
            ],
        );
        let stmts = structurize(&g).unwrap();
        assert_each_block_once(&stmts, &[0, 1, 2, 3, 4]);
        let mut leaves = Vec::new();
        let mut jumps = Vec::new();
        walk(&stmts, &mut leaves, &mut jumps);
        assert!(jumps.contains(&Stmt::Exit(3)));
        assert!(jumps.contains(&Stmt::Exit(4)));
        assert!(jumps.contains(&Stmt::Continue(1)));
        assert!(jumps.contains(&Stmt::BreakLoop(1)));
        let (mut blocks, mut loops) = (Vec::new(), Vec::new());
        labels(&stmts, &mut blocks, &mut loops);
        assert_eq!(loops, vec![1]);
        assert!(blocks.is_empty(), "single-predecessor exits need no block");
        assert_eq!(stmts[2..], [Stmt::Leaf(4), Stmt::Exit(4)]);
    }

    #[test]
    fn switch_arms_falling_through_to_otherwise() {
        // 0: switch [1 -> 1, 2 -> 2, 3 -> 2] otherwise 3; 1, 2 -> 3; 3: exit
        let g = cfg(
            0,
            vec![
                Flow::Switch {
                    arms: vec![1, 2, 2],
                    otherwise: 3,
                },
                Flow::Goto(3),
                Flow::Goto(3),
                Flow::Exit,
            ],
        );
        let stmts = structurize(&g).unwrap();
        assert_each_block_once(&stmts, &[0, 1, 2, 3]);
        let Stmt::Block { label: 3, body } = &stmts[0] else {
            panic!("merge block for 3 first: {stmts:?}");
        };
        let Stmt::Switch {
            block: 0,
            arms,
            otherwise,
        } = &body[1]
        else {
            panic!("switch after leaf 0: {body:?}");
        };
        assert_eq!(arms.len(), 2, "two targets, three keys");
        assert_eq!(arms[1].0, vec![1, 2], "keys sharing a target are grouped");
        assert_eq!(*otherwise, vec![Stmt::Break(3)]);
        assert_eq!(stmts[1..], [Stmt::Leaf(3), Stmt::Exit(3)]);
    }

    #[test]
    fn switch_arms_sharing_the_default_fold_into_it() {
        // 0: switch [1 -> 2, 2 -> 1] otherwise 2; 1 -> 3; 2 -> 3; 3: exit
        let g = cfg(
            0,
            vec![
                Flow::Switch {
                    arms: vec![2, 1],
                    otherwise: 2,
                },
                Flow::Goto(3),
                Flow::Goto(3),
                Flow::Exit,
            ],
        );
        let stmts = structurize(&g).unwrap();
        assert_each_block_once(&stmts, &[0, 1, 2, 3]);
        let Stmt::Block { label: 3, body } = &stmts[0] else {
            panic!("{stmts:?}");
        };
        let Stmt::Switch { arms, .. } = &body[1] else {
            panic!("{body:?}");
        };
        assert_eq!(
            arms.len(),
            1,
            "the arm that shares bb2 with `otherwise` is folded"
        );
        assert_eq!(arms[0].0, vec![1]);

        // Every arm shares the default: no conditional at all.
        let g = cfg(
            0,
            vec![
                Flow::Switch {
                    arms: vec![1, 1],
                    otherwise: 1,
                },
                Flow::Exit,
            ],
        );
        assert_eq!(
            structurize(&g).unwrap(),
            vec![Stmt::Leaf(0), Stmt::Leaf(1), Stmt::Exit(1)]
        );
    }

    #[test]
    fn loop_with_two_exits_to_distinct_blocks() {
        // 0 -> 1 (header): branch 2 / 4; 2: branch 5 / 3; 3: goto 1; 4: goto 6; 5: goto 6; 6: exit
        let g = cfg(
            0,
            vec![
                Flow::Goto(1),
                branch(2, 4),
                branch(5, 3),
                Flow::Goto(1),
                Flow::Goto(6),
                Flow::Goto(6),
                Flow::Exit,
            ],
        );
        let stmts = structurize(&g).unwrap();
        assert_each_block_once(&stmts, &[0, 1, 2, 3, 4, 5, 6]);
        let (mut blocks, mut loops) = (Vec::new(), Vec::new());
        labels(&stmts, &mut blocks, &mut loops);
        assert_eq!(loops, vec![1]);
        assert_eq!(
            blocks,
            vec![6],
            "6 merges the two exit paths around the loop"
        );
        // 4 is the natural exit (right after the `loop`); 5 has one
        // predecessor inside the loop and is inlined there. Both then break
        // to the block for 6, which wraps the loop.
        let Stmt::Block { label: 6, body } = &stmts[1] else {
            panic!("{stmts:?}");
        };
        assert!(matches!(body[0], Stmt::Loop { label: 1, .. }), "{body:?}");
        assert_eq!(body[1..], [Stmt::Leaf(4), Stmt::Break(6)]);
        let mut leaves = Vec::new();
        let mut jumps = Vec::new();
        walk(&stmts, &mut leaves, &mut jumps);
        assert_eq!(jumps.iter().filter(|j| **j == Stmt::Break(6)).count(), 2);
        assert_eq!(
            jumps.iter().filter(|j| **j == Stmt::BreakLoop(1)).count(),
            1
        );
    }

    #[test]
    fn loop_header_that_is_also_a_merge_node() {
        // 0: branch 1 / 2; 1, 2 -> 3 (header): branch 4 / 5; 4: goto 3; 5: exit
        let g = cfg(
            0,
            vec![
                branch(1, 2),
                Flow::Goto(3),
                Flow::Goto(3),
                branch(4, 5),
                Flow::Goto(3),
                Flow::Exit,
            ],
        );
        let stmts = structurize(&g).unwrap();
        assert_each_block_once(&stmts, &[0, 1, 2, 3, 4, 5]);
        let (mut blocks, mut loops) = (Vec::new(), Vec::new());
        labels(&stmts, &mut blocks, &mut loops);
        assert_eq!(blocks, vec![3]);
        assert_eq!(loops, vec![3]);
        assert!(matches!(stmts[1], Stmt::Loop { label: 3, .. }));
    }

    #[test]
    fn branch_with_one_target_is_a_fallthrough() {
        let g = cfg(0, vec![branch(1, 1), Flow::Exit]);
        assert_eq!(
            structurize(&g).unwrap(),
            vec![Stmt::Leaf(0), Stmt::Leaf(1), Stmt::Exit(1)]
        );
    }

    #[test]
    fn irreducible_graph_is_rejected() {
        // 0: branch 1 / 2; 1 -> 2; 2 -> 1 : a two-entry cycle.
        let g = cfg(0, vec![branch(1, 2), Flow::Goto(2), Flow::Goto(1)]);
        assert!(matches!(
            structurize(&g),
            Err(StructureError::Irreducible { .. })
        ));
    }

    #[test]
    fn unreachable_blocks_are_dropped() {
        let g = cfg(0, vec![Flow::Exit, Flow::Goto(0)]);
        assert_eq!(structurize(&g).unwrap(), vec![Stmt::Leaf(0), Stmt::Exit(0)]);
    }

    #[test]
    fn malformed_graphs_are_reported() {
        assert_eq!(
            structurize(&cfg(0, vec![Flow::Goto(7)])),
            Err(StructureError::UnknownBlock { from: 0, to: 7 })
        );
        assert_eq!(
            structurize(&cfg(3, vec![Flow::Exit])),
            Err(StructureError::MissingEntry(3))
        );
    }
}
