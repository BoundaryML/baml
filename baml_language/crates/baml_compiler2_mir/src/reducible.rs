//! Exact reducibility test for an optimized MIR body.
//!
//! The builder guarantees lowering is reducible (`Structure` in builder.rs).
//! Optimization rewrites the CFG afterwards, so its output is tested here.
//!
//! Compute dominators, delete every edge `u -> v` where `v` dominates `u`, and
//! check that the rest is acyclic. A CFG is reducible exactly when this holds
//! (Hecht & Ullman 1974), so the test has no false alarms. Edges are
//! terminator successors plus each block's implicit edge to its `unwind`
//! handler; only blocks reachable from the entry count.

use crate::{BlockId, MirFunctionBody, Terminator};

/// The full edge set of `body`, as successor lists by block index.
fn successors(body: &MirFunctionBody<'_>) -> Vec<Vec<usize>> {
    body.blocks
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
        .collect()
}

/// Check that `body`'s CFG is reducible. On failure, returns the blocks left
/// over once every acyclic part is peeled away: a cycle with more than one
/// entry, and what only it reaches.
pub(crate) fn check(body: &MirFunctionBody<'_>) -> Result<(), Vec<BlockId>> {
    check_graph(body.entry.0, &successors(body))
        .map_err(|blocks| blocks.into_iter().map(BlockId).collect())
}

/// The reducibility test over plain successor lists.
fn check_graph(entry: usize, succs: &[Vec<usize>]) -> Result<(), Vec<usize>> {
    let n = succs.len();

    // Reverse postorder of the blocks reachable from `entry`.
    let mut rpo = Vec::with_capacity(n);
    let mut visited = vec![false; n];
    let mut stack = vec![(entry, 0usize)];
    visited[entry] = true;
    while let Some(&(node, next)) = stack.last() {
        if let Some(&succ) = succs[node].get(next) {
            stack.last_mut().expect("non-empty").1 += 1;
            if !visited[succ] {
                visited[succ] = true;
                stack.push((succ, 0));
            }
        } else {
            rpo.push(node);
            stack.pop();
        }
    }
    rpo.reverse();
    let mut order = vec![usize::MAX; n];
    for (i, &node) in rpo.iter().enumerate() {
        order[node] = i;
    }

    let mut preds = vec![Vec::new(); n];
    for &u in &rpo {
        for &v in &succs[u] {
            preds[v].push(u);
        }
    }

    // Immediate dominators (Cooper, Harvey & Kennedy, "A Simple, Fast
    // Dominance Algorithm"), indexed by block.
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
            let mut new_idom = usize::MAX;
            for &p in &preds[node] {
                if idom[p] == usize::MAX {
                    continue;
                }
                new_idom = if new_idom == usize::MAX {
                    p
                } else {
                    intersect(&idom, p, new_idom)
                };
            }
            if idom[node] != new_idom {
                idom[node] = new_idom;
                changed = true;
            }
        }
    }
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
    let mut indegree = vec![0usize; n];
    let mut forward = vec![Vec::new(); n];
    for &u in &rpo {
        for &v in &succs[u] {
            if !dominates(v, u) {
                forward[u].push(v);
                indegree[v] += 1;
            }
        }
    }
    let mut ready: Vec<usize> = rpo.iter().copied().filter(|&b| indegree[b] == 0).collect();
    let mut removed = 0;
    while let Some(u) = ready.pop() {
        removed += 1;
        for &v in &forward[u] {
            indegree[v] -= 1;
            if indegree[v] == 0 {
                ready.push(v);
            }
        }
    }
    if removed == rpo.len() {
        Ok(())
    } else {
        Err(rpo.into_iter().filter(|&b| indegree[b] > 0).collect())
    }
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
