using System.Diagnostics.CodeAnalysis;

namespace Baml;

public readonly struct BamlNullable<T> : IEquatable<BamlNullable<T>>, Bridge.IBamlNullableValue
{
    private readonly T _value;
    private readonly bool _hasValue;

    private BamlNullable(T value)
    {
        _value = value;
        _hasValue = value is not null;
    }

    public bool IsNull => !_hasValue;

    object? Bridge.IBamlNullableValue.Value => IsNull ? null : _value;

    public T Value => !IsNull
        ? _value
        : throw new InvalidOperationException("The BAML value is null.");

    public static BamlNullable<T> Null => default;

    public static BamlNullable<T> FromValue(T value) => new(value);

    public bool TryGetValue([MaybeNullWhen(false)] out T value)
    {
        value = _value;
        return !IsNull;
    }

    public TResult Match<TResult>(Func<TResult> onNull, Func<T, TResult> onValue)
    {
        ArgumentNullException.ThrowIfNull(onNull);
        ArgumentNullException.ThrowIfNull(onValue);
        return IsNull ? onNull() : onValue(_value);
    }

    public static implicit operator BamlNullable<T>(T value) => FromValue(value);

    public bool Equals(BamlNullable<T> other) =>
        IsNull == other.IsNull
        && (IsNull || EqualityComparer<T>.Default.Equals(_value, other._value));

    public override bool Equals(object? obj) => obj is BamlNullable<T> other && Equals(other);

    public override int GetHashCode() => IsNull
        ? 0
        : HashCode.Combine(1, EqualityComparer<T>.Default.GetHashCode(_value!));

    public static bool operator ==(BamlNullable<T> left, BamlNullable<T> right) => left.Equals(right);

    public static bool operator !=(BamlNullable<T> left, BamlNullable<T> right) => !left.Equals(right);

    public override string ToString() => IsNull ? "<null>" : _value?.ToString() ?? "<null>";
}

public static class BamlNullable
{
    public static BamlNullable<T> Null<T>() => BamlNullable<T>.Null;

    public static BamlNullable<T> FromValue<T>(T value) => BamlNullable<T>.FromValue(value);
}
