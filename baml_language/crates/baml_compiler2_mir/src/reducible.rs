//! Exact reducibility test for a MIR body.
//!
//! Reducible means every loop has one way in: its header dominates every block
//! of the loop. Structured control flow (`while`, `for`, `break`, `continue`)
//! produces only such loops, and the Rust backend's structurer depends on it.
//! Lowering alone never makes an irreducible graph, but optimization rewrites
//! the CFG afterwards, so the debug verifier tests its output here.
//!
//! Compute dominators, delete every edge `u -> v` where `v` dominates `u`,
//! and check that what remains is acyclic. A CFG is reducible exactly when
//! this holds (Hecht & Ullman 1974), so the test has no false alarms. Edges
//! are terminator successors plus each block's implicit edge to its `unwind`
//! handler; only blocks reachable from the entry count.

use crate::{BlockId, MirFunctionBody, Terminator};

/// Check that `body`'s CFG is reducible. On failure, returns the blocks left
/// once every acyclic part is peeled away: a cycle with more than one entry,
/// and what only it reaches.
pub(crate) fn check(body: &MirFunctionBody<'_>) -> Result<(), Vec<BlockId>> {
    let succs: Vec<Vec<usize>> = body
        .blocks
        .iter()
        .map(|block| {
            block
                .terminator
                .iter()
                .flat_map(Terminator::successors)
                .chain(block.unwind)
                .map(|b| b.0)
                .collect()
        })
        .collect();
    check_graph(body.entry.0, &succs).map_err(|blocks| blocks.into_iter().map(BlockId).collect())
}

/// The reducibility test over plain successor lists.
fn check_graph(entry: usize, succs: &[Vec<usize>]) -> Result<(), Vec<usize>> {
    let rpo = reverse_postorder(entry, succs);
    let idom = immediate_dominators(entry, succs, &rpo);
    let dominates = |a: usize, mut b: usize| loop {
        if a == b {
            return true;
        }
        if b == entry {
            return false;
        }
        b = idom[b];
    };
    // Delete the dominator back edges; what remains must be acyclic.
    let mut forward = vec![Vec::new(); succs.len()];
    for &u in &rpo {
        forward[u].extend(succs[u].iter().copied().filter(|&v| !dominates(v, u)));
    }
    let stuck = cycle_residue(&rpo, &forward);
    if stuck.is_empty() {
        return Ok(());
    }
    Err(stuck)
}

/// Peel the acyclic part off `forward` (Kahn's algorithm over the blocks in
/// `rpo`); what is left lies on or behind a cycle.
fn cycle_residue(rpo: &[usize], forward: &[Vec<usize>]) -> Vec<usize> {
    let mut indegree = vec![0usize; forward.len()];
    for &u in rpo {
        for &v in &forward[u] {
            indegree[v] += 1;
        }
    }
    let mut ready: Vec<usize> = rpo.iter().copied().filter(|&b| indegree[b] == 0).collect();
    while let Some(u) = ready.pop() {
        for &v in &forward[u] {
            indegree[v] -= 1;
            if indegree[v] == 0 {
                ready.push(v);
            }
        }
    }
    rpo.iter().copied().filter(|&b| indegree[b] > 0).collect()
}

/// The blocks reachable from `entry`, in reverse postorder.
fn reverse_postorder(entry: usize, succs: &[Vec<usize>]) -> Vec<usize> {
    let mut postorder = Vec::with_capacity(succs.len());
    let mut visited = vec![false; succs.len()];
    let mut stack = vec![(entry, 0usize)];
    visited[entry] = true;
    while let Some(&(node, next)) = stack.last() {
        let Some(&succ) = succs[node].get(next) else {
            postorder.push(node);
            stack.pop();
            continue;
        };
        stack.last_mut().expect("non-empty").1 += 1;
        if !visited[succ] {
            visited[succ] = true;
            stack.push((succ, 0));
        }
    }
    postorder.reverse();
    postorder
}

/// Immediate dominators indexed by block (Cooper, Harvey & Kennedy, "A
/// Simple, Fast Dominance Algorithm"). The entry is its own dominator;
/// unreachable blocks keep `usize::MAX`.
fn immediate_dominators(entry: usize, succs: &[Vec<usize>], rpo: &[usize]) -> Vec<usize> {
    let n = succs.len();
    let mut order = vec![usize::MAX; n];
    for (i, &node) in rpo.iter().enumerate() {
        order[node] = i;
    }
    let mut preds = vec![Vec::new(); n];
    for &u in rpo {
        for &v in &succs[u] {
            preds[v].push(u);
        }
    }

    let mut idom = vec![usize::MAX; n];
    idom[entry] = entry;
    let intersect = |idom: &[usize], mut a: usize, mut b: usize| {
        while a != b {
            while order[a] > order[b] {
                a = idom[a];
            }
            while order[b] > order[a] {
                b = idom[b];
            }
        }
        a
    };
    let mut changed = true;
    while changed {
        changed = false;
        for &node in rpo.iter().skip(1) {
            let new_idom = preds[node]
                .iter()
                .copied()
                .filter(|&p| idom[p] != usize::MAX)
                .reduce(|a, b| intersect(&idom, a, b))
                .expect("a reachable block has a processed predecessor");
            if idom[node] != new_idom {
                idom[node] = new_idom;
                changed = true;
            }
        }
    }
    idom
}

#[cfg(test)]
mod tests {
    use super::check_graph;

    #[test]
    fn straight_line_and_diamond_are_reducible() {
        assert_eq!(check_graph(0, &[vec![1], vec![2], vec![]]), Ok(()));
        assert_eq!(
            check_graph(0, &[vec![1, 2], vec![3], vec![3], vec![]]),
            Ok(())
        );
    }

    #[test]
    fn natural_loops_are_reducible() {
        // 0 -> 1 (header) -> 2 -> 1, 1 -> 3
        assert_eq!(
            check_graph(0, &[vec![1], vec![2, 3], vec![1], vec![]]),
            Ok(())
        );
        // Self-loop.
        assert_eq!(check_graph(0, &[vec![1], vec![1, 2], vec![]]), Ok(()));
        // Nested loops with a break out of both and a continue of the outer.
        // 0 -> 1 (outer) -> 2 (inner) -> 3 -> {2, 1, 4}; 2 -> 1; 1 -> 4
        assert_eq!(
            check_graph(0, &[vec![1], vec![2, 4], vec![3, 1], vec![2, 1, 4], vec![]]),
            Ok(())
        );
    }

    #[test]
    fn two_entry_loop_is_irreducible() {
        // The classic: 0 enters the 1 <-> 2 cycle at both nodes.
        let err = check_graph(0, &[vec![1, 2], vec![2], vec![1]]).unwrap_err();
        assert_eq!(err, vec![1, 2]);
    }

    #[test]
    fn jump_into_loop_body_is_irreducible() {
        // Loop 1 -> 2 -> 1, but 0 also jumps straight to 2.
        assert!(check_graph(0, &[vec![1, 2], vec![2, 3], vec![1], vec![]]).is_err());
    }

    #[test]
    fn unreachable_irreducible_part_is_ignored() {
        // 3 and 4 form a two-entry cycle with 5, but nothing reaches them.
        assert_eq!(
            check_graph(0, &[vec![1], vec![], vec![], vec![4, 5], vec![5], vec![4]]),
            Ok(())
        );
    }
}
