namespace Baml.Bridge;

internal interface IBamlUnionValue
{
    int ActiveCase { get; }

    object? Value { get; }
}
