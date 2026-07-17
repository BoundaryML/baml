# BAML C# bridge

`baml-bridge` is the `net10.0` managed host for BAML's native C ABI. Generated
C# source embeds one compiled BAML program and exposes its API under the
`BamlSdk` namespace. The package is not published from this branch yet.

The current implementation supports:

- sync and async free functions, static methods, and instance methods;
- `CancellationToken` on async calls;
- primitives, literals, lists, maps, `object?`, classes, enums, structural
  unions, flattened non-recursive aliases, nominal recursive alias wrappers,
  generic functions/classes/methods, and explicit generic bindings;
- `BamlOptional<T>` for defaulted arguments and `BamlNullable<T>` for nullable
  unconstrained type parameters;
- image, audio, video, PDF, and opaque native handles with deterministic
  clone/dispose ownership;
- generated streaming companions returning `BamlStream<TPartial, TFinal>`,
  with sync/async pulls, async enumeration, final-value access, cancellation,
  and deterministic disposal;
- generated prompt-rendering companions returning owned `BamlPromptAst`
  resources, with sync/async text and structured message access;
- generated build-request companions returning immutable `BamlHttpRequest`
  values with method, URL, headers, and body inspection;
- generated parse-stream companions accepting owned `BamlSseStream` resources,
  with sync/async event pulls, close, clone, and deterministic disposal;
- typed LLM client overrides through `BamlClient`, including named and
  shorthand primitive clients plus recursive fallback/round-robin structure;
- cloneable/disposable `BamlFile`, `BamlHttpResponse`, and `BamlGlob`
  resources with sync/async standard-library accessors and preserved native
  state, plus immutable `BamlGlobScanOptions`;
- cloneable/disposable `BamlCancelToken` values with shared one-shot runtime
  cancellation state, distinct from per-call `CancellationToken` controls;
- cloneable/disposable `BamlTaskGroup` values with sync/async limit, name,
  activity, queue, mutation, and cancellation accessors;
- cloneable/disposable `BamlCsvReader`, `BamlCsvWriter`, and `BamlCsvRecord`
  resources with typed generic row/cell decoding, iterator completion,
  position/count access, close/flush operations, and immutable reader/writer
  option values;
- required-argument host callables projected as `Func`/`Action`, plus generated
  delegate contracts for optional-parameter callbacks, including generic
  callback signatures; both have
  `ValueTask` overloads, managed exception identity round trips, and native
  release-driven registry ownership;
- BAML errors, panics, engine cancellation, traces, process exit, and
  fail-closed value validation.

`BamlBridge.FlushEvents()` forwards to the versioned native API. Hard BAML
process exits flush first and then call `Environment.Exit` with the exact BAML
exit code.

Unlisted resource types have no typed v1 API; `$rust_type` values that cross a
supported signature use opaque cloneable/disposable `BamlHandle`. Non-recursive BAML
aliases use their underlying CLR type because C# cannot export an
assembly-level alias. Recursive aliases use generated nominal wrappers around
their underlying value; erased outbound values must match exactly one
structural arm. Trimming and NativeAOT are explicit v1 non-goals; the current
nominal, alias, union, stream, and callback decoders use reflection.

## Source-tree build

```bash
cd baml_language
cargo build --locked --release -p bridge_cffi

cd sdks/csharp/bridge_csharp
dotnet test --solution Baml.Bridge.slnx --configuration Release
```

At runtime, normal NuGet RID probing is attempted first. Development builds can
set `BAML_RUNTIME_PATH` to an absolute `bridge_cffi` path.

The package/tool versions are pinned in `Directory.Packages.props`. Protobuf
sources are generated internally beneath `obj/` from the canonical
`bridge_ctypes` schemas and are not committed.

## Generated API

Generated source references the runtime package and initializes its embedded
bytecode on first use. A process may register one distinct BAML program; a
second fingerprint throws `BamlProgramConflictException`. Generated source and
the runtime package must have the same release version; a mismatch throws
`BamlSdkVersionMismatchException` before native initialization and names both
versions.

The generated files compile directly into the consuming assembly under the
fixed `BamlSdk` namespace root. They do not create another project, assembly,
resource target, or official BAML package. Reference the exact runtime version
matching the BAML CLI that generated them:

```xml
<PackageReference Include="baml-bridge" Version="0.15.0" />
```

Exactly one generated `BamlGeneratedProgram.g.cs` carries bytecode as split
base64 constants. Generation rejects raw bytecode above 8 MiB. The runtime
decodes the segments directly into one preallocated byte array, verifies their
SHA-256 fingerprint, and reports missing, malformed, oversized, or corrupt
carriers as `BamlBridgeException` before native initialization. This
source-in-project carrier requires no generated `.csproj` or resource target;
non-trimmed single-file and package-reference consumers are covered by the
repository probes. Trimming and NativeAOT are not supported in v1.

`baml generate` writes C# source directly beneath the configured `baml_sdk/`
directory and records every owned relative path and SHA-256 digest in
`.baml-generated-files.json`. A repeated generation stages the complete next
output inside that directory, verifies every previously owned file before it
is overwritten or deleted, backs up affected files, and installs the manifest
last. Unrelated files are never added to the manifest and are preserved.

Do not edit generated files. If an owned file differs from its recorded hash,
generation fails before changing any output. A missing or invalid manifest,
unsafe or case-insensitively ambiguous path, symbolic-link ancestor, or two
generator blocks resolving to the same output directory also fails closed.
Pre-manifest C# files are adopted only when they begin with BAML's generated
banner. Ordinary returned I/O failures roll back the file transaction. A
concurrent or hard-killed run leaves `.baml-generation-lock` and possibly a
`.baml-generation-*` staging directory; the next run refuses to proceed until
an operator confirms no generator is active and inspects that state. The CLI
does not guess whether interrupted output should be committed or restored.

```csharp
using Baml;
using BamlSdk;

var sync = Functions.Echo("hello");

using var cancellation = new CancellationTokenSource(TimeSpan.FromSeconds(5));
var asyncValue = await Functions.EchoAsync("hello", cancellation.Token);

var model = new ProbeModel
{
    Name = "example",
    Count = null,
    Label = ProbeLabel.Good,
    Tags = new(),
    Scores = new(),
};
```

Defaulted arguments distinguish omission from an explicitly supplied value.
Nullable unconstrained generics use a separate value wrapper so closing `T`
over `long` does not erase BAML nullability.

```csharp
Functions.AddWithDefault(10);                  // BAML evaluates its default
Functions.AddWithDefault(10, amount: 7);       // supplied value

Functions.GenericDefaultedOptional<long>();
Functions.GenericDefaultedOptional<long>(BamlNullable.Null<long>());
Functions.GenericDefaultedOptional<long>(BamlNullable.FromValue(42L));
```

LLM companions accept `BamlOptional<BamlClient>`. Omit it to use the declared
default, or pass a typed override. `FromShorthand` is the concise form for a
provider/model shorthand; the full constructor also represents named,
fallback, and round-robin clients with an optional retry policy.

```csharp
var client = BamlClient.FromShorthand("openai/gpt-4o-mini");
var result = await Functions.ExtractAsync(input, client, cancellation.Token);
```

Prompt-rendering companions return an owned `BamlPromptAst`. Clone it when two
owners need independent lifetimes, and dispose every copy. `Text`/`TextAsync`
returns the readable prompt rendering; `Messages`/`MessagesAsync` returns its
role/content message sequence. `ToString()` also renders text and therefore
requires a live prompt handle.

```csharp
using var prompt = await Functions.ExtractRenderPromptAsync(input, cancellation.Token);
Console.WriteLine(await prompt.TextAsync(cancellation.Token));

foreach (var message in await prompt.MessagesAsync(cancellation.Token))
{
    Console.WriteLine($"[{message.Role}] {message.Content}");
}

using var retained = prompt.Clone();
```

Build-request companions do not send a network request. They return an
immutable `BamlHttpRequest` for logging, testing, or inspection immediately
before provider I/O.

```csharp
var request = await Functions.ExtractBuildRequestAsync(input, cancellation.Token);
Console.WriteLine($"{request.Method} {request.Url}");
Console.WriteLine(request.Body);
```

Use implicit union conversions when arm types are distinct. Use `FromTn` when
arms overlap, and handle results with `Match` or `Switch`. A default union has
no active case and throws when read.

```csharp
BamlUnion<long, string> value = 42L;
var text = value.Match(
    number => $"number:{number}",
    message => $"text:{message}");

var overlapping = BamlUnion<string, string>.FromT1("second arm");
```

Generated streaming companions return an owned
`BamlStream<TPartial, TFinal>`. Pull explicitly with `NextAsync`, or enumerate
partials once with `await foreach`; after normal completion, `FinalAsync`
returns the validated final value. Dispose the stream with `await using`.

```csharp
await using var stream = await Functions.ExtractStreamAsync(input, cancellation.Token);
await foreach (var partial in stream.WithCancellation(cancellation.Token))
{
    Console.WriteLine(partial);
}

var final = await stream.FinalAsync(cancellation.Token);
```

`Next`/`NextAsync` returns `BamlUnion<TPartial, BamlStreamFinished>` so a
nullable partial is distinct from completion. Pulls on one stream are
serialized. A stream permits one async enumerator; disposing that enumerator
before completion disposes its stream, while normal enumeration completion
leaves the stream available for `Final`/`FinalAsync`. Repeated final calls are
allowed. A pre-canceled pull does not consume a partial.

Host callables require the generated outer async method. Required callback
parameters project to normal `Func`/`Action` delegates; a second overload uses
`ValueTask<T>` (or `ValueTask` for void callbacks). The bridge runs callbacks
off the native dispatch thread, restores the `ExecutionContext` captured when
the argument was encoded, and does not block native progress while awaiting an
asynchronous callback.

Optional callback parameters use a generated delegate whose parameter type is
`BamlOptional<T>`. `IsSet == false` means BAML omitted the named argument;
`IsSet == true` preserves a supplied value, including explicit null when `T`
permits it. Delegate parameter metadata retains the original BAML wire name, so
partial named calls do not depend on C# lambda parameter names. When the
callback signature references a BAML type parameter, the generated delegate is
generic and closes over the same CLR type argument as its containing function
or method.

```csharp
Func<long, string> syncCallback = value => $"value:{value}";
var syncResult = await Functions.CallWithCallbackAsync(syncCallback, 42);

Func<long, ValueTask<string>> asyncCallback = async value =>
{
    await Task.Yield();
    return $"value:{value}";
};
var asyncResult = await Functions.CallWithCallbackAsync(asyncCallback, 42);
```

An arbitrary managed exception propagating back out of BAML is rethrown as the
same exception object. Throwing `BamlError` with a generated BAML value instead
preserves that value's BAML class identity for typed BAML `catch` arms. Native
release callbacks remove registry roots; cancellation may finish the managed
call before a callback returns, and a late callback completion is benign.

Caller-token cancellation completes the async operation as a token-associated
canceled `Task`. Engine-originated `baml.panics.Cancelled` values instead throw
`BamlCancelledException`, which derives from `OperationCanceledException` and
retains the decoded `Value`, `ClassName`, and `BamlTrace`.

Call-boundary `baml.errors.TypeMismatch` values throw
`BamlTypeMismatchException`, an `ArgumentException` subtype whose message,
decoded value, class name, and BAML trace remain available. Other error values
continue to throw `BamlError`.

Media, file, HTTP response, glob, prompt, BAML stream, SSE stream, CSV, and
opaque handle wrappers own native references. Dispose every returned value;
clones remain valid after the original is disposed. `BamlFile.Close`,
`BamlSseStream.Close`, and CSV `Close` methods close shared underlying state,
while `Dispose` releases only that wrapper's native reference.

```csharp
using var image = BamlImage.FromUrl("https://example.com/image.png", "image/png");
using var returned = Functions.RoundTripImage(image);
Console.WriteLine(returned.Url);
```

`BamlGlob` exposes sync/async `Matches` and `Scan` methods. Scan with a root
string for defaults or use `BamlGlobScanOptions` for dotfiles, absolute paths,
symlink policy, and directory inclusion.

`BamlCancelToken` is a BAML runtime resource returned by generated functions.
Its clones share state; `Cancel` returns `1` for the first transition and `0`
afterward, while `IsCancelled` observes the shared state. It does not replace
the final .NET `CancellationToken` parameter on generated async calls.

`BamlTaskGroup` is likewise a shared runtime resource. Its clones observe the
same concurrency limit and counts; `SetLimit` mutates that shared state and
`Cancel` selects pending and/or active group members.

`BamlCsvWriter` writes raw records or typed rows and exposes its in-memory text,
record count, headers, flush, and close operations. `BamlCsvReader.Next` returns
`BamlUnion<BamlCsvRecord, BamlIteratorDone>`; records expose typed `Get<T>`,
`GetAt<T>`, and `Decode<T>` operations plus raw fields, maps, and positions.
Reader and writer options are immutable managed values that preserve the exact
standard-library configuration when crossing the boundary.

CSV skip diagnostics currently decode as `List<object?>`, rather than a typed
`CsvError` model. A managed `BamlCsvReaderOptions.OnSkip` delegate can be sent
to BAML, but a non-null callback returned inside an options value cannot be
rehydrated from its opaque native handle and is rejected.

`BamlHttpResponse` exposes status, headers, URL, `Ok`, and sync/async body
access. `BamlFile` exposes sync/async read, write, seek, text, bytes, and close
operations; calls on a disposed wrapper throw `ObjectDisposedException` before
native dispatch.

The release package contains all eight supported native RIDs. The frozen
release plan stamps the NuGet version, generated-code marker, and native SDK
handshake from one canonical version. `tools/Baml.NuGet.Normalize` rewrites both
the unsigned package and symbol package into deterministic OPC/ZIP form before
signing or publishing.

The release assembler expects this input tree and rejects missing, extra, or
misplaced native files:

```text
native/
  runtimes/linux-x64/native/libbridge_cffi.so
  runtimes/linux-arm64/native/libbridge_cffi.so
  runtimes/linux-musl-x64/native/libbridge_cffi.so
  runtimes/linux-musl-arm64/native/libbridge_cffi.so
  runtimes/osx-x64/native/libbridge_cffi.dylib
  runtimes/osx-arm64/native/libbridge_cffi.dylib
  runtimes/win-x64/native/bridge_cffi.dll
  runtimes/win-arm64/native/bridge_cffi.dll
```

```bash
tools/pack-all-native.sh native artifacts
```

The package's bounded `buildTransitive` target fails early when an explicit
`RuntimeIdentifier` or `RuntimeIdentifiers` value is outside that set. Runtime
asset selection and copying remain standard NuGet/.NET behavior. At runtime,
distro-specific identifiers such as `ubuntu.26.04-x64` normalize to the
portable packaged RID; unsupported OS, architecture, or Android/Bionic hosts
fail with `PlatformNotSupportedException` and the supported-RID list.

The single-RID packaging probe can be reproduced with either wrapper below.
Each command validates the platform filename and writes only the normalized
unsigned package to the output directory.

```bash
tools/pack-native.sh /path/to/libbridge_cffi.so linux-x64 artifacts
```

```powershell
tools/pack-native.ps1 -NativeLibrary C:\path\to\bridge_cffi.dll `
  -Rid win-x64 -OutputDirectory artifacts
```

These wrappers remain useful for focused RID probes; `pack-all-native.sh` is
the release assembler.

`BamlUnion.cs` is mechanically generated for arities 2 through 32. Regenerate
it after changing the shared union shape with:

```bash
dotnet run --project tools/Baml.Union.Generate -- \
  src/Baml.Bridge/BamlUnion.cs
```

The v1 union binary layout uses one typed field per arm plus the case tag. Its
arity 2/8/16/32 size, copy, and allocation comparison is maintained as:

```bash
dotnet run --project tools/Baml.Union.LayoutProbe --configuration Release
```
