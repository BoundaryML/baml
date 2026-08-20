# Namespace redesign — implementation log

Branch `agent/reflect-namespace`, worktree
`/media/tony/WesternDigitalNvmeSsd/Code/baml-namespace`.

## Base and coordination

- Implemented the namespace redesign as one commit, initially coordinated on
  the in-flight mint-key work so the overlapping reflection runtime changes
  were not raced.
- After mint-key PR #4536 merged, fetched `origin` and rebased the completed
  namespace commit directly onto `origin/canary` at
  `54fc33b4fc1f5d82448e8634374e08088e127ecc` with
  `git rebase --onto origin/canary a5a02e2c8 agent/reflect-namespace`.
- Kept the canary versions of the mint-key runtime refactors while applying
  the root-package spellings to them. The CHANGELOG Fixes history from canary
  is preserved, including #4536 and the earlier #4516 entry, with their type
  spellings migrated to `reflect.Type`.
- Regenerated every conflicted or post-rebase compiler snapshot through the
  corpus snapshot test; no snapshot was hand-edited.
- The draft PR targets `canary` directly:
  https://github.com/BoundaryML/baml/pull/4543
- Nothing was enqueued; coordinator review comes first.

## Contract decisions

- `reflect` is an ordinary, reserved stdlib package named `reflect`; there is
  no rewrite to a `baml.reflect` namespace.
- The runtime metatype is spelled `reflect.Type` in annotations and
  expressions. The bare `type` token remains only in type/associated-type
  declarations and in `type T = unreflect(value)` scope bindings.
- `json` remains the one namespace shorthand and continues to resolve to
  `baml.json.json` in type position.
- `env.NAME` remains special syntax and now performs the ratified eager strict
  lookup (`baml.env.get_or_panic("NAME")`). Explicit
  `baml.env.Ref`/`baml.env.ref` remains the separate late-bound API.
- `baml.AnyClass` and `baml.AnyFunction` remain in `baml`.

## Implementation

- Moved the reflection declarations and native VM implementation from `baml`
  into a root `reflect` package with its own manifest and ordinary standard
  dependency, including the runtime metatype and the static
  `reflect.Type.of<T>()` intrinsic.
- Deleted the `reflect` and bare `type` shorthand paths from parsing,
  lowering, inference, diagnostics, rendering, completions, describe output,
  generated SDK routes, fixtures, docs, and website search data. `json` is the
  sole remaining package shorthand.
- Made eager environment lookup usable in top-level client declarations by
  allowing only the environment-get operation during synchronous package
  initialization; other initialization-time I/O remains rejected.
- Retained explicit late-bound credentials in declarations that need them.
  The final post-rebase gate exposed the request-preview fixture as one such
  site, so its deliberately missing credential names now use
  `baml.env.ref("NAME")` instead of strict `env.NAME`.
- Added source-less compiled-package dispatch for the static
  `reflect.Type.of<T>()` method and a lowering-only reflection schema view for
  `baml.AnyClass` default methods without adding a `baml` to `reflect`
  dependency cycle.
- Migrated standard-library internals, native BAML and Rust tests, LSP and CLI
  surfaces, SDK generators and fixtures, bridge comments and generated
  clients, TypeScript grammar fixtures, docs, and changelog references.
- Regenerated the Swift protobuf clients and their input-hash manifest from
  the updated bridge protos; the generated bindings now document
  `reflect.Type` and `reflect.Type.of<T>()`.

## Validation

- Focused checks passed during implementation: HIR package-interface tests
  (8), environment tests (8), Go SDK generator tests (84), Python SDK
  generator tests (123), TypeScript SDK generator tests (47), and
  `cargo check --workspace --tests`.
- The pre-merge pinned accepting and non-accepting gates each passed
  3,145/3,145 tests with 24 skips, green doctests, no unreferenced snapshots,
  and no pending snapshot review.
- The pre-merge remaining-workspace CI mirror passed 5,078/5,078 tests across
  142 binaries with 10 configured skips.
- After rebasing onto merged #4536, the accepting corpus run regenerated the
  mint-key-era bytecode, MIR, and PPIR snapshots. A subsequent full run found
  only the intentional request-preview credentials still using strict
  `env.NAME`; after migrating them to `baml.env.ref`, all five focused preview
  tests and the regenerated corpus snapshot test passed.
- The definitive post-rebase pinned non-accepting gate passed
  3,167/3,167 tests across 93 binaries, with 24 configured skips. Doctests
  passed or were ignored as configured, and Insta reported no unreferenced
  snapshots and nothing to review.
- The definitive post-rebase remaining-workspace CI mirror passed
  5,107/5,107 tests across 142 binaries, with 10 configured skips and one
  excluded binary.
- `RUSTDOCFLAGS="-D warnings" cargo doc --all --no-deps` passed on the final
  rebased tree.
- The checked-in TypeScript grammar package build and all 1,904 package tests
  passed; the namespace rebase did not touch those grammar sources or their
  regenerated token-scope snapshots.
- Final audits pass: Rust formatting, `git diff --check`, no `*.snap.new`
  files, no generated SDK client directories, no bare `type` value
  annotations, and no legacy namespace spellings outside explicit migration
  prose and negative tests for the deleted APIs.
