using System.Collections.ObjectModel;
using System.ComponentModel;

namespace Baml.Generated.V1;

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed class BamlGeneratedArgumentsBuilder<TResult>
{
    private readonly BamlGeneratedRegistry registry;
    private readonly FunctionDeclaration function;
    private readonly Dictionary<ArgumentDeclaration, BamlGeneratedValue> values = [];
    private readonly HashSet<ArgumentDeclaration> omitted = [];
    private bool built;

    internal BamlGeneratedArgumentsBuilder(
        BamlGeneratedRegistry registry,
        FunctionDeclaration function)
    {
        this.registry = registry;
        this.function = function;
    }

    public void Add<TArgument>(
        BamlGeneratedArgument<TResult, TArgument> argument,
        TArgument value)
    {
        EnsureMutable();
        RequireArgument(argument.Owner, argument.Function, argument.Declaration);
        if (omitted.Contains(argument.Declaration))
        {
            throw new InvalidOperationException(
                "An omitted generated argument cannot also be supplied.");
        }

        if (!values.TryAdd(
                argument.Declaration,
                registry.Encode(argument.Declaration.TypedType, value)))
        {
            throw new InvalidOperationException(
                "The generated argument was already supplied.");
        }
    }

    public void Omit<TArgument>(BamlGeneratedArgument<TResult, TArgument> argument)
    {
        EnsureMutable();
        RequireArgument(argument.Owner, argument.Function, argument.Declaration);
        if (!argument.Declaration.Optional)
        {
            throw new InvalidOperationException(
                "A required generated argument cannot be omitted.");
        }

        if (values.ContainsKey(argument.Declaration)
            || !omitted.Add(argument.Declaration))
        {
            throw new InvalidOperationException(
                "The generated argument was already supplied or omitted.");
        }
    }

    public BamlGeneratedArguments<TResult> Build()
    {
        EnsureMutable();
        foreach (ArgumentDeclaration argument in function.Arguments)
        {
            if (!argument.Optional && !values.ContainsKey(argument))
            {
                throw new InvalidOperationException(
                    $"Required generated argument {argument.WireIdentity} was not supplied.");
            }
        }

        built = true;
        return new BamlGeneratedArguments<TResult>(
            registry,
            function,
            new Dictionary<ArgumentDeclaration, BamlGeneratedValue>(values));
    }

    private void RequireArgument(
        RegistryOwner owner,
        FunctionDeclaration argumentFunction,
        ArgumentDeclaration argument)
    {
        if (!ReferenceEquals(owner, registry.Owner)
            || !ReferenceEquals(argumentFunction, function))
        {
            throw new InvalidOperationException(
                "The generated argument token belongs to another function or registry.");
        }

        registry.RequireArgument(function, argument);
    }

    private void EnsureMutable()
    {
        if (built)
        {
            throw new InvalidOperationException(
                "The generated arguments builder is already frozen.");
        }
    }
}

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed class BamlGeneratedArguments<TResult>
{
    private readonly IReadOnlyDictionary<ArgumentDeclaration, BamlGeneratedValue> values;

    internal BamlGeneratedArguments(
        BamlGeneratedRegistry registry,
        FunctionDeclaration function,
        Dictionary<ArgumentDeclaration, BamlGeneratedValue> values)
    {
        Registry = registry;
        Function = function;
        this.values =
            new ReadOnlyDictionary<ArgumentDeclaration, BamlGeneratedValue>(values);
    }

    internal BamlGeneratedRegistry Registry { get; }

    internal FunctionDeclaration Function { get; }

    internal IEnumerable<(ArgumentDeclaration Argument, BamlGeneratedValue Value)> Supplied()
    {
        foreach (ArgumentDeclaration argument in Function.Arguments)
        {
            if (values.TryGetValue(argument, out BamlGeneratedValue? value))
            {
                yield return (argument, value);
            }
        }
    }

    internal BamlGeneratedArguments<TResult> SnapshotForDeferredCall(
        out IDisposable ownership)
    {
        var resources = new List<IDisposable>();
        try
        {
            var snapshot = new Dictionary<ArgumentDeclaration, BamlGeneratedValue>();
            foreach ((ArgumentDeclaration argument, BamlGeneratedValue value) in values)
            {
                snapshot.Add(argument, value.SnapshotForDeferredCall(resources));
            }

            ownership = new DeferredCallOwnership(resources);
            return new BamlGeneratedArguments<TResult>(Registry, Function, snapshot);
        }
        catch
        {
            DeferredCallOwnership.DisposeAll(resources);
            throw;
        }
    }
}

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed class BamlGeneratedGenericArgumentsBuilder<TResult>
{
    private readonly BamlGeneratedRegistry registry;
    private readonly BoundGenericFunctionDeclaration<TResult> function;
    private readonly Dictionary<GenericArgumentDeclaration, BamlGeneratedValue> values = [];
    private readonly HashSet<GenericArgumentDeclaration> omitted = [];
    private bool built;

    internal BamlGeneratedGenericArgumentsBuilder(
        BamlGeneratedRegistry registry,
        BoundGenericFunctionDeclaration<TResult> function)
    {
        this.registry = registry;
        this.function = function;
    }

    public void Add<TArgument>(
        BamlGeneratedGenericArgument argument,
        BamlGeneratedType<TArgument> type,
        TArgument value)
    {
        EnsureMutable();
        RequireArgument(argument);
        if (omitted.Contains(argument.Declaration))
        {
            throw new InvalidOperationException(
                "An omitted generated generic argument cannot also be supplied.");
        }

        if (!values.TryAdd(argument.Declaration, registry.Encode(type, value)))
        {
            throw new InvalidOperationException(
                "The generated generic argument was already supplied.");
        }
    }

    public void AddHostCallable<TDelegate>(
        BamlGeneratedGenericArgument argument,
        TDelegate callback,
        Func<BamlGeneratedCodecContext, TDelegate, BamlGeneratedValue> encode)
        where TDelegate : Delegate
    {
        EnsureMutable();
        RequireArgument(argument);
        ArgumentNullException.ThrowIfNull(callback);
        ArgumentNullException.ThrowIfNull(encode);
        if (omitted.Contains(argument.Declaration))
        {
            throw new InvalidOperationException(
                "An omitted generated generic argument cannot also be supplied.");
        }

        BamlGeneratedValue value = encode(
            new BamlGeneratedCodecContext(registry),
            callback);
        ArgumentNullException.ThrowIfNull(value);
        if (!values.TryAdd(argument.Declaration, value))
        {
            throw new InvalidOperationException(
                "The generated generic argument was already supplied.");
        }
    }

    public void Omit(BamlGeneratedGenericArgument argument)
    {
        EnsureMutable();
        RequireArgument(argument);
        if (!argument.Declaration.Optional)
        {
            throw new InvalidOperationException(
                "A required generated generic argument cannot be omitted.");
        }

        if (values.ContainsKey(argument.Declaration)
            || !omitted.Add(argument.Declaration))
        {
            throw new InvalidOperationException(
                "The generated generic argument was already supplied or omitted.");
        }
    }

    public BamlGeneratedGenericArguments<TResult> Build()
    {
        EnsureMutable();
        foreach (GenericArgumentDeclaration argument in function.Definition.Arguments)
        {
            if (!argument.Optional && !values.ContainsKey(argument))
            {
                throw new InvalidOperationException(
                    $"Required generated argument {argument.WireIdentity} was not supplied.");
            }
        }

        built = true;
        return new BamlGeneratedGenericArguments<TResult>(
            registry,
            function,
            new Dictionary<GenericArgumentDeclaration, BamlGeneratedValue>(values));
    }

    private void RequireArgument(BamlGeneratedGenericArgument argument)
    {
        if (!ReferenceEquals(argument.Owner, registry.Owner)
            || !ReferenceEquals(argument.Function, function.Definition))
        {
            throw new InvalidOperationException(
                "The generated generic argument belongs to another function or registry.");
        }

        registry.RequireGenericArgument(function.Definition, argument.Declaration);
    }

    private void EnsureMutable()
    {
        if (built)
        {
            throw new InvalidOperationException(
                "The generated generic arguments builder is already frozen.");
        }
    }
}

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed class BamlGeneratedGenericArguments<TResult>
{
    private readonly IReadOnlyDictionary<GenericArgumentDeclaration, BamlGeneratedValue> values;

    internal BamlGeneratedGenericArguments(
        BamlGeneratedRegistry registry,
        BoundGenericFunctionDeclaration<TResult> function,
        Dictionary<GenericArgumentDeclaration, BamlGeneratedValue> values)
    {
        Registry = registry;
        Function = function;
        this.values =
            new ReadOnlyDictionary<GenericArgumentDeclaration, BamlGeneratedValue>(values);
    }

    internal BamlGeneratedRegistry Registry { get; }

    internal BoundGenericFunctionDeclaration<TResult> Function { get; }

    internal IEnumerable<(GenericArgumentDeclaration Argument, BamlGeneratedValue Value)> Supplied()
    {
        foreach (GenericArgumentDeclaration argument in Function.Definition.Arguments)
        {
            if (values.TryGetValue(argument, out BamlGeneratedValue? value))
            {
                yield return (argument, value);
            }
        }
    }

    internal BamlGeneratedGenericArguments<TResult> SnapshotForDeferredCall(
        out IDisposable ownership)
    {
        var resources = new List<IDisposable>();
        try
        {
            var snapshot = new Dictionary<GenericArgumentDeclaration, BamlGeneratedValue>();
            foreach ((GenericArgumentDeclaration argument, BamlGeneratedValue value) in values)
            {
                snapshot.Add(argument, value.SnapshotForDeferredCall(resources));
            }

            ownership = new DeferredCallOwnership(resources);
            return new BamlGeneratedGenericArguments<TResult>(Registry, Function, snapshot);
        }
        catch
        {
            DeferredCallOwnership.DisposeAll(resources);
            throw;
        }
    }
}

internal sealed class DeferredCallOwnership(IReadOnlyList<IDisposable> resources)
    : IDisposable
{
    private IReadOnlyList<IDisposable>? resources = resources;

    public void Dispose()
    {
        IReadOnlyList<IDisposable>? current = Interlocked.Exchange(ref resources, null);
        if (current is not null)
        {
            DisposeAll(current);
        }
    }

    internal static void DisposeAll(IEnumerable<IDisposable> resources)
    {
        foreach (IDisposable resource in resources.Reverse())
        {
            resource.Dispose();
        }
    }
}
