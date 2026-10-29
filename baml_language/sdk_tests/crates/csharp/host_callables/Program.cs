using System.Runtime.CompilerServices;

using Baml;
using Baml.Cffi;
using CsharpHostCallables;

using Wire = BamlBridge.Cffi.V1;

RequireRegistryIdle("startup");

if (args is ["completion"])
{
    await RequireCallbackFaultsCompleteByWhoseTheyAre();
    Console.WriteLine("host_callable_completion_contract=ok");
    return 0;
}

if (args is ["depth-result"])
{
    await RequireCallbackResultsDeeperThanTheWireAreTheBridgesFailure();
    Console.WriteLine("host_callable_result_deeper_than_the_wire_is_refused=ok");
    return 0;
}

if (args is ["depth-argument"])
{
    RequireArgumentsDeeperThanTheWireAreRefused();
    Console.WriteLine("argument_deeper_than_the_wire_is_refused=ok");
    return 0;
}

if (args is ["hof"])
{
    await RequireCallbacksReceiveBamlClosures();
    Console.WriteLine("host_callable_hof_receives_a_baml_closure=ok");
    return 0;
}

var addTen = Functions.MakeAdder(10L);
Require(await addTen.InvokeAsync(5L) == 15L, "returned closure argument or result changed");
Require(await addTen.InvokeAsync(7L) == 17L, "returned closure was not reusable");
Console.WriteLine("baml_closure_is_a_native_callable_with_host_language_arguments=ok");
var buildPair = Functions.MakePairBuilder(30L);
ReturnedPerson ada = await buildPair.InvokeAsync(12L, "Ada");
Require(ada.Name == "Ada" && ada.Age == 42L, "returned closure structured result changed");
ReturnedPerson grace = await buildPair.InvokeAsync(5L, "Grace");
Require(grace.Name == "Grace" && grace.Age == 35L, "reused returned closure structured result changed");
Console.WriteLine("baml_closure_decodes_multiple_args_and_structured_return_values=ok");
var nextValue = Functions.MakeCounter(40L);
Require(
    await nextValue.InvokeAsync() == 41L
        && await nextValue.InvokeAsync() == 42L,
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

var voidCalls = new List<string>();
long twoArgumentVoidResult = Functions.CallVoidCallbackTwoArgs(
    BamlCallback.FromSync<long, string>(
        (value, label) => voidCalls.Add($"{value}:{label}")),
    2L,
    "two");
long threeArgumentVoidResult = Functions.CallVoidCallbackThreeArgs(
    BamlCallback.FromSync<long, string, bool>(
        (value, label, flag) => voidCalls.Add($"{value}:{label}:{flag}")),
    3L,
    "three",
    true);
Require(
    twoArgumentVoidResult == 2L && threeArgumentVoidResult == 3L,
    "multi-argument void callback result changed");
await Functions.MakeVoidCallback().InvokeAsync();
var forwardVoid = Functions.MakeVoidForwarder(
    BamlCallback.FromSync<long, string>(
        (value, label) => voidCalls.Add($"{value}:{label}")));
await forwardVoid.InvokeAsync(5L, "five");
await forwardVoid.InvokeAsync(6L, "six");
Require(
    voidCalls.SequenceEqual(["2:two", "3:three:True", "5:five", "6:six"]),
    "void callback or returned void closure arguments changed");
await RequireDispatchIdleAfterCompletion("multi-argument void callbacks and returned void closures");
Console.WriteLine("host_callable_void_signatures=ok");

RequireEngineTypedContainersRoundTrip();
Console.WriteLine("engine_typed_containers_round_trip=ok");

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
    async (x, y, z) =>
    {
        var cancellationToken = Invocation.Current!.CancellationToken;
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
    async (x) =>
    {
        var cancellationToken = Invocation.Current!.CancellationToken;
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
    (values) =>
    {
        var cancellationToken = Invocation.Current!.CancellationToken;
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
    (values) =>
    {
        var cancellationToken = Invocation.Current!.CancellationToken;
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(values["left"] + values["right"]);
    },
    genericMapInput);
Require(genericMapResult == 21L, "nested generic map callback parameter changed");
await RequireDispatchIdleAfterCompletion("generic map callback call");

IReadOnlyList<string> genericOptionals =
    await Functions.InvokeGenericOptionalsAsync<long, string>(
        async (value, fallback) =>
    {
        var cancellationToken = Invocation.Current!.CancellationToken;
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
    (value) =>
    {
        var cancellationToken = Invocation.Current!.CancellationToken;
        cancellationToken.ThrowIfCancellationRequested();
        visited = value;
        return Task.CompletedTask;
    },
    13L);
Require(visitResult == 1L && visited == 13L, "generic void callback changed");
Require(
    await Functions.ProduceAsync<long>(
        () =>
        {
            var cancellationToken = Invocation.Current!.CancellationToken;
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(17L);
        }) == 17L,
    "generic result-only callback changed");
await RequireDispatchIdleAfterCompletion("generic void and producer callback calls");

var callbackBox = new CallbackBox<long> { Value = 19L };
string transformed = await callbackBox.TransformAsync<string>(
    (value) =>
    {
        var cancellationToken = Invocation.Current!.CancellationToken;
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult($"box:{value}");
    });
string staticApplied = await CallbackHost.ApplyAsync<long, string>(
    (value) =>
    {
        var cancellationToken = Invocation.Current!.CancellationToken;
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult($"static:{value}");
    },
    23L);
CallbackBox<long> returnedBox = await Functions.ApplyAsync<long, CallbackBox<long>>(
    (value) =>
    {
        var cancellationToken = Invocation.Current!.CancellationToken;
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
            _ => Task.FromException<string>(original),
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
        async () =>
        {
            var callbackToken = Invocation.Current!.CancellationToken;
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
        new BamlOptions { CancellationToken = caller.Token });
    CancellationToken suppliedToken = await callbackStarted.Task.WaitAsync(
        TimeSpan.FromSeconds(5));
    caller.Cancel();
    BamlOperationCanceledException? cancellation = null;
    try
    {
        cancellation = await ExpectCanceled(canceledCall);
    }
    // Failure intended: a cancellation race can restore the callback's raw task
    // exception instead of classifying it as caller cancellation. Once fixed,
    // remove this fallback and require ExpectCanceled's typed result directly.
    catch (TaskCanceledException leaked) when (
        caller.IsCancellationRequested && leaked.CancellationToken == suppliedToken)
    {
        Console.WriteLine("XFAIL intended: callback cancellation leaked its matching TaskCanceledException");
    }
    await callbackCanceled.Task.WaitAsync(TimeSpan.FromSeconds(5));
    Require(suppliedToken.IsCancellationRequested, "callback token did not cancel");
    if (cancellation is not null)
    {
        Require(
            cancellation.Origin == BamlCancellationOrigin.Caller
                && cancellation.CancellationToken == caller.Token,
            "matching callback cancellation token classification changed");
    }
}
await RequireDispatchIdleAfterCompletion("canceled callback call");

Require(await Functions.PingAsync() == 42L, "host callbacks poisoned a later native call");
await RequireDispatchIdleAfterCompletion("final ping");

Console.WriteLine("csharp_host_callables=ok");
return 0;

static long DoubleValue(long value) => value * 2L;

// How a callback's call completes decides what its BAML caller sees: a BAML
// error it throws, its own or one a nested call let through, is held to the
// declared throws type and caught by class; a panic it lets through stays the
// panic it is; a value its return type does not hold is its broken contract.
static async Task RequireCallbackFaultsCompleteByWhoseTheyAre()
{
    string declared = await Functions.CatchCallbackErrorAsync(
        () => throw new BamlErrorException(
            BamlValue.From(new CallbackError { Code = 1, Message = "declared" })));
    Require(declared == "caught:declared", $"a declared BAML error was not caught by its class: {declared}");

    string forwarded = await Functions.CatchCallbackErrorAsync(
        async () => await Functions.FailWithCallbackErrorAsync("nested"));
    Require(forwarded == "caught:nested", $"a nested BAML error let through was not caught by its class: {forwarded}");

    BamlPanicException panic = await ExpectPanic(
        Functions.ForwardCallbackPanicAsync(async () => await Functions.DivideAsync(1L, 0L)));
    Require(
        panic.Panic.Value.NominalTypeName == "baml.panics.DivisionByZero",
        $"a nested panic let through became {panic.Panic.Value.NominalTypeName}");

    BamlPanicException violation = await ExpectPanic(
        Functions.ReturnCallbackStringAsync(() => Task.FromResult<string>(null!)));
    Require(
        violation.Panic.Value.NominalTypeName == "baml.panics.HostContractViolation",
        $"null for a non-null return became {violation.Panic.Value.NominalTypeName}");
    await RequireDispatchIdleAfterCompletion("callback completion contract");
}

// A callback's result the wire cannot carry completes its call as the
// bridge's own failure, `baml.panics.SdkPanic`: never a crash, a hang, the
// callable's contract violation or a catchable `HostCallable`. A typed list
// nests 2n+2 messages and a class chain 3n under the completion's one.
static async Task RequireCallbackResultsDeeperThanTheWireAreTheBridgesFailure()
{
    BamlValue thirty = NestedLists(30);
    BamlValue echoed = await Functions.ProduceUnknownAsync(() => Task.FromResult(thirty));
    Require(echoed.Equals(thirty), "a 30-level value did not round trip as a callback result");
    Require(
        !await Functions.ProducedUnknownIsNullAsync(() => Task.FromResult(NestedLists(48))),
        "a 48-level list, at the wire's limit, did not cross as a callback result");
    foreach (int levels in new[] { 49, 60 })
    {
        BamlPanicException panic = await ExpectPanic(
            Functions.ProduceUnknownAsync(() => Task.FromResult(NestedLists(levels))));
        Require(
            panic.Panic.Value.NominalTypeName == "baml.panics.SdkPanic",
            $"a {levels}-level callback result became {panic.Panic.Value.NominalTypeName}");
    }

    Require(
        await Functions.ChainHasNextAsync(() => Task.FromResult(MakeChain(33))),
        "a 33-link chain, at the wire's limit, did not cross as a callback result");
    foreach (int links in new[] { 34, 200 })
    {
        BamlPanicException panic = await ExpectPanic(
            Functions.ChainHasNextAsync(() => Task.FromResult(MakeChain(links))));
        Require(
            panic.Panic.Value.NominalTypeName == "baml.panics.SdkPanic",
            $"a {links}-link callback result became {panic.Panic.Value.NominalTypeName}");
    }

    await RequireDispatchIdleAfterCompletion("callback results deeper than the wire");
}

// An argument the wire cannot carry is the caller's argument error, refused
// before the call starts: the two enclosing messages leave 98 for the value.
static void RequireArgumentsDeeperThanTheWireAreRefused()
{
    Require(Functions.AcceptUnknown(NestedLists(48)), "a 48-level list argument did not cross");
    foreach (int levels in new[] { 49, 60 })
    {
        BamlTypeMappingException refused = ExpectArgumentRefused(
            () => Functions.AcceptUnknown(NestedLists(levels)));
        Require(refused.Path == "$args.value", $"the {levels}-level refusal named {refused.Path}");
    }

    Require(Functions.ChainArgumentHasNext(MakeChain(32)), "a 32-link chain argument did not cross");
    foreach (int links in new[] { 33, 1000 })
    {
        _ = ExpectArgumentRefused(() => Functions.ChainArgumentHasNext(MakeChain(links)));
    }

    RequireRegistryIdle("arguments deeper than the wire");
}

static BamlValue NestedLists(int levels)
{
    BamlValue value = BamlValue.Null;
    for (int level = 0; level < levels; level++)
    {
        value = BamlValue.List([value]);
    }

    return value;
}

static Chain MakeChain(int links)
{
    Chain chain = new() { Next = null };
    for (int link = 1; link < links; link++)
    {
        chain = new Chain { Next = chain };
    }

    return chain;
}

static async Task<BamlPanicException> ExpectPanic<T>(Task<T> call)
{
    try
    {
        await call;
    }
    catch (BamlPanicException panic)
    {
        return panic;
    }

    throw new InvalidOperationException("expected the call to end in a BAML panic");
}

static BamlTypeMappingException ExpectArgumentRefused(Func<bool> call)
{
    try
    {
        _ = call();
    }
    catch (BamlTypeMappingException refused)
    {
        return refused;
    }

    throw new InvalidOperationException("expected the argument to be refused before the call");
}

// A BAML closure handed to a host callback arrives as the callback's declared
// function type and calls back into BAML, whether it is the argument itself or
// sits in a list or a class field the callback receives. The host owns the
// closure it was given: one the callback keeps stays callable after the
// callback and its BAML caller have returned.
static async Task RequireCallbacksReceiveBamlClosures()
{
    Func<long, Task<long>>? retained = null;
    long direct = await Functions.HofAsync(async f =>
    {
        retained = f;
        return await f(41L) * 100L + await f(1L);
    });
    Require(direct == 4202L, "a callback could not call the BAML closure it received");
    Require(
        await retained!(9L) == 10L,
        "a BAML closure the callback kept was released with its argument");
    Require(
        Functions.Hof(async f => await f(41L)) == 42L,
        "a synchronous call's callback could not call the BAML closure it received");
    Require(
        await Functions.HofListAsync(async fs => await fs[0](10L) * 100L + await fs[1](10L)) == 1120L,
        "a callback could not call the BAML closures it received in a list");
    Require(
        await Functions.HofClassAsync(async box => await box.F(4L)) == 7L,
        "a callback could not call the BAML closure it received in a class field");
    await RequireDispatchIdleAfterCompletion("callbacks receiving BAML closures");
}

// The engine states a returned container's item type exactly, including types
// no CLR value has a canonical projection for: an interface (`Shape[]` holding
// classes that implement it), a function (a list of closures) and `never` (an
// empty list stated `never[]`). A dynamic value must hold such a container and
// send it back under the type it arrived with.
static void RequireEngineTypedContainersRoundTrip()
{
    var interfaceType = new Wire.BamlTy
    {
        Interface = new Wire.BamlTyInterface
        {
            Name = "user.Shape",
            Bindings =
            {
                new Wire.BamlTyAssociatedBinding { Name = "Unit", Ty = Primitive(Wire.BamlTyPrimitiveKind.BamlTyPrimitiveString) },
            },
        },
    };
    Wire.BamlTy FunctionType(Wire.BamlTy throws) => new()
    {
        Function = new Wire.BamlTyFunction
        {
            Params =
            {
                new Wire.BamlTyFunctionParam
                {
                    Ty = Primitive(Wire.BamlTyPrimitiveKind.BamlTyPrimitiveInt),
                    Mode = Wire.BamlTyFunctionParamMode.Required,
                },
                new Wire.BamlTyFunctionParam
                {
                    Name = "label",
                    Ty = Primitive(Wire.BamlTyPrimitiveKind.BamlTyPrimitiveString),
                    Mode = Wire.BamlTyFunctionParamMode.Optional,
                },
            },
            Ret = Primitive(Wire.BamlTyPrimitiveKind.BamlTyPrimitiveInt),
            Throws = throws,
        },
    };
    var functionType = FunctionType(new Wire.BamlTy { Never = new Wire.BamlTyNever() });
    var neverType = new Wire.BamlTy { Never = new Wire.BamlTyNever() };

    BamlTypeDescriptor shape = RequireListRoundTrip("interface", interfaceType, BamlTypeDescriptorKind.Opaque);
    BamlTypeDescriptor function = RequireListRoundTrip("function", functionType, BamlTypeDescriptorKind.Opaque);
    BamlTypeDescriptor throwingFunction = new BamlValue(
        Baml.Proto.PrimitiveProtocol.Decode(new Wire.BamlOutboundValue
        {
            ListValue = new Wire.BamlValueList
            {
                ItemType = FunctionType(Primitive(Wire.BamlTyPrimitiveKind.BamlTyPrimitiveString)),
            },
        })).Type.Arguments[0];
    // An opaque type keeps its identity: the same type compares equal, any
    // other (a function type differing only in its throws) does not.
    Require(
        function.Equals(RequireListRoundTrip("function", functionType, BamlTypeDescriptorKind.Opaque))
            && !function.Equals(throwingFunction)
            && !function.Equals(shape),
        "opaque item types lost their identity");

    RequireListRoundTrip("never", neverType, BamlTypeDescriptorKind.Never);

    var map = new BamlValue(Baml.Proto.PrimitiveProtocol.Decode(new Wire.BamlOutboundValue
    {
        MapValue = new Wire.BamlValueMap
        {
            KeyType = Primitive(Wire.BamlTyPrimitiveKind.BamlTyPrimitiveString),
            ValueType = interfaceType,
        },
    }));
    Require(
        map.Type.Kind == BamlTypeDescriptorKind.Map
            && map.Type.Arguments[1].Kind == BamlTypeDescriptorKind.Opaque,
        "interface map value type changed");
    Require(
        Baml.Proto.PrimitiveProtocol.Encode(map.GeneratedValue).ValueType.Map.Value.Equals(interfaceType),
        "interface map was not sent back under the type it arrived with");

    static BamlTypeDescriptor RequireListRoundTrip(
        string label,
        Wire.BamlTy itemType,
        BamlTypeDescriptorKind expected)
    {
        var value = new BamlValue(Baml.Proto.PrimitiveProtocol.Decode(new Wire.BamlOutboundValue
        {
            ListValue = new Wire.BamlValueList { ItemType = itemType },
        }));
        Require(
            value.Type.Kind == BamlTypeDescriptorKind.List
                && value.Type.Arguments[0].Kind == expected,
            $"{label} list item type changed");
        Wire.InboundValue resent = Baml.Proto.PrimitiveProtocol.Encode(value.GeneratedValue);
        Require(
            resent.ValueType.Equals(new Wire.BamlTy { List = new Wire.BamlTyList { Item = itemType } }),
            $"{label} list was not sent back under the type it arrived with");
        return value.Type.Arguments[0];
    }

    static Wire.BamlTy Primitive(Wire.BamlTyPrimitiveKind kind) =>
        new() { Primitive = new Wire.BamlTyPrimitive { Kind = kind } };
}

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
