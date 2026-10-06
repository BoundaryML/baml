# Recorded type definitions: A/B implementation spec

Two implementations of the same feature are built on top of this base, one per design, and compared on the same tests and benchmarks. This document is the shared contract. It is temporary: the winning branch rewrites it into the feature's documentation before merging.

## Goal

From one recorded span, `baml query` must give everything needed to write a BAML test that reproduces the call: the arguments, the output, and the full definition of every class and enum involved, including classes built at run time. Today a captured value names a class (`{"type": "class", "name": "Person"}`) but not what it contains, and two runtime classes with the same name cannot be told apart.

In scope: recording definitions, storing them, and rendering them in local `baml query`. Out of scope until A or B is chosen: BAML Cloud (bcs), the `--repro` test generator, LLM call replay.

## What a definition holds

Every class and enum a captured value names, declared in source or built at run time:

- Class: name, number of generic parameters, description, alias, docstring, other attributes, `@@stream.done`, and its fields in order. A field: name, type (generic parameters by position), description, alias, docstring, other attributes, `skip`, `@stream.done`, `@stream.must_exist`.
- Enum: name, description, alias, docstring, other attributes, and its variants in order. A variant: name, description, alias, docstring, other attributes, `skip`.

Source: the runtime `Class` and `Enum` objects (`bex_vm_types/src/types/class.rs`, `enums.rs`). A field's type comes from `field_template`, whose `TypeArgRef(N)` is generic parameter `N`.

## When definitions are recorded

Exactly when the capture that names the class is recorded:

- a generic call's `type_args`, and types passed as values: when the span captures its inputs (the #5145 rule, including per-call trace overrides);
- instances and enum values in `input_args`: same rule;
- in `output_value` and `error_value`: when the output or error is captured.

A span that captures nothing does no definition work at all. Timing-only calls are untouched.

## Output contract (identical in A and B)

Rendering lives in `btel_reader` (`value.rs`, `value/ty.rs`), so local `baml query` and BAML Cloud share it. The shape below is provisional: it is reader-only and can change after the decision without re-recording anything.

**References.** Every class or enum reference carries the id of its definition:

- type nodes (in `$type`, and in a definition's field types): `"def": "<id>"` on `class`, `enum` and `enumVariant` nodes;
- instances: `"$def": "<id>"`, right after `"$class"` and before the fields;
- enum values: `"$def": "<id>"`, after `"$enum"` and `"$variant"`.

**Definitions.** Each rendered cell includes a definition once, at the first reference to it in document order, under `"definition"` (type nodes) or `"$definition"` (instances and enum values). Later references in the cell carry the id alone. Within a type node the order is `type`, `name`, `args`, `def`, `definition`, so a generic argument's definition comes before the generic class's own. Recursive references inside a definition are therefore ids alone.

```json
{"type": "class", "name": "user.Employee", "def": "d0", "definition": {
  "kind": "class", "name": "user.Employee",
  "fields": [
    {"name": "name", "schema": {"type": "string"}},
    {"name": "manager", "schema": {"type": "optional", "inner": {"type": "class", "name": "user.Employee", "def": "d0"}}}
  ]}}
```

**Definition bodies.** Keys that are absent, empty or false are omitted.

- Class: `{"kind": "class", "name", "type_params"?: <count>, "description"?, "alias"?, "docstring"?, "attributes"?: {..}, "stream_done"?: true, "fields": [..]}`
- Field: `{"name", "schema": <type node>, "description"?, "alias"?, "docstring"?, "attributes"?: {..}, "skip"?: true, "stream_done"?: true, "must_exist"?: true}`
- Enum: `{"kind": "enum", "name", "description"?, "alias"?, "docstring"?, "attributes"?: {..}, "variants": [..]}`
- Variant: `{"name", "description"?, "alias"?, "docstring"?, "attributes"?: {..}, "skip"?: true}`
- A generic parameter in a field type: `{"type": "typeParam", "index": N}`.

Names follow #5147: full names for declared types (`user.Resume`), the bare name for a runtime class.

**Identity.** Ids are opaque strings.

- Every reference to the same class within a recording has the same id, in every cell and capture.
- Different definitions have different ids, including two runtime classes with the same name.
- Equal ids imply identical definitions. Whether two distinct classes with identical definitions share an id is up to the design (B's content hashes do, A's runtime identities need not).

## The two designs

**A: the handoff's design.** Extend the function-identity mechanism to classes and enums: a stable nominal identity per declaration, registered lazily before its first captured reference; a weak identity-to-heap lookup kept valid by the collector; a resolver that copies the definition out of the heap later, off the VM thread; and a recorder that publishes each definition once per recording. Read `Dynamic type telemetry engineering handoff.md` (provided with the task) for the full design and its constraints: the drain thread must never wait for heap access (`Found`, `Missing`, `Busy`), and definitions must survive collection of their class.

**B: definitions in the CAS.** Copy a definition at capture time, on the VM thread, which already holds the heap permit while capturing. Store each definition as its own content-addressed blob and reference it by hash from the captures that name it, with #5102's child references. Cache, per type, the hash of its definition so each type is copied once per process; bound and prune the cache. Mutually recursive classes form one blob, ordered canonically so equal groups hash equally. Kai's shaping rule that every blob carries the declarations it names stays for the small inline declaration, which gains the reference to its definition blob. The blob format version goes to 4; readers keep reading 3.

Both may change any crate. Both must leave the timing-only and capture-off paths as they are.

## Shared tests

`baml_query_btel/tests/type_definitions.rs` is the executable contract. Both branches must pass it **unchanged**. On this base, three of its four tests fail (no definitions are recorded yet). The fourth, capture off, passes already and must keep passing.

Each design adds its own unit and stress tests for its risks:

- A: definitions resolved while classes are collected, a drain thread that never blocks on heap access, a full transport with a pending collection (the handoff's deadlock), and bounded pending work.
- B: canonical hashing (the same definition always hashes the same, across runs and processes; groups of mutually recursive classes), and cache bounds and pruning.

Both: the existing `cargo test` suites stay green, and `baml query`'s other columns are unchanged.

## Shared benchmark

`baml_tests/benches/telemetry_overhead.rs`, run with `cargo bench -p baml_tests --bench telemetry_overhead`. Nine scenarios, each with telemetry off and on:

| Scenario | What it measures |
|-|-|
| `timing_only` | calls with no span: the handoff's 10 ns target |
| `span_without_capture` | spans capturing nothing |
| `type_args_repeated` | one declared generic argument, captured, repeated: the steady state |
| `instance_inputs_repeated` | captured instances and enum values of declared types |
| `type_args_alternating` | four declared types in turn |
| `new_runtime_class_each_call` | a new runtime class per call: every call a first sighting (B's worst case) |
| `runtime_classes_under_collection` | runtime classes collected soon after use (A's worst case) |
| `nested_type_args_repeated` | a captured type argument naming a nested schema (classes, enums, self-reference): cost that grows with the schema |
| `nested_instance_inputs_repeated` | a captured instance of that nested schema |

Results are compared per scenario as the median with telemetry on, against the same scenario on this base, on the same machine (rtxloco). Telemetry already costs something today, so "on minus off" alone would mix the existing cost into the comparison.

## Decision criteria

In order:

1. **Correctness.** The shared tests pass unchanged, the design's own stress tests pass, and nothing else regresses.
2. **Hot paths unchanged.** `timing_only` and `span_without_capture` within noise of the base (at most 2% or 2 ns per call, whichever is larger).
3. **Steady state.** `type_args_repeated`, `instance_inputs_repeated`, `type_args_alternating`, `nested_type_args_repeated`, `nested_instance_inputs_repeated`: the smaller overhead over the base wins. More than 10% over the base needs a reason.
4. **First sightings and collection.** `new_runtime_class_each_call`, `runtime_classes_under_collection`: reported, and compared against the cost of building the class itself.
5. **Complexity.** Lines changed, new concurrency (threads, locks, heap access off the VM thread), and new failure modes. With performance within noise, the simpler design wins.

## Working rules

- A works on branch `antoniosarosi/dyn-types-a` (the Mac), B on `antoniosarosi/dyn-types-b` (rtxloco), both from this base. One draft PR each, stacked on the base PR.
- Do not change this spec, the shared tests or the benchmark. If one of them is wrong, report it to the coordinator.
- Commit and push as you go; the coordinator reviews the branches.
- Report: what was built, test results, the benchmark table (on and off, all seven scenarios), and the open problems.
