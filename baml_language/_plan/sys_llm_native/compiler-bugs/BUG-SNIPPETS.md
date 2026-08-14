# Compiler bugs found & fixed during the sys_llm→native migration (Aug 2026)

Minimal repros for each bug. All are **fixed on `canary`** (commits `0dd1cff77`,
`8b09b2855`, `340ba4e2b`) — run a snippet against a pre-fix build to reproduce.
Each entry: snippet → what it did wrong → root cause → fix + regression tests.
Deeper write-ups: `POST-MERGE-STATUS.md`, `runtime-ty-unknown/`, `shorthand-if-let/`
in this directory.

---

## 1. `x = expr ?? x` assigned null

```baml
function bug1() -> int? {
    let a: int? = 3;
    let n: int? = null;
    a = n ?? a;    // expected: a stays 3.  BUG: a became null.
    a
}
```

Also broke the field form (`s.a = n ?? s.a`), nested coalesces
(`a = n ?? (n ?? a)`), and method-call LHS (`a = xs.at(0) ?? a`). The `let v =
n ?? a; a = v;` spelling was fine — which is how it hid.

**Root cause**: `lower_null_coalesce` (MIR) was destination-threaded and wrote
`dest := lhs` *before* the null-test branch; `AstStmt::Assign` passes the
assignment target's `Place` through as `dest`, so the fallback expression read
an already-clobbered slot.
**Fix**: `baml_compiler2_mir/src/lower.rs` — branch first (lower_if's shape),
write `dest` on exactly one path after that path's value is known.
**Tests**: `crates/baml_tests/baml_src/ns_null_coalesce_assign/`.

---

## 2. A string literal containing "client" or "prompt" turned a plain function into an LLM function

```baml
function bug2() -> string {
    "the client sent nothing"
    // BUG: E0010 "Only 'client', 'tools' and 'prompt' allowed in LLM
    // function" — a cascade of them — on a function with no LLM anything.
}
```

Same for `"the prompt was empty"` and backtick strings. Bonus corruption: a
literal containing `{` desynchronized the scanner's brace depth, and `//`
inside a literal tripped its comment-skipper.

**Root cause**: the LLM-function classifier pre-scans the *raw token stream* at
brace depth 1, and the lexer has no string token at that layer — quote contents
lex as ordinary tokens, so `client` inside a literal is `KW_CLIENT`. A comment
in the scanner already documented this hazard (it's why `tools` was removed as
a trigger) but `client`/`prompt` were left in.
**Fix**: `baml_compiler_parser/src/parser.rs` — the pre-scan is now
string-aware (`skip_string_literal_from` handling `"…"`, `b"…"`, `#"…"#`,
backtick strings incl. `${…}` interpolation braces). Triggers kept; real LLM
functions still classify.
**Tests**: `crates/baml_tests/baml_src/ns_llm_classifier_strings/` + 2 parser
unit tests.

---

## 3. `from_json` through a union of media types always picked the first member

```baml
function bug3() -> string {
    let a: image | audio = audio.from_base64("aGk=", "audio/mpeg");
    let j = baml.json.to_json(a);                    // envelope carries "kind": "audio"
    let back = baml.json.from_json<image | audio>(j);
    // BUG: `back` was an IMAGE (first union member wins).
    // Reversing the union order flipped it: image in → Audio out.
    match (back) { let i: image => "image", let s: audio => "audio" }
}
```

**Root cause**: `deserialize_media` (`bex_vm/src/package_baml/json.rs`) read
`source`/`value`/`mime` but never compared the envelope's `"kind"` field
against the target kind — even though `serialize_media` writes exactly that tag
— so a media decode could never fail and the union's first-match-wins loop
never moved on.
**Fix**: the one guard — mismatched kind rejects that arm, letting the union
try the next member.
**Tests**: `crates/baml_tests/baml_src/ns_media_json_union/`.

---

## 4. `v is image` compiled to constant false (media primitives in unions matched nothing / the wrong arm)

```baml
function bug4() -> string {
    let v: string | image = image.from_base64("aGk=", "image/png");
    if (v is image) { return "image"; }   // BUG: always false
    if (v is string) { return "string"; } // BUG: also false!
    "neither"                              // ...and a 2-arm match sent the
}                                          // image through the STRING arm,
                                           // stringifying the debug repr.
```

And `image | audio`: an image narrowed to the *audio* arm (actually last-arm
fall-through, since the media arm's test was constant false and the final
exhaustive arm's test is elided).

**Root cause**: `is_type` in `baml_compiler2_emit/src/emit.rs` routes types
without a dedicated arm to a tagless-leaf fallback that emits **constant
false**; `TyTemplate::Media` had no arm. The VM was already correct — the
bytecode never asked it.
**Fix**: `TyTemplate::Media(..) => emit_structural(...)` — one arm; the
structural matcher discriminates by `MediaKind`.
**Tests**: `crates/baml_tests/baml_src/ns_media_union_narrowing/` (17 cases).

---

## 5. `v is type` — same defect, different leaf

```baml
function bug5() -> string {
    let v: type | int = reflect.type_of<int>();
    if (v is type) { return "type"; }   // BUG: always false
    if (v is int) { return "int"; }     // BUG: also false — matched nothing
    "neither"
}
```

**Root cause**: same tagless-leaf fallback — `RealizedTy::Type` missing from
`realized_type_tag`, even though `typetag::TYPE` exists and both
`type_tag_for_ty` (MIR) and `value_type_tag` (VM) already handle it.
**Fix**: one arm in `realized_type_tag` (tag route, so `is`-chains and
match-tag-switches agree). `RustType`/`Resource`/`PromptAst` deliberately left
(values are `Object::RustData`, unreachable from source — documented at the
fallback arm).
**Tests**: `crates/baml_tests/baml_src/ns_type_value_narrowing/` (20 cases).

---

## 6. Property shorthand couldn't see pattern binders — and fired on *quoted* keys

```baml
function bug6(v: string?) -> string {
    if let key: string = v {
        // BUG: E0003 "property shorthand `key` requires an in-scope value
        // named `key`" — on a QUOTED key, with `key` bound right above.
        let params: map<string, string> = { "key": key };
        params.get("key") ?? ""
    } else { "" }
}
```

Genuine shorthand under any pattern binder also failed:

```baml
function bug6b(v: string?) -> string {
    if let name: string = v {
        let m: map<string, string> = { name };   // BUG: same E0003
        m.get("name") ?? ""
    } else { "" }
}
```

Same for `match`-arm, `for`, `catch`, and destructure binders — and a plain
`let` inside a nested block. Renaming the binder (or moving to a plain
top-level `let`) made it compile, which is why it looked so arbitrary.

**Root cause** (two compounding defects in `baml_compiler2_hir_ty/src/infer.rs`):
(a) the `Expr::Map` arm re-derived shorthand-ness *textually* (key text ==
value identifier) instead of consulting the parser's authoritative
`property_shorthand_exprs` marker — so quoted keys were treated as shorthand;
(b) the in-scope check walked ancestors of the *body* scope rather than the
expression's own scope, so every binder introduced by a nested block/pattern
was invisible.
**Fix**: use the source-map marker for shorthand identification; resolve the
value through the same path a plain expression uses (expression-scope
resolution), so shorthand works wherever a bare `key` would.
**Tests**: `crates/baml_tests/baml_src/ns_property_shorthand_binders/` (20
cases) + 3 phase3a unit tests. Original filing: `shorthand-if-let/`.

---

## 7. Catch binder silently typed `X | Unknown` → ICE in bytecode lowering

```baml
function may_throw() -> string throws baml.errors.Io {
    throw baml.errors.Io { message: "boom" }
}

function bug7() -> string {
    // `.to_string()` is checker sugar with no real callee; the catch-binder's
    // throw-fact walker charged it as an UNACCOUNTED callee => Ty::Unknown.
    // `e` was typed `baml.errors.Io | Unknown` — with NO diagnostic —
    // and bytecode lowering then panicked:
    //   internal error: `Unknown` is not a valid RuntimeTy:
    //   an error-recovery type reached runtime lowering
    may_throw().to_string() catch_all (e) {
        _ => e.to_string(),
    }
}
```

**Root cause** (two independent defects that had to line up):
(a) TIR's `collect_throw_facts_from_expr` was a drifted partial copy of
`throws_analysis::collect_from_expr` — missing the three sugar-fallback guards
(`to_string`/`to_json`/`from_json`), so those calls hit the unaccounted-callee
default and charged `Unknown`; (b) MIR's sugar-fallback guards tested
`Unknown` only at the *top level* of the receiver type, right next to a
*recursive* typevar check — a sentinel nested in a union sailed into
`ty_to_template`, which panics. Reachable independently: an unaccounted callee
contributes `Unknown` *by design*, so MIR must tolerate it.
**Fix**: shared `sugar_fallback_call_throws()` so the walkers can't drift;
MIR uses recursive `contains_error_recovery` and **erases** to `unknown`
(dropping to zero type-args traps the VM — `String.from<T>`'s body reads `T`
from a frame slot, so the old guard's "safely drops to ntypeargs=0" comment
was false). The TIR half was later subsumed by the hir_ty rewrite (which
computes catch types correctly); the MIR erasure remains as a guard.
**Repros + analysis**: `runtime-ty-unknown/` (README + runnable repro A/B).

---

## Appendix A — fixed by the hir_ty merge (a0f4605e8), verified by re-probe

- **Phantom `unknown` in catch types**: a cross-package call delegating to a
  directly-recursive function (`ai.wire.merge_request_body` → `_merge_json`)
  was charged as unaccounted; `catch_all` over `aws.internal.build_request`
  reported 8 throw types instead of its declared 7. Now exact.
- **Root-namespace generic + closure MIR panic**: `fn serve<T, E>(body: (S) ->
  T throws E)` declared at project root and called from a test panicked MIR
  lowering with `type variable not found in type args: E`. 5/5 shape variants
  now pass.
- **`catch (e)` multi-throw narrowing**: typed arms narrow correctly (8/8
  probes) — earlier reports of non-narrowing don't reproduce; behavior matches
  documented design.

## Appendix B — deliberate limitations we chose not to "fix" (know about them)

- `function` is an illegal BAML field name, and `@alias` does **not** rename
  `baml.json` wire keys (it's a SAP-layer annotation). The sanctioned pattern
  for wire shapes needing reserved words is `implements baml.ToJson/FromJson`
  overrides — see the chat client's `tool_calls` handling (field `fn`,
  serialized as `"function"`).
- `v is <RustData-backed type>` (`ai.Prompt`'s payload etc.) stays false —
  those values aren't spellable in source; documented at the emit fallback.
