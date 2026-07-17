namespace Baml;

[AttributeUsage(AttributeTargets.Class, Inherited = false)]
public sealed class BamlTypeAliasAttribute : Attribute
{
    public BamlTypeAliasAttribute(string name)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(name);
        Name = name;
    }

    public string Name { get; }
}

public interface IBamlTypeAliasValue
{
    object? UntypedValue { get; }
}
