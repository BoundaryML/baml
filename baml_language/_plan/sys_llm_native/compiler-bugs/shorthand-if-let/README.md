# FIXED — Property-shorthand check re-derived shorthand-ness and could not see pattern binders

Status: **FIXED** (2026-08-14) in `crates/baml_compiler2_hir_ty/src/infer.rs`.

## What was broken

`{ "key": key }` — a QUOTED map key whose text equals the value identifier —
errored with E0003 "property shorthand `key` requires an in-scope value named
`key`" whenever the value identifier was bound by an enclosing `if let`.
Renaming the binder (function `g` in repro.baml) compiled fine.

The scope was wider than originally filed: it fired for **every** pattern binder
(`if let`, match arms, `for (let x in …)`, `catch`/`catch_all`, class
destructures) and for any binder in a **nested block**, and it made even
*genuine* unquoted `{ key }` shorthand a hard error in those positions — which
quoting could not work around.

Found: 2026-08-13, first check after merging a0f4605e8 (hir_ty S0–S5).

## Root cause — two compounding defects in the `Expr::Map` arm

1. **Shorthand-ness was re-derived textually.** AST lowering already records the
   parser's fact in `AstSourceMap::property_shorthand_exprs`
   (`lower_expr_body.rs`, both sites gated on `!seen_colon`). The hir_ty walk
   ignored it and instead asked "is the key a string literal whose text equals a
   single-segment value path?" — which a written `{ "key": key }` satisfies by
   coincidence. That is why quoting did not escape the diagnostic.

2. **Scope was re-derived from a bespoke name list.** The in-scope test used
   `local_binding_names()`, which walked `index.ancestor_scopes(current_scope)`.
   `current_scope` is the *body's* (or lambda's) scope, so the walk started above
   every nested block and never reached pattern binders at all — only top-level
   `let`s and parameters were visible. The underlying expression typed fine
   either way; this was a spurious hard error on valid code.

## The fix

The `Expr::Map` arm now consults the two authorities instead of re-deriving:

* in-scope-ness is the semantic index's `path_resolution` via
  `path_resolves_locally(value)` (plus the template-param and package-item tiers
  `resolve_value_path` uses) — the shorthand value is an ordinary path
  expression, so it resolves exactly when a plain use of the same name does,
  through every binder form and every nesting depth;
* shorthand-ness is the parser's marker, read through the new
  `is_property_shorthand(expr)` helper (a lazily materialized `OnceCell` over the
  owner's `AstSourceMap`, touched only on the diagnostic path). A written
  `{ "key": key }` with an unbound value now reports the honest generic
  `unresolved name` instead of a shorthand rewrite hint.

`local_binding_names` — the near-match suggestion pool — now starts from the
expression's own scope, so a pattern binder is offered as the explicit-mapping
suggestion.

## Regressions

* Corpus: `crates/baml_tests/baml_src/ns_property_shorthand_binders/` (20 tests)
  — quoted and genuine shorthand under `if let`, match arm, `for`, `catch`,
  class destructure, nested block, param, body `let`, closure param; shadowing;
  quoted key that differs from the binder.
* Unit: `crates/baml_tests/src/compiler2_tir/phase3a.rs` —
  `quoted_key_matching_value_name_is_not_property_shorthand`,
  `property_shorthand_resolves_pattern_binders`,
  `property_shorthand_suggests_a_pattern_binder`.

## Workaround removed

`crates/baml_builtins2/baml_std/google/ns_internal/vertex.baml`'s `vertex_url`
binder is `key` again with `{ "key": key }` restored; `baml-cli test -i
llm_google` is 30/30.
