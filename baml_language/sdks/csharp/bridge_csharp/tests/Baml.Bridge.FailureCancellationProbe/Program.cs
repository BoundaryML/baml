using System.Collections.Concurrent;
using System.Diagnostics;
using System.Reflection;
using System.Runtime.ExceptionServices;
using Baml;

internal static class Program
{
    private const int HardExitCode = 37;
    private const string HardExitArgument = "--hard-exit-child";
    private const string SensitiveValue =
        "Bearer super-secret prompt-body signed-url";

    public static async Task<int> Main(string[] args)
    {
        if (args.Length == 1
            && StringComparer.Ordinal.Equals(
                args[0],
                HardExitArgument))
        {
            RunHardExitChild();
            return 99;
        }

        VerifyExceptionHierarchy();
        VerifyWireRepresentableDiagnosticsAndRedaction();
        await VerifyCancellationOriginsAsync();
        await VerifyCallbackExceptionIdentityAsync();
        await VerifyUnrelatedCallbackCancellationIsFaultedAsync();
        await VerifyMatchingCallbackCancellationIsCanceledAsync();
        await VerifyTerminalRaceAsync();
        await VerifyHardExitChildAsync();

        Console.WriteLine("exception_hierarchy=complete");
        Console.WriteLine("wire_diagnostics=exact_immutable_and_redacted");
        Console.WriteLine("cancellation_origins=3/3");
        Console.WriteLine("custom_canceled_task=subtype_token_status_preserved");
        Console.WriteLine("sync_direct_rethrow=no_aggregate");
        Console.WriteLine("callback_exception_identity=object_and_stack_preserved");
        Console.WriteLine("unrelated_token_callback=faulted_exact_exception");
        Console.WriteLine("matching_token_callback=canceled");
        Console.WriteLine("terminal_race=single_winner_exact_release");
        Console.WriteLine("hard_exit_child=bounded_exit_no_finally");
        return 0;
    }

    private static void VerifyExceptionHierarchy()
    {
        RequireAbstract<BamlException>();
        RequireAbstract<BamlExecutionException>();
        RequireAbstract<BamlInitializationException>();
        RequireAbstract<BamlInteropException>();

        RequireSealed<BamlTypeMismatchException>();
        RequireSealed<BamlPanicException>();
        RequireSealed<BamlProgramConflictException>();
        RequireSealed<BamlVersionMismatchException>();
        RequireSealed<BamlProgramIntegrityException>();
        RequireSealed<BamlNativeLibraryLoadException>();
        RequireSealed<BamlProtocolException>();
        RequireSealed<BamlHostCallbackException>();
        RequireSealed<BamlTypeMappingException>();
        RequireSealed<BamlOperationCanceledException>();
        RequireSealed<BamlValue>();
        RequireSealed<BamlTrace>();
        RequireSealed<BamlPanicInfo>();

        Require(
            typeof(BamlErrorException).BaseType
            == typeof(BamlExecutionException),
            "BamlErrorException category mismatch");
        Require(
            typeof(BamlTypeMismatchException).BaseType
            == typeof(BamlErrorException),
            "BamlTypeMismatchException category mismatch");
        Require(
            !typeof(BamlException).IsAssignableFrom(
                typeof(BamlOperationCanceledException)),
            "operation cancellation must remain outside BamlException");
        Require(
            typeof(OperationCanceledException).IsAssignableFrom(
                typeof(BamlOperationCanceledException)),
            "operation cancellation must remain catchable as OperationCanceledException");

        Assembly assembly = typeof(BamlException).Assembly;
        Require(
            assembly.GetType("Baml.BamlTraceFrame") is null,
            "the wire has rendered trace lines, not structured trace frames");

        ConstructorInfo[] publicConstructors = assembly
            .GetExportedTypes()
            .Where(type => type.Name.StartsWith("Baml", StringComparison.Ordinal))
            .Where(type => typeof(Exception).IsAssignableFrom(type))
            .SelectMany(type => type.GetConstructors())
            .ToArray();
        Require(
            publicConstructors.Length == 0,
            "bridge exceptions must not expose invariant-breaking public constructors");

        RequireNoPublicConstructors<BamlValue>();
        RequireNoPublicConstructors<BamlTrace>();
        RequireNoPublicConstructors<BamlPanicInfo>();

        RequireDeclaredProperties<BamlExecutionException>(
            (nameof(BamlExecutionException.BamlFunction), typeof(string)),
            (nameof(BamlExecutionException.Trace), typeof(BamlTrace)));
        RequireDeclaredProperties<BamlErrorException>(
            (nameof(BamlErrorException.ThrownValue), typeof(BamlValue)),
            (nameof(BamlErrorException.ErrorName), typeof(string)));
        RequireDeclaredProperties<BamlTypeMismatchException>();
        RequireDeclaredProperties<BamlPanicException>(
            (nameof(BamlPanicException.Panic), typeof(BamlPanicInfo)));
        RequireDeclaredProperties<BamlPanicInfo>(
            (nameof(BamlPanicInfo.Value), typeof(BamlValue)),
            (nameof(BamlPanicInfo.IsExitPanic), typeof(bool)),
            (nameof(BamlPanicInfo.ExitCode), typeof(long?)));
        RequireDeclaredProperties<BamlTrace>(
            (nameof(BamlTrace.Lines), typeof(IReadOnlyList<string>)));
        RequireDeclaredProperties<BamlTypeMappingException>(
            (nameof(BamlTypeMappingException.ClrType), typeof(Type)),
            (nameof(BamlTypeMappingException.Position), typeof(string)),
            (nameof(BamlTypeMappingException.Path), typeof(string)),
            (nameof(BamlTypeMappingException.CanonicalReplacement), typeof(string)));
        RequireDeclaredProperties<BamlOperationCanceledException>(
            (nameof(BamlOperationCanceledException.Origin), typeof(BamlCancellationOrigin)),
            (nameof(BamlOperationCanceledException.BamlFunction), typeof(string)),
            (nameof(BamlOperationCanceledException.Trace), typeof(BamlTrace)));

        Require(
            Enum.GetUnderlyingType(typeof(BamlCancellationOrigin)) == typeof(int),
            "BamlCancellationOrigin underlying type changed");
        Require(
            (int)BamlCancellationOrigin.Caller == 0
            && (int)BamlCancellationOrigin.Engine == 1
            && (int)BamlCancellationOrigin.StreamDisposed == 2,
            "BamlCancellationOrigin numeric values changed");
    }

    private static void VerifyWireRepresentableDiagnosticsAndRedaction()
    {
        List<string> mutableTraceSource =
        [
            "File \"fixture.baml\", line 17, in pkg.fixture.run",
            "File \"fixture.baml\", line 29, in pkg.fixture.callback",
        ];
        BamlTrace trace = new(mutableTraceSource);
        mutableTraceSource[0] = "mutated after decode";
        BamlTrace equalTrace = CreateTrace();
        Require(trace.Equals(equalTrace), "trace structural equality failed");
        Require(
            trace.GetHashCode() == equalTrace.GetHashCode(),
            "trace structural hash failed");
        Require(
            StringComparer.Ordinal.Equals(
                trace.Lines[0],
                "File \"fixture.baml\", line 17, in pkg.fixture.run"),
            "trace did not snapshot the ordered rendered wire lines");
        Require(
            trace.Lines is not IList<string> mutable
            || mutable.IsReadOnly,
            "trace lines exposed mutable storage");

        BamlValue panicValue = new("baml.panics.UserPanic");
        BamlPanicInfo panic = new(
            panicValue,
            isExitPanic: false,
            exitCode: null);
        BamlPanicInfo equalPanic = new(
            panicValue,
            isExitPanic: false,
            exitCode: null);
        Require(panic.Equals(equalPanic), "panic structural equality failed");
        Require(
            ReferenceEquals(panic.Value, panicValue)
            && !panic.IsExitPanic
            && panic.ExitCode is null,
            "catchable panic metadata did not match the wire envelope");

        BamlProtocolException protocol = new(
            "The managed/native result envelope was invalid.",
            SensitiveValue);
        Require(
            protocol.SensitiveDiagnostic.Contains(
                SensitiveValue,
                StringComparison.Ordinal),
            "probe did not retain deliberate structured sensitive detail");
        RequireRedacted(protocol.Message);
        RequireRedacted(protocol.ToString());

        BamlValue thrownValue = new("fixture.errors.FixtureError");
        OutboundErrorEnvelope errorEnvelope = new(
            thrownValue,
            trace.Lines);
        BamlErrorException error = DecodeErrorEnvelope(
            errorEnvelope,
            callFunction: "pkg.fixture.run",
            typeMismatch: false);
        Require(
            StringComparer.Ordinal.Equals(
                error.BamlFunction,
                "pkg.fixture.run")
            && StringComparer.Ordinal.Equals(
                error.ErrorName,
                "fixture.errors.FixtureError")
            && ReferenceEquals(error.ThrownValue, thrownValue)
            && error.Trace.Equals(trace),
            "error properties did not come from decoded value, trace, and call context");
        RequireRedacted(error.ToString());

        BamlErrorException identityAbsent = DecodeErrorEnvelope(
            new OutboundErrorEnvelope(
                new BamlValue(nominalTypeName: null),
                []),
            callFunction: null,
            typeMismatch: false);
        Require(
            identityAbsent.BamlFunction is null
            && identityAbsent.ErrorName is null,
            "missing function/error identity was replaced with invented defaults");

        BamlTypeMismatchException mismatch =
            (BamlTypeMismatchException)DecodeErrorEnvelope(
                new OutboundErrorEnvelope(
                    new BamlValue("baml.errors.TypeMismatch"),
                    trace.Lines),
                callFunction: "pkg.fixture.run",
                typeMismatch: true);
        Require(
            ReferenceEquals(
                mismatch.ThrownValue,
                ((BamlErrorException)mismatch).ThrownValue),
            "type mismatch did not retain the decoded thrown value");
        Require(
            !typeof(BamlTypeMismatchException)
                .GetProperties(BindingFlags.Public | BindingFlags.Instance)
                .Any(
                    property => property.Name is "Expected"
                        or "Actual"
                        or "Path"),
            "type mismatch exposed metadata absent from the outbound envelope");

        BamlPanicException panicException = DecodePanicEnvelope(
            new OutboundPanicEnvelope(
                panicValue,
                trace.Lines,
                IsExitPanic: false,
                ExitCode: 912),
            callFunction: "pkg.fixture.run");
        Require(
            ReferenceEquals(panicException.Panic.Value, panicValue)
            && !panicException.Panic.IsExitPanic
            && panicException.Panic.ExitCode is null
            && panicException.Trace.Equals(trace),
            "catchable panic retained invented exit metadata or lost wire fields");
        Require(
            !typeof(BamlPanicInfo)
                .GetProperties(BindingFlags.Public | BindingFlags.Instance)
                .Any(
                    property => property.Name is "Category"
                        or "Reason"
                        or "Location"),
            "panic info exposed metadata absent from the outbound envelope");

        BamlTypeMappingException mapping = new(
            "The CLR type has no canonical BAML mapping.",
            typeof(HashSet<long>),
            "argument values",
            "$.values[2]",
            "IReadOnlyList<long>");
        Require(
            mapping.ClrType == typeof(HashSet<long>)
            && StringComparer.Ordinal.Equals(
                mapping.Position,
                "argument values")
            && StringComparer.Ordinal.Equals(
                mapping.Path,
                "$.values[2]")
            && StringComparer.Ordinal.Equals(
                mapping.CanonicalReplacement,
                "IReadOnlyList<long>"),
            "type-mapping structured properties changed");
    }

    private static BamlErrorException DecodeErrorEnvelope(
        OutboundErrorEnvelope envelope,
        string? callFunction,
        bool typeMismatch)
    {
        ArgumentNullException.ThrowIfNull(envelope);
        BamlTrace trace = new(envelope.Trace);
        return typeMismatch
            ? new BamlTypeMismatchException(
                "BAML execution produced a type-mismatch error.",
                envelope.Value,
                callFunction,
                trace)
            : new BamlErrorException(
                "BAML execution produced an error.",
                envelope.Value,
                callFunction,
                trace);
    }

    private static BamlPanicException DecodePanicEnvelope(
        OutboundPanicEnvelope envelope,
        string? callFunction)
    {
        ArgumentNullException.ThrowIfNull(envelope);
        BamlPanicInfo panic = new(
            envelope.Value,
            envelope.IsExitPanic,
            envelope.IsExitPanic ? envelope.ExitCode : null);
        return new BamlPanicException(
            "The BAML runtime panicked.",
            callFunction,
            panic,
            new BamlTrace(envelope.Trace));
    }

    private static async Task VerifyCancellationOriginsAsync()
    {
        foreach (BamlCancellationOrigin origin in Enum.GetValues<
                     BamlCancellationOrigin>())
        {
            using CancellationTokenSource source = new();
            source.Cancel();
            CancellationToken winningToken = source.Token;
            BamlOperationCanceledException? constructed = null;

            Task<int> publicTask = MapInternalCancellationAsync(
                Task.FromCanceled(winningToken),
                () =>
                {
                    constructed = new BamlOperationCanceledException(
                        $"BAML operation canceled by {origin}.",
                        origin,
                        winningToken,
                        "pkg.fixture.run",
                        CreateTrace());
                    return constructed;
                });

            BamlOperationCanceledException asyncCaught;
            try
            {
                _ = await publicTask.ConfigureAwait(false);
                throw new InvalidOperationException(
                    "public cancellation task unexpectedly succeeded");
            }
            catch (BamlOperationCanceledException exception)
            {
                asyncCaught = exception;
            }

            Require(
                ReferenceEquals(asyncCaught, constructed),
                "await did not preserve the custom cancellation instance");
            Require(
                asyncCaught.CancellationToken == winningToken,
                "await changed the winning cancellation token");
            Require(
                asyncCaught.Origin == origin,
                "await changed the cancellation origin");
            Require(
                publicTask.Status == TaskStatus.Canceled,
                "public cancellation task was not Canceled");
            Require(
                publicTask.Exception is null,
                "a canceled task exposed a fault aggregate");

            try
            {
                _ = publicTask.GetAwaiter().GetResult();
                throw new InvalidOperationException(
                    "sync cancellation unexpectedly succeeded");
            }
            catch (BamlOperationCanceledException syncCaught)
            {
                Require(
                    ReferenceEquals(syncCaught, constructed),
                    "sync GetAwaiter().GetResult changed the exception instance");
                Require(
                    syncCaught.CancellationToken == winningToken,
                    "sync GetAwaiter().GetResult changed the token");
            }
            catch (AggregateException)
            {
                throw new InvalidOperationException(
                    "sync GetAwaiter().GetResult introduced AggregateException");
            }
        }

        using CancellationTokenSource notCanceled = new();
        try
        {
            _ = new BamlOperationCanceledException(
                "invalid",
                BamlCancellationOrigin.Caller,
                notCanceled.Token,
                null,
                null);
            throw new InvalidOperationException(
                "uncanceled token was accepted");
        }
        catch (ArgumentException)
        {
        }
    }

    private static async Task<int> MapInternalCancellationAsync(
        Task internalCancellation,
        Func<BamlOperationCanceledException> exceptionFactory)
    {
        try
        {
            await internalCancellation.ConfigureAwait(false);
        }
        catch (OperationCanceledException)
        {
            throw exceptionFactory();
        }

        throw new InvalidOperationException(
            "the internal cancellation task unexpectedly completed");
    }

    private static async Task VerifyCallbackExceptionIdentityAsync()
    {
        CallbackExceptionRegistry registry = new();
        FixtureCallbackException thrown = new("application callback failure");
        long identity = registry.CaptureFromCallback(
            () => ThrowAtCallbackSource(thrown));

        await Task.Yield();
        Task<int> restoredTask = registry.RestoreAsFaultedTask(identity);
        Require(
            restoredTask.Status == TaskStatus.Faulted,
            "restored callback task was not faulted");

        try
        {
            _ = await restoredTask.ConfigureAwait(false);
            throw new InvalidOperationException(
                "restored callback task unexpectedly succeeded");
        }
        catch (FixtureCallbackException caught)
        {
            Require(
                ReferenceEquals(caught, thrown),
                "callback exception object identity changed");
            Require(
                caught.StackTrace?.Contains(
                    nameof(ThrowAtCallbackSource),
                    StringComparison.Ordinal)
                == true,
                "callback exception lost its original managed stack");
        }

        Task<int> missing = registry.RestoreAsFaultedTask(identity);
        try
        {
            _ = await missing.ConfigureAwait(false);
            throw new InvalidOperationException(
                "missing callback identity unexpectedly succeeded");
        }
        catch (BamlHostCallbackException)
        {
            Require(
                missing.Status == TaskStatus.Faulted,
                "missing callback identity fallback was not faulted");
        }
    }

    private static async Task VerifyUnrelatedCallbackCancellationIsFaultedAsync()
    {
        using CancellationTokenSource operation = new();
        using CancellationTokenSource unrelated = new();
        operation.Cancel();

        OperationCanceledException thrown = new(
            "callback used an unrelated token",
            unrelated.Token);
        CallbackExceptionRegistry registry = new();
        long identity = registry.CaptureFromCallback(
            () => throw thrown);

        Task<int> restoredTask = registry.RestoreAsFaultedTask(identity);
        Require(
            restoredTask.Status == TaskStatus.Faulted,
            "unrelated-token OperationCanceledException was misclassified as canceled");
        try
        {
            _ = await restoredTask.ConfigureAwait(false);
            throw new InvalidOperationException(
                "unrelated-token callback unexpectedly succeeded");
        }
        catch (OperationCanceledException caught)
        {
            Require(
                ReferenceEquals(caught, thrown),
                "unrelated-token callback exception identity changed");
            Require(
                caught.CancellationToken == unrelated.Token,
                "unrelated-token callback exception token changed");
        }

        Require(
            restoredTask.Status == TaskStatus.Faulted,
            "await reclassified the unrelated-token callback task");
    }

    private static async Task VerifyMatchingCallbackCancellationIsCanceledAsync()
    {
        using CancellationTokenSource linked = new();
        linked.Cancel();
        OperationCanceledException callbackCancellation = new(linked.Token);
        Require(
            ClassifyCallbackCancellation(
                callbackCancellation,
                linked.Token),
            "matching canceled callback token was not acknowledged");

        BamlOperationCanceledException? constructed = null;
        Task<int> outer = MapInternalCancellationAsync(
            Task.FromCanceled(linked.Token),
            () =>
            {
                constructed = new BamlOperationCanceledException(
                    "BAML operation canceled after callback acknowledgment.",
                    BamlCancellationOrigin.Caller,
                    linked.Token,
                    "pkg.fixture.run",
                    null);
                return constructed;
            });
        try
        {
            _ = await outer.ConfigureAwait(false);
            throw new InvalidOperationException(
                "matching-token callback cancellation unexpectedly succeeded");
        }
        catch (BamlOperationCanceledException caught)
        {
            Require(
                ReferenceEquals(caught, constructed),
                "matching-token callback changed custom cancellation identity");
        }

        Require(
            outer.Status == TaskStatus.Canceled,
            "matching-token callback outer task was not canceled");

        using CancellationTokenSource uncanceled = new();
        Require(
            !ClassifyCallbackCancellation(
                new OperationCanceledException(uncanceled.Token),
                uncanceled.Token),
            "uncanceled callback token was treated as acknowledgment");
    }

    private static bool ClassifyCallbackCancellation(
        OperationCanceledException exception,
        CancellationToken linkedCallbackToken) =>
        linkedCallbackToken.IsCancellationRequested
        && exception.CancellationToken == linkedCallbackToken;

    private static async Task VerifyTerminalRaceAsync()
    {
        TerminalArbiter arbiter = new();
        using ManualResetEventSlim start = new(initialState: false);
        const int signalCount = 64;
        OwnedPayload[] payloads = Enumerable.Range(0, signalCount)
            .Select(index => new OwnedPayload(index))
            .ToArray();

        Task<bool>[] signals = payloads
            .Select(
                (payload, index) => Task.Run(
                    () =>
                    {
                        start.Wait();
                        return arbiter.TryComplete(
                            (TerminalKind)((index % 3) + 1),
                            payload);
                    }))
            .ToArray();
        start.Set();
        bool[] outcomes = await Task.WhenAll(signals).ConfigureAwait(false);

        Require(
            outcomes.Count(outcome => outcome) == 1,
            "terminal race did not produce exactly one winner");
        Require(
            arbiter.RegistryRemovalCount == 1,
            "terminal race removed the registry more than once");
        Require(
            payloads.All(payload => payload.DisposeCount == 1),
            "terminal race did not release every signal payload exactly once");

        OwnedPayload late = new(signalCount);
        Require(
            !arbiter.TryComplete(TerminalKind.Result, late),
            "late completion replaced the terminal winner");
        Require(
            late.DisposeCount == 1,
            "late completion payload was not released exactly once");
    }

    private static async Task VerifyHardExitChildAsync()
    {
        ProcessStartInfo startInfo = CreateChildStartInfo();
        using Process child = Process.Start(startInfo)
            ?? throw new InvalidOperationException(
                "failed to start hard-exit child");

        Task<string> stdoutTask = child.StandardOutput.ReadToEndAsync();
        Task<string> stderrTask = child.StandardError.ReadToEndAsync();
        using CancellationTokenSource timeout = new(TimeSpan.FromSeconds(10));
        await child.WaitForExitAsync(timeout.Token).ConfigureAwait(false);
        string stdout = await stdoutTask.ConfigureAwait(false);
        string stderr = await stderrTask.ConfigureAwait(false);

        Require(
            child.ExitCode == HardExitCode,
            $"hard-exit child returned {child.ExitCode}: {stderr}");
        Require(
            stdout.Contains(
                "hard_exit_child=before_exit",
                StringComparison.Ordinal),
            "hard-exit child did not emit its bounded pre-exit signal");
        Require(
            !stdout.Contains(
                "hard_exit_child=finally",
                StringComparison.Ordinal),
            "Environment.Exit unexpectedly ran ordinary finally cleanup");
    }

    private static ProcessStartInfo CreateChildStartInfo()
    {
        string processPath = Environment.ProcessPath
            ?? throw new InvalidOperationException(
                "the current process path is unavailable");
        ProcessStartInfo startInfo = new(processPath)
        {
            UseShellExecute = false,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            CreateNoWindow = true,
        };

        if (StringComparer.OrdinalIgnoreCase.Equals(
                Path.GetFileNameWithoutExtension(processPath),
                "dotnet"))
        {
            startInfo.ArgumentList.Add(
                Assembly.GetExecutingAssembly().Location);
        }

        startInfo.ArgumentList.Add(HardExitArgument);
        return startInfo;
    }

    private static void RunHardExitChild()
    {
        try
        {
            Console.WriteLine("hard_exit_child=before_exit");
            Console.Out.Flush();
            DispatchPanicEnvelope(
                new OutboundPanicEnvelope(
                    new BamlValue("baml.panics.Exit"),
                    [],
                    IsExitPanic: true,
                    ExitCode: HardExitCode),
                callFunction: "pkg.fixture.exit");
        }
        finally
        {
            Console.WriteLine("hard_exit_child=finally");
            Console.Out.Flush();
        }
    }

    private static void DispatchPanicEnvelope(
        OutboundPanicEnvelope envelope,
        string? callFunction)
    {
        BamlPanicInfo panic = new(
            envelope.Value,
            envelope.IsExitPanic,
            envelope.IsExitPanic ? envelope.ExitCode : null);
        if (panic.IsExitPanic)
        {
            Environment.Exit(checked((int)panic.ExitCode!.Value));
        }

        throw new BamlPanicException(
            "The BAML runtime panicked.",
            callFunction,
            panic,
            new BamlTrace(envelope.Trace));
    }

    private static void ThrowAtCallbackSource(
        FixtureCallbackException exception) =>
        throw exception;

    private static BamlTrace CreateTrace() =>
        new(
            [
                "File \"fixture.baml\", line 17, in pkg.fixture.run",
                "File \"fixture.baml\", line 29, in pkg.fixture.callback",
            ]);

    private static void RequireNoPublicConstructors<T>() =>
        Require(
            typeof(T).GetConstructors().Length == 0,
            $"{typeof(T)} must not expose public constructors");

    private static void RequireDeclaredProperties<T>(
        params (string Name, Type Type)[] expected)
    {
        PropertyInfo[] actual = typeof(T)
            .GetProperties(
                BindingFlags.Public
                | BindingFlags.Instance
                | BindingFlags.DeclaredOnly)
            .OrderBy(property => property.Name, StringComparer.Ordinal)
            .ToArray();
        (string Name, Type Type)[] orderedExpected = expected
            .OrderBy(property => property.Name, StringComparer.Ordinal)
            .ToArray();
        Require(
            actual.Length == orderedExpected.Length,
            $"{typeof(T)} public declared property count changed");
        for (int index = 0; index < orderedExpected.Length; index++)
        {
            Require(
                StringComparer.Ordinal.Equals(
                    actual[index].Name,
                    orderedExpected[index].Name)
                && actual[index].PropertyType == orderedExpected[index].Type
                && actual[index].GetMethod?.IsPublic == true
                && actual[index].SetMethod is null,
                $"{typeof(T)} public property contract changed at index {index}");
        }
    }

    private static void RequireAbstract<T>()
    {
        Type type = typeof(T);
        Require(type.IsPublic && type.IsAbstract, $"{type} must be public abstract");
    }

    private static void RequireSealed<T>()
    {
        Type type = typeof(T);
        Require(type.IsPublic && type.IsSealed, $"{type} must be public sealed");
    }

    private static void RequireRedacted(string text) =>
        Require(
            !text.Contains(
                SensitiveValue,
                StringComparison.Ordinal),
            "default exception formatting leaked sensitive content");

    private static void Require(bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidOperationException(message);
        }
    }

    private sealed class CallbackExceptionRegistry
    {
        private readonly ConcurrentDictionary<long, ExceptionDispatchInfo>
            entries = new();
        private long nextIdentity;

        public long CaptureFromCallback(Action callback)
        {
            try
            {
                callback();
            }
            catch (Exception exception)
            {
                long identity = Interlocked.Increment(ref nextIdentity);
                if (!entries.TryAdd(
                        identity,
                        ExceptionDispatchInfo.Capture(exception)))
                {
                    throw new InvalidOperationException(
                        "callback exception identity collision");
                }

                return identity;
            }

            throw new InvalidOperationException(
                "the callback unexpectedly succeeded");
        }

        public Task<int> RestoreAsFaultedTask(long identity)
        {
            TaskCompletionSource<int> completion = new(
                TaskCreationOptions.RunContinuationsAsynchronously);
            if (!entries.TryRemove(
                    identity,
                    out ExceptionDispatchInfo? dispatchInfo))
            {
                completion.TrySetException(
                    new BamlHostCallbackException(
                        "The original managed callback exception is unavailable."));
                return completion.Task;
            }

            try
            {
                dispatchInfo.Throw();
                throw new InvalidOperationException(
                    "ExceptionDispatchInfo.Throw unexpectedly returned");
            }
            catch (Exception exception)
            {
                completion.TrySetException(exception);
            }

            return completion.Task;
        }
    }

    private enum TerminalKind
    {
        Result = 1,
        Error = 2,
        Cancellation = 3,
    }

    private sealed class TerminalArbiter
    {
        private int registryRemovalCount;
        private int terminal;

        public int RegistryRemovalCount =>
            Volatile.Read(ref registryRemovalCount);

        public bool TryComplete(
            TerminalKind kind,
            OwnedPayload payload)
        {
            bool won;
            try
            {
                won = Interlocked.CompareExchange(
                    ref terminal,
                    (int)kind,
                    comparand: 0) == 0;
                if (won)
                {
                    Interlocked.Increment(ref registryRemovalCount);
                }
            }
            finally
            {
                payload.Dispose();
            }

            return won;
        }
    }

    private sealed class OwnedPayload : IDisposable
    {
        private int disposeCount;

        public OwnedPayload(int identity)
        {
            Identity = identity;
        }

        public int Identity { get; }

        public int DisposeCount => Volatile.Read(ref disposeCount);

        public void Dispose()
        {
            if (Interlocked.Increment(ref disposeCount) != 1)
            {
                throw new InvalidOperationException(
                    $"payload {Identity} was released more than once");
            }
        }
    }

    private sealed class FixtureCallbackException : Exception
    {
        public FixtureCallbackException(string message)
            : base(message)
        {
        }
    }

    private sealed record OutboundErrorEnvelope(
        BamlValue Value,
        IReadOnlyList<string> Trace);

    private sealed record OutboundPanicEnvelope(
        BamlValue Value,
        IReadOnlyList<string> Trace,
        bool IsExitPanic,
        long ExitCode);
}
