using Baml;
using Probe.Generated;

internal static class Generated
{
    internal static T Identity<T>(T value) => value;

    internal static T ResultOnly<T>() => default!;

    internal static BamlOptional<T> Optional<T>(
        BamlOptional<T> value = default) =>
        value;

    internal static BamlNullable<T> Nullable<T>(
        BamlNullable<T> value) =>
        value;

    internal static BamlOptional<BamlNullable<T>> Composed<T>(
        BamlOptional<BamlNullable<T>> value = default) =>
        value;

    internal static T Head<T>(IReadOnlyList<T> values) =>
        values[0];

    internal static T Lookup<T>(
        IReadOnlyDictionary<string, T> values,
        string key) =>
        values[key];

    internal static BamlUnion<T0, T1> Union<T0, T1>(
        BamlUnion<T0, T1> value) =>
        value;

    internal static T Unbox<T>(Box<T> value) =>
        value.Value;
}

internal sealed class GenericOwner<TClass>
{
    internal (TClass ClassValue, TMethod MethodValue)
        Method<TMethod>(
            TClass classValue,
            TMethod methodValue) =>
        (classValue, methodValue);
}
