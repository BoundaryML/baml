# Query stack on the CAS v2 parent

What changed for `baml query` and the playground when this branch was merged
onto parent `bf96803` (upstream PR #4958). The parent removed type attributes
(BEP-075), and that changed how captured values are stored.

## Captured values: CAS format 2

The parent writes captured values as CAS blob format 2, with snapshot IDs
from hash domain v2. Blobs live in `.baml/btel/cas/v2`. Embedded type
descriptions no longer carry attribute bytes. The version and hash-domain
change also changes IDs for values without type descriptions, such as
integers. This merge retains our BTEL recording format 2.2; a recording does
not say which CAS version its snapshot IDs belong to.

The reader follows the parent:

- The decoder reads format 2 only. Any other version is rejected as
  `BlobError::Version`, which a query reports as `cas_unsupported_version`.
  It never reads a format 1 blob as if it were format 2.
- The reader finds blobs through the same path function the writer uses
  (`btel_file::cas_path`), so it only looks in `cas/v2`.
- A recording made before the upgrade names v1 snapshot IDs. Its captured
  values show as `cas_missing`, even though the old blobs are still in
  `cas/v1`. Nothing migrates or reads v1 blobs. Call, timing, source-site and
  error-raise metadata still indexes. Captured error values, like arguments
  and outputs, are unavailable without the supported CAS blobs.

A nested type now costs one byte per level instead of about four. A
`list<...>` level used to carry three attribute bytes. So the 128-byte bound
for decoding a type in place now allows about 127 levels, not about 31.
Measured after the change: at most 4 KiB of stack per level in debug builds
and about 1 KiB in release. That is roughly half a MiB for 127 levels. A test
decodes that deepest in-place type on a 2 MiB stack. Deeper types are still only measured,
on the 64 MiB helper thread.

The cloud contract fixtures were regenerated for format 2 together with this
branch's header 2.2 and outcome counts. The fixture README says how.

## A captured catch binding's `throw e` starts a new error

```baml
Thrower(n) catch (e) {
    Failure => {
        let f = () -> int { e.code };
        throw e
    }
}
```

The parent's compiler reads a captured binding through its cell
(`Place::Deref`). But `operand_is_marked_rethrow` only recognizes a plain `Place::Local`, so the
compiler emits `Throw`. The VM then treats it as a new failure: a new trace,
with the handled error as its cause. The recording follows the VM. The raise
is a `throw`, `fresh`, and starts a second occurrence. No origin is wrongly
proven. Without the closure, `throw e` still records `rethrow` and `proven`.

This is the parent's behavior, not the merge's. Our compiler changes against
`bf96803` only fix source spans, and a probe on real recordings showed both
cases above. The parent itself was not built. The follow-up belongs in the
compiler: preserve rethrow classification when lowering captured catch
bindings. This merge does not change it.

Landing notes still hold. The unwinder writes the caught error into the
handler's plain error slot. A captured binding is a separate cell copied from
that slot, as it was before the parent. Assigning to it writes the cell, not
the error slot. And the parent added no opcodes.

## Test notes

In one parallel run of 956 tests, two integration tests failed once. The
await-origin test failed with `telemetry chunk transport failed`, and the
cloud capture-pressure test hit its 10-second timeout. They took 19 and 29
seconds there, and about 2 and 3 seconds alone. Both passed in isolation and
in three serial runs with the same package selection. 48 direct runs of the
await test, 24 at a time, reproduced the transport failure once. The root cause
is not established. The transport code is unchanged by this merge apart from
upstream's type changes.
