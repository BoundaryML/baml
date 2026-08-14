using System.ComponentModel;

namespace Baml.Generated.V1;

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed class BamlGeneratedHostParameter
{
    private readonly Func<BamlGeneratedValue, object?> decode;
    private readonly Func<object?> createUnset;

    internal BamlGeneratedHostParameter(
        TypeDeclaration type,
        string wireIdentity,
        bool optional,
        Func<BamlGeneratedValue, object?> decode,
        Func<object?> createUnset)
    {
        Type = type;
        WireIdentity = wireIdentity;
        Optional = optional;
        this.decode = decode;
        this.createUnset = createUnset;
    }

    internal TypeDeclaration Type { get; }

    internal string WireIdentity { get; }

    internal bool Optional { get; }

    internal object? Decode(BamlGeneratedValue value) => decode(value);

    internal object? CreateUnset() => createUnset();
}

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed class BamlGeneratedHostResult
{
    private readonly Func<object?, BamlGeneratedValue>? encode;

    internal BamlGeneratedHostResult(
        TypeDeclaration? type,
        Func<object?, BamlGeneratedValue>? encode)
    {
        Type = type;
        this.encode = encode;
    }

    internal TypeDeclaration? Type { get; }

    internal BamlGeneratedValue Encode(object? value) => encode is null
        ? BamlGeneratedValue.CreateNull()
        : encode(value);
}

[EditorBrowsable(EditorBrowsableState.Never)]
public delegate Task<object?> BamlGeneratedHostInvoker(
    Delegate callback,
    IReadOnlyList<object?> arguments,
    CancellationToken cancellationToken);

internal sealed class BamlGeneratedHostCallableDescriptor
{
    internal BamlGeneratedHostCallableDescriptor(
        IReadOnlyList<BamlGeneratedHostParameter> parameters,
        BamlGeneratedHostResult result,
        BamlGeneratedHostInvoker invoke)
    {
        if (parameters.Count > 15)
        {
            throw new ArgumentOutOfRangeException(
                nameof(parameters),
                "A generated C# host callable supports at most 15 BAML parameters because the trailing CLR delegate parameter is CancellationToken.");
        }

        var snapshot = new BamlGeneratedHostParameter[parameters.Count];
        bool sawOptional = false;
        var optionalNames = new HashSet<string>(StringComparer.Ordinal);
        for (int index = 0; index < parameters.Count; index++)
        {
            BamlGeneratedHostParameter parameter = parameters[index]
                ?? throw new ArgumentException(
                    $"Generated host-callable parameter {index} is null.",
                    nameof(parameters));
            if (parameter.Optional)
            {
                sawOptional = true;
                if (string.IsNullOrWhiteSpace(parameter.WireIdentity)
                    || !optionalNames.Add(parameter.WireIdentity))
                {
                    throw new ArgumentException(
                        $"Generated optional host-callable parameter {index} has an empty or duplicate wire identity.",
                        nameof(parameters));
                }
            }
            else if (sawOptional || parameter.WireIdentity.Length != 0)
            {
                throw new ArgumentException(
                    "Generated required host-callable parameters must precede optionals and have no wire identity on the callback wire.",
                    nameof(parameters));
            }

            snapshot[index] = parameter;
        }

        Parameters = Array.AsReadOnly(snapshot);
        Result = result;
        Invoke = invoke;
    }

    internal IReadOnlyList<BamlGeneratedHostParameter> Parameters { get; }

    internal BamlGeneratedHostResult Result { get; }

    internal BamlGeneratedHostInvoker Invoke { get; }
}

internal sealed class BamlGeneratedHostCallable
{
    internal BamlGeneratedHostCallable(
        Delegate callback,
        BamlGeneratedHostCallableDescriptor descriptor)
    {
        Callback = callback;
        Descriptor = descriptor;
    }

    internal Delegate Callback { get; }

    internal BamlGeneratedHostCallableDescriptor Descriptor { get; }
}

[EditorBrowsable(EditorBrowsableState.Never)]
public static class BamlGeneratedHostCallableRuntime
{
    public static async Task<object?> Await<T>(Task<T>? task)
    {
        if (task is null)
        {
            throw new BamlProtocolException(
                "A generated host callback returned a null Task.",
                $"Expected {typeof(Task<T>)}.");
        }

        return await task.ConfigureAwait(false);
    }

    public static async Task<object?> Await(Task? task)
    {
        if (task is null)
        {
            throw new BamlProtocolException(
                "A generated host callback returned a null Task.",
                $"Expected {typeof(Task)}.");
        }

        await task.ConfigureAwait(false);
        return null;
    }
}
