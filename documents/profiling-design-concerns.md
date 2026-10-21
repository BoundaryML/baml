# Profiling design concerns

Open questions to revisit, not approved implementation changes.

- [ ] **Function ID assignment separate from function entry.** Why emit a
  separate marker when the initial boundary ID is assigned at entry? Consider
  including it in `FunctionEnter`. Verify whether later reassignment is still
  needed: the legacy `baml.id.set()` API currently permits it.
- [ ] **128-bit boundary IDs: overkill?** Establish the actual uniqueness scope
  and generation volume. Distinguish the public ID format from how IDs need to
  be represented inside the profiling buffer.
- [ ] **Two different thread-start markers.** `BexThreadStart` and
  `BexThreadStartSpawned` both record a thread starting; the latter adds the
  spawn source location. Consider one `BexThreadStart` with an optional spawn site.
  Check whether preserving the two wire encodings is still necessary.
