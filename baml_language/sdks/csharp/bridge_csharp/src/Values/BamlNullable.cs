using System.Diagnostics.CodeAnalysis;

namespace Baml;

public readonly struct BamlNullable<T> : IEquatable<BamlNullable<T>>
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
        : throw new InvalidOperationException("The BAML value is null.");

    public static BamlNullable<T> Null => default;

    public static BamlNullable<T> FromValue(T value) => new(value);

    public bool TryGetValue([MaybeNullWhen(false)] out T result)
    {
        result = value;
        return !IsNull;
    }

    public TResult Match<TResult>(Func<TResult> onNull, Func<T, TResult> onValue)
    {
        ArgumentNullException.ThrowIfNull(onNull);
        ArgumentNullException.ThrowIfNull(onValue);
        return IsNull ? onNull() : onValue(value);
    }

    public static implicit operator BamlNullable<T>(T value) => FromValue(value);

    public bool Equals(BamlNullable<T> other) =>
        IsNull == other.IsNull
        && (IsNull || EqualityComparer<T>.Default.Equals(value, other.value));

    public override bool Equals(object? obj) =>
        obj is BamlNullable<T> other && Equals(other);

    public override int GetHashCode() =>
        IsNull
            ? 0
            : HashCode.Combine(1, EqualityComparer<T>.Default.GetHashCode(value!));

    public static bool operator ==(BamlNullable<T> left, BamlNullable<T> right) =>
        left.Equals(right);

    public static bool operator !=(BamlNullable<T> left, BamlNullable<T> right) =>
        !left.Equals(right);

    public override string ToString() =>
        IsNull ? "<null>" : value?.ToString() ?? "<null>";
}

public static class BamlNullable
{
    public static BamlNullable<T> Null<T>() => BamlNullable<T>.Null;

    public static BamlNullable<T> FromValue<T>(T value) =>
        BamlNullable<T>.FromValue(value);
}
