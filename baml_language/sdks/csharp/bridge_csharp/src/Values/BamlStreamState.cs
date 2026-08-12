namespace Baml;

public enum BamlStreamStateKind : int
{
    Pending = 0,
    Incomplete = 1,
    Complete = 2,
}

public readonly struct BamlStreamState<T> : IEquatable<BamlStreamState<T>>
{
    private readonly T value;

    internal BamlStreamState(BamlStreamStateKind state, T value)
    {
        if (state is < BamlStreamStateKind.Pending or > BamlStreamStateKind.Complete)
        {
            throw new ArgumentOutOfRangeException(nameof(state));
        }

        State = state;
        this.value = value;
    }

    public BamlStreamStateKind State { get; }

    public T Value => value;

    public bool IsComplete => State == BamlStreamStateKind.Complete;

    internal static BamlStreamState<T> Incomplete(T value) =>
        new(BamlStreamStateKind.Incomplete, value);

    internal static BamlStreamState<T> Complete(T value) =>
        new(BamlStreamStateKind.Complete, value);

    public bool Equals(BamlStreamState<T> other) =>
        State == other.State
        && EqualityComparer<T>.Default.Equals(value, other.value);

    public override bool Equals(object? obj) =>
        obj is BamlStreamState<T> other && Equals(other);

    public override int GetHashCode() => HashCode.Combine(State, value);

    public static bool operator ==(BamlStreamState<T> left, BamlStreamState<T> right) =>
        left.Equals(right);

    public static bool operator !=(BamlStreamState<T> left, BamlStreamState<T> right) =>
        !left.Equals(right);

    public override string ToString() => $"{State}(<redacted>)";
}
