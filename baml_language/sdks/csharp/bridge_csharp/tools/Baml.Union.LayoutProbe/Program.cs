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
    private static readonly MethodInfo ConstructionMethod =
        GetGenericMethod(nameof(MeasureConstruction), 3, 2);
    private static readonly MethodInfo MatchMethod =
        GetGenericMethod(nameof(MeasureMatches), 3, 2);
    private static readonly MethodInfo PayloadConstructionMethod =
        GetGenericMethod(nameof(MeasurePayloadConstruction), 2, 1);
    private static readonly MethodInfo PayloadMatchMethod =
        GetGenericMethod(nameof(MeasurePayloadMatches), 2, 1);

    public static void Main()
    {
        VerifySemantics();
        IReadOnlyDictionary<int, Type> unionDefinitions = new Dictionary<int, Type>
        {
            [2] = typeof(FieldUnion<,>),
            [8] = typeof(FieldUnion<,,,,,,,>),
            [16] = typeof(FieldUnion<,,,,,,,,,,,,,,,>),
            [32] = typeof(FieldUnion<,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,>),
        };
        var scenarios = new[]
        {
            new Scenario("reference", typeof(string), "payload"),
            new Scenario("primitive", typeof(long), 42L),
            new Scenario("enum", typeof(ProbeEnum), ProbeEnum.Second),
            new Scenario(
                "bigint",
                typeof(BigInteger),
                BigInteger.Parse("1208925819614629174706177")),
            new Scenario("class", typeof(ProbeModel), new ProbeModel("payload")),
            new Scenario(
                "mixed",
                typeof(BigInteger),
                BigInteger.Parse("1208925819614629174706177"),
                Mixed: true),
        };

        Console.WriteLine(
            "| Arity | Payload | Fields bytes | Payload/tag bytes | Fields copy ns/op | Payload/tag copy ns/op | Fields construct B/op | Payload/tag construct B/op | Fields match B/op | Payload/tag match B/op |");
        Console.WriteLine(
            "| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
        foreach (int arity in new[] { 2, 8, 16, 32 })
        {
            foreach (Scenario scenario in scenarios)
            {
                Type[] typeArguments = scenario.Mixed
                    ? MixedTypes(arity)
                    : Enumerable.Repeat(scenario.Type, arity).ToArray();
                Type unionType = unionDefinitions[arity].MakeGenericType(typeArguments);
                MethodInfo fromT0 =
                    unionType.GetMethod("FromT0", BindingFlags.Public | BindingFlags.Static)
                    ?? throw new InvalidOperationException($"{unionType} has no FromT0 factory.");
                object union =
                    fromT0.Invoke(null, [scenario.Value])
                    ?? throw new InvalidOperationException($"{unionType}.FromT0 returned null.");
                Type factoryType = typeof(Func<,>).MakeGenericType(typeArguments[0], unionType);
                Delegate factory = fromT0.CreateDelegate(factoryType);
                Delegate selector = CreateSelector(unionType, typeArguments[0]);
                var payload = new PayloadUnion(scenario.Value, 1);

                int fieldsSize = (int)SizeMethod.MakeGenericMethod(unionType).Invoke(null, null)!;
                int payloadSize = Unsafe.SizeOf<PayloadUnion>();
                double fieldsCopy = (double)CopyMethod.MakeGenericMethod(unionType)
                    .Invoke(null, [union, CopyIterations])!;
                double payloadCopy = MeasureCopies<PayloadUnion>(payload, CopyIterations);
                double fieldsConstruction = (double)ConstructionMethod
                    .MakeGenericMethod(unionType, typeArguments[0])
                    .Invoke(null, [factory, scenario.Value, OperationIterations])!;
                double payloadConstruction = (double)PayloadConstructionMethod
                    .MakeGenericMethod(typeArguments[0])
                    .Invoke(null, [scenario.Value, OperationIterations])!;
                double fieldsMatch = (double)MatchMethod
                    .MakeGenericMethod(unionType, typeArguments[0])
                    .Invoke(null, [selector, union, OperationIterations])!;
                double payloadMatch = (double)PayloadMatchMethod
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
        FieldUnion<string, string> first = FieldUnion<string, string>.FromT0("same");
        FieldUnion<string, string> second = FieldUnion<string, string>.FromT1("same");
        if (!first.IsT0 || !second.IsT1 || first.Equals(second))
        {
            throw new InvalidOperationException(
                "Duplicate closed union arms did not retain distinct cases.");
        }

        try
        {
            _ = default(FieldUnion<string, long>).AsT0;
            throw new InvalidOperationException(
                "The default union unexpectedly selected its first arm.");
        }
        catch (InvalidOperationException error)
            when (error.Message.Contains("no active case", StringComparison.Ordinal))
        {
        }
    }

    private static Type[] MixedTypes(int arity)
    {
        Type[] types =
        [
            typeof(BigInteger),
            typeof(string),
            typeof(long),
            typeof(ProbeEnum),
            typeof(ProbeModel),
        ];
        return Enumerable.Range(0, arity).Select(index => types[index % types.Length]).ToArray();
    }

    private static Delegate CreateSelector(Type unionType, Type valueType)
    {
        ParameterExpression value = Expression.Parameter(unionType, "value");
        MemberExpression body = Expression.Property(value, "AsT0");
        return Expression.Lambda(
                typeof(Func<,>).MakeGenericType(unionType, valueType),
                body,
                value)
            .Compile();
    }

    private static MethodInfo GetGenericMethod(
        string name,
        int parameterCount,
        int genericArity) =>
        typeof(Program)
            .GetMethods(BindingFlags.NonPublic | BindingFlags.Static)
            .Single(method =>
                method.Name == name
                && method.IsGenericMethodDefinition
                && method.GetParameters().Length == parameterCount
                && method.GetGenericArguments().Length == genericArity);

    private static int SizeOf<T>() => Unsafe.SizeOf<T>();

    private static double MeasureCopies<T>(object rawValue, int iterations)
    {
        var value = (T)rawValue;
        var source = Enumerable.Repeat(value, 256).ToArray();
        var destination = new T[256];
        Stopwatch stopwatch = Stopwatch.StartNew();
        for (int index = 0; index < iterations; index++)
        {
            destination[index & 255] = source[(index + 1) & 255];
        }

        stopwatch.Stop();
        Consume(EqualityComparer<T>.Default.GetHashCode(destination[iterations & 255]!));
        return NanosecondsPerOperation(stopwatch, iterations);
    }

    private static double MeasureConstruction<TUnion, TValue>(
        Delegate rawFactory,
        object rawValue,
        int iterations)
    {
        var factory = (Func<TValue, TUnion>)rawFactory;
        var value = (TValue)rawValue;
        _ = factory(value);
        long before = GC.GetAllocatedBytesForCurrentThread();
        var last = default(TUnion);
        for (int index = 0; index < iterations; index++)
        {
            last = factory(value);
        }

        long allocated = GC.GetAllocatedBytesForCurrentThread() - before;
        Consume(EqualityComparer<TUnion>.Default.GetHashCode(last!));
        return (double)allocated / iterations;
    }

    private static double MeasurePayloadConstruction<TValue>(
        object rawValue,
        int iterations)
    {
        var value = (TValue)rawValue;
        _ = new PayloadUnion(value!, 1);
        long before = GC.GetAllocatedBytesForCurrentThread();
        var last = default(PayloadUnion);
        for (int index = 0; index < iterations; index++)
        {
            last = new PayloadUnion(value!, 1);
        }

        long allocated = GC.GetAllocatedBytesForCurrentThread() - before;
        Consume(last.GetHashCode());
        return (double)allocated / iterations;
    }

    private static double MeasureMatches<TUnion, TValue>(
        Delegate rawSelector,
        object rawUnion,
        int iterations)
    {
        var selector = (Func<TUnion, TValue>)rawSelector;
        var union = (TUnion)rawUnion;
        _ = selector(union);
        long before = GC.GetAllocatedBytesForCurrentThread();
        var last = default(TValue);
        for (int index = 0; index < iterations; index++)
        {
            last = selector(union);
        }

        long allocated = GC.GetAllocatedBytesForCurrentThread() - before;
        Consume(EqualityComparer<TValue>.Default.GetHashCode(last!));
        return (double)allocated / iterations;
    }

    private static double MeasurePayloadMatches<TValue>(
        object rawPayload,
        int iterations)
    {
        var payload = (PayloadUnion)rawPayload;
        _ = (TValue)payload.Value;
        long before = GC.GetAllocatedBytesForCurrentThread();
        var last = default(TValue);
        for (int index = 0; index < iterations; index++)
        {
            last = (TValue)payload.Value;
        }

        long allocated = GC.GetAllocatedBytesForCurrentThread() - before;
        Consume(EqualityComparer<TValue>.Default.GetHashCode(last!));
        return (double)allocated / iterations;
    }

    private static double NanosecondsPerOperation(
        Stopwatch stopwatch,
        int iterations) =>
        stopwatch.ElapsedTicks
        * (1_000_000_000.0 / Stopwatch.Frequency)
        / iterations;

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static void Consume(int value) => Volatile.Write(ref _sink, value);

    private sealed record Scenario(
        string Name,
        Type Type,
        object Value,
        bool Mixed = false);

    private sealed record ProbeModel(string Value);

    private enum ProbeEnum : long
    {
        First = 1,
        Second = 2,
    }

    private readonly struct PayloadUnion(object value, byte activeCase)
    {
        internal object Value { get; } = value;

        internal byte ActiveCase { get; } = activeCase;
    }
}

internal readonly struct FieldUnion<T0, T1> : IEquatable<FieldUnion<T0, T1>>
{
    private readonly T0? _value0;
    private readonly T1? _value1;
    private readonly byte _activeCase;

    private FieldUnion(byte activeCase, T0? value0, T1? value1)
    {
        _activeCase = activeCase;
        _value0 = value0;
        _value1 = value1;
    }

    public bool IsT0 => _activeCase == 1;

    public bool IsT1 => _activeCase == 2;

    public T0 AsT0 =>
        IsT0 ? _value0! : throw new InvalidOperationException("The union has no active case T0.");

    public static FieldUnion<T0, T1> FromT0(T0 value) => new(1, value, default);

    public static FieldUnion<T0, T1> FromT1(T1 value) => new(2, default, value);

    public bool Equals(FieldUnion<T0, T1> other) =>
        _activeCase == other._activeCase
        && _activeCase switch
        {
            0 => true,
            1 => EqualityComparer<T0>.Default.Equals(_value0!, other._value0!),
            2 => EqualityComparer<T1>.Default.Equals(_value1!, other._value1!),
            _ => false,
        };
}

#pragma warning disable CS0414 // Inactive arms are intentionally retained for layout measurement.
internal readonly struct FieldUnion<T0, T1, T2, T3, T4, T5, T6, T7>
{
    private readonly T0? _value0;
    private readonly T1? _value1;
    private readonly T2? _value2;
    private readonly T3? _value3;
    private readonly T4? _value4;
    private readonly T5? _value5;
    private readonly T6? _value6;
    private readonly T7? _value7;
    private readonly byte _activeCase;

    private FieldUnion(T0 value)
    {
        _value0 = value;
        _value1 = default;
        _value2 = default;
        _value3 = default;
        _value4 = default;
        _value5 = default;
        _value6 = default;
        _value7 = default;
        _activeCase = 1;
    }

    public T0 AsT0 =>
        _activeCase == 1
            ? _value0!
            : throw new InvalidOperationException("The union has no active case T0.");

    public static FieldUnion<T0, T1, T2, T3, T4, T5, T6, T7> FromT0(T0 value) => new(value);
}

internal readonly struct FieldUnion<T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15>
{
    private readonly T0? _value0;
    private readonly T1? _value1;
    private readonly T2? _value2;
    private readonly T3? _value3;
    private readonly T4? _value4;
    private readonly T5? _value5;
    private readonly T6? _value6;
    private readonly T7? _value7;
    private readonly T8? _value8;
    private readonly T9? _value9;
    private readonly T10? _value10;
    private readonly T11? _value11;
    private readonly T12? _value12;
    private readonly T13? _value13;
    private readonly T14? _value14;
    private readonly T15? _value15;
    private readonly byte _activeCase;

    private FieldUnion(T0 value)
    {
        _value0 = value;
        _value1 = default;
        _value2 = default;
        _value3 = default;
        _value4 = default;
        _value5 = default;
        _value6 = default;
        _value7 = default;
        _value8 = default;
        _value9 = default;
        _value10 = default;
        _value11 = default;
        _value12 = default;
        _value13 = default;
        _value14 = default;
        _value15 = default;
        _activeCase = 1;
    }

    public T0 AsT0 =>
        _activeCase == 1
            ? _value0!
            : throw new InvalidOperationException("The union has no active case T0.");

    public static FieldUnion<T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15> FromT0(T0 value) =>
        new(value);
}

internal readonly struct FieldUnion<T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20, T21, T22, T23, T24, T25, T26, T27, T28, T29, T30, T31>
{
    private readonly T0? _value0;
    private readonly T1? _value1;
    private readonly T2? _value2;
    private readonly T3? _value3;
    private readonly T4? _value4;
    private readonly T5? _value5;
    private readonly T6? _value6;
    private readonly T7? _value7;
    private readonly T8? _value8;
    private readonly T9? _value9;
    private readonly T10? _value10;
    private readonly T11? _value11;
    private readonly T12? _value12;
    private readonly T13? _value13;
    private readonly T14? _value14;
    private readonly T15? _value15;
    private readonly T16? _value16;
    private readonly T17? _value17;
    private readonly T18? _value18;
    private readonly T19? _value19;
    private readonly T20? _value20;
    private readonly T21? _value21;
    private readonly T22? _value22;
    private readonly T23? _value23;
    private readonly T24? _value24;
    private readonly T25? _value25;
    private readonly T26? _value26;
    private readonly T27? _value27;
    private readonly T28? _value28;
    private readonly T29? _value29;
    private readonly T30? _value30;
    private readonly T31? _value31;
    private readonly byte _activeCase;

    private FieldUnion(T0 value)
    {
        _value0 = value;
        _value1 = default;
        _value2 = default;
        _value3 = default;
        _value4 = default;
        _value5 = default;
        _value6 = default;
        _value7 = default;
        _value8 = default;
        _value9 = default;
        _value10 = default;
        _value11 = default;
        _value12 = default;
        _value13 = default;
        _value14 = default;
        _value15 = default;
        _value16 = default;
        _value17 = default;
        _value18 = default;
        _value19 = default;
        _value20 = default;
        _value21 = default;
        _value22 = default;
        _value23 = default;
        _value24 = default;
        _value25 = default;
        _value26 = default;
        _value27 = default;
        _value28 = default;
        _value29 = default;
        _value30 = default;
        _value31 = default;
        _activeCase = 1;
    }

    public T0 AsT0 =>
        _activeCase == 1
            ? _value0!
            : throw new InvalidOperationException("The union has no active case T0.");

    public static FieldUnion<T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20, T21, T22, T23, T24, T25, T26, T27, T28, T29, T30, T31> FromT0(T0 value) =>
        new(value);
}
#pragma warning restore CS0414
