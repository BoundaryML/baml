using System.CodeDom.Compiler;
using System.Collections.Concurrent;
using System.Reflection;
using System.Runtime.Serialization;

namespace Baml.Bridge;

internal static class GeneratedContracts
{
    private const string ToolName = "BAML";

    private static readonly ConcurrentDictionary<Type, GeneratedClassContract> Classes = new();
    private static readonly ConcurrentDictionary<Type, GeneratedEnumContract> Enums = new();
    private static readonly ConcurrentDictionary<Type, GeneratedTypeAliasContract> TypeAliases = new();

    internal static GeneratedClassContract? GetClass(Type type)
    {
        if (!IsGenerated(type) || !type.IsClass || type.ContainsGenericParameters)
        {
            return null;
        }

        var dataContract = type.GetCustomAttribute<DataContractAttribute>();
        if (string.IsNullOrWhiteSpace(dataContract?.Name))
        {
            return null;
        }

        return Classes.GetOrAdd(type, static (key, name) => CreateClass(key, name), dataContract.Name);
    }

    internal static GeneratedEnumContract? GetEnum(Type type)
    {
        if (!IsGenerated(type) || !type.IsEnum)
        {
            return null;
        }

        var dataContract = type.GetCustomAttribute<DataContractAttribute>();
        if (string.IsNullOrWhiteSpace(dataContract?.Name))
        {
            return null;
        }

        return Enums.GetOrAdd(type, static (key, name) => CreateEnum(key, name), dataContract.Name);
    }

    internal static GeneratedTypeAliasContract? GetTypeAlias(Type type)
    {
        if (!IsGenerated(type)
            || !type.IsClass
            || type.ContainsGenericParameters
            || !typeof(IBamlTypeAliasValue).IsAssignableFrom(type))
        {
            return null;
        }

        var attribute = type.GetCustomAttribute<BamlTypeAliasAttribute>();
        if (string.IsNullOrWhiteSpace(attribute?.Name))
        {
            return null;
        }

        return TypeAliases.GetOrAdd(type, static (key, name) => CreateTypeAlias(key, name), attribute.Name);
    }

    private static bool IsGenerated(MemberInfo type) =>
        type.GetCustomAttributes<GeneratedCodeAttribute>()
            .Any(static attribute => string.Equals(attribute.Tool, ToolName, StringComparison.Ordinal));

    private static GeneratedClassContract CreateClass(Type type, string wireName)
    {
        var properties = type.GetProperties(BindingFlags.Instance | BindingFlags.Public)
            .Select(static property => (Property: property, Attribute: property.GetCustomAttribute<DataMemberAttribute>()))
            .Where(static item => item.Attribute is not null)
            .Select(static item => new GeneratedPropertyContract(
                item.Attribute!.Name ?? item.Property.Name,
                item.Attribute.Order,
                item.Property))
            .OrderBy(static property => property.Order)
            .ThenBy(static property => property.WireName, StringComparer.Ordinal)
            .ToArray();

        var duplicate = properties.GroupBy(static property => property.WireName, StringComparer.Ordinal)
            .FirstOrDefault(static group => group.Count() > 1);
        if (duplicate is not null)
        {
            throw new BamlBridgeException(
                $"Generated BAML class {type.FullName} has duplicate wire field {duplicate.Key}.");
        }

        foreach (var property in properties)
        {
            if (property.Property.GetMethod is null || property.Property.SetMethod is null)
            {
                throw new BamlBridgeException(
                    $"Generated BAML property {type.FullName}.{property.Property.Name} must have public get/init accessors.");
            }
        }

        return new GeneratedClassContract(type, wireName, properties);
    }

    private static GeneratedEnumContract CreateEnum(Type type, string wireName)
    {
        var members = type.GetFields(BindingFlags.Public | BindingFlags.Static)
            .Select(static field => (Field: field, Attribute: field.GetCustomAttribute<EnumMemberAttribute>()))
            .Where(static item => item.Attribute is not null)
            .Select(static item => new GeneratedEnumMemberContract(
                item.Attribute!.Value ?? item.Field.Name,
                item.Field.GetValue(null)!))
            .ToArray();

        if (members.Length == 0)
        {
            throw new BamlBridgeException($"Generated BAML enum {type.FullName} has no wire members.");
        }

        if (members.Select(static member => member.WireName).Distinct(StringComparer.Ordinal).Count() != members.Length
            || members.Select(static member => member.Value).Distinct().Count() != members.Length)
        {
            throw new BamlBridgeException($"Generated BAML enum {type.FullName} has duplicate wire or CLR values.");
        }

        return new GeneratedEnumContract(type, wireName, members);
    }

    private static GeneratedTypeAliasContract CreateTypeAlias(Type type, string wireName)
    {
        var property = type.GetProperty("Value", BindingFlags.Instance | BindingFlags.Public)
            ?? throw new BamlBridgeException(
                $"Generated BAML type alias {type.FullName} has no public value property.");
        var constructor = type.GetConstructor([property.PropertyType])
            ?? throw new BamlBridgeException(
                $"Generated BAML type alias {type.FullName} has no constructor accepting {property.PropertyType.FullName}.");
        return new GeneratedTypeAliasContract(type, wireName, property, constructor);
    }
}

internal sealed record GeneratedClassContract(
    Type Type,
    string WireName,
    IReadOnlyList<GeneratedPropertyContract> Properties);

internal sealed record GeneratedPropertyContract(string WireName, int Order, PropertyInfo Property);

internal sealed record GeneratedEnumContract(
    Type Type,
    string WireName,
    IReadOnlyList<GeneratedEnumMemberContract> Members)
{
    internal GeneratedEnumMemberContract? FindByValue(object value) =>
        Members.FirstOrDefault(member => member.Value.Equals(value));

    internal GeneratedEnumMemberContract? FindByWireName(string wireName) =>
        Members.FirstOrDefault(member => string.Equals(member.WireName, wireName, StringComparison.Ordinal));
}

internal sealed record GeneratedEnumMemberContract(string WireName, object Value);

internal sealed record GeneratedTypeAliasContract(
    Type Type,
    string WireName,
    PropertyInfo ValueProperty,
    ConstructorInfo Constructor);
