using Baml;
using System.Diagnostics;
using System.Linq.Expressions;
using System.Numerics;
using System.Reflection;
using System.Runtime.CompilerServices;

internal static class Program
{
    private const int CopyIterations = 250_000;
    private const int OperationIterations = 100_000;
    private static int _sink;

    private static readonly MethodInfo SizeMethod = GetGenericMethod(nameof(SizeOf), 0, 1);
    private static readonly MethodInfo CopyMethod = GetGenericMethod(nameof(MeasureCopies), 2, 1);
    private static readonly MethodInfo ConstructionMethod = GetGenericMethod(nameof(MeasureConstruction), 3, 2);
    private static readonly MethodInfo MatchMethod = GetGenericMethod(nameof(MeasureMatches), 3, 2);
    private static readonly MethodInfo PayloadConstructionMethod = GetGenericMethod(nameof(MeasurePayloadConstruction), 2, 1);
    private static readonly MethodInfo PayloadMatchMethod = GetGenericMethod(nameof(MeasurePayloadMatches), 2, 1);

    public static void Main()
    {
        VerifySemantics();

        var unionDefinitions = typeof(BamlUnion<,>).Assembly
            .GetTypes()
            .Where(type => type.Namespace == "Baml" && type.Name.StartsWith("BamlUnion`", StringComparison.Ordinal))
            .ToDictionary(type => type.GetGenericArguments().Length);
        var scenarios = new[]
        {
            new Scenario("reference", typeof(string), "payload"),
            new Scenario("primitive", typeof(long), 42L),
            new Scenario("enum", typeof(ProbeEnum), ProbeEnum.Second),
            new Scenario("bigint", typeof(BigInteger), BigInteger.Parse("1208925819614629174706177")),
            new Scenario("class", typeof(ProbeModel), new ProbeModel("payload")),
            new Scenario("mixed", typeof(BigInteger), BigInteger.Parse("1208925819614629174706177"), Mixed: true),
        };

        Console.WriteLine("| Arity | Payload | Fields bytes | Payload/tag bytes | Fields copy ns/op | Payload/tag copy ns/op | Fields construct B/op | Payload/tag construct B/op | Fields match B/op | Payload/tag match B/op |");
        Console.WriteLine("| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
        foreach (var arity in new[] { 2, 8, 16, 32 })
        {
            foreach (var scenario in scenarios)
            {
                var typeArguments = scenario.Mixed
                    ? MixedTypes(arity)
                    : Enumerable.Repeat(scenario.Type, arity).ToArray();
                var unionType = unionDefinitions[arity].MakeGenericType(typeArguments);
                var fromT0 = unionType.GetMethod("FromT0", BindingFlags.Public | BindingFlags.Static)
                    ?? throw new InvalidOperationException($"{unionType} has no FromT0 factory.");
                var union = fromT0.Invoke(null, [scenario.Value])
                    ?? throw new InvalidOperationException($"{unionType}.FromT0 returned null.");
                var factoryType = typeof(Func<,>).MakeGenericType(typeArguments[0], unionType);
                var factory = fromT0.CreateDelegate(factoryType);
                var selector = CreateSelector(unionType, typeArguments[0]);
                var payload = new PayloadUnion(scenario.Value, 1);

                var fieldsSize = (int)SizeMethod.MakeGenericMethod(unionType).Invoke(null, null)!;
                var payloadSize = Unsafe.SizeOf<PayloadUnion>();
                var fieldsCopy = (double)CopyMethod.MakeGenericMethod(unionType)
                    .Invoke(null, [union, CopyIterations])!;
                var payloadCopy = MeasureCopies<PayloadUnion>(payload, CopyIterations);
                var fieldsConstruction = (double)ConstructionMethod
                    .MakeGenericMethod(unionType, typeArguments[0])
                    .Invoke(null, [factory, scenario.Value, OperationIterations])!;
                var payloadConstruction = (double)PayloadConstructionMethod
                    .MakeGenericMethod(typeArguments[0])
                    .Invoke(null, [scenario.Value, OperationIterations])!;
                var fieldsMatch = (double)MatchMethod
                    .MakeGenericMethod(unionType, typeArguments[0])
                    .Invoke(null, [selector, union, OperationIterations])!;
                var payloadMatch = (double)PayloadMatchMethod
                    .MakeGenericMethod(typeArguments[0])
                    .Invoke(null, [payload, OperationIterations])!;

                Console.WriteLine(
                    $"| {arity} | {scenario.Name} | {fieldsSize} | {payloadSize} | {fieldsCopy:F2} | {payloadCopy:F2} | {fieldsConstruction:F2} | {payloadConstruction:F2} | {fieldsMatch:F2} | {payloadMatch:F2} |");
            }
        }
        GC.KeepAlive(_sink);
    }

    private static void VerifySemantics()
    {
        var first = BamlUnion<string, string>.FromT0("same");
        var second = BamlUnion<string, string>.FromT1("same");
        if (!first.IsT0 || !second.IsT1 || first.Equals(second))
        {
            throw new InvalidOperationException("Duplicate closed union arms did not retain distinct cases.");
        }

        var rejectedDefault = false;
        try
        {
            _ = default(BamlUnion<string, long>).AsT0;
        }
        catch (InvalidOperationException)
        {
            rejectedDefault = true;
        }
        if (!rejectedDefault)
        {
            throw new InvalidOperationException("The default union unexpectedly selected its first arm.");
        }
    }

    private static Type[] MixedTypes(int arity)
    {
        var types = new[]
        {
            typeof(BigInteger),
            typeof(string),
            typeof(long),
            typeof(ProbeEnum),
            typeof(ProbeModel),
        };
        return Enumerable.Range(0, arity).Select(index => types[index % types.Length]).ToArray();
    }

    private static Delegate CreateSelector(Type unionType, Type valueType)
    {
        var value = Expression.Parameter(unionType, "value");
        var body = Expression.Property(value, "AsT0");
        return Expression.Lambda(typeof(Func<,>).MakeGenericType(unionType, valueType), body, value).Compile();
    }

    private static MethodInfo GetGenericMethod(string name, int parameterCount, int genericArity) =>
        typeof(Program).GetMethods(BindingFlags.NonPublic | BindingFlags.Static)
            .Single(method => method.Name == name
                && method.IsGenericMethodDefinition
                && method.GetParameters().Length == parameterCount
                && method.GetGenericArguments().Length == genericArity);

    private static int SizeOf<T>() => Unsafe.SizeOf<T>();

    private static double MeasureCopies<T>(object rawValue, int iterations)
    {
        var value = (T)rawValue;
        var source = Enumerable.Repeat(value, 256).ToArray();
        var destination = new T[256];
        var stopwatch = Stopwatch.StartNew();
        for (var index = 0; index < iterations; index++)
        {
            destination[index & 255] = source[(index + 1) & 255];
        }
        stopwatch.Stop();
        Consume(EqualityComparer<T>.Default.GetHashCode(destination[iterations & 255]!));
        return NanosecondsPerOperation(stopwatch, iterations);
    }

    private static double MeasureConstruction<TUnion, TValue>(Delegate rawFactory, object rawValue, int iterations)
    {
        var factory = (Func<TValue, TUnion>)rawFactory;
        var value = (TValue)rawValue;
        _ = factory(value);
        var before = GC.GetAllocatedBytesForCurrentThread();
        var last = default(TUnion);
        for (var index = 0; index < iterations; index++)
        {
            last = factory(value);
        }
        var allocated = GC.GetAllocatedBytesForCurrentThread() - before;
        Consume(EqualityComparer<TUnion>.Default.GetHashCode(last!));
        return (double)allocated / iterations;
    }

    private static double MeasurePayloadConstruction<TValue>(object rawValue, int iterations)
    {
        var value = (TValue)rawValue;
        _ = new PayloadUnion(value, 1);
        var before = GC.GetAllocatedBytesForCurrentThread();
        var last = default(PayloadUnion);
        for (var index = 0; index < iterations; index++)
        {
            last = new PayloadUnion(value, 1);
        }
        var allocated = GC.GetAllocatedBytesForCurrentThread() - before;
        Consume(last.GetHashCode());
        return (double)allocated / iterations;
    }

    private static double MeasureMatches<TUnion, TValue>(Delegate rawSelector, object rawUnion, int iterations)
    {
        var selector = (Func<TUnion, TValue>)rawSelector;
        var union = (TUnion)rawUnion;
        _ = selector(union);
        var before = GC.GetAllocatedBytesForCurrentThread();
        var last = default(TValue);
        for (var index = 0; index < iterations; index++)
        {
            last = selector(union);
        }
        var allocated = GC.GetAllocatedBytesForCurrentThread() - before;
        Consume(EqualityComparer<TValue>.Default.GetHashCode(last!));
        return (double)allocated / iterations;
    }

    private static double MeasurePayloadMatches<TValue>(object rawPayload, int iterations)
    {
        var payload = (PayloadUnion)rawPayload;
        _ = (TValue)payload.Value;
        var before = GC.GetAllocatedBytesForCurrentThread();
        var last = default(TValue);
        for (var index = 0; index < iterations; index++)
        {
            last = (TValue)payload.Value;
        }
        var allocated = GC.GetAllocatedBytesForCurrentThread() - before;
        Consume(EqualityComparer<TValue>.Default.GetHashCode(last!));
        return (double)allocated / iterations;
    }

    private static double NanosecondsPerOperation(Stopwatch stopwatch, int iterations) =>
        stopwatch.ElapsedTicks * (1_000_000_000.0 / Stopwatch.Frequency) / iterations;

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static void Consume(int value) => Volatile.Write(ref _sink, value);

    private sealed record Scenario(string Name, Type Type, object Value, bool Mixed = false);
    private sealed record ProbeModel(string Value);
    private enum ProbeEnum : long { First = 1, Second = 2 }

    private readonly struct PayloadUnion(object value, byte activeCase)
    {
        internal object Value { get; } = value;
        internal byte ActiveCase { get; } = activeCase;
    }
}
