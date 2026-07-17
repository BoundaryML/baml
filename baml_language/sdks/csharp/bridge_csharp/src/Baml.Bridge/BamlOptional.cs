using System.Diagnostics.CodeAnalysis;

namespace Baml;

public readonly struct BamlOptional<T> : IEquatable<BamlOptional<T>>
{
    private readonly T _value;

    private BamlOptional(T value)
    {
        _value = value;
        IsSet = true;
    }

    public bool IsSet { get; }

    public T Value => IsSet
        ? _value
        : throw new InvalidOperationException("The BAML optional value is unset.");

    public static BamlOptional<T> Unset => default;

    public static BamlOptional<T> FromValue(T value) => new(value);

    public bool TryGetValue([MaybeNullWhen(false)] out T value)
    {
        value = _value;
        return IsSet;
    }

    public static implicit operator BamlOptional<T>(T value) => FromValue(value);

    public bool Equals(BamlOptional<T> other) =>
        IsSet == other.IsSet
        && (!IsSet || EqualityComparer<T>.Default.Equals(_value, other._value));

    public override bool Equals(object? obj) => obj is BamlOptional<T> other && Equals(other);

    public override int GetHashCode() => !IsSet
        ? 0
        : HashCode.Combine(1, EqualityComparer<T>.Default.GetHashCode(_value!));

    public static bool operator ==(BamlOptional<T> left, BamlOptional<T> right) => left.Equals(right);

    public static bool operator !=(BamlOptional<T> left, BamlOptional<T> right) => !left.Equals(right);

    public override string ToString() => !IsSet
        ? "<unset>"
        : _value is null ? "<null>" : _value.ToString() ?? "<null>";
}
