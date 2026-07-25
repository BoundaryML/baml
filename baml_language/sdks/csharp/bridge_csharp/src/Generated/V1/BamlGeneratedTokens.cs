using System.ComponentModel;
using System.Reflection;

namespace Baml.Generated.V1;

[EditorBrowsable(EditorBrowsableState.Never)]
public readonly struct BamlGeneratedType<T>
{
    private readonly RegistryOwner? owner;
    private readonly TypeDeclaration<T>? declaration;

    internal BamlGeneratedType(RegistryOwner owner, TypeDeclaration<T> declaration)
    {
        this.owner = owner;
        this.declaration = declaration;
    }

    internal RegistryOwner Owner => owner
        ?? throw new InvalidOperationException("The default generated type token is invalid.");

    internal TypeDeclaration<T> Declaration => declaration
        ?? throw new InvalidOperationException("The default generated type token is invalid.");
}

[EditorBrowsable(EditorBrowsableState.Never)]
public readonly struct BamlGeneratedTypeArgument
{
    private readonly RegistryOwner? owner;
    private readonly TypeDeclaration? declaration;

    internal BamlGeneratedTypeArgument(RegistryOwner owner, TypeDeclaration declaration)
    {
        this.owner = owner;
        this.declaration = declaration;
    }

    internal RegistryOwner Owner => owner
        ?? throw new InvalidOperationException(
            "The default generated type-argument token is invalid.");

    internal TypeDeclaration Declaration => declaration
        ?? throw new InvalidOperationException(
            "The default generated type-argument token is invalid.");
}

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed class BamlGeneratedTypeFactoryResult
{
    internal BamlGeneratedTypeFactoryResult(
        RegistryOwner owner,
        IResolvedGeneratedType resolved)
    {
        Owner = owner;
        Resolved = resolved;
    }

    internal RegistryOwner Owner { get; }

    internal IResolvedGeneratedType Resolved { get; }
}

[EditorBrowsable(EditorBrowsableState.Never)]
public readonly struct BamlGeneratedFunction<TResult>
{
    private readonly RegistryOwner? owner;
    private readonly FunctionDeclaration? declaration;
    private readonly TypeDeclaration<TResult>? result;

    internal BamlGeneratedFunction(
        RegistryOwner owner,
        FunctionDeclaration declaration,
        TypeDeclaration<TResult> result)
    {
        this.owner = owner;
        this.declaration = declaration;
        this.result = result;
    }

    internal RegistryOwner Owner => owner
        ?? throw new InvalidOperationException("The default generated function token is invalid.");

    internal FunctionDeclaration Declaration => declaration
        ?? throw new InvalidOperationException("The default generated function token is invalid.");

    internal TypeDeclaration<TResult> Result => result
        ?? throw new InvalidOperationException("The default generated function token is invalid.");
}

[EditorBrowsable(EditorBrowsableState.Never)]
public readonly struct BamlGeneratedArgument<TResult, TArgument>
{
    private readonly RegistryOwner? owner;
    private readonly FunctionDeclaration? function;
    private readonly ArgumentDeclaration<TArgument>? declaration;

    internal BamlGeneratedArgument(
        RegistryOwner owner,
        FunctionDeclaration function,
        ArgumentDeclaration<TArgument> declaration)
    {
        this.owner = owner;
        this.function = function;
        this.declaration = declaration;
    }

    internal RegistryOwner Owner => owner
        ?? throw new InvalidOperationException("The default generated argument token is invalid.");

    internal FunctionDeclaration Function => function
        ?? throw new InvalidOperationException("The default generated argument token is invalid.");

    internal ArgumentDeclaration<TArgument> Declaration => declaration
        ?? throw new InvalidOperationException("The default generated argument token is invalid.");
}

[EditorBrowsable(EditorBrowsableState.Never)]
public readonly struct BamlGeneratedGenericFunction
{
    private readonly RegistryOwner? owner;
    private readonly GenericFunctionDeclaration? declaration;

    internal BamlGeneratedGenericFunction(
        RegistryOwner owner,
        GenericFunctionDeclaration declaration)
    {
        this.owner = owner;
        this.declaration = declaration;
    }

    internal RegistryOwner Owner => owner
        ?? throw new InvalidOperationException(
            "The default generated generic function token is invalid.");

    internal GenericFunctionDeclaration Declaration => declaration
        ?? throw new InvalidOperationException(
            "The default generated generic function token is invalid.");
}

[EditorBrowsable(EditorBrowsableState.Never)]
public readonly struct BamlGeneratedTypeParameter
{
    private readonly RegistryOwner? owner;
    private readonly GenericFunctionDeclaration? function;
    private readonly TypeParameterDeclaration? declaration;

    internal BamlGeneratedTypeParameter(
        RegistryOwner owner,
        GenericFunctionDeclaration function,
        TypeParameterDeclaration declaration)
    {
        this.owner = owner;
        this.function = function;
        this.declaration = declaration;
    }

    internal RegistryOwner Owner => owner
        ?? throw new InvalidOperationException(
            "The default generated type-parameter token is invalid.");

    internal GenericFunctionDeclaration Function => function
        ?? throw new InvalidOperationException(
            "The default generated type-parameter token is invalid.");

    internal TypeParameterDeclaration Declaration => declaration
        ?? throw new InvalidOperationException(
            "The default generated type-parameter token is invalid.");
}

[EditorBrowsable(EditorBrowsableState.Never)]
public readonly struct BamlGeneratedGenericArgument
{
    private readonly RegistryOwner? owner;
    private readonly GenericFunctionDeclaration? function;
    private readonly GenericArgumentDeclaration? declaration;

    internal BamlGeneratedGenericArgument(
        RegistryOwner owner,
        GenericFunctionDeclaration function,
        GenericArgumentDeclaration declaration)
    {
        this.owner = owner;
        this.function = function;
        this.declaration = declaration;
    }

    internal RegistryOwner Owner => owner
        ?? throw new InvalidOperationException(
            "The default generated generic-argument token is invalid.");

    internal GenericFunctionDeclaration Function => function
        ?? throw new InvalidOperationException(
            "The default generated generic-argument token is invalid.");

    internal GenericArgumentDeclaration Declaration => declaration
        ?? throw new InvalidOperationException(
            "The default generated generic-argument token is invalid.");
}

[EditorBrowsable(EditorBrowsableState.Never)]
public readonly struct BamlGeneratedTypeBinding
{
    private readonly RegistryOwner? owner;
    private readonly GenericFunctionDeclaration? function;
    private readonly TypeParameterDeclaration? parameter;
    private readonly TypeDeclaration? type;

    internal BamlGeneratedTypeBinding(
        RegistryOwner owner,
        GenericFunctionDeclaration function,
        TypeParameterDeclaration parameter,
        TypeDeclaration type)
    {
        this.owner = owner;
        this.function = function;
        this.parameter = parameter;
        this.type = type;
    }

    internal RegistryOwner Owner => owner
        ?? throw new InvalidOperationException(
            "The default generated type-binding token is invalid.");

    internal GenericFunctionDeclaration Function => function
        ?? throw new InvalidOperationException(
            "The default generated type-binding token is invalid.");

    internal TypeParameterDeclaration Parameter => parameter
        ?? throw new InvalidOperationException(
            "The default generated type-binding token is invalid.");

    internal TypeDeclaration Type => type
        ?? throw new InvalidOperationException(
            "The default generated type-binding token is invalid.");
}

[EditorBrowsable(EditorBrowsableState.Never)]
public readonly struct BamlGeneratedBoundFunction<TResult>
{
    private readonly RegistryOwner? owner;
    private readonly BoundGenericFunctionDeclaration<TResult>? declaration;

    internal BamlGeneratedBoundFunction(
        RegistryOwner owner,
        BoundGenericFunctionDeclaration<TResult> declaration)
    {
        this.owner = owner;
        this.declaration = declaration;
    }

    internal RegistryOwner Owner => owner
        ?? throw new InvalidOperationException(
            "The default bound generated function token is invalid.");

    internal BoundGenericFunctionDeclaration<TResult> Declaration => declaration
        ?? throw new InvalidOperationException(
            "The default bound generated function token is invalid.");
}

internal sealed class RegistryOwner;

internal abstract class TypeDeclaration(int id, string identity, byte[]? metadata = null)
{
    internal int Id { get; } = id;

    internal string Identity { get; } = identity;

    internal byte[] Metadata { get; } = metadata?.ToArray() ?? [];

    internal abstract Type ClrType { get; }
}

internal sealed class TypeDeclaration<T>(int id, string identity, byte[]? metadata = null)
    : TypeDeclaration(id, identity, metadata)
{
    internal override Type ClrType => typeof(T);
}

internal abstract class ArgumentDeclaration(
    string wireIdentity,
    TypeDeclaration type,
    bool optional,
    bool isSelf)
{
    internal string WireIdentity { get; } = wireIdentity;

    internal TypeDeclaration Type { get; } = type;

    internal bool Optional { get; } = optional;

    internal bool IsSelf { get; } = isSelf;
}

internal sealed class ArgumentDeclaration<T>(
    string wireIdentity,
    TypeDeclaration<T> type,
    bool optional,
    bool isSelf)
    : ArgumentDeclaration(wireIdentity, type, optional, isSelf)
{
    internal TypeDeclaration<T> TypedType { get; } = type;
}

internal readonly record struct FunctionIdentity(string BamlIdentity, string Variant);

internal sealed class TypeParameterDeclaration(string wireIdentity, int index)
{
    internal string WireIdentity { get; } = wireIdentity;

    internal int Index { get; } = index;
}

internal sealed class GenericArgumentDeclaration(
    string wireIdentity,
    bool optional,
    bool isSelf,
    int index)
{
    internal string WireIdentity { get; } = wireIdentity;

    internal bool Optional { get; } = optional;

    internal bool IsSelf { get; } = isSelf;

    internal int Index { get; } = index;
}

internal sealed class GenericFunctionDeclaration(
    int id,
    string identity,
    string variant)
{
    private readonly List<TypeParameterDeclaration> typeParameters = [];
    private readonly List<GenericArgumentDeclaration> arguments = [];
    private readonly HashSet<string> typeParameterIdentities = new(StringComparer.Ordinal);
    private readonly HashSet<string> argumentIdentities = new(StringComparer.Ordinal);
    private bool hasSelf;

    internal int Id { get; } = id;

    internal string Identity { get; } = identity;

    internal string Variant { get; } = variant;

    internal IReadOnlyList<TypeParameterDeclaration> TypeParameters => typeParameters;

    internal IReadOnlyList<GenericArgumentDeclaration> Arguments => arguments;

    internal TypeParameterDeclaration AddTypeParameter(string wireIdentity)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(wireIdentity);
        if (!typeParameterIdentities.Add(wireIdentity))
        {
            throw new InvalidOperationException(
                $"Generated type parameter {wireIdentity} is already declared for {Identity} ({Variant}).");
        }

        var declaration = new TypeParameterDeclaration(wireIdentity, typeParameters.Count);
        typeParameters.Add(declaration);
        return declaration;
    }

    internal GenericArgumentDeclaration AddArgument(
        string wireIdentity,
        bool optional,
        bool isSelf)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(wireIdentity);
        if (!argumentIdentities.Add(wireIdentity))
        {
            throw new InvalidOperationException(
                $"Generated argument {wireIdentity} is already declared for {Identity} ({Variant}).");
        }

        if (isSelf && hasSelf)
        {
            throw new InvalidOperationException(
                $"Generated function {Identity} ({Variant}) already has a receiver.");
        }

        if (isSelf && optional)
        {
            throw new InvalidOperationException("A generated receiver cannot be optional.");
        }

        hasSelf |= isSelf;
        var declaration = new GenericArgumentDeclaration(
            wireIdentity,
            optional,
            isSelf,
            arguments.Count);
        arguments.Add(declaration);
        return declaration;
    }

    internal bool Contains(TypeParameterDeclaration parameter) =>
        typeParameters.Any(candidate => ReferenceEquals(candidate, parameter));

    internal bool Contains(GenericArgumentDeclaration argument) =>
        arguments.Any(candidate => ReferenceEquals(candidate, argument));
}

internal sealed class BoundGenericFunctionDeclaration<TResult>(
    GenericFunctionDeclaration definition,
    TypeDeclaration<TResult> result,
    IReadOnlyList<BamlGeneratedTypeBinding> typeBindings)
{
    internal GenericFunctionDeclaration Definition { get; } = definition;

    internal TypeDeclaration<TResult> Result { get; } = result;

    internal IReadOnlyList<BamlGeneratedTypeBinding> TypeBindings { get; } = typeBindings;
}

internal sealed class FunctionDeclaration(
    int id,
    string identity,
    string variant,
    TypeDeclaration result)
{
    private readonly List<ArgumentDeclaration> arguments = [];
    private readonly HashSet<string> wireIdentities = new(StringComparer.Ordinal);
    private bool hasSelf;

    internal int Id { get; } = id;

    internal string Identity { get; } = identity;

    internal string Variant { get; } = variant;

    internal TypeDeclaration Result { get; } = result;

    internal IReadOnlyList<ArgumentDeclaration> Arguments => arguments;

    internal void AddArgument(ArgumentDeclaration argument)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(argument.WireIdentity);
        if (!wireIdentities.Add(argument.WireIdentity))
        {
            throw new InvalidOperationException(
                $"Generated argument {argument.WireIdentity} is already declared for {Identity} ({Variant}).");
        }

        if (argument.IsSelf && hasSelf)
        {
            throw new InvalidOperationException(
                $"Generated function {Identity} ({Variant}) already has a receiver.");
        }

        if (argument.IsSelf && argument.Optional)
        {
            throw new InvalidOperationException("A generated receiver cannot be optional.");
        }

        hasSelf |= argument.IsSelf;
        arguments.Add(argument);
    }

    internal bool ContainsArgument(ArgumentDeclaration argument) =>
        arguments.Any(candidate => ReferenceEquals(candidate, argument));
}

internal interface ICodecBox
{
    BamlGeneratedValue Encode(BamlGeneratedCodecContext context, object? value);

    object? Decode(BamlGeneratedCodecContext context, BamlGeneratedValue value);
}

internal sealed class CodecBox<T>(IBamlGeneratedCodec<T> codec) : ICodecBox
{
    internal BamlGeneratedValue Encode(BamlGeneratedCodecContext context, T value) =>
        codec.Encode(context, value);

    internal T Decode(BamlGeneratedCodecContext context, BamlGeneratedValue value) =>
        codec.Decode(context, value);

    BamlGeneratedValue ICodecBox.Encode(BamlGeneratedCodecContext context, object? value)
    {
        if (value is not T typed)
        {
            throw new BamlTypeMappingException(
                typeof(T),
                "generated dynamic codec",
                "$",
                $"The registered generated codec received {value?.GetType().ToString() ?? "CLR null"}.");
        }

        return codec.Encode(context, typed);
    }

    object? ICodecBox.Decode(BamlGeneratedCodecContext context, BamlGeneratedValue value) =>
        codec.Decode(context, value);
}

internal interface IResolvedGeneratedType
{
    TypeDeclaration Declaration { get; }

    ICodecBox Codec { get; }
}

internal sealed class ResolvedGeneratedType<T>(
    TypeDeclaration<T> declaration,
    CodecBox<T> codec) : IResolvedGeneratedType
{
    public TypeDeclaration Declaration { get; } = declaration;

    public ICodecBox Codec { get; } = codec;
}

internal sealed class UntypedResolvedGeneratedType(
    TypeDeclaration declaration,
    ICodecBox codec) : IResolvedGeneratedType
{
    public TypeDeclaration Declaration { get; } = declaration;

    public ICodecBox Codec { get; } = codec;
}

internal sealed class GenericTypeFactoryDeclaration(
    Type genericTypeDefinition,
    MethodInfo factory)
{
    internal Type GenericTypeDefinition { get; } = genericTypeDefinition;

    internal MethodInfo Factory { get; } = factory;
}
