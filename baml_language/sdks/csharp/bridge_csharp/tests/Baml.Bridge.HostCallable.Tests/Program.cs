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

internal static class Program
{
    private static readonly AsyncLocal<string?> Ambient = new();
    private static readonly ConcurrentQueue<HostCompletion> Completions = new();
    private static TaskCompletionSource completionSignal = NewSignal();
    private static int callbackThread;

    public static async Task<int> Main()
    {
        await VerifySyncAdapterSurfaceAndBehavior();
        VerifyRegistryLifecycle();
        VerifyStrictOptionalBindingAndArity();
        using NativeFixture native = new();
        NativeCallbacks.Register(native.Api);
        await VerifyAsyncDispatchContextAndBorrowedCopy(native.Api);
        await VerifyCancellationClassification(native.Api);
        await VerifyReentrancy(native.Api);
        await VerifyExactExceptionRoundTrip(native.Api);
        Console.WriteLine("managed_host_callable=ok");
        return 0;
    }

    private static async Task VerifySyncAdapterSurfaceAndBehavior()
    {
        MethodInfo[] methods = typeof(BamlCallback).GetMethods(
            BindingFlags.Public | BindingFlags.Static | BindingFlags.DeclaredOnly);
        Require(methods.Length == 32, "BamlCallback must expose exactly 32 public adapter overloads");
        Require(
            methods.All(method => method.Name == nameof(BamlCallback.FromSync)),
            "BamlCallback exposed a public member outside the canonical FromSync family");

        for (int arity = 0; arity <= 15; arity++)
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
        Func<CancellationToken, Task<long>> zeroValue = BamlCallback.FromSync<long>(() =>
        {
            valueCalls++;
            return 41L;
        });
        Require(valueCalls == 0, "value callback ran while its adapter was constructed");
        using var canceled = new CancellationTokenSource();
        canceled.Cancel();
        Require(
            await zeroValue(canceled.Token) == 41L && valueCalls == 1,
            "zero-argument value adapter observed the injected token or changed its result");

        int voidCalls = 0;
        Func<CancellationToken, Task> zeroVoid = BamlCallback.FromSync(() => voidCalls++);
        Require(voidCalls == 0, "void callback ran while its adapter was constructed");
        await zeroVoid(canceled.Token);
        Require(voidCalls == 1, "zero-argument void adapter did not invoke its callback once");

        Func<long, BamlOptional<string>, CancellationToken, Task<string>> optional =
            BamlCallback.FromSync<long, BamlOptional<string>, string>(
                static (value, suffix) =>
                    suffix.IsSet ? $"{value}:{suffix.Value}" : $"{value}:unset");
        Require(
            await optional(7L, BamlOptional<string>.Unset, canceled.Token) == "7:unset"
                && await optional(7L, BamlOptional<string>.FromValue("tail"), canceled.Token)
                    == "7:tail",
            "BamlOptional did not flow naturally through the synchronous adapter");

        Func<long, long, long, long, long, long, long, long, long, long, long, long, long, long, long, CancellationToken, Task<long>> maxValue =
            BamlCallback.FromSync<long, long, long, long, long, long, long, long, long, long, long, long, long, long, long, long>(
                static (v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v11, v12, v13, v14, v15) =>
                    v1 + v2 + v3 + v4 + v5 + v6 + v7 + v8 + v9 + v10 + v11 + v12 + v13 + v14 + v15);
        Require(
            await maxValue(1L, 2L, 3L, 4L, 5L, 6L, 7L, 8L, 9L, 10L, 11L, 12L, 13L, 14L, 15L, canceled.Token) == 120L,
            "maximum-arity value adapter changed argument order");

        long maxVoidSum = 0L;
        Func<long, long, long, long, long, long, long, long, long, long, long, long, long, long, long, CancellationToken, Task> maxVoid =
            BamlCallback.FromSync<long, long, long, long, long, long, long, long, long, long, long, long, long, long, long>(
                (v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v11, v12, v13, v14, v15) =>
                    maxVoidSum = v1 + v2 + v3 + v4 + v5 + v6 + v7 + v8 + v9 + v10 + v11 + v12 + v13 + v14 + v15);
        await maxVoid(1L, 2L, 3L, 4L, 5L, 6L, 7L, 8L, 9L, 10L, 11L, 12L, 13L, 14L, 15L, canceled.Token);
        Require(maxVoidSum == 120L, "maximum-arity void adapter changed argument order");

        var original = new SyncSentinelException();
        Func<CancellationToken, Task<string>> throwing =
            BamlCallback.FromSync<string>(() => throw original);
        Exception? observed = null;
        try
        {
            _ = throwing(CancellationToken.None);
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
                && returnType.GetGenericTypeDefinition().FullName == $"System.Func`{arity + 2}",
            $"{method} did not return the canonical Func delegate at arity {arity}");
        Type[] arguments = returnType.GetGenericArguments();
        Require(
            arguments[arity] == typeof(CancellationToken),
            $"{method} omitted the injected CancellationToken at arity {arity}");
        Type task = arguments[arity + 1];
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

    private static void VerifyRegistryLifecycle()
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
                new BamlToHostCall().ToByteArray(),
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
                new BamlToHostCall().ToByteArray(),
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

    private static void VerifyStrictOptionalBindingAndArity()
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
            valid.ToByteArray());
        Require(
            (long)bound[0]! == 7
                && ((BamlOptional<string>)bound[1]!).Value == "x"
                && ((BamlOptional<long>)bound[2]!).Value == 9,
            "named optional callback arguments did not bind by wire identity");

        bound = HostCallableProtocol.BindArguments(
            callable.Descriptor,
            Call(RequiredInt(8)).ToByteArray());
        Require(
            !((BamlOptional<string>)bound[1]!).IsSet
                && !((BamlOptional<long>)bound[2]!).IsSet,
            "omitted optional callback arguments did not remain unset");

        Expect<BamlProtocolException>(() => HostCallableProtocol.BindArguments(
            callable.Descriptor,
            Call().ToByteArray()));
        Expect<BamlProtocolException>(() => HostCallableProtocol.BindArguments(
            callable.Descriptor,
            Call(OptionalString("first", "x"), RequiredInt(1)).ToByteArray()));
        Expect<BamlProtocolException>(() => HostCallableProtocol.BindArguments(
            callable.Descriptor,
            Call(RequiredInt(1), OptionalString("first", "x"), OptionalString("first", "y")).ToByteArray()));
        Expect<BamlProtocolException>(() => HostCallableProtocol.BindArguments(
            callable.Descriptor,
            Call(RequiredInt(1), OptionalString("unknown", "x")).ToByteArray()));
        Expect<BamlProtocolException>(() => HostCallableProtocol.BindArguments(
            callable.Descriptor,
            Call(new BamlToHostArg
            {
                ArgName = "not-positional",
                Value = new BamlOutboundValue { IntValue = 1 },
            }).ToByteArray()));

        BamlGeneratedHostParameter[] tooMany = Enumerable.Range(0, 16)
            .Select(_ => context.Required(integer))
            .ToArray();
        Expect<ArgumentOutOfRangeException>(() => context.HostCallable(
            callback,
            tooMany,
            context.Result(text),
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
            completion.CallId == hostCallId && completion.IsError == 0,
            "async host dispatch did not complete as success");
        InboundValue result = InboundValue.Parser.ParseFrom(completion.Bytes);
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
        Require(
            supplied.IsCancellationRequested && Completions.IsEmpty,
            "exact supplied-token cancellation was misclassified as a callback fault");

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
                && first.IsError == 0
                && second.IsError == 0,
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
            completion.CallId == hostCallId && completion.IsError == 1,
            "an unrelated callback exception was misclassified as cancellation");
        InboundValue inbound = InboundValue.Parser.ParseFrom(completion.Bytes);
        BamlBridge.Cffi.V1.BamlHandle exceptionHandle = inbound.ClassValue.Fields
            .Single(field => field.StringKey == "_handle")
            .Value.Handle;
        Require(
            exceptionHandle.HandleType == BamlHandleType.HostValueOpaque,
            "callback exception did not carry an opaque rehydration handle");

        HostValueRegistry.Shared.Release(exceptionHandle.Key);
        BamlOutboundResult outbound = ToOutboundError(inbound);
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

    private static BamlOutboundResult ToOutboundError(InboundValue inbound)
    {
        var value = new BamlValueClass { Name = inbound.ValueType.ClassTy.Name };
        foreach (InboundMapEntry field in inbound.ClassValue.Fields)
        {
            BamlOutboundValue outbound = field.Value.ValueCase switch
            {
                InboundValue.ValueOneofCase.StringValue =>
                    new BamlOutboundValue { StringValue = field.Value.StringValue },
                InboundValue.ValueOneofCase.Handle =>
                    new BamlOutboundValue
                    {
                        HandleValue = new BamlOutboundHandle
                        {
                            Key = field.Value.Handle.Key,
                            HandleType = field.Value.Handle.HandleType,
                        },
                    },
                InboundValue.ValueOneofCase.None => new BamlOutboundValue(),
                _ => throw new InvalidOperationException(
                    $"unsupported HostCallable test field {field.Value.ValueCase}"),
            };
            value.Fields.Add(new BamlOutboundMapEntry
            {
                Key = field.StringKey,
                Value = outbound,
            });
        }

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
        int isError,
        byte* content,
        nuint length)
    {
        byte[] bytes = length == 0
            ? []
            : new ReadOnlySpan<byte>(content, checked((int)length)).ToArray();
        Completions.Enqueue(new HostCompletion(callId, isError, bytes));
        Volatile.Read(ref completionSignal).TrySetResult();
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static unsafe void RegisterResult(
        delegate* unmanaged[Cdecl]<uint, byte*, nuint, void> callback)
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
                AbiVersion = 2,
                StructSize = (nuint)sizeof(BamlApiV1),
                RegisterCallback = &RegisterResult,
                RegisterHostDispatchCallback = &RegisterHostDispatch,
                RegisterHostReleaseCallback = &RegisterHostRelease,
                CompleteHostCall = &CompleteHostCall,
            };
            Api = new NativeApi(table, "host-callable-test");
        }

        internal NativeApi Api { get; }

        public void Dispose() => NativeMemory.Free(table);
    }

    private readonly record struct HostCompletion(uint CallId, int IsError, byte[] Bytes);
}
