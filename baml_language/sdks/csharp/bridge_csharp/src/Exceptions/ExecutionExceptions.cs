using System.Collections.ObjectModel;

namespace Baml;

public abstract class BamlExecutionException : BamlException
{
    internal BamlExecutionException(
        string message,
        string? bamlFunction,
        BamlTrace trace)
        : base(message)
    {
        ArgumentNullException.ThrowIfNull(trace);
        BamlFunction = bamlFunction;
        Trace = trace;
    }

    public string? BamlFunction { get; }

    public BamlTrace Trace { get; }
}

public class BamlErrorException : BamlExecutionException
{
    internal BamlErrorException(
        string message,
        BamlValue thrownValue,
        string? bamlFunction,
        BamlTrace trace)
        : base(message, bamlFunction, trace)
    {
        ArgumentNullException.ThrowIfNull(thrownValue);
        ThrownValue = thrownValue;
        ErrorName = thrownValue.NominalTypeName;
    }

    public BamlValue ThrownValue { get; }

    public string? ErrorName { get; }
}

public sealed class BamlTypeMismatchException : BamlErrorException
{
    internal BamlTypeMismatchException(
        string message,
        BamlValue thrownValue,
        string? bamlFunction,
        BamlTrace trace)
        : base(message, thrownValue, bamlFunction, trace)
    {
    }
}

public sealed class BamlPanicException : BamlExecutionException
{
    internal BamlPanicException(
        string message,
        string? bamlFunction,
        BamlPanicInfo panic,
        BamlTrace trace)
        : base(message, bamlFunction, trace)
    {
        ArgumentNullException.ThrowIfNull(panic);
        if (panic.IsExitPanic)
        {
            throw new ArgumentException(
                "An exit panic must terminate the process instead of becoming catchable.",
                nameof(panic));
        }

        Panic = panic;
    }

    public BamlPanicInfo Panic { get; }
}

public enum BamlCancellationOrigin : int
{
    Caller = 0,
    Engine = 1,
    StreamDisposed = 2,
}

public sealed class BamlOperationCanceledException : OperationCanceledException
{
    internal BamlOperationCanceledException(
        string message,
        BamlCancellationOrigin origin,
        CancellationToken cancellationToken,
        string? bamlFunction,
        BamlTrace? trace)
        : base(message, innerException: null, cancellationToken)
    {
        if (!cancellationToken.IsCancellationRequested)
        {
            throw new ArgumentException(
                "A BAML operation cancellation requires an already canceled token.",
                nameof(cancellationToken));
        }

        Origin = origin;
        BamlFunction = bamlFunction;
        Trace = trace;
    }

    public BamlCancellationOrigin Origin { get; }

    public string? BamlFunction { get; }

    public BamlTrace? Trace { get; }
}

public sealed class BamlTrace : IEquatable<BamlTrace>
{
    private readonly ReadOnlyCollection<string> lines;

    internal BamlTrace(IEnumerable<string> lines)
    {
        ArgumentNullException.ThrowIfNull(lines);
        string[] snapshot = lines.ToArray();
        if (snapshot.Any(line => line is null))
        {
            throw new ArgumentException(
                "A BAML trace cannot contain a null rendered line.",
                nameof(lines));
        }

        this.lines = Array.AsReadOnly(snapshot);
    }

    public IReadOnlyList<string> Lines => lines;

    public bool Equals(BamlTrace? other) =>
        other is not null
        && lines.SequenceEqual(other.lines, StringComparer.Ordinal);

    public override bool Equals(object? obj) =>
        obj is BamlTrace other && Equals(other);

    public override int GetHashCode()
    {
        HashCode hash = new();
        foreach (string line in lines)
        {
            hash.Add(line, StringComparer.Ordinal);
        }

        return hash.ToHashCode();
    }
}

public sealed class BamlPanicInfo : IEquatable<BamlPanicInfo>
{
    internal BamlPanicInfo(
        BamlValue value,
        bool isExitPanic,
        long? exitCode)
    {
        ArgumentNullException.ThrowIfNull(value);
        if (isExitPanic != exitCode.HasValue)
        {
            throw new ArgumentException(
                "Exit metadata requires both the discriminator and exit code.");
        }

        Value = value;
        IsExitPanic = isExitPanic;
        ExitCode = exitCode;
    }

    public BamlValue Value { get; }

    public bool IsExitPanic { get; }

    public long? ExitCode { get; }

    public bool Equals(BamlPanicInfo? other) =>
        other is not null
        && EqualityComparer<BamlValue>.Default.Equals(Value, other.Value)
        && IsExitPanic == other.IsExitPanic
        && ExitCode == other.ExitCode;

    public override bool Equals(object? obj) =>
        obj is BamlPanicInfo other && Equals(other);

    public override int GetHashCode() =>
        HashCode.Combine(Value, IsExitPanic, ExitCode);
}

public sealed class BamlHostCallbackException : BamlInteropException
{
    internal BamlHostCallbackException(string message)
        : base(message)
    {
    }

    internal BamlHostCallbackException(string message, Exception innerException)
        : base(message, innerException)
    {
    }
}
