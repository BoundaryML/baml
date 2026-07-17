# C# managed failure and cancellation evidence

Status: B7 passed locally on 2026-07-17. The fixture now freezes only the
diagnostic information representable by the current
`BamlOutboundError`/`BamlOutboundPanic` envelopes. Product protocol decoding,
native callback/cancellation integration, and shared parity remain
implementation gates.

## Target and source

- Branch/start commit: `paulo/csharp-bridge` /
  `1ebf901f7896faaec4672fdc4b2f2835db2f1cc0`.
- Host: Linux 7.0.0-27-generic x64; .NET SDK `10.0.110`, runtime `10.0.10`;
  C# 14 / `net10.0`.
- Fixture:
  `baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.FailureCancellationProbe`.
- The executable launches its own apphost as the hard-exit child. No
  `Environment.Exit` path runs in the parent/test process.

Source SHA-256:

| Source | SHA-256 |
| --- | --- |
| `Baml.Bridge.FailureCancellationProbe.csproj` | `083bf25d9e1fcad0bff524b36ba3f2920a31ce2a79b85ce8707834c39db3a67e` |
| `ProbeExceptions.cs` | `69a3598367536a86175dd4a3a7a06f8058740f9470bc7637e5cea7e4776b1974` |
| `Program.cs` | `538e74b55486c73403db6eeed7e86822c7c2cd506589e382199d426d8a9c752b` |
| `README.md` | `0f09f719c51144749c66ce3b264b06bc5cd2603f2002760db1ebd1ac6f57a7b5` |
| `baml_outbound.proto` (unchanged wire input) | `ee2a6765c918bfd57a6b910831edb1d90fc55e3b7b4d0c10b453618b03eb1ac8` |

## Command and output

```text
dotnet build \
  baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.FailureCancellationProbe/Baml.Bridge.FailureCancellationProbe.csproj \
  --configuration Release --nologo

dotnet run \
  --project baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.FailureCancellationProbe/Baml.Bridge.FailureCancellationProbe.csproj \
  --configuration Release --no-build
```

The Release build completed with zero warnings/errors. Exact parent output:

```text
exception_hierarchy=complete
wire_diagnostics=exact_immutable_and_redacted
cancellation_origins=3/3
custom_canceled_task=subtype_token_status_preserved
sync_direct_rethrow=no_aggregate
callback_exception_identity=object_and_stack_preserved
unrelated_token_callback=faulted_exact_exception
matching_token_callback=canceled
terminal_race=single_winner_exact_release
hard_exit_child=bounded_exit_no_finally
```

## Frozen wire-representable public contract

The current outbound error envelope contains one decoded value and ordered
pre-rendered trace strings. The current panic envelope contains the same two
fields plus `is_exit_panic` and `exit_code`. It does not contain structured
trace-frame fields, type-mismatch expected/actual/path descriptors, or panic
category/reason/location fields. The fixture therefore proves the following
exact v1 surface:

- `BamlExecutionException` exposes `string? BamlFunction` and non-null
  `BamlTrace Trace`. The function is populated only from the managed call
  context and remains null when that context is unavailable.
- `BamlErrorException` exposes non-null `BamlValue ThrownValue` and
  `string? ErrorName`. The error identity is taken only from the decoded
  value's nominal identity and remains null when the value has none.
- `BamlTypeMismatchException` is a sealed `BamlErrorException` and adds no
  `Expected`, `Actual`, or `Path` properties because those values are absent
  from the envelope. It still preserves the decoded thrown value, call
  context when available, and rendered trace.
- `BamlPanicException` exposes non-null `BamlPanicInfo Panic`.
  `BamlPanicInfo` exposes exactly non-null `BamlValue Value`,
  `bool IsExitPanic`, and `long? ExitCode`; it has no category, reason, or
  location. A catchable non-exit panic has `IsExitPanic == false` and
  `ExitCode == null`, ignoring the proto3 integer carrier. An exit panic has
  both discriminator and code and is dispatched to process exit instead of
  constructing a catchable exception.
- `BamlTrace` is a sealed immutable snapshot exposing exactly
  `IReadOnlyList<string> Lines`. It preserves wire order and exact rendered
  text with ordinal structural equality/hashing. There is no public
  `BamlTraceFrame`.
- `BamlTypeMappingException` remains a managed-origin failure and exposes
  exactly non-null `Type ClrType`, non-null `string Position`, non-null
  `string Path`, and nullable `string? CanonicalReplacement`.

The reflection audit also pins abstract category bases, sealing of concrete
leaves (except the intentionally extensible `BamlErrorException` base),
getter-only property types, internal invariant-preserving constructors, and
the absence of public exception constructors. `BamlValue`, `BamlTrace`, and
`BamlPanicInfo` are sealed with no public constructors. The scoped enum is
frozen as `BamlCancellationOrigin : int` with `Caller = 0`, `Engine = 1`, and
`StreamDisposed = 2`.

## Behavioral proof

- A decoded error preserves the exact `BamlValue` object and snapshots the
  ordered rendered trace lines. Mutating the simulated wire list after decode
  cannot change the public trace. Safe default exception formatting omits a
  deliberately retained sensitive diagnostic containing an authorization
  value, prompt/body text, and signed URL.
- Mapping an internally canceled task to a thrown
  `BamlOperationCanceledException` yields `TaskStatus.Canceled`, a null
  `Task.Exception`, and the exact same custom exception object, associated
  canceled token, origin, function, and trace at `await`.
- Caller, engine, and stream-disposal origins each use their own already
  canceled token. `GetAwaiter().GetResult()` rethrows the same custom subtype
  and object directly without `AggregateException`.
- A callback boundary captures `ExceptionDispatchInfo`, simulates an
  asynchronous registry/native round trip, restores the exact application
  exception object, and retains the original callback-source stack frame.
  Reusing the consumed identity produces the designed
  `BamlHostCallbackException` fallback.
- An `OperationCanceledException` carrying an unrelated/uncanceled callback
  token is installed into a `TaskCompletionSource` with `TrySetException`.
  The public task remains `Faulted` before and after `await`, while `await`
  rethrows the exact original exception and token.
- A matching already-canceled linked callback token is classified as a
  cancellation acknowledgment and maps to the outer custom canceled task.
- Sixty-four simultaneous result/error/cancellation signals produce exactly
  one atomic terminal winner, one registry removal, and one release for every
  owned signal payload. A late signal cannot replace the winner and releases
  its payload once.
- The child decodes an exit panic envelope, emits a bounded pre-exit signal,
  calls `Environment.Exit(37)`, returns exit code 37 within ten seconds, and
  does not execute its `finally` block. No `BamlPanicException` is surfaced
  for that envelope.

## Remaining implementation proof

The fixture freezes CLR behavior and a wire-compatible public taxonomy; it is
not a substitute for the final bridge. Product tests must decode actual
`BamlOutboundError` and `BamlOutboundPanic` messages, use the shared dynamic
value decoder, propagate real managed call context when available, integrate
native callback identities/tokens, race real call-registry completions, and
port the matching shared parity cases. Those items gate `supported` rows but
do not reopen the wire-representable managed contract proved here.
