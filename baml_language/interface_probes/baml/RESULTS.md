# Probe results

## Baseline

- Repository: `/Users/aaron/projects/baml/baml_language`
- Git commit: `1c13423479966ba706aaf981bc317a9ef04bf6f4`
- Branch: `aaron/journal-user-content`
- Source-built CLI: `baml-cli 0.18.0`, rebuilt from this checkout with:

  ```sh
  cargo run -p baml_cli --bin baml-cli -- --version
  ```

- Installed comparison toolchain: wrapper `0.2.4`, toolchain
  `0.18.1-nightly.20260901.a`.

Both CLIs produced the same compile and runtime results. The authoritative
results below are from the CLI rebuilt from the checked-out source.

## Commands

From the workspace root:

```sh
interface_probes/baml/run_probes.sh
```

The script expands to `check`, `run`, and `test` for each positive project and
`check` for each negative project. It can be run against the installed nightly
with:

```sh
BAML_CLI=/opt/homebrew/bin/baml interface_probes/baml/run_probes.sh
```

## Empty interface membership remains explicit

The positive runtime observation was:

```json
{"explicit_reflects":true,"same_shape_reflects":false,"explicit_matches":"marker","same_shape_matches":"other"}
```

Its test passed. The member and nonmember classes have the same empty field
shape; only `ExplicitMember` has an `implements Marker {}` declaration.

The negative project failed compilation as expected:

```text
E0001 mismatched types
expected `Marker`, found `SameShapeWithoutImpl`
```

This agrees with `TYPE_SYSTEM.md`: interface membership is a subtyping fact only
for a concrete type that implements the interface. An empty `media` interface
therefore remains nominal and opt-in. Its lack of members does not make every
value a member and does not justify duck typing in an SDK.

## An open interface match still needs a wildcard

The positive match, which lists both implementors declared by the project and
then a wildcard, returned:

```json
{"a":"a","b":"b"}
```

Its test passed. Removing only the wildcard failed compilation:

```text
E0062 non-exhaustive match on type OpenMarker; missing: OpenMarker {}
```

The implementation does not close the existential merely because it can see
all current `implements` declarations. This agrees with the open-world model in
the design and with `TYPE_SYSTEM.md`'s subset-based exhaustiveness rule. A
`media` match that lists `image`, `audio`, `video`, and `pdf` must retain `_`;
only a separate concrete union can be exhaustively enumerated.

## Classes alias inside BAML; a copied outer can retain live children

The identity/result probe returned:

```json
{"after_local_alias_write":2,"after_return_alias_write":3,"holder_second_after_first_write":4,"result_field_after_extracted_write":6,"original_payload_after_result_alias_write":7,"detached_old_payload_after_field_replacement":7,"replacement_payload_value":8,"original_journal_after_outer_copy_append":[1,2],"original_journal_after_copy_field_replacement":[1,2],"replacement_journal":[99,100],"callback_first_copy_call":1,"callback_second_copy_call":2}
```

This proves, inside the BAML runtime:

- Assigning or returning a class value preserves its identity. Writes through
  one alias are visible through the others.
- Reading a class-valued field returns the same nested object, not a deep copy.
- Replacing a field changes only that outer object's slot; prior aliases retain
  the former child.
- Constructing a new outer result record can deliberately retain the same
  nested journal. Appending through the new outer is observed through the
  original; replacing the new outer's journal field does not replace the
  original's field.
- Two separately constructed callback envelopes can carry the same closure and
  observe one captured mutable state (`1`, then `2`).

There is a real boundary difference in the current implementation. Statically
compiled ordinary class instances are recursively exported as structural
`BexExternalValue::Instance` values in
`crates/bex_engine/src/conversion.rs:687-817`; only runtime-created classes and
two trusted stdlib capability kinds (`FunctionSpec`, `Stream`) take a rooted
handle path today (`conversion.rs:117-142`, `702-745`). Thus current SDK DTO
crossings copy ordinary class envelopes and do not preserve all in-runtime
class aliases. This is the observed implementation/model mismatch relevant to
the design. It is not a marker or match mismatch.

## RunResult projection conclusion

No host language forces `RunResult<Out>` itself to be a transitive live facade.
The BAML declaration is a methodless three-field class
(`ai/runner.baml:15-19`). A generated, by-value outer envelope can expose normal
fields synchronously while recursively retaining capability children:

```text
RunResult<Out> {
  value: projected according to Out,
  journal: JournalRef,
  usage: portable Usage,
}
```

Such an envelope is session-local when a child is live. It must not claim to
preserve the outer BAML object's identity, field replacement, or arbitrary
round-trip mutation. This is already compatible with the design's mixed-value
rule. The nested `JournalRef` preserves the journal identity that runners and
clients need, and a live/interface/resource-valued `Out` remains a checked ref.

Async `get_value`/`get_journal`/`get_usage` is required only if the outer
`RunResult` is itself kept as a rooted BAML object whose fields are read across
the runtime boundary. It is unnecessary for a copied outer envelope. The
copied-envelope projection is simpler and consistent with existing ordinary
DTO behavior, provided the API states its by-value outer semantics.

The same reasoning applies to methodless `ModelTurnInput`: it can be a copied
mixed envelope with a callable prompt and live Journal/Toolbox/type children.
Its outer field replacement would be local. `Journal` and `Toolbox` themselves
have authored methods and mutable state, so keeping those children live avoids
silently reimplementing their behavior or losing shared state.

If the product instead promises that every generated BAML class preserves
language-level identity across SDK calls, then both `RunResult` and all ordinary
DTO classes need live references; making only transitive capability aggregates
live would be an inconsistent halfway rule. The current design and SDKs already
distinguish portable/mixed records from live capabilities, so that stronger
global promise is not presently required.

## Interface field writes and nested mutable aliases

The field mutability probe uses four explicit links from interface names to
distinct backing names (`count as stored_count`, and corresponding list, map,
and child links). Its runtime observation was:

```json
{"concrete_scalar_seen_through_interface":2,"interface_scalar_seen_through_concrete":3,"default_after_alias_mutations":{"count":3,"first_item":11,"item_count":2,"map_a":11,"map_b":2,"child_value":12},"concrete_list_first_after_alias_mutation":11,"concrete_list_count_after_alias_mutation":2,"concrete_map_a_after_alias_mutation":11,"concrete_map_b_after_alias_mutation":2,"concrete_child_after_alias_mutation":12,"default_after_root_replacement":{"count":3,"first_item":99,"item_count":1,"map_a":9,"map_b":null,"child_value":8},"concrete_list_first_after_old_alias_mutation":99,"concrete_list_count_after_old_alias_mutation":1,"concrete_map_a_after_old_alias_mutation":9,"concrete_map_b_after_old_alias_mutation":null,"concrete_child_after_old_alias_mutation":8,"detached_list_first":111,"detached_list_count":3,"detached_map_a":111,"detached_map_b":22,"detached_child_value":120}
```

Its test passed. This establishes the current source-built runtime behavior:

- A concrete scalar write is immediately visible through the interface field,
  and a scalar assignment through the interface changes the concrete class's
  linked backing field.
- Reading an array, map, or class-valued interface field and assigning it to a
  local preserves identity. Element insertion/replacement and nested class
  field writes through the local alias are visible from the original owner.
- The interface default method reads those current linked values, including
  mutations performed through aliases.
- Assigning a replacement array, map, or child to the interface field updates
  the owner's root slot. Previously read aliases retain the old objects. Later
  mutations to those aliases do not affect the replacement roots, while the
  aliases themselves remain mutable.

This behavior follows the implementation rather than an accidental rewrite by
the probe. The emitter has distinct `VirtualLoadField` and
`VirtualStoreField` instructions (`crates/baml_compiler2_emit/src/emit.rs`),
and the VM resolves the interface field index through the concrete
implementation's baked `field_links` before directly loading or storing the
physical instance slot (`crates/bex_vm/src/vm.rs:4960-4990,8158-8226`). A load
pushes the stored `Value`; it does not call a root-field setter after later
operations on a loaded container or child.

### Foreign `HostAccessor` implication

A proposed `HostAccessor(get, set)` cannot preserve these semantics if `get`
decodes a native mutable array, map, or class into a detached BAML copy and
`set` runs only for direct root assignment. For example, `let xs = view.items;
xs.push(20)` has no later virtual root store for an accessor to intercept, but
current BAML requires the owner to observe the push. The same applies to map
element writes and nested child field writes.

An implementation that promises BAML field semantics must therefore have an
identity-bearing result from `get`: a rooted VM value, or a live foreign
container/class proxy whose element and nested operations route to the host.
A transactional read-modify-write wrapper is possible only if every operation
on the loaded value is mediated and aliases share one wrapper; plain
decode-copy plus root setter is insufficient. Direct replacement still has to
detach earlier aliases as the probe demonstrates. Alternatively, the bridge
must reject mutable aggregate types for host-backed interface fields and permit
only immutable/copy-safe getter results; this is a real restriction and should
be explicit in validation and generated APIs.
