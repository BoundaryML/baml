// Two or more enum variants that serialize to the same JSON key produce an
// unsatisfiable schema (B-649, mirroring the class-field case in B-615):
// `ctx.output_format` renders duplicate labels and a value the model returns
// under the shared label can never be resolved back to a unique variant. Each
// of these enums must be rejected with E0149. (Key matching is case-sensitive,
// mirroring emit's raw string equality on variant names/aliases.)

// Case 1: two variants share the same @alias value.
enum AliasVsAlias {
  A @alias("x")
  B @alias("x")
}

// Case 2: a variant's declared name equals another variant's @alias.
enum NameVsAlias {
  Shared
  B @alias("Shared")
}

// Case 3: three variants collapse onto one key via a mix of the above.
enum TripleCollision {
  First @alias("Shared")
  Second @alias("Shared")
  Shared
}

// Case 4: a double-quoted alias and a raw-string alias normalize to the same
// value and collide.
enum NormalizedCollision {
  A @alias("q")
  B @alias(#"q"#)
}

// ─── Controls: these must NOT be flagged ─────────────────────────────────────

// A variant whose alias equals its OWN name is fine (single occupant of key `A`).
enum AliasEqualsOwnName {
  A @alias("A")
  B
}

// A @skip'd variant is excluded from the schema, so it cannot collide.
enum SkippedDoesNotCollide {
  A @alias("x")
  B @alias("x") @skip
}

// Distinct aliases, distinct names — no collision.
enum AllDistinct {
  A @alias("alpha")
  B @alias("beta")
  C
}

// Distinct casing does NOT collide (`Value` name vs `value` alias are different
// keys), so this enum must also stay clean.
enum CaseSensitive {
  Value
  Other @alias("value")
}
