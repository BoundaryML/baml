using System.Diagnostics.CodeAnalysis;
using System.Reflection;

namespace Baml.Generated;

public sealed class RootedPerson
{
    public RootedPerson()
    {
    }

    public string Name { get; init; } = String.Empty;
}

public sealed class UnrootedPerson
{
    public UnrootedPerson()
    {
    }

    public string Name { get; init; } = String.Empty;
}

internal static class Program
{
    private const DynamicallyAccessedMemberTypes UserReflectionMembers =
        DynamicallyAccessedMemberTypes.PublicConstructors
        | DynamicallyAccessedMemberTypes.PublicProperties;

    public static int Main(string[] args)
    {
        if (args.Length != 1 || args[0] is not ("rooted" or "unrooted"))
        {
            Console.Error.WriteLine(
                "usage: Baml.Bridge.ReflectionRootProbe <rooted|unrooted>");
            return 2;
        }

        if (args[0] == "rooted")
        {
            VerifyUserOwnedRoot(typeof(RootedPerson));
            Console.WriteLine("reflection_root=public_constructor_and_properties");
            return 0;
        }

        VerifyUnrootedTypeWasTrimmed();
        Console.WriteLine("reflection_unrooted=removed");
        return 0;
    }

    private static void VerifyUserOwnedRoot(
        [DynamicallyAccessedMembers(UserReflectionMembers)] Type type)
    {
        ConstructorInfo constructor =
            type.GetConstructor(Type.EmptyTypes)
            ?? throw new InvalidOperationException(
                "rooted public constructor was removed");
        PropertyInfo property =
            type.GetProperty(
                "Name",
                BindingFlags.Instance | BindingFlags.Public)
            ?? throw new InvalidOperationException(
                "rooted public property was removed");
        object instance = constructor.Invoke(parameters: null);
        property.SetValue(instance, "Ada");
        Require(
            StringComparer.Ordinal.Equals(
                property.GetValue(instance) as string,
                "Ada"),
            "rooted reflected property did not round trip");
    }

    [UnconditionalSuppressMessage(
        "Trimming",
        "IL2026",
        Justification =
            "This negative fixture deliberately performs unrooted application reflection and asserts that trimming removed the type.")]
    private static void VerifyUnrootedTypeWasTrimmed()
    {
        string typeName = String.Concat(
            "Baml.Generated.",
            "Unrooted",
            "Person");
        Type? type = Assembly.GetExecutingAssembly().GetType(
            typeName,
            throwOnError: false);
        Require(type is null, "unrooted reflection-only type survived trimming");
    }

    private static void Require(bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidOperationException(message);
        }
    }
}
