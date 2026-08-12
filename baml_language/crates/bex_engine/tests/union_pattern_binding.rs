//! Regression tests for union-typed refutable pattern bindings (`x: A | B`).
//!
//! A binding ascribed a union type is refutable: it matches when the
//! scrutinee's runtime type is *any* member of the union, and binds the value.
//! The runtime `is`-type test must therefore check every member.
//!
//! Regression: the union multi-target branch in TIR pattern lowering
//! (`lower_pat_dispatch`) lowers the pattern once per scrutinee union member
//! and recorded each member's type into `pattern_types[subpat]` last-write-
//! wins, so only the LAST member survived. MIR reads that per-PatId entry to
//! emit the runtime test, so `x: A | B` tested only `B` — a value of the first
//! member `A` never matched (the loop fell through / `else` was taken). The fix
//! joins the per-member types so the recorded type is the full `A | B`.

mod common;

use bex_engine::BexExternalValue;
use common::{EngineProgram, assert_engine_executes};

// `let x: A | B = cur else { ... }` where `cur` is the FIRST union member.
// Must bind (result 3), not fall through to `else`.
#[tokio::test]
async fn union_typed_let_else_binds_first_member() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
interface WlHasV { v int }
class WlA { v int  implements WlHasV {} }
class WlB { v int  implements WlHasV {} }
class WlStop {}

function f() -> int {
    let cur: WlA | WlB | WlStop = WlA { v: 3 };
    let x: WlA | WlB = cur else { return 0; };
    x.v
}
function main() -> int { f() }
"#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(3)),
        ..Default::default()
    })
    .await
}

// Same, with `cur` being the SECOND union member (always worked; guards the fix
// against regressing the last member).
#[tokio::test]
async fn union_typed_let_else_binds_second_member() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
interface WlHasV { v int }
class WlA { v int  implements WlHasV {} }
class WlB { v int  implements WlHasV {} }
class WlStop {}

function f() -> int {
    let cur: WlA | WlB | WlStop = WlB { v: 7 };
    let x: WlA | WlB = cur else { return 0; };
    x.v
}
function main() -> int { f() }
"#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(7)),
        ..Default::default()
    })
    .await
}

// Control: single-type refutable binding (no union) — one member, never
// clobbered.
#[tokio::test]
async fn single_typed_let_else_binds() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
class WlA { v int }
class WlStop {}

function f() -> int {
    let cur: WlA | WlStop = WlA { v: 3 };
    let x: WlA = cur else { return 0; };
    x.v
}
function main() -> int { f() }
"#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(3)),
        ..Default::default()
    })
    .await
}

// The same union binding as a `match` arm (expression position), first member.
#[tokio::test]
async fn union_typed_match_arm_binds_first_member() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
interface WlHasV { v int }
class WlA { v int  implements WlHasV {} }
class WlB { v int  implements WlHasV {} }
class WlStop {}

function f() -> int {
    let cur: WlA | WlB | WlStop = WlA { v: 3 };
    match (cur) {
        let x: WlA | WlB => x.v,
        _ => 0,
    }
}
function main() -> int { f() }
"#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(3)),
        ..Default::default()
    })
    .await
}

// `match` union arm, second member (guards the last member).
#[tokio::test]
async fn union_typed_match_arm_binds_second_member() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
interface WlHasV { v int }
class WlA { v int  implements WlHasV {} }
class WlB { v int  implements WlHasV {} }
class WlStop {}

function f() -> int {
    let cur: WlA | WlB | WlStop = WlB { v: 7 };
    match (cur) {
        let x: WlA | WlB => x.v,
        _ => 0,
    }
}
function main() -> int { f() }
"#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(7)),
        ..Default::default()
    })
    .await
}

// Control: the `else` branch is genuinely taken when the scrutinee is a
// non-targeted member.
#[tokio::test]
async fn union_typed_let_else_takes_else_on_non_member() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
interface WlHasV { v int }
class WlA { v int  implements WlHasV {} }
class WlB { v int  implements WlHasV {} }
class WlStop {}

function f() -> int {
    let cur: WlA | WlB | WlStop = WlStop {};
    let x: WlA | WlB = cur else { return -1; };
    x.v
}
function main() -> int { f() }
"#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(-1)),
        ..Default::default()
    })
    .await
}
