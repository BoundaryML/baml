using System.Diagnostics.CodeAnalysis;

namespace Baml;

public readonly struct BamlOptional<T> : IEquatable<BamlOptional<T>>
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
        : throw new InvalidOperationException("The BAML optional value is unset.");

    public static BamlOptional<T> Unset => default;

    public static BamlOptional<T> FromValue(T value) => new(value);

    public bool TryGetValue([MaybeNullWhen(false)] out T result)
    {
        result = value;
        return IsSet;
    }

    public static implicit operator BamlOptional<T>(T value) => FromValue(value);

    public bool Equals(BamlOptional<T> other) =>
        IsSet == other.IsSet
        && (!IsSet || EqualityComparer<T>.Default.Equals(value, other.value));

    public override bool Equals(object? obj) =>
        obj is BamlOptional<T> other && Equals(other);

    public override int GetHashCode() =>
        !IsSet
            ? 0
            : HashCode.Combine(1, EqualityComparer<T>.Default.GetHashCode(value!));

    public static bool operator ==(BamlOptional<T> left, BamlOptional<T> right) =>
        left.Equals(right);

    public static bool operator !=(BamlOptional<T> left, BamlOptional<T> right) =>
        !left.Equals(right);

    public override string ToString() =>
        IsSet ? value?.ToString() ?? "<null>" : "<unset>";
}
