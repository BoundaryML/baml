// Unreachable arms should emit diagnostics
enum Status {
  Active
  Inactive
}

// Wildcard before typed pattern
function WildcardBeforeTyped(x: int) -> string {
  match (x) {
    _ => "catch all",
    let n: int => "unreachable"
  }
}

// Duplicate enum variant
function DuplicateVariant(s: Status) -> string {
  match (s) {
    Status.Active => "first",
    Status.Active => "duplicate",
    Status.Inactive => "inactive"
  }
}

// Duplicate literal
function DuplicateLiteral(x: int) -> string {
  match (x) {
    42 => "first",
    42 => "duplicate",
    _ => "other"
  }
}

//----
//- diagnostics
// E0063
//
//   × unreachable arm
//     ╭─[match_match_unreachable_arms.baml:11:19]
//  11 │     let n: int => "unreachable"
//     ·                   ─────────────
//     ╰────
// E0063
//
//   × unreachable arm
//     ╭─[match_match_unreachable_arms.baml:19:22]
//  19 │     Status.Active => "duplicate",
//     ·                      ───────────
//     ╰────
// E0063
//
//   × unreachable arm
//     ╭─[match_match_unreachable_arms.baml:28:11]
//  28 │     42 => "duplicate",
//     ·           ───────────
//     ╰────
