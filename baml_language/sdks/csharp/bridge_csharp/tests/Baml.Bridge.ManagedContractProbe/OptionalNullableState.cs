using System.Diagnostics.CodeAnalysis;

namespace Baml;

public readonly struct BamlOptional<T>
    : IEquatable<BamlOptional<T>>
{
    private readonly T value;

    private BamlOptional(T value)
    {
        this.value = value;
        IsSet = true;
    }

    public bool IsSet { get; }

    public T Value => IsSet
        ? value
        : throw new InvalidOperationException(
            "The BAML optional value is unset.");

    public static BamlOptional<T> Unset => default;

    public static BamlOptional<T> FromValue(T value) =>
        new(value);

    public bool TryGetValue(
        [MaybeNullWhen(false)] out T result)
    {
        result = value;
        return IsSet;
    }

    public static implicit operator BamlOptional<T>(T value) =>
        FromValue(value);

    public bool Equals(BamlOptional<T> other) =>
        IsSet == other.IsSet
        && (!IsSet
            || EqualityComparer<T>.Default.Equals(
                value,
                other.value));

    public override bool Equals(object? obj) =>
        obj is BamlOptional<T> other && Equals(other);

    public override int GetHashCode() =>
        !IsSet
            ? 0
            : HashCode.Combine(
                1,
                EqualityComparer<T>.Default.GetHashCode(value!));

    public static bool operator ==(
        BamlOptional<T> left,
        BamlOptional<T> right) =>
        left.Equals(right);

    public static bool operator !=(
        BamlOptional<T> left,
        BamlOptional<T> right) =>
        !left.Equals(right);

    public override string ToString() =>
        IsSet ? value?.ToString() ?? "<null>" : "<unset>";
}

public readonly struct BamlNullable<T>
    : IEquatable<BamlNullable<T>>
{
    private readonly T value;
    private readonly bool hasValue;

    private BamlNullable(T value)
    {
        this.value = value;
        hasValue = value is not null;
    }

    public bool IsNull => !hasValue;

    public T Value => !IsNull
        ? value
        : throw new InvalidOperationException(
            "The BAML value is null.");

    public static BamlNullable<T> Null => default;

    public static BamlNullable<T> FromValue(T value) =>
        new(value);

    public bool TryGetValue(
        [MaybeNullWhen(false)] out T result)
    {
        result = value;
        return !IsNull;
    }

    public TResult Match<TResult>(
        Func<TResult> onNull,
        Func<T, TResult> onValue)
    {
        ArgumentNullException.ThrowIfNull(onNull);
        ArgumentNullException.ThrowIfNull(onValue);
        return IsNull ? onNull() : onValue(value);
    }

    public static implicit operator BamlNullable<T>(T value) =>
        FromValue(value);

    public bool Equals(BamlNullable<T> other) =>
        IsNull == other.IsNull
        && (IsNull
            || EqualityComparer<T>.Default.Equals(
                value,
                other.value));

    public override bool Equals(object? obj) =>
        obj is BamlNullable<T> other && Equals(other);

    public override int GetHashCode() =>
        IsNull
            ? 0
            : HashCode.Combine(
                1,
                EqualityComparer<T>.Default.GetHashCode(value!));

    public static bool operator ==(
        BamlNullable<T> left,
        BamlNullable<T> right) =>
        left.Equals(right);

    public static bool operator !=(
        BamlNullable<T> left,
        BamlNullable<T> right) =>
        !left.Equals(right);

    public override string ToString() =>
        IsNull ? "<null>" : value?.ToString() ?? "<null>";
}

public static class BamlNullable
{
    public static BamlNullable<T> Null<T>() =>
        BamlNullable<T>.Null;

    public static BamlNullable<T> FromValue<T>(T value) =>
        BamlNullable<T>.FromValue(value);
}

public enum BamlStreamStateKind : int
{
    Pending = 0,
    Incomplete = 1,
    Complete = 2,
}

public readonly struct BamlStreamState<T>
    : IEquatable<BamlStreamState<T>>
{
    private readonly T value;

    internal BamlStreamState(
        BamlStreamStateKind state,
        T value)
    {
        if (state is < BamlStreamStateKind.Pending
            or > BamlStreamStateKind.Complete)
        {
            throw new ArgumentOutOfRangeException(nameof(state));
        }

        State = state;
        this.value = value;
    }

    public BamlStreamStateKind State { get; }

    public T Value => value;

    public bool IsComplete =>
        State == BamlStreamStateKind.Complete;

    internal static BamlStreamState<T> Incomplete(T value) =>
        new(BamlStreamStateKind.Incomplete, value);

    internal static BamlStreamState<T> Complete(T value) =>
        new(BamlStreamStateKind.Complete, value);

    public bool Equals(BamlStreamState<T> other) =>
        State == other.State
        && EqualityComparer<T>.Default.Equals(
            value,
            other.value);

    public override bool Equals(object? obj) =>
        obj is BamlStreamState<T> other && Equals(other);

    public override int GetHashCode() =>
        HashCode.Combine(State, value);

    public static bool operator ==(
        BamlStreamState<T> left,
        BamlStreamState<T> right) =>
        left.Equals(right);

    public static bool operator !=(
        BamlStreamState<T> left,
        BamlStreamState<T> right) =>
        !left.Equals(right);

    public override string ToString() =>
        $"{State}(<redacted>)";
}
