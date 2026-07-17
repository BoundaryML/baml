namespace Baml.Bridge;

internal interface IBamlNullableValue
{
    bool IsNull { get; }

    object? Value { get; }
}
