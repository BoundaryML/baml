// obs-bench workload: recursion depth (design §10.2 `deep/recursion_depth`).
//
// The design calls for depth-200 (no fold) and depth-1024 (fold engages past
// 512) variants. The VM caps bytecode call depth at MAX_FRAMES = 256
// (bex_vm/src/vm.rs:103), so a >512-deep stack cannot exist end-to-end and
// the §5.6 recursion fold can never fire on real VM traffic today. This file
// therefore covers the runnable half of the contract:
//
//   depth 200 -> below both the VM cap and the fold threshold: every frame a
//                distinct CCT node, `folded_frames == 0`, no RECURSION_FOLD
//                flags. (The committed CI variant.)
//   depth 240 -> near the VM ceiling, still far below 512: asserts headroom
//                behavior is unchanged right up to the cap.
//
// The >512 fold itself is pinned at the consumer level: the CCT engine eats
// synthetic ring records (crates/bex_events cct tests build a depth-1024
// record stream directly), which is the only way depth > MAX_FRAMES can
// reach it. If MAX_FRAMES ever rises past 512, add the end-to-end depth-1024
// leg here.
//
// `reps` re-runs the descent so per-call cost is measurable, not
// setup-dominated. The two-function mutual recursion (down_a <-> down_b)
// makes the folded back-edge target unambiguous: the §5.6 scan must find the
// matching function among the <=8 nearest ancestors.

function down_a(depth: int) -> int {
    if (depth <= 0) {
        0
    } else {
        down_b(depth - 1) + 1
    }
}

function down_b(depth: int) -> int {
    if (depth <= 0) {
        0
    } else {
        down_a(depth - 1) + 1
    }
}

function main(depth: int, reps: int) -> int {
    let total = 0;
    let r = 0;
    while (r < reps) {
        total += down_a(depth);
        r += 1;
    }
    total
}
