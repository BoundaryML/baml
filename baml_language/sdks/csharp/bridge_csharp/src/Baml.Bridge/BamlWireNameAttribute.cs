namespace Baml;

[AttributeUsage(AttributeTargets.Parameter)]
public sealed class BamlWireNameAttribute : Attribute
{
    public BamlWireNameAttribute(string name)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(name);
        Name = name;
    }

    public string Name { get; }
}
