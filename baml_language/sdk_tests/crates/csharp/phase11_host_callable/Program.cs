using System.Runtime.CompilerServices;

using Baml;
using Baml.Cffi;
using CsharpPhase11;

RequireRegistryIdle("startup");

Func<long, CancellationToken, Task<long>> addTen = Functions.MakeAdder(10L);
Require(await addTen(5L, CancellationToken.None) == 15L, "returned closure argument or result changed");
Require(await addTen(7L, CancellationToken.None) == 17L, "returned closure was not reusable");
Console.WriteLine("baml_closure_is_a_native_callable_with_host_language_arguments=ok");
Func<long, string, CancellationToken, Task<ReturnedPerson>> buildPair = Functions.MakePairBuilder(30L);
ReturnedPerson ada = await buildPair(12L, "Ada", CancellationToken.None);
Require(ada.Name == "Ada" && ada.Age == 42L, "returned closure structured result changed");
ReturnedPerson grace = await buildPair(5L, "Grace", CancellationToken.None);
Require(grace.Name == "Grace" && grace.Age == 35L, "reused returned closure structured result changed");
Console.WriteLine("baml_closure_decodes_multiple_args_and_structured_return_values=ok");
Func<CancellationToken, Task<long>> nextValue = Functions.MakeCounter(40L);
Require(
    await nextValue(CancellationToken.None) == 41L
        && await nextValue(CancellationToken.None) == 42L,
    "returned closure mutable capture changed");
Console.WriteLine("baml_closure_is_reusable_and_retains_mutable_captures=ok");

long synchronousLambda = Functions.InvokeDeferred(
    BamlCallback.FromSync<long, long>(value => value * 2L),
    5L);
Require(synchronousLambda == 11L, "synchronous callback lambda result changed");
await RequireDispatchIdleAfterCompletion("synchronous callback lambda");

long synchronousMethodGroup = Functions.InvokeDeferred(
    BamlCallback.FromSync<long, long>(DoubleValue),
    6L);
Require(synchronousMethodGroup == 13L, "synchronous callback method group result changed");
await RequireDispatchIdleAfterCompletion("synchronous callback method group");

long synchronousVisited = 0L;
long synchronousVisitResult = Functions.Visit<long>(
    BamlCallback.FromSync<long>(value => synchronousVisited = value),
    13L);
Require(
    synchronousVisitResult == 1L && synchronousVisited == 13L,
    "synchronous Action<T> callback changed");

int zeroVoidCalls = 0;
long zeroVoidResult = Functions.InvokeZeroVoid(
    BamlCallback.FromSync(() => zeroVoidCalls++));
long zeroValueResult = Functions.Produce<long>(
    BamlCallback.FromSync<long>(() => 17L));
Require(
    zeroVoidResult == 1L && zeroVoidCalls == 1 && zeroValueResult == 17L,
    "zero-argument synchronous value or void callback changed");
await RequireDispatchIdleAfterCompletion("synchronous void and zero-argument callbacks");

int synchronousOptionalCalls = 0;
IReadOnlyList<string> synchronousOptionals = Functions.InvokeOptionals(
    BamlCallback.FromSync<
        long,
        BamlOptional<long>,
        BamlOptional<string>,
        string>((x, y, z) =>
        {
            synchronousOptionalCalls++;
            string yState = y.IsSet ? y.Value.ToString() : "unset";
            string zState = z.IsSet ? z.Value : "unset";
            return $"{x}:{yState}:{zState}";
        }),
    7L);
Require(
    synchronousOptionalCalls == 3
        && synchronousOptionals.SequenceEqual(
            new[] { "7:unset:unset", "7:unset:tail", "7:2:both" },
            StringComparer.Ordinal),
    "BamlOptional callback arguments changed through the synchronous adapter");
await RequireDispatchIdleAfterCompletion("synchronous optional callback calls");

int optionalCalls = 0;
IReadOnlyList<string> optionals = await Functions.InvokeOptionalsAsync(
    async (x, y, z, cancellationToken) =>
    {
        await Task.Yield();
        cancellationToken.ThrowIfCancellationRequested();
        Interlocked.Increment(ref optionalCalls);
        string yState = y.IsSet ? y.Value.ToString() : "unset";
        string zState = z.IsSet ? z.Value : "unset";
        return $"{x}:{yState}:{zState}";
    },
    7L);
Require(
    optionalCalls == 3
        && optionals.SequenceEqual(
            new[] { "7:unset:unset", "7:unset:tail", "7:2:both" },
            StringComparer.Ordinal),
    "required and named optional callback arguments changed");
await RequireDispatchIdleAfterCompletion("optional callback calls");

var deferredStarted = new TaskCompletionSource(
    TaskCreationOptions.RunContinuationsAsynchronously);
var releaseDeferred = new TaskCompletionSource(
    TaskCreationOptions.RunContinuationsAsynchronously);
Task<long> deferredCall = Functions.InvokeDeferredAsync(
    async (x, cancellationToken) =>
    {
        deferredStarted.TrySetResult();
        await releaseDeferred.Task.WaitAsync(cancellationToken);
        return x * 2;
    },
    5L);
await deferredStarted.Task.WaitAsync(TimeSpan.FromSeconds(5));
Require(!deferredCall.IsCompleted, "native dispatch did not await the asynchronous callback");
releaseDeferred.TrySetResult();
Require(await deferredCall == 11L, "asynchronous callback result changed");
await RequireDispatchIdleAfterCompletion("deferred callback call");

long applied = await Functions.ApplyAsync<long, long>(
    BamlCallback.FromSync<long, long>(value => value + 1L),
    5L);
Require(applied == 6L, "explicitly closed generic callback result changed");
await RequireDispatchIdleAfterCompletion("generic apply callback call");

var nominalInput = new CallbackBox<long> { Value = 31L };
CallbackBox<long> nominalResult = Functions.Apply<CallbackBox<long>, CallbackBox<long>>(
    BamlCallback.FromSync<CallbackBox<long>, CallbackBox<long>>(
        value => new CallbackBox<long> { Value = value.Value + 1L }),
    nominalInput);
Require(
    nominalResult.Value == 32L,
    "generated nominal callback parameter or result changed through the synchronous adapter");
await RequireDispatchIdleAfterCompletion("synchronous nominal callback call");

IReadOnlyList<long> genericInput = Array.AsReadOnly([2L, 3L, 5L]);
string genericListResult = await Functions.ApplyAsync<IReadOnlyList<long>, string>(
    (values, cancellationToken) =>
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(string.Join(",", values));
    },
    genericInput);
Require(genericListResult == "2,3,5", "nested generic callback parameter changed");
await RequireDispatchIdleAfterCompletion("generic list callback call");

IReadOnlyDictionary<string, long> genericMapInput = new Dictionary<string, long>
{
    ["left"] = 8L,
    ["right"] = 13L,
};
long genericMapResult = await Functions.ApplyAsync<IReadOnlyDictionary<string, long>, long>(
    (values, cancellationToken) =>
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(values["left"] + values["right"]);
    },
    genericMapInput);
Require(genericMapResult == 21L, "nested generic map callback parameter changed");
await RequireDispatchIdleAfterCompletion("generic map callback call");

IReadOnlyList<string> genericOptionals =
    await Functions.InvokeGenericOptionalsAsync<long, string>(
        async (value, fallback, cancellationToken) =>
        {
            await Task.Yield();
            cancellationToken.ThrowIfCancellationRequested();
            return fallback.IsSet
                ? $"{value}:{fallback.Value}"
                : $"{value}:unset";
        },
        7L,
        11L);
Require(
    genericOptionals.SequenceEqual(
        new[] { "7:unset", "7:11" },
        StringComparer.Ordinal),
    "generic callback optional wire identities changed");
await RequireDispatchIdleAfterCompletion("generic optional callback calls");

long visited = 0L;
long visitResult = await Functions.VisitAsync<long>(
    (value, cancellationToken) =>
    {
        cancellationToken.ThrowIfCancellationRequested();
        visited = value;
        return Task.CompletedTask;
    },
    13L);
Require(visitResult == 1L && visited == 13L, "generic void callback changed");
Require(
    await Functions.ProduceAsync<long>(
        cancellationToken =>
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(17L);
        }) == 17L,
    "generic result-only callback changed");
await RequireDispatchIdleAfterCompletion("generic void and producer callback calls");

var callbackBox = new CallbackBox<long> { Value = 19L };
string transformed = await callbackBox.TransformAsync<string>(
    (value, cancellationToken) =>
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult($"box:{value}");
    });
string staticApplied = await CallbackHost.ApplyAsync<long, string>(
    (value, cancellationToken) =>
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult($"static:{value}");
    },
    23L);
CallbackBox<long> returnedBox = await Functions.ApplyAsync<long, CallbackBox<long>>(
    (value, cancellationToken) =>
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(new CallbackBox<long> { Value = value + 1L });
    },
    29L);
Require(
    transformed == "box:19"
        && staticApplied == "static:23"
        && returnedBox.Value == 30L,
    "generic callback class method or generated result codec changed");
await RequireDispatchIdleAfterCompletion("generic callback method calls");

var synchronousOriginal = new InvalidOperationException("synchronous host sentinel");
string ThrowSynchronousHostSentinel(long _) => throw synchronousOriginal;
Exception synchronousRestored = ExpectSynchronousFault(
    () => Functions.PropagateHostThrow(
        BamlCallback.FromSync<long, string>(ThrowSynchronousHostSentinel),
        9L));
Require(
    ReferenceEquals(synchronousRestored, synchronousOriginal)
        && synchronousRestored.StackTrace?.Contains(
            nameof(ThrowSynchronousHostSentinel),
            StringComparison.Ordinal) == true,
    "synchronously thrown callback exception did not restore the exact managed object and stack");
await RequireDispatchIdleAfterCompletion("synchronously throwing callback call");

using (var unrelatedCancellation = new CancellationTokenSource())
{
    unrelatedCancellation.Cancel();
    OperationCanceledException original = CaptureUnrelatedCancellation(
        unrelatedCancellation.Token);
    Exception restored = ExpectSynchronousFault(
        () => Functions.PropagateHostThrow(
            (_, _) => Task.FromException<string>(original),
            9L));
    Require(
        ReferenceEquals(restored, original)
            && restored.StackTrace?.Contains(
                nameof(ThrowUnrelatedCancellation),
                StringComparison.Ordinal) == true,
        "unrelated cancellation did not restore the exact managed exception and stack");
}
await RequireDispatchIdleAfterCompletion("throwing callback call");

using (var caller = new CancellationTokenSource())
{
    var callbackStarted = new TaskCompletionSource<CancellationToken>(
        TaskCreationOptions.RunContinuationsAsynchronously);
    var callbackCanceled = new TaskCompletionSource(
        TaskCreationOptions.RunContinuationsAsynchronously);
    Task<string> canceledCall = Functions.InvokeCancelableAsync(
        async callbackToken =>
        {
            callbackStarted.TrySetResult(callbackToken);
            try
            {
                await Task.Delay(Timeout.InfiniteTimeSpan, callbackToken);
                return "unreachable";
            }
            finally
            {
                callbackCanceled.TrySetResult();
            }
        },
        caller.Token);
    CancellationToken suppliedToken = await callbackStarted.Task.WaitAsync(
        TimeSpan.FromSeconds(5));
    caller.Cancel();
    BamlOperationCanceledException cancellation = await ExpectCanceled(canceledCall);
    await callbackCanceled.Task.WaitAsync(TimeSpan.FromSeconds(5));
    Require(
        suppliedToken.IsCancellationRequested
            && cancellation.Origin == BamlCancellationOrigin.Caller
            && cancellation.CancellationToken == caller.Token,
        "matching callback cancellation token classification changed");
}
await RequireDispatchIdleAfterCompletion("canceled callback call");

Require(await Functions.PingAsync() == 42L, "host callbacks poisoned a later native call");
await RequireDispatchIdleAfterCompletion("final ping");

Console.WriteLine("csharp_phase11_host_callable=ok");
return 0;

static long DoubleValue(long value) => value * 2L;

static OperationCanceledException CaptureUnrelatedCancellation(
    CancellationToken cancellationToken)
{
    try
    {
        ThrowUnrelatedCancellation(cancellationToken);
    }
    catch (OperationCanceledException exception)
    {
        return exception;
    }

    throw new InvalidOperationException("unreachable");
}

[MethodImpl(MethodImplOptions.NoInlining)]
static void ThrowUnrelatedCancellation(CancellationToken cancellationToken) =>
    throw new OperationCanceledException("unrelated host cancellation", cancellationToken);

static Exception ExpectSynchronousFault(Func<object?> call)
{
    try
    {
        _ = call();
    }
    catch (Exception exception)
    {
        return exception;
    }

    throw new InvalidOperationException("expected host callback fault");
}

static async Task<BamlOperationCanceledException> ExpectCanceled(Task task)
{
    try
    {
        await task;
    }
    catch (BamlOperationCanceledException exception)
    {
        Require(task.IsCanceled, "caller-canceled host callback task was not canceled");
        return exception;
    }

    throw new InvalidOperationException("expected caller cancellation");
}

static async Task RequireDispatchIdleAfterCompletion(string operation)
{
    Require(Functions.Ping() == 42L, $"{operation} safepoint call failed");
    using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(5));
    while (!DispatchIdle())
    {
        await Task.Delay(1, timeout.Token);
    }

    Require(
        DispatchIdle(),
        $"{operation} left dispatch leases or native results pending");
}

// A committed host value remains registered until Canary's ordinary heap GC
// sends its release callback. Managed call completion guarantees that no
// callback invocation or result is still active; it does not force global GC.
static bool DispatchIdle() =>
    HostValueRegistry.Shared.InvocationCount == 0
    && NativeCallbacks.PendingCount == 0;

static bool RegistryIdle() =>
    HostValueRegistry.Shared.EntryCount == 0
    && HostValueRegistry.Shared.InvocationCount == 0
    && NativeCallbacks.PendingCount == 0;

static void RequireRegistryIdle(string operation) => Require(
    RegistryIdle(),
    $"{operation} left host values, dispatch leases, or native results pending");

static void Require(bool condition, string message)
{
    if (!condition)
    {
        throw new InvalidOperationException(message);
    }
}
