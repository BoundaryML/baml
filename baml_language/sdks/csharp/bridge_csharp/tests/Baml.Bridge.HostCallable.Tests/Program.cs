using System.Collections.Concurrent;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.ExceptionServices;
using System.Runtime.InteropServices;

using Baml;
using Baml.Cffi;
using Baml.Generated.V1;
using Baml.Proto;
using BamlBridge.Cffi.V1;
using Google.Protobuf;
using HostInvocation = Baml.Cffi.HostInvocation;

internal static class Program
{
    private static readonly AsyncLocal<string?> Ambient = new();
    private static readonly ConcurrentQueue<HostCompletion> Completions = new();
    private static TaskCompletionSource completionSignal = NewSignal();
    private static int callbackThread;

    public static async Task<int> Main()
    {
        await VerifySyncAdapterSurfaceAndBehavior();
        using NativeFixture native = new();
        VerifyRegistryLifecycle(native.Api);
        VerifyStrictOptionalBindingAndArity(native.Api);
        NativeCallbacks.Register(native.Api);
        await VerifyAsyncDispatchContextAndBorrowedCopy(native.Api);
        await VerifyCancellationClassification(native.Api);
        await VerifyReentrancy(native.Api);
        await VerifyExactExceptionRoundTrip(native.Api);
        await VerifyCompletionContract(native.Api);
        Console.WriteLine("managed_host_callable=ok");
        return 0;
    }

    private static async Task VerifySyncAdapterSurfaceAndBehavior()
    {
        MethodInfo[] methods = typeof(BamlCallback).GetMethods(
            BindingFlags.Public | BindingFlags.Static | BindingFlags.DeclaredOnly);
        Require(methods.Length == 34, "BamlCallback must expose exactly 34 public adapter overloads");
        Require(
            methods.All(method => method.Name == nameof(BamlCallback.FromSync)),
            "BamlCallback exposed a public member outside the canonical FromSync family");

        for (int arity = 0; arity <= 16; arity++)
        {
            MethodInfo value = methods.Single(method =>
                method.GetGenericArguments().Length == arity + 1
                && method.GetParameters()[0].ParameterType.Name == $"Func`{arity + 1}");
            RequireCanonicalAdapterReturn(value, arity, returnsValue: true);

            MethodInfo @void = methods.Single(method =>
                method.GetGenericArguments().Length == arity
                && (arity == 0
                    ? method.GetParameters()[0].ParameterType == typeof(Action)
                    : method.GetParameters()[0].ParameterType.Name == $"Action`{arity}"));
            RequireCanonicalAdapterReturn(@void, arity, returnsValue: false);
        }

        Expect<ArgumentNullException>(() =>
            BamlCallback.FromSync<long>((Func<long>)null!));
        Expect<ArgumentNullException>(() =>
            BamlCallback.FromSync((Action)null!));

        int valueCalls = 0;
        Func<Task<long>> zeroValue = BamlCallback.FromSync<long>(() =>
        {
            valueCalls++;
            return 41L;
        });
        Require(valueCalls == 0, "value callback ran while its adapter was constructed");
        Require(
            await zeroValue() == 41L && valueCalls == 1,
            "zero-argument value adapter changed its result");

        int voidCalls = 0;
        Func<Task> zeroVoid = BamlCallback.FromSync(() => voidCalls++);
        Require(voidCalls == 0, "void callback ran while its adapter was constructed");
        await zeroVoid();
        Require(voidCalls == 1, "zero-argument void adapter did not invoke its callback once");

        Func<long, BamlOptional<string>, Task<string>> optional =
            BamlCallback.FromSync<long, BamlOptional<string>, string>(
                static (value, suffix) =>
                    suffix.IsSet ? $"{value}:{suffix.Value}" : $"{value}:unset");
        Require(
            await optional(7L, BamlOptional<string>.Unset) == "7:unset"
                && await optional(7L, BamlOptional<string>.FromValue("tail"))
                    == "7:tail",
            "BamlOptional did not flow naturally through the synchronous adapter");

        Func<long, long, long, long, long, long, long, long, long, long, long, long, long, long, long, long, Task<long>> maxValue =
            BamlCallback.FromSync<long, long, long, long, long, long, long, long, long, long, long, long, long, long, long, long, long>(
                static (v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v11, v12, v13, v14, v15, v16) =>
                    v1 + v2 + v3 + v4 + v5 + v6 + v7 + v8 + v9 + v10 + v11 + v12 + v13 + v14 + v15 + v16);
        Require(
            await maxValue(1L, 2L, 3L, 4L, 5L, 6L, 7L, 8L, 9L, 10L, 11L, 12L, 13L, 14L, 15L, 16L) == 136L,
            "maximum-arity value adapter changed argument order");

        long maxVoidSum = 0L;
        Func<long, long, long, long, long, long, long, long, long, long, long, long, long, long, long, long, Task> maxVoid =
            BamlCallback.FromSync<long, long, long, long, long, long, long, long, long, long, long, long, long, long, long, long>(
                (v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v11, v12, v13, v14, v15, v16) =>
                    maxVoidSum = v1 + v2 + v3 + v4 + v5 + v6 + v7 + v8 + v9 + v10 + v11 + v12 + v13 + v14 + v15 + v16);
        await maxVoid(1L, 2L, 3L, 4L, 5L, 6L, 7L, 8L, 9L, 10L, 11L, 12L, 13L, 14L, 15L, 16L);
        Require(maxVoidSum == 136L, "maximum-arity void adapter changed argument order");

        var original = new SyncSentinelException();
        Func<Task<string>> throwing =
            BamlCallback.FromSync<string>(() => throw original);
        Exception? observed = null;
        try
        {
            _ = throwing();
        }
        catch (Exception exception)
        {
            observed = exception;
        }

        Require(
            ReferenceEquals(observed, original),
            "the adapter replaced a synchronously thrown callback exception");
    }

    private static void RequireCanonicalAdapterReturn(
        MethodInfo method,
        int arity,
        bool returnsValue)
    {
        Type returnType = method.ReturnType;
        Require(
            returnType.IsGenericType
                && returnType.GetGenericTypeDefinition().FullName == $"System.Func`{arity + 1}",
            $"{method} did not return the canonical Func delegate at arity {arity}");
        Type[] arguments = returnType.GetGenericArguments();
        Type task = arguments[arity];
        Require(
            returnsValue
                ? task.IsGenericType
                    && task.GetGenericTypeDefinition() == typeof(Task<>)
                    && task.GetGenericArguments()[0] == method.GetGenericArguments()[arity]
                : task == typeof(Task),
            $"{method} introduced a non-Task callback runtime path at arity {arity}");
        Require(
            !method.ToString()!.Contains("ValueTask", StringComparison.Ordinal),
            $"{method} accidentally exposed ValueTask");
    }

    private static void VerifyRegistryLifecycle(NativeApi api)
    {
        (BamlGeneratedRegistry registry, BamlGeneratedType<long> integer, _) =
            CreateRegistry();
        var context = new BamlGeneratedCodecContext(registry);
        Func<long, CancellationToken, Task<long>> callback =
            static (value, _) => Task.FromResult(value);
        BamlGeneratedHostCallable callable = context.HostCallable(
            callback,
            [context.Required(integer)],
            context.Result(integer),
            context.UnknownThrows(),
            static (target, arguments, token) => BamlGeneratedHostCallableRuntime.Await(
                ((Func<long, CancellationToken, Task<long>>)target)(
                    (long)arguments[0]!,
                    token)))
            .ReadHostCallable();

        var hostValues = new HostValueRegistry();
        using (HostValueRegistration unpublished = hostValues.RegisterCallable(
            callable,
            parentFunctionCallId: 90))
        {
            Require(hostValues.EntryCount == 1, "pending callable registration was lost");
        }
        Require(hostValues.EntryCount == 0, "unpublished callable registration did not abort");

        hostValues.BeginFunctionCall(91, CancellationToken.None);
        using (HostValueRegistration published = hostValues.RegisterCallable(
            callable,
            parentFunctionCallId: 91))
        {
            published.Commit();
            ulong firstKey = published.Key;
            HostInvocation invocation = hostValues.TryStartInvocation(
                firstKey,
                hostCallId: 1,
                Arguments(new BamlToHostCall(), api),
                out string? diagnostic)
                ?? throw new InvalidOperationException(diagnostic);
            hostValues.Release(firstKey);
            Require(
                hostValues.EntryCount == 1,
                "native release invalidated an active managed dispatch lease");
            invocation.Complete();
            Require(
                hostValues.EntryCount == 0,
                "released callable was not collected after its final dispatch lease");

            using HostValueRegistration replacement = hostValues.RegisterCallable(
                callable,
                parentFunctionCallId: 91);
            replacement.Commit();
            Require(
                replacement.Key != firstKey,
                "a recycled host-value slot reused a stale generation key");
            hostValues.Release(replacement.Key);
        }
        hostValues.CompleteFunctionCall(91);

        using (var caller = new CancellationTokenSource())
        {
            hostValues.BeginFunctionCall(92, caller.Token);
            using HostValueRegistration canceled = hostValues.RegisterCallable(
                callable,
                parentFunctionCallId: 92);
            canceled.Commit();
            caller.Cancel();
            HostInvocation canceledInvocation = hostValues.TryStartInvocation(
                canceled.Key,
                hostCallId: 77,
                Arguments(new BamlToHostCall(), api),
                out string? diagnostic)
                ?? throw new InvalidOperationException(diagnostic);
            Require(
                canceledInvocation.CancellationToken.IsCancellationRequested,
                "caller cancellation before dispatch did not reach the invocation token");
            canceledInvocation.Complete();
            hostValues.Release(canceled.Key);
            hostValues.CompleteFunctionCall(92);
        }

        hostValues.BeginFunctionCall(93, CancellationToken.None);
        using (HostValueRegistration exception = hostValues.RegisterException(
            ExceptionDispatchInfo.Capture(new InvalidOperationException("settled")),
            parentFunctionCallId: 93))
        {
            exception.Commit();
            hostValues.CompleteFunctionCall(93);
            Require(
                hostValues.EntryCount == 0,
                "a settled operation retained an exception that native never adopted");
        }
    }

    private static void VerifyStrictOptionalBindingAndArity(NativeApi api)
    {
        (BamlGeneratedRegistry registry, BamlGeneratedType<long> integer, BamlGeneratedType<string> text) =
            CreateRegistry();
        var context = new BamlGeneratedCodecContext(registry);
        Func<long, BamlOptional<string>, BamlOptional<long>, CancellationToken, Task<string>> callback =
            static (required, first, later, _) =>
                Task.FromResult($"{required}:{first}:{later}");
        BamlGeneratedHostCallable callable = context.HostCallable(
            callback,
            [
                context.Required(integer),
                context.Optional("first", text),
                context.Optional("later", integer),
            ],
            context.Result(text),
            context.UnknownThrows(),
            static (target, arguments, token) => BamlGeneratedHostCallableRuntime.Await(
                ((Func<long, BamlOptional<string>, BamlOptional<long>, CancellationToken, Task<string>>)target)(
                    (long)arguments[0]!,
                    (BamlOptional<string>)arguments[1]!,
                    (BamlOptional<long>)arguments[2]!,
                    token)))
            .ReadHostCallable();

        BamlToHostCall valid = Call(
            RequiredInt(7),
            OptionalInt("later", 9),
            OptionalString("first", "x"));
        IReadOnlyList<object?> bound = HostCallableProtocol.BindArguments(
            callable.Descriptor,
            Arguments(valid, api));
        Require(
            (long)bound[0]! == 7
                && ((BamlOptional<string>)bound[1]!).Value == "x"
                && ((BamlOptional<long>)bound[2]!).Value == 9,
            "named optional callback arguments did not bind by wire identity");

        bound = HostCallableProtocol.BindArguments(
            callable.Descriptor,
            Arguments(Call(RequiredInt(8)), api));
        Require(
            !((BamlOptional<string>)bound[1]!).IsSet
                && !((BamlOptional<long>)bound[2]!).IsSet,
            "omitted optional callback arguments did not remain unset");

        Expect<BamlProtocolException>(() => HostCallableProtocol.BindArguments(
            callable.Descriptor,
            Arguments(Call(), api)));
        Expect<BamlProtocolException>(() => HostCallableProtocol.BindArguments(
            callable.Descriptor,
            Arguments(Call(OptionalString("first", "x"), RequiredInt(1)), api)));
        Expect<BamlProtocolException>(() => HostCallableProtocol.BindArguments(
            callable.Descriptor,
            Arguments(Call(RequiredInt(1), OptionalString("first", "x"), OptionalString("first", "y")), api)));
        Expect<BamlProtocolException>(() => HostCallableProtocol.BindArguments(
            callable.Descriptor,
            Arguments(Call(RequiredInt(1), OptionalString("unknown", "x")), api)));
        Expect<BamlProtocolException>(() => HostCallableProtocol.BindArguments(
            callable.Descriptor,
            Arguments(Call(new BamlToHostArg
            {
                ArgName = "not-positional",
                Value = new BamlOutboundValue { IntValue = 1 },
            }), api)));

        BamlGeneratedHostParameter[] tooMany = Enumerable.Range(0, 16)
            .Select(_ => context.Required(integer))
            .ToArray();
        Expect<ArgumentOutOfRangeException>(() => context.HostCallable(
            callback,
            tooMany,
            context.Result(text),
            context.UnknownThrows(),
            static (_, _, _) => Task.FromResult<object?>(null)));
    }

    private static async Task VerifyAsyncDispatchContextAndBorrowedCopy(NativeApi api)
    {
        ResetCompletions();
        (BamlGeneratedRegistry registry, BamlGeneratedType<long> integer, BamlGeneratedType<string> text) =
            CreateRegistry();
        var context = new BamlGeneratedCodecContext(registry);
        int dispatchThread = Environment.CurrentManagedThreadId;
        var synchronizationContext = new SynchronizationContext();
        SynchronizationContext.SetSynchronizationContext(synchronizationContext);
        Ambient.Value = "captured";
        Func<long, BamlOptional<string>, CancellationToken, Task<string>> callback =
            async (value, suffix, token) =>
            {
                callbackThread = Environment.CurrentManagedThreadId;
                Require(
                    Ambient.Value == "captured",
                    "callback execution context was not captured at registration");
                Require(
                    SynchronizationContext.Current is null,
                    "callback inherited a synchronization context");
                await Task.Yield();
                token.ThrowIfCancellationRequested();
                Require(
                    SynchronizationContext.Current is null,
                    "callback continuation captured a synchronization context");
                return value.ToString() + (suffix.IsSet ? suffix.Value : string.Empty);
            };
        BamlGeneratedHostCallable callable = context.HostCallable(
            callback,
            [context.Required(integer), context.Optional("suffix", text)],
            context.Result(text),
            context.UnknownThrows(),
            static (target, arguments, token) => BamlGeneratedHostCallableRuntime.Await(
                ((Func<long, BamlOptional<string>, CancellationToken, Task<string>>)target)(
                    (long)arguments[0]!,
                    (BamlOptional<string>)arguments[1]!,
                    token)))
            .ReadHostCallable();
        const ulong functionCallId = 1001;
        const uint hostCallId = 101;
        HostValueRegistry.Shared.BeginFunctionCall(functionCallId, CancellationToken.None);
        HostValueRegistration registration =
            HostValueRegistry.Shared.RegisterCallable(callable, functionCallId);
        registration.Commit();
        SynchronizationContext.SetSynchronizationContext(null);
        Ambient.Value = "changed";
        byte[] payload = Call(RequiredInt(12), OptionalString("suffix", "!"))
            .ToByteArray();
        Dispatch(registration.Key, hostCallId, payload);
        Array.Fill(payload, (byte)0xff);

        HostCompletion completion = await NextCompletion();
        Require(
            completion.CallId == hostCallId && completion.Outcome.Returned is not null,
            "async host dispatch did not complete as success");
        InboundValue result = completion.Outcome.Returned
            ?? throw new InvalidOperationException("the completion lost its returned value");
        Require(
            result.StringValue == "12!",
            "host dispatch did not copy its borrowed argument buffer before returning");
        Require(
            callbackThread != dispatchThread,
            "host callback ran inline on the unmanaged dispatch thread");

        await WaitUntil(() => HostValueRegistry.Shared.InvocationCount == 0);
        HostValueRegistry.Shared.Release(registration.Key);
        registration.Dispose();
        HostValueRegistry.Shared.CompleteFunctionCall(functionCallId);
    }

    private static async Task VerifyCancellationClassification(NativeApi api)
    {
        ResetCompletions();
        (BamlGeneratedRegistry registry, _, BamlGeneratedType<string> text) = CreateRegistry();
        var context = new BamlGeneratedCodecContext(registry);
        var tokenSeen = new TaskCompletionSource<CancellationToken>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var canceled = new TaskCompletionSource(
            TaskCreationOptions.RunContinuationsAsynchronously);
        Func<CancellationToken, Task<string>> callback = async token =>
        {
            tokenSeen.TrySetResult(token);
            try
            {
                await Task.Delay(Timeout.InfiniteTimeSpan, token);
                return "unreachable";
            }
            finally
            {
                canceled.TrySetResult();
            }
        };
        BamlGeneratedHostCallable callable = context.HostCallable(
            callback,
            [],
            context.Result(text),
            context.UnknownThrows(),
            static (target, _, token) => BamlGeneratedHostCallableRuntime.Await(
                ((Func<CancellationToken, Task<string>>)target)(token)))
            .ReadHostCallable();
        const ulong functionCallId = 1002;
        const uint hostCallId = 102;
        using var caller = new CancellationTokenSource();
        HostValueRegistry.Shared.BeginFunctionCall(functionCallId, caller.Token);
        HostValueRegistration registration =
            HostValueRegistry.Shared.RegisterCallable(callable, functionCallId);
        registration.Commit();
        byte[] payload = new BamlToHostCall().ToByteArray();
        Dispatch(registration.Key, hostCallId, payload);

        CancellationToken supplied = await tokenSeen.Task.WaitAsync(TimeSpan.FromSeconds(5));
        caller.Cancel();
        await canceled.Task.WaitAsync(TimeSpan.FromSeconds(5));
        await WaitUntil(() => HostValueRegistry.Shared.InvocationCount == 0);
        HostCompletion completion = await NextCompletion();
        Require(
            supplied.IsCancellationRequested
                && completion.CallId == hostCallId
                && completion.Outcome.Raised is not null
                && Completions.IsEmpty,
            "a canceled callback must report its actual exit exactly once");
        ulong exceptionKey = (completion.Outcome.Raised
            ?? throw new InvalidOperationException("the completion lost its raised exception")).Handle;
        HostValueRegistry.Shared.Release(exceptionKey);

        HostValueRegistry.Shared.Release(registration.Key);
        registration.Dispose();
        HostValueRegistry.Shared.CompleteFunctionCall(functionCallId);
    }

    private static async Task VerifyReentrancy(NativeApi api)
    {
        ResetCompletions();
        (BamlGeneratedRegistry registry, BamlGeneratedType<long> integer, _) = CreateRegistry();
        var context = new BamlGeneratedCodecContext(registry);
        ulong hostKey = 0;
        const ulong functionCallId = 1003;
        Func<long, CancellationToken, Task<long>> callback = async (value, token) =>
        {
            if (value == 1)
            {
                byte[] nested = Call(RequiredInt(2)).ToByteArray();
                Dispatch(hostKey, hostCallId: 104, nested);
                await Task.Yield();
            }

            token.ThrowIfCancellationRequested();
            return value;
        };
        BamlGeneratedHostCallable callable = context.HostCallable(
            callback,
            [context.Required(integer)],
            context.Result(integer),
            context.UnknownThrows(),
            static (target, arguments, token) => BamlGeneratedHostCallableRuntime.Await(
                ((Func<long, CancellationToken, Task<long>>)target)(
                    (long)arguments[0]!,
                    token)))
            .ReadHostCallable();
        HostValueRegistry.Shared.BeginFunctionCall(functionCallId, CancellationToken.None);
        HostValueRegistration registration =
            HostValueRegistry.Shared.RegisterCallable(callable, functionCallId);
        registration.Commit();
        hostKey = registration.Key;
        byte[] outer = Call(RequiredInt(1)).ToByteArray();
        Dispatch(hostKey, hostCallId: 103, outer);

        HostCompletion first = await NextCompletion();
        HostCompletion second = await NextCompletion();
        uint[] ids = [first.CallId, second.CallId];
        Array.Sort(ids);
        Require(
            ids.SequenceEqual(new uint[] { 103, 104 })
                && first.Outcome.Returned is not null
                && second.Outcome.Returned is not null,
            "reentrant callback dispatch did not complete independently");

        await WaitUntil(() => HostValueRegistry.Shared.InvocationCount == 0);
        HostValueRegistry.Shared.Release(registration.Key);
        registration.Dispose();
        HostValueRegistry.Shared.CompleteFunctionCall(functionCallId);
    }

    private static async Task VerifyExactExceptionRoundTrip(NativeApi api)
    {
        ResetCompletions();
        (BamlGeneratedRegistry registry, _, BamlGeneratedType<string> text) = CreateRegistry();
        var context = new BamlGeneratedCodecContext(registry);
        using var unrelatedCancellation = new CancellationTokenSource();
        unrelatedCancellation.Cancel();
        OperationCanceledException original = CaptureOriginalException(
            unrelatedCancellation.Token);
        Func<CancellationToken, Task<string>> callback =
            _ => Task.FromException<string>(original);
        BamlGeneratedHostCallable callable = context.HostCallable(
            callback,
            [],
            context.Result(text),
            context.UnknownThrows(),
            static (target, _, token) => BamlGeneratedHostCallableRuntime.Await(
                ((Func<CancellationToken, Task<string>>)target)(token)))
            .ReadHostCallable();
        const ulong functionCallId = 1004;
        const uint hostCallId = 105;
        HostValueRegistry.Shared.BeginFunctionCall(functionCallId, CancellationToken.None);
        HostValueRegistration registration =
            HostValueRegistry.Shared.RegisterCallable(callable, functionCallId);
        registration.Commit();
        byte[] payload = new BamlToHostCall().ToByteArray();
        Dispatch(registration.Key, hostCallId, payload);

        HostCompletion completion = await NextCompletion();
        Require(
            completion.CallId == hostCallId && completion.Outcome.Raised is not null,
            "an unrelated callback exception was misclassified as cancellation");
        HostException raised = completion.Outcome.Raised
            ?? throw new InvalidOperationException("the completion lost its raised exception");
        Require(
            raised.Handle != 0,
            "callback exception did not carry its registration for rehydration");

        HostValueRegistry.Shared.Release(raised.Handle);
        BamlOutboundResult outbound = ToOutboundError(raised);
        Exception? restored = null;
        try
        {
            _ = PrimitiveProtocol.DecodeCallResult(outbound.ToByteArray(), "test.callback", api);
        }
        catch (Exception error)
        {
            restored = error;
        }

        Require(
            ReferenceEquals(restored, original)
                && restored.StackTrace?.Contains(
                    nameof(ThrowOriginalException),
                    StringComparison.Ordinal) == true,
            "HostCallable round-trip did not rethrow the exact managed exception via EDI");

        BamlOutboundResult foreign = outbound.Clone();
        foreign.Error.Value.ClassValue.Fields
            .Single(field => field.Key == "_handle")
            .Value.HandleValue.Key ^= 0x1000;
        Expect<BamlHostCallbackException>(() =>
            PrimitiveProtocol.DecodeCallResult(foreign.ToByteArray(), "test.callback", api));

        await WaitUntil(() => HostValueRegistry.Shared.InvocationCount == 0);
        HostValueRegistry.Shared.Release(registration.Key);
        registration.Dispose();
        HostValueRegistry.Shared.CompleteFunctionCall(functionCallId);
    }

    /// <summary>
    /// Every fault a callback can end in completes its call with the one
    /// outcome that names whose fault it is.
    /// </summary>
    private static async Task VerifyCompletionContract(NativeApi api)
    {
        (
            BamlGeneratedRegistry registry,
            BamlGeneratedType<string> text,
            BamlGeneratedType<BamlValue> unknown,
            BamlGeneratedType<double> number,
            BamlGeneratedType<long> integer) = CreateContractRegistry();
        var context = new BamlGeneratedCodecContext(registry);
        BamlGeneratedHostThrows never = context.Throws(
            new BamlTy { Never = new BamlTyNever() }.ToByteArray());

        // The callable's declared BAML error is thrown, stated as the declared
        // type; one the declared type does not hold is its broken contract.
        HostCallCompletion declared = await CompleteOne(
            api,
            context,
            context.Result(text),
            context.Throws(text),
            static () => throw new BamlErrorException(BamlValue.String("declared")));
        Require(
            declared.Thrown?.StringValue == "declared" && declared.Thrown.ValueType is null,
            "a declared BAML error was not thrown under the declared throws type");
        HostCallCompletion undeclared = await CompleteOne(
            api,
            context,
            context.Result(text),
            context.Throws(text),
            static () => throw new BamlErrorException(BamlValue.Int(7)));
        Require(
            undeclared.OutcomeCase == HostCallCompletion.OutcomeOneofCase.ContractViolation,
            "a BAML error outside the declared throws type was not the callable's contract violation");
        HostCallCompletion neverThrows = await CompleteOne(
            api,
            context,
            context.Result(text),
            never,
            static () => throw new BamlErrorException(BamlValue.String("never")));
        Require(
            neverThrows.OutcomeCase == HostCallCompletion.OutcomeOneofCase.ContractViolation,
            "a BAML error under `throws never` was not the callable's contract violation");

        // A BAML error a nested call threw, let through, is thrown again.
        BamlErrorException nested = CaptureNested<BamlErrorException>(new BamlOutboundResult
        {
            Error = new BamlOutboundError { Value = new BamlOutboundValue { StringValue = "nested" } },
        });
        HostCallCompletion forwardedError = await CompleteOne(
            api,
            context,
            context.Result(text),
            context.UnknownThrows(),
            () => throw nested);
        Require(
            forwardedError.Thrown?.StringValue == "nested",
            "a nested BAML error let through was not thrown");

        // A nested panic, the engine's cancellation included, stays a panic
        // whatever the callable declares: never held to `throws never`.
        BamlPanicException panic = CaptureNested<BamlPanicException>(PanicResult("baml.panics.DivisionByZero"));
        HostCallCompletion forwardedPanic = await CompleteOne(
            api,
            context,
            context.Result(text),
            never,
            () => throw panic);
        Require(
            forwardedPanic.Thrown?.ClassValue is not null
                && forwardedPanic.Thrown.ValueType.ClassTy.Name == "baml.panics.DivisionByZero",
            "a nested panic let through was not thrown as the panic it is");
        BamlOperationCanceledException cancelled =
            CaptureNested<BamlOperationCanceledException>(PanicResult("baml.panics.Cancelled"));
        HostCallCompletion forwardedCancel = await CompleteOne(
            api,
            context,
            context.Result(text),
            never,
            () => throw cancelled);
        Require(
            forwardedCancel.Thrown?.ValueType?.ClassTy?.Name == "baml.panics.Cancelled",
            "the engine's cancellation let through was not thrown as its panic");

        // A value the declared return type does not hold is the callable's
        // broken contract.
        HostCallCompletion nullString = await CompleteOne(
            api,
            context,
            context.Result(text),
            context.UnknownThrows(),
            static () => null);
        Require(
            nullString.OutcomeCase == HostCallCompletion.OutcomeOneofCase.ContractViolation,
            "null for a non-null return was not the callable's contract violation");
        HostCallCompletion notANumber = await CompleteOne(
            api,
            context,
            context.Result(number),
            context.UnknownThrows(),
            static () => double.NaN);
        HostCallCompletion outOfRange = await CompleteOne(
            api,
            context,
            context.Result(integer),
            context.UnknownThrows(),
            static () => long.MaxValue);
        Require(
            notANumber.OutcomeCase == HostCallCompletion.OutcomeOneofCase.ContractViolation
                && outOfRange.OutcomeCase == HostCallCompletion.OutcomeOneofCase.ContractViolation,
            "a NaN float or an out-of-range int result was not the callable's contract violation");

        // A value the type holds that the wire cannot carry is the bridge's:
        // a typed list nests 2n+2 messages under its one enclosing message.
        HostCallCompletion fits = await CompleteOne(
            api,
            context,
            context.Result(unknown),
            context.UnknownThrows(),
            static () => NestedLists(48));
        Require(
            fits.OutcomeCase == HostCallCompletion.OutcomeOneofCase.Returned,
            "a 48-level list, at the wire's limit, did not return");
        HostCallCompletion tooDeep = await CompleteOne(
            api,
            context,
            context.Result(unknown),
            context.UnknownThrows(),
            static () => NestedLists(49));
        Require(
            tooDeep.OutcomeCase == HostCallCompletion.OutcomeOneofCase.BridgeFailure,
            "a 49-level list, one past the wire's limit, was not the bridge's failure");
        HostCallCompletion mapsFit = await CompleteOne(
            api,
            context,
            context.Result(unknown),
            context.UnknownThrows(),
            static () => NestedMaps(32));
        HostCallCompletion mapsTooDeep = await CompleteOne(
            api,
            context,
            context.Result(unknown),
            context.UnknownThrows(),
            static () => NestedMaps(33));
        Require(
            mapsFit.OutcomeCase == HostCallCompletion.OutcomeOneofCase.Returned
                && mapsTooDeep.OutcomeCase == HostCallCompletion.OutcomeOneofCase.BridgeFailure,
            "maps did not cross up to 32 levels and fail as the bridge's past it");
        HostCallCompletion thrownTooDeep = await CompleteOne(
            api,
            context,
            context.Result(text),
            context.UnknownThrows(),
            static () => throw new BamlErrorException(NestedLists(49)));
        Require(
            thrownTooDeep.OutcomeCase == HostCallCompletion.OutcomeOneofCase.BridgeFailure,
            "a thrown value deeper than the wire was not the bridge's failure");

        // A handle the bridge cannot clone is the bridge's failure.
        HostCallCompletion uncloneable = await CompleteOne(
            api,
            context,
            context.Result(unknown),
            context.UnknownThrows(),
            static () => UncloneableHandle());
        Require(
            uncloneable.OutcomeCase == HostCallCompletion.OutcomeOneofCase.BridgeFailure,
            "a handle clone failure was not the bridge's failure");

        // Arguments the callable's adapter cannot bind are the bridge's.
        HostCallCompletion unbound = await CompleteOne(
            api,
            context,
            context.Result(text),
            context.UnknownThrows(),
            static () => "unreachable",
            Call(RequiredInt(1)));
        Require(
            unbound.OutcomeCase == HostCallCompletion.OutcomeOneofCase.BridgeFailure,
            "an argument the adapter could not bind was not the bridge's failure");

        // A callback started after its call was canceled never runs.
        int lateRuns = 0;
        using (var caller = new CancellationTokenSource())
        {
            caller.Cancel();
            HostCallCompletion late = await CompleteOne(
                api,
                context,
                context.Result(text),
                context.UnknownThrows(),
                () =>
                {
                    lateRuns++;
                    return "late";
                },
                callerToken: caller.Token);
            Require(
                late.OutcomeCase == HostCallCompletion.OutcomeOneofCase.BridgeFailure && lateRuns == 0,
                "a callback invoked after cancellation ran or was not the bridge's failure");
        }

        // A dispatch naming no callable is the bridge's failure.
        ResetCompletions();
        Dispatch(hostKey: 0xdead_0000, hostCallId: 190, new BamlToHostCall().ToByteArray());
        HostCompletion stale = await NextCompletion();
        Require(
            stale.CallId == 190
                && stale.Outcome.OutcomeCase == HostCallCompletion.OutcomeOneofCase.BridgeFailure,
            "a dispatch to an unknown callable key was not the bridge's failure");
    }

    private static uint nextContractCallId = 150;

    /// <summary>
    /// Runs one zero-argument callback through native dispatch and returns how
    /// its call completed. A raised exception's registration is released.
    /// </summary>
    private static async Task<HostCallCompletion> CompleteOne(
        NativeApi api,
        BamlGeneratedCodecContext context,
        BamlGeneratedHostResult result,
        BamlGeneratedHostThrows throws,
        Func<object?> body,
        BamlToHostCall? call = null,
        CancellationToken callerToken = default)
    {
        ResetCompletions();
        Func<CancellationToken, Task<object?>> callback = _ => Task.FromResult(body());
        BamlGeneratedHostCallable callable = context.HostCallable(
            callback,
            [],
            result,
            throws,
            static (target, _, token) => ((Func<CancellationToken, Task<object?>>)target)(token))
            .ReadHostCallable();
        uint hostCallId = nextContractCallId++;
        ulong functionCallId = 2000 + hostCallId;
        HostValueRegistry.Shared.BeginFunctionCall(functionCallId, callerToken);
        HostValueRegistration registration =
            HostValueRegistry.Shared.RegisterCallable(callable, functionCallId);
        registration.Commit();
        Dispatch(registration.Key, hostCallId, (call ?? new BamlToHostCall()).ToByteArray());
        HostCompletion completion = await NextCompletion();
        Require(completion.CallId == hostCallId, "a completion named another call");
        await WaitUntil(() => HostValueRegistry.Shared.InvocationCount == 0);
        HostCallCompletion outcome = completion.Outcome;
        if (outcome.Raised is { } raised)
        {
            HostValueRegistry.Shared.Release(raised.Handle);
        }

        HostValueRegistry.Shared.Release(registration.Key);
        registration.Dispose();
        HostValueRegistry.Shared.CompleteFunctionCall(functionCallId);
        return outcome;
    }

    /// <summary>The exception a nested BAML call ending in <paramref name="result"/> throws.</summary>
    private static TException CaptureNested<TException>(BamlOutboundResult result)
        where TException : Exception
    {
        try
        {
            _ = PrimitiveProtocol.DecodeCallResult(result.ToByteArray(), "test.nested");
        }
        catch (TException error)
        {
            return error;
        }

        throw new InvalidOperationException($"the nested call did not throw {typeof(TException).Name}");
    }

    private static BamlOutboundResult PanicResult(string panicClass)
    {
        var value = new BamlValueClass { Name = panicClass };
        value.Fields.Add(new BamlOutboundMapEntry
        {
            Key = "message",
            Value = new BamlOutboundValue { StringValue = "nested panic" },
        });
        return new BamlOutboundResult
        {
            Panic = new BamlOutboundPanic { Value = new BamlOutboundValue { ClassValue = value } },
        };
    }

    private static BamlValue NestedLists(int levels)
    {
        BamlValue value = BamlValue.Null;
        for (int level = 0; level < levels; level++)
        {
            value = BamlValue.List([value]);
        }

        return value;
    }

    private static BamlValue NestedMaps(int levels)
    {
        BamlValue value = BamlValue.Null;
        for (int level = 0; level < levels; level++)
        {
            value = BamlValue.Map([new("next", value)]);
        }

        return value;
    }

    private static unsafe BamlValue UncloneableHandle() =>
        new(BamlGeneratedValue.CreateHandle(new Baml.BamlHandle(
            new BamlSafeHandle(4242, &RefuseClone, &AcceptRelease),
            BamlTypeDescriptor.CreateHandle("test.Resource"),
            BamlHandleType.UntaggedRustData)));

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static unsafe BamlCffiStatus RefuseClone(ulong key, ulong* output) =>
        BamlCffiStatus.InvalidHandle;

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static BamlCffiStatus AcceptRelease(ulong key) => BamlCffiStatus.Ok;

    /// <summary>
    /// A registry whose types state themselves on the wire, as generated ones
    /// do: <c>string</c>, and <c>unknown</c> as a dynamic value.
    /// </summary>
    private static (
        BamlGeneratedRegistry Registry,
        BamlGeneratedType<string> String,
        BamlGeneratedType<BamlValue> Unknown,
        BamlGeneratedType<double> Float,
        BamlGeneratedType<long> Int) CreateContractRegistry()
    {
        BamlGeneratedRegistryBuilder builder =
            BamlGeneratedContract.CreateRegistryBuilder(BamlGeneratedContract.Version);
        BamlGeneratedType<string> text = builder.DeclareType<string>(
            "string",
            new BamlTy { Primitive = new BamlTyPrimitive { Kind = BamlTyPrimitiveKind.BamlTyPrimitiveString } }.ToByteArray());
        BamlGeneratedType<BamlValue> unknown = builder.DeclareType<BamlValue>(
            "unknown",
            new BamlTy { Unknown = new BamlTyUnknown() }.ToByteArray());
        BamlGeneratedType<double> number = builder.DeclareType<double>(
            "float",
            new BamlTy { Primitive = new BamlTyPrimitive { Kind = BamlTyPrimitiveKind.BamlTyPrimitiveFloat } }.ToByteArray());
        BamlGeneratedType<long> integer = builder.DeclareType<long>(
            "int",
            new BamlTy { Primitive = new BamlTyPrimitive { Kind = BamlTyPrimitiveKind.BamlTyPrimitiveInt } }.ToByteArray());
        builder.RegisterCodec(text, new StringCodec());
        builder.RegisterCodec(unknown, new DynamicCodec());
        builder.RegisterCodec(number, new FloatCodec());
        builder.RegisterCodec(integer, new IntCodec());
        return (builder.Build(), text, unknown, number, integer);
    }

    private static (BamlGeneratedRegistry Registry, BamlGeneratedType<long> Int, BamlGeneratedType<string> String)
        CreateRegistry()
    {
        BamlGeneratedRegistryBuilder builder =
            BamlGeneratedContract.CreateRegistryBuilder(BamlGeneratedContract.Version);
        BamlGeneratedType<long> integer = builder.DeclareType<long>("int");
        BamlGeneratedType<string> text = builder.DeclareType<string>("string");
        builder.RegisterCodec(integer, new IntCodec());
        builder.RegisterCodec(text, new StringCodec());
        return (builder.Build(), integer, text);
    }

    /// <summary>
    /// The <c>baml.errors.HostCallable</c> the engine builds from a raised
    /// exception, as it reaches the host when the throw escapes the call.
    /// </summary>
    private static BamlOutboundResult ToOutboundError(HostException raised)
    {
        var value = new BamlValueClass { Name = "baml.errors.HostCallable" };
        void Field(string key, BamlOutboundValue outbound) =>
            value.Fields.Add(new BamlOutboundMapEntry { Key = key, Value = outbound });
        Field("message", new BamlOutboundValue { StringValue = raised.Message });
        Field("class_name", new BamlOutboundValue { StringValue = raised.ClassName });
        Field("language", new BamlOutboundValue { StringValue = raised.Language });
        Field(
            "traceback",
            raised.HasTraceback
                ? new BamlOutboundValue { StringValue = raised.Traceback }
                : new BamlOutboundValue());
        Field(
            "_handle",
            new BamlOutboundValue
            {
                HandleValue = new BamlOutboundHandle
                {
                    Key = raised.Handle,
                    HandleType = BamlHandleType.HostValueOpaque,
                },
            });

        return new BamlOutboundResult
        {
            Error = new BamlOutboundError
            {
                Value = new BamlOutboundValue { ClassValue = value },
            },
        };
    }

    private static OperationCanceledException CaptureOriginalException(
        CancellationToken cancellationToken)
    {
        try
        {
            ThrowOriginalException(cancellationToken);
        }
        catch (OperationCanceledException error)
        {
            return error;
        }

        throw new InvalidOperationException("unreachable");
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static void ThrowOriginalException(CancellationToken cancellationToken) =>
        throw new OperationCanceledException("host sentinel", cancellationToken);

    /// <summary>
    /// The call's arguments as the dispatcher receives them. These payloads
    /// carry scalars only, so the ownership scope has nothing to release.
    /// </summary>
    private static HostCallArguments Arguments(BamlToHostCall call, NativeApi api) =>
        HostCallArguments.Parse(call.ToByteArray(), api);

    private static BamlToHostCall Call(params BamlToHostArg[] arguments)
    {
        var call = new BamlToHostCall();
        call.Args.Add(arguments);
        return call;
    }

    private static BamlToHostArg RequiredInt(long value) =>
        new() { Value = new BamlOutboundValue { IntValue = value } };

    private static BamlToHostArg OptionalInt(string name, long value) =>
        new()
        {
            ArgName = name,
            IsOptionalArg = true,
            Value = new BamlOutboundValue { IntValue = value },
        };

    private static BamlToHostArg OptionalString(string name, string value) =>
        new()
        {
            ArgName = name,
            IsOptionalArg = true,
            Value = new BamlOutboundValue { StringValue = value },
        };

    private static void ResetCompletions()
    {
        while (Completions.TryDequeue(out _))
        {
        }
        Volatile.Write(ref completionSignal, NewSignal());
    }

    private static async Task<HostCompletion> NextCompletion()
    {
        HostCompletion completion;
        while (!Completions.TryDequeue(out completion))
        {
            TaskCompletionSource signal = Volatile.Read(ref completionSignal);
            await signal.Task.WaitAsync(TimeSpan.FromSeconds(5));
            Interlocked.CompareExchange(ref completionSignal, NewSignal(), signal);
        }

        return completion;
    }

    private static async Task WaitUntil(Func<bool> predicate)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(5));
        while (!predicate())
        {
            await Task.Delay(1, timeout.Token);
        }
    }

    private static TaskCompletionSource NewSignal() =>
        new(TaskCreationOptions.RunContinuationsAsynchronously);

    private static void Expect<T>(Action action)
        where T : Exception
    {
        try
        {
            action();
        }
        catch (T)
        {
            return;
        }

        throw new InvalidOperationException($"expected {typeof(T).Name}");
    }

    private static void Require(bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidOperationException(message);
        }
    }

    private static unsafe void Dispatch(
        ulong hostKey,
        uint hostCallId,
        byte[] payload)
    {
        fixed (byte* pointer = payload)
        {
            NativeCallbacks.HostDispatchPointer(
                hostKey,
                hostCallId,
                pointer,
                (nuint)payload.Length);
        }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static unsafe void CompleteHostCall(
        uint callId,
        byte* content,
        nuint length)
    {
        byte[] bytes = length == 0
            ? []
            : new ReadOnlySpan<byte>(content, checked((int)length)).ToArray();
        Completions.Enqueue(new HostCompletion(callId, bytes));
        Volatile.Read(ref completionSignal).TrySetResult();
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static unsafe void RegisterResult(
        delegate* unmanaged[Cdecl]<uint, byte*, nuint, void> callback)
    {
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static unsafe void RegisterHostDispatchV2(
        delegate* unmanaged[Cdecl]<byte*, nuint, void> callback)
    {
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static unsafe void RegisterHostCancel(
        delegate* unmanaged[Cdecl]<uint, void> callback)
    {
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static unsafe void RegisterHostDispatch(
        delegate* unmanaged[Cdecl]<ulong, uint, byte*, nuint, void> callback)
    {
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static unsafe void RegisterHostRelease(
        delegate* unmanaged[Cdecl]<ulong, void> callback)
    {
    }

    private sealed class IntCodec : IBamlGeneratedCodec<long>
    {
        public BamlGeneratedValue Encode(BamlGeneratedCodecContext context, long value) =>
            context.Int(value);

        public long Decode(BamlGeneratedCodecContext context, BamlGeneratedValue value) =>
            context.ReadInt(value);
    }

    private sealed class StringCodec : IBamlGeneratedCodec<string>
    {
        public BamlGeneratedValue Encode(BamlGeneratedCodecContext context, string value) =>
            context.String(value);

        public string Decode(BamlGeneratedCodecContext context, BamlGeneratedValue value) =>
            context.ReadString(value);
    }

    private sealed class FloatCodec : IBamlGeneratedCodec<double>
    {
        public BamlGeneratedValue Encode(BamlGeneratedCodecContext context, double value) =>
            context.Float(value);

        public double Decode(BamlGeneratedCodecContext context, BamlGeneratedValue value) =>
            context.ReadFloat(value);
    }

    private sealed class DynamicCodec : IBamlGeneratedCodec<BamlValue>
    {
        public BamlGeneratedValue Encode(BamlGeneratedCodecContext context, BamlValue value) =>
            context.Value(value);

        public BamlValue Decode(BamlGeneratedCodecContext context, BamlGeneratedValue value) =>
            new(value);
    }

    private sealed class SyncSentinelException : Exception
    {
    }

    private sealed unsafe class NativeFixture : IDisposable
    {
        private readonly BamlApiV1* table;

        internal NativeFixture()
        {
            table = (BamlApiV1*)NativeMemory.AllocZeroed((nuint)sizeof(BamlApiV1));
            *table = new BamlApiV1
            {
                AbiVersion = 5,
                StructSize = (nuint)sizeof(BamlApiV1),
                RegisterCallback = &RegisterResult,
                RegisterHostDispatchCallback = &RegisterHostDispatch,
                RegisterHostDispatchV2 = &RegisterHostDispatchV2,
                RegisterHostCancelCallback = &RegisterHostCancel,
                RegisterHostReleaseCallback = &RegisterHostRelease,
                CompleteHostCall = &CompleteHostCall,
            };
            Api = new NativeApi(table, "host-callable-test");
        }

        internal NativeApi Api { get; }

        public void Dispose() => NativeMemory.Free(table);
    }

    private readonly record struct HostCompletion(uint CallId, byte[] Bytes)
    {
        internal HostCallCompletion Outcome => HostCallCompletion.Parser.ParseFrom(Bytes);
    }
}
