# BAML interface semantic probes

These projects isolate language questions used by the interface/media bridge
design:

- `marker_membership`: positive empty-interface membership and type matching.
- `marker_implicit_assignment_rejected`: negative structural/duck-typing case.
- `open_interface_match`: open match with the required wildcard.
- `open_interface_match_without_fallback`: negative exhaustive-match case after
  listing every implementor declared in the project.
- `class_identity_and_result_fields`: class aliasing, returned-field mutation,
  and a copied outer result envelope that intentionally preserves nested
  journal/callback identity.
- `interface_field_mutability`: field-link read/write behavior plus list, map,
  and child-class aliasing through an interface existential. This informs deferred
  field interoperability; it is not part of the current PR implementation scope.
- `stateful_methods`: method-only Counter interface, BAML-owned state updates,
  interface alias/pass-back identity, and detached scalar return values. Run its
  focused commands from `INTERFACE_PROBE_RESULTS.md`; the existing aggregate
  runner is unchanged.

Run all probes from the `baml_language` workspace root:

```sh
interface_probes/baml/run_probes.sh
```

The script uses `target/debug/baml-cli` by default. To compare an installed
toolchain:

```sh
BAML_CLI=/opt/homebrew/bin/baml interface_probes/baml/run_probes.sh
```

The two negative projects are expected to fail `baml check`; the runner treats
those failures as success. See `RESULTS.md` for the captured baseline and its
design implications.
