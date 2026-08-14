using System.Collections.ObjectModel;
using System.ComponentModel;
using System.Numerics;
using System.Security.Cryptography;

namespace Baml.Generated.V1;

[EditorBrowsable(EditorBrowsableState.Never)]
public static class BamlGeneratedContract
{
    public const int ContractVersion = 1;
    public const string RuntimePackageVersion = "0.0.0-a3";
    public const string BridgeVersion = "bridge-v1";
    public const long MinimumInteger = -9_007_199_254_740_991;
    public const long MaximumInteger = 9_007_199_254_740_991;

    public static BamlGeneratedRegistryBuilder CreateRegistryBuilder(
        int generatedContractVersion,
        string generatedRuntimePackageVersion,
        string requiredBridgeVersion)
    {
        RequireVersions(
            generatedContractVersion,
            generatedRuntimePackageVersion,
            requiredBridgeVersion);
        return new BamlGeneratedRegistryBuilder(new RegistryOwner());
    }

    public static BamlGeneratedProgram RegisterProgram(
        int generatedContractVersion,
        string generatedRuntimePackageVersion,
        string requiredBridgeVersion,
        ReadOnlyMemory<byte> bytecode,
        string fingerprint,
        BamlGeneratedRegistry registry)
    {
        // Generated/runtime/bridge compatibility is deliberately checked before
        // bytecode, fingerprint, or registry state is observed.
        RequireVersions(
            generatedContractVersion,
            generatedRuntimePackageVersion,
            requiredBridgeVersion);

        ArgumentException.ThrowIfNullOrWhiteSpace(fingerprint);
        ArgumentNullException.ThrowIfNull(registry);
        if (bytecode.IsEmpty)
        {
            throw new InvalidOperationException(
                "Generated BAML bytecode must not be empty.");
        }

        string actualFingerprint = Convert
            .ToHexString(SHA256.HashData(bytecode.Span))
            .ToLowerInvariant();
        if (!StringComparer.Ordinal.Equals(fingerprint, actualFingerprint))
        {
            throw new InvalidOperationException(
                "Generated BAML bytecode fingerprint mismatch.");
        }

        return new BamlGeneratedProgram(
            registry,
            fingerprint,
            generatedRuntimePackageVersion,
            requiredBridgeVersion);
    }

    private static void RequireVersions(
        int generatedContractVersion,
        string generatedRuntimePackageVersion,
        string requiredBridgeVersion)
    {
        if (generatedContractVersion != ContractVersion)
        {
            throw new NotSupportedException(
                $"Generated-code contract {generatedContractVersion} is incompatible with runtime contract {ContractVersion}.");
        }

        if (!StringComparer.Ordinal.Equals(
                generatedRuntimePackageVersion,
                RuntimePackageVersion))
        {
            throw new NotSupportedException(
                $"Generated runtime package {generatedRuntimePackageVersion} is incompatible with runtime package {RuntimePackageVersion}.");
        }

        if (!StringComparer.Ordinal.Equals(
                requiredBridgeVersion,
                BridgeVersion))
        {
            throw new NotSupportedException(
                $"Generated bridge requirement {requiredBridgeVersion} is incompatible with bridge {BridgeVersion}.");
        }
    }
}

[EditorBrowsable(EditorBrowsableState.Never)]
public readonly struct BamlGeneratedType<T>
{
    private readonly RegistryOwner? _owner;
    private readonly TypeDeclaration<T>? _declaration;

    internal BamlGeneratedType(
        RegistryOwner owner,
        TypeDeclaration<T> declaration)
    {
        _owner = owner;
        _declaration = declaration;
    }

    internal RegistryOwner Owner =>
        _owner
        ?? throw new InvalidOperationException(
            "The default generated type token is invalid.");

    internal TypeDeclaration<T> Declaration =>
        _declaration
        ?? throw new InvalidOperationException(
            "The default generated type token is invalid.");
}

[EditorBrowsable(EditorBrowsableState.Never)]
public readonly struct BamlGeneratedFunction<TResult>
{
    private readonly RegistryOwner? _owner;
    private readonly FunctionDeclaration? _declaration;
    private readonly TypeDeclaration<TResult>? _resultDeclaration;

    internal BamlGeneratedFunction(
        RegistryOwner owner,
        FunctionDeclaration declaration,
        TypeDeclaration<TResult> resultDeclaration)
    {
        _owner = owner;
        _declaration = declaration;
        _resultDeclaration = resultDeclaration;
    }

    internal RegistryOwner Owner =>
        _owner
        ?? throw new InvalidOperationException(
            "The default generated function token is invalid.");

    internal FunctionDeclaration Declaration =>
        _declaration
        ?? throw new InvalidOperationException(
            "The default generated function token is invalid.");

    internal TypeDeclaration<TResult> ResultDeclaration =>
        _resultDeclaration
        ?? throw new InvalidOperationException(
            "The default generated function token is invalid.");
}

[EditorBrowsable(EditorBrowsableState.Never)]
public readonly struct BamlGeneratedStreamFunction<TPartial, TFinal>
{
    private readonly RegistryOwner? _owner;
    private readonly FunctionDeclaration? _declaration;
    private readonly TypeDeclaration<TPartial>? _partialDeclaration;
    private readonly TypeDeclaration<TFinal>? _finalDeclaration;

    internal BamlGeneratedStreamFunction(
        RegistryOwner owner,
        FunctionDeclaration declaration,
        TypeDeclaration<TPartial> partialDeclaration,
        TypeDeclaration<TFinal> finalDeclaration)
    {
        _owner = owner;
        _declaration = declaration;
        _partialDeclaration = partialDeclaration;
        _finalDeclaration = finalDeclaration;
    }

    internal RegistryOwner Owner =>
        _owner
        ?? throw new InvalidOperationException(
            "The default generated stream function token is invalid.");

    internal FunctionDeclaration Declaration =>
        _declaration
        ?? throw new InvalidOperationException(
            "The default generated stream function token is invalid.");

    internal TypeDeclaration<TPartial> PartialDeclaration =>
        _partialDeclaration
        ?? throw new InvalidOperationException(
            "The default generated stream function token is invalid.");

    internal TypeDeclaration<TFinal> FinalDeclaration =>
        _finalDeclaration
        ?? throw new InvalidOperationException(
            "The default generated stream function token is invalid.");
}

[EditorBrowsable(EditorBrowsableState.Never)]
public readonly struct BamlGeneratedGenericFunction
{
    private readonly RegistryOwner? _owner;
    private readonly FunctionDeclaration? _declaration;

    internal BamlGeneratedGenericFunction(
        RegistryOwner owner,
        FunctionDeclaration declaration)
    {
        _owner = owner;
        _declaration = declaration;
    }

    internal RegistryOwner Owner =>
        _owner
        ?? throw new InvalidOperationException(
            "The default generated generic function token is invalid.");

    internal FunctionDeclaration Declaration =>
        _declaration
        ?? throw new InvalidOperationException(
            "The default generated generic function token is invalid.");
}

[EditorBrowsable(EditorBrowsableState.Never)]
public readonly struct BamlGeneratedResultTypeParameter
{
    private readonly RegistryOwner? _owner;
    private readonly FunctionDeclaration? _function;
    private readonly ResultTypeParameterDeclaration? _declaration;

    internal BamlGeneratedResultTypeParameter(
        RegistryOwner owner,
        FunctionDeclaration function,
        ResultTypeParameterDeclaration declaration)
    {
        _owner = owner;
        _function = function;
        _declaration = declaration;
    }

    internal RegistryOwner Owner =>
        _owner
        ?? throw new InvalidOperationException(
            "The default generated result type parameter token is invalid.");

    internal FunctionDeclaration Function =>
        _function
        ?? throw new InvalidOperationException(
            "The default generated result type parameter token is invalid.");

    internal ResultTypeParameterDeclaration Declaration =>
        _declaration
        ?? throw new InvalidOperationException(
            "The default generated result type parameter token is invalid.");
}

[EditorBrowsable(EditorBrowsableState.Never)]
public readonly struct BamlGeneratedTypeBinding<T>
{
    private readonly RegistryOwner? _owner;
    private readonly FunctionDeclaration? _function;
    private readonly ResultTypeParameterDeclaration? _parameter;
    private readonly TypeDeclaration<T>? _type;

    internal BamlGeneratedTypeBinding(
        RegistryOwner owner,
        FunctionDeclaration function,
        ResultTypeParameterDeclaration parameter,
        TypeDeclaration<T> type)
    {
        _owner = owner;
        _function = function;
        _parameter = parameter;
        _type = type;
    }

    internal RegistryOwner Owner =>
        _owner
        ?? throw new InvalidOperationException(
            "The default generated type binding token is invalid.");

    internal FunctionDeclaration Function =>
        _function
        ?? throw new InvalidOperationException(
            "The default generated type binding token is invalid.");

    internal ResultTypeParameterDeclaration Parameter =>
        _parameter
        ?? throw new InvalidOperationException(
            "The default generated type binding token is invalid.");

    internal TypeDeclaration<T> Type =>
        _type
        ?? throw new InvalidOperationException(
            "The default generated type binding token is invalid.");
}

[EditorBrowsable(EditorBrowsableState.Never)]
public readonly struct BamlGeneratedArgument<TResult, TArgument>
{
    private readonly RegistryOwner? _owner;
    private readonly FunctionDeclaration? _function;
    private readonly ArgumentDeclaration<TArgument>? _declaration;

    internal BamlGeneratedArgument(
        RegistryOwner owner,
        FunctionDeclaration function,
        ArgumentDeclaration<TArgument> declaration)
    {
        _owner = owner;
        _function = function;
        _declaration = declaration;
    }

    internal RegistryOwner Owner =>
        _owner
        ?? throw new InvalidOperationException(
            "The default generated argument token is invalid.");

    internal FunctionDeclaration Function =>
        _function
        ?? throw new InvalidOperationException(
            "The default generated argument token is invalid.");

    internal ArgumentDeclaration<TArgument> Declaration =>
        _declaration
        ?? throw new InvalidOperationException(
            "The default generated argument token is invalid.");
}

[EditorBrowsable(EditorBrowsableState.Never)]
public readonly struct BamlGeneratedStreamArgument<TPartial, TFinal, TArgument>
{
    private readonly RegistryOwner? _owner;
    private readonly FunctionDeclaration? _function;
    private readonly ArgumentDeclaration<TArgument>? _declaration;

    internal BamlGeneratedStreamArgument(
        RegistryOwner owner,
        FunctionDeclaration function,
        ArgumentDeclaration<TArgument> declaration)
    {
        _owner = owner;
        _function = function;
        _declaration = declaration;
    }

    internal RegistryOwner Owner =>
        _owner
        ?? throw new InvalidOperationException(
            "The default generated stream argument token is invalid.");

    internal FunctionDeclaration Function =>
        _function
        ?? throw new InvalidOperationException(
            "The default generated stream argument token is invalid.");

    internal ArgumentDeclaration<TArgument> Declaration =>
        _declaration
        ?? throw new InvalidOperationException(
            "The default generated stream argument token is invalid.");
}

[EditorBrowsable(EditorBrowsableState.Never)]
public interface IBamlGeneratedCodec<T>
{
    BamlGeneratedValue Encode(BamlGeneratedCodecContext context, T value);

    T Decode(BamlGeneratedCodecContext context, BamlGeneratedValue value);
}

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed class BamlGeneratedRegistryBuilder
{
    private readonly RegistryOwner _owner;
    private readonly Dictionary<int, TypeDeclaration> _types = [];
    private readonly Dictionary<int, ICodecBox> _codecs = [];
    private readonly Dictionary<int, FunctionDeclaration> _functions = [];
    private readonly HashSet<string> _typeIdentities =
        new(StringComparer.Ordinal);
    private readonly HashSet<FunctionIdentity> _functionIdentities = [];
    private int _nextTypeId = 1;
    private int _nextFunctionId = 1;
    private bool _built;

    internal BamlGeneratedRegistryBuilder(RegistryOwner owner)
    {
        _owner = owner;
    }

    public BamlGeneratedType<T> DeclareType<T>(string bamlIdentity)
    {
        EnsureMutable();
        ArgumentException.ThrowIfNullOrWhiteSpace(bamlIdentity);
        if (!_typeIdentities.Add(bamlIdentity))
        {
            throw new InvalidOperationException(
                $"Generated BAML type {bamlIdentity} is already declared.");
        }

        int id = checked(_nextTypeId++);
        var declaration = new TypeDeclaration<T>(id, bamlIdentity);
        _types.Add(id, declaration);
        return new BamlGeneratedType<T>(_owner, declaration);
    }

    public void RegisterCodec<T>(
        BamlGeneratedType<T> type,
        IBamlGeneratedCodec<T> codec)
    {
        EnsureMutable();
        ArgumentNullException.ThrowIfNull(codec);
        TypeDeclaration<T> declaration = RequireType(type);
        if (!_codecs.TryAdd(declaration.Id, new CodecBox<T>(codec)))
        {
            throw new InvalidOperationException(
                $"A codec is already registered for {declaration.Identity}.");
        }
    }

    public BamlGeneratedFunction<TResult> DeclareFunction<TResult>(
        string bamlIdentity,
        string variant,
        BamlGeneratedType<TResult> resultType)
    {
        EnsureMutable();
        TypeDeclaration<TResult> result = RequireType(resultType);
        FunctionDeclaration function = AddFunction(
            bamlIdentity,
            variant,
            result,
            null,
            null);
        return new BamlGeneratedFunction<TResult>(_owner, function, result);
    }

    public BamlGeneratedStreamFunction<TPartial, TFinal> DeclareStreamFunction<TPartial, TFinal>(
        string bamlIdentity,
        string variant,
        BamlGeneratedType<TPartial> partialType,
        BamlGeneratedType<TFinal> finalType)
    {
        EnsureMutable();
        TypeDeclaration<TPartial> partial = RequireType(partialType);
        TypeDeclaration<TFinal> final = RequireType(finalType);
        FunctionDeclaration function = AddFunction(
            bamlIdentity,
            variant,
            null,
            partial,
            final);
        return new BamlGeneratedStreamFunction<TPartial, TFinal>(
            _owner,
            function,
            partial,
            final);
    }

    public BamlGeneratedGenericFunction DeclareResultGenericFunction(
        string bamlIdentity,
        string variant,
        string resultTypeParameter,
        out BamlGeneratedResultTypeParameter parameter)
    {
        EnsureMutable();
        ArgumentException.ThrowIfNullOrWhiteSpace(resultTypeParameter);
        var resultParameter =
            new ResultTypeParameterDeclaration(resultTypeParameter);
        FunctionDeclaration function = AddFunction(
            bamlIdentity,
            variant,
            null,
            null,
            null);
        function.ResultTypeParameter = resultParameter;
        parameter = new BamlGeneratedResultTypeParameter(
            _owner,
            function,
            resultParameter);
        return new BamlGeneratedGenericFunction(_owner, function);
    }

    public BamlGeneratedArgument<TResult, TArgument> DeclareArgument<TResult, TArgument>(
        BamlGeneratedFunction<TResult> function,
        string wireIdentity,
        BamlGeneratedType<TArgument> type,
        bool optional = false,
        bool isSelf = false)
    {
        EnsureMutable();
        FunctionDeclaration declaration = RequireFunction(function);
        TypeDeclaration<TArgument> argumentType = RequireType(type);
        var argument = new ArgumentDeclaration<TArgument>(
            wireIdentity,
            argumentType,
            optional,
            isSelf);
        declaration.AddArgument(argument);
        return new BamlGeneratedArgument<TResult, TArgument>(
            _owner,
            declaration,
            argument);
    }

    public BamlGeneratedStreamArgument<TPartial, TFinal, TArgument>
        DeclareStreamArgument<TPartial, TFinal, TArgument>(
            BamlGeneratedStreamFunction<TPartial, TFinal> function,
            string wireIdentity,
            BamlGeneratedType<TArgument> type,
            bool optional = false,
            bool isSelf = false)
    {
        EnsureMutable();
        FunctionDeclaration declaration = RequireStreamFunction(function);
        TypeDeclaration<TArgument> argumentType = RequireType(type);
        var argument = new ArgumentDeclaration<TArgument>(
            wireIdentity,
            argumentType,
            optional,
            isSelf);
        declaration.AddArgument(argument);
        return new BamlGeneratedStreamArgument<TPartial, TFinal, TArgument>(
            _owner,
            declaration,
            argument);
    }

    public BamlGeneratedRegistry Build()
    {
        EnsureMutable();
        foreach (TypeDeclaration declaration in _types.Values)
        {
            if (!_codecs.ContainsKey(declaration.Id))
            {
                throw new InvalidOperationException(
                    $"No generated codec was registered for {declaration.Identity}.");
            }
        }

        _built = true;
        return new BamlGeneratedRegistry(
            _owner,
            new Dictionary<int, TypeDeclaration>(_types),
            new Dictionary<int, ICodecBox>(_codecs),
            new Dictionary<int, FunctionDeclaration>(_functions));
    }

    private FunctionDeclaration AddFunction(
        string bamlIdentity,
        string variant,
        TypeDeclaration? result,
        TypeDeclaration? partial,
        TypeDeclaration? final)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(bamlIdentity);
        ArgumentException.ThrowIfNullOrWhiteSpace(variant);
        var identity = new FunctionIdentity(bamlIdentity, variant);
        if (!_functionIdentities.Add(identity))
        {
            throw new InvalidOperationException(
                $"Generated BAML function {bamlIdentity} variant {variant} is already declared.");
        }

        int id = checked(_nextFunctionId++);
        var function = new FunctionDeclaration(
            id,
            bamlIdentity,
            variant,
            result,
            partial,
            final);
        _functions.Add(id, function);
        return function;
    }

    private TypeDeclaration<T> RequireType<T>(BamlGeneratedType<T> type)
    {
        if (!ReferenceEquals(type.Owner, _owner)
            || !_types.TryGetValue(type.Declaration.Id, out TypeDeclaration? stored)
            || !ReferenceEquals(stored, type.Declaration))
        {
            throw new InvalidOperationException(
                "The generated type token does not belong to this registry builder.");
        }

        return type.Declaration;
    }

    private FunctionDeclaration RequireFunction<TResult>(
        BamlGeneratedFunction<TResult> function)
    {
        if (!ReferenceEquals(function.Owner, _owner)
            || !_functions.TryGetValue(
                function.Declaration.Id,
                out FunctionDeclaration? stored)
            || !ReferenceEquals(stored, function.Declaration)
            || !ReferenceEquals(
                stored.ResultDeclaration,
                function.ResultDeclaration))
        {
            throw new InvalidOperationException(
                "The generated function token does not belong to this registry builder.");
        }

        return stored;
    }

    private FunctionDeclaration RequireStreamFunction<TPartial, TFinal>(
        BamlGeneratedStreamFunction<TPartial, TFinal> function)
    {
        if (!ReferenceEquals(function.Owner, _owner)
            || !_functions.TryGetValue(
                function.Declaration.Id,
                out FunctionDeclaration? stored)
            || !ReferenceEquals(
                stored.PartialDeclaration,
                function.PartialDeclaration)
            || !ReferenceEquals(
                stored.FinalDeclaration,
                function.FinalDeclaration))
        {
            throw new InvalidOperationException(
                "The generated stream function token does not belong to this registry builder.");
        }

        return stored;
    }

    private void EnsureMutable()
    {
        if (_built)
        {
            throw new InvalidOperationException(
                "The generated registry builder is already frozen.");
        }
    }
}

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed class BamlGeneratedRegistry
{
    private readonly RegistryOwner _owner;
    private readonly IReadOnlyDictionary<int, TypeDeclaration> _types;
    private readonly IReadOnlyDictionary<int, ICodecBox> _codecs;
    private readonly IReadOnlyDictionary<int, FunctionDeclaration> _functions;

    internal BamlGeneratedRegistry(
        RegistryOwner owner,
        Dictionary<int, TypeDeclaration> types,
        Dictionary<int, ICodecBox> codecs,
        Dictionary<int, FunctionDeclaration> functions)
    {
        _owner = owner;
        _types = new ReadOnlyDictionary<int, TypeDeclaration>(types);
        _codecs = new ReadOnlyDictionary<int, ICodecBox>(codecs);
        _functions =
            new ReadOnlyDictionary<int, FunctionDeclaration>(functions);
    }

    public BamlGeneratedValue Encode<T>(
        BamlGeneratedType<T> type,
        T value) =>
        GetCodec(RequireType(type)).Encode(
            new BamlGeneratedCodecContext(this),
            value);

    public T Decode<T>(
        BamlGeneratedType<T> type,
        BamlGeneratedValue value)
    {
        ArgumentNullException.ThrowIfNull(value);
        return GetCodec(RequireType(type)).Decode(
            new BamlGeneratedCodecContext(this),
            value);
    }

    public BamlGeneratedArgumentsBuilder<TResult> CreateArgumentsBuilder<TResult>(
        BamlGeneratedFunction<TResult> function) =>
        new(this, RequireFunction(function));

    public BamlGeneratedStreamArgumentsBuilder<TPartial, TFinal>
        CreateArgumentsBuilder<TPartial, TFinal>(
            BamlGeneratedStreamFunction<TPartial, TFinal> function) =>
            new(this, RequireStreamFunction(function));

    public BamlGeneratedTypeBinding<T> BindResultType<T>(
        BamlGeneratedResultTypeParameter parameter,
        BamlGeneratedType<T> type)
    {
        FunctionDeclaration function =
            RequireResultTypeParameter(parameter);
        TypeDeclaration<T> typeDeclaration = RequireType(type);
        return new BamlGeneratedTypeBinding<T>(
            _owner,
            function,
            parameter.Declaration,
            typeDeclaration);
    }

    public BamlGeneratedFunction<T> BindResult<T>(
        BamlGeneratedGenericFunction function,
        BamlGeneratedTypeBinding<T> binding)
    {
        FunctionDeclaration declaration =
            RequireGenericFunction(function);
        if (!ReferenceEquals(binding.Owner, _owner)
            || !ReferenceEquals(binding.Function, declaration)
            || !ReferenceEquals(
                declaration.ResultTypeParameter,
                binding.Parameter))
        {
            throw new InvalidOperationException(
                "The generated type binding does not belong to this function.");
        }

        _ = GetCodec(binding.Type);
        return new BamlGeneratedFunction<T>(
            _owner,
            declaration,
            binding.Type);
    }

    internal TypeDeclaration<T> RequireType<T>(
        BamlGeneratedType<T> type)
    {
        if (!ReferenceEquals(type.Owner, _owner)
            || !_types.TryGetValue(
                type.Declaration.Id,
                out TypeDeclaration? stored)
            || !ReferenceEquals(stored, type.Declaration))
        {
            throw new InvalidOperationException(
                "The generated type token does not belong to this registry.");
        }

        return type.Declaration;
    }

    internal FunctionDeclaration RequireFunction<TResult>(
        BamlGeneratedFunction<TResult> function)
    {
        if (!ReferenceEquals(function.Owner, _owner)
            || !_functions.TryGetValue(
                function.Declaration.Id,
                out FunctionDeclaration? stored)
            || !ReferenceEquals(stored, function.Declaration)
            || (!ReferenceEquals(
                    stored.ResultDeclaration,
                    function.ResultDeclaration)
                && (stored.ResultTypeParameter is null
                    || !ReferenceEquals(
                        stored.ResultTypeParameter,
                        function.Declaration.ResultTypeParameter))))
        {
            throw new InvalidOperationException(
                "The generated function token does not belong to this registry or has a contradictory result type.");
        }

        _ = GetCodec(function.ResultDeclaration);
        return stored;
    }

    internal FunctionDeclaration RequireStreamFunction<TPartial, TFinal>(
        BamlGeneratedStreamFunction<TPartial, TFinal> function)
    {
        if (!ReferenceEquals(function.Owner, _owner)
            || !_functions.TryGetValue(
                function.Declaration.Id,
                out FunctionDeclaration? stored)
            || !ReferenceEquals(stored, function.Declaration)
            || !ReferenceEquals(
                stored.PartialDeclaration,
                function.PartialDeclaration)
            || !ReferenceEquals(
                stored.FinalDeclaration,
                function.FinalDeclaration))
        {
            throw new InvalidOperationException(
                "The generated stream function token does not belong to this registry or has contradictory result types.");
        }

        _ = GetCodec(function.PartialDeclaration);
        _ = GetCodec(function.FinalDeclaration);
        return stored;
    }

    internal BamlGeneratedValue Encode(
        TypeDeclaration declaration,
        object? value)
    {
        if (!_codecs.TryGetValue(
                declaration.Id,
                out ICodecBox? codec)
            || !ReferenceEquals(
                _types[declaration.Id],
                declaration))
        {
            throw new InvalidOperationException(
                "The generated argument type is not registered.");
        }

        return codec.EncodeObject(
            new BamlGeneratedCodecContext(this),
            value);
    }

    internal object? Decode(
        TypeDeclaration declaration,
        BamlGeneratedValue value)
    {
        if (!_codecs.TryGetValue(
                declaration.Id,
                out ICodecBox? codec)
            || !ReferenceEquals(
                _types[declaration.Id],
                declaration))
        {
            throw new InvalidOperationException(
                "The generated result type is not registered.");
        }

        return codec.DecodeObject(
            new BamlGeneratedCodecContext(this),
            value);
    }

    internal void RequireArgument(
        FunctionDeclaration function,
        ArgumentDeclaration argument)
    {
        if (!_functions.TryGetValue(
                function.Id,
                out FunctionDeclaration? stored)
            || !ReferenceEquals(stored, function)
            || !stored.ContainsArgument(argument))
        {
            throw new InvalidOperationException(
                "The generated argument token does not belong to this function.");
        }
    }

    private CodecBox<T> GetCodec<T>(
        TypeDeclaration<T> declaration)
    {
        if (!_codecs.TryGetValue(
                declaration.Id,
                out ICodecBox? untyped)
            || untyped is not CodecBox<T> typed
            || !_types.TryGetValue(
                declaration.Id,
                out TypeDeclaration? stored)
            || !ReferenceEquals(stored, declaration))
        {
            throw new InvalidOperationException(
                $"No compatible generated codec is registered for {declaration.Identity}.");
        }

        return typed;
    }

    private FunctionDeclaration RequireGenericFunction(
        BamlGeneratedGenericFunction function)
    {
        if (!ReferenceEquals(function.Owner, _owner)
            || !_functions.TryGetValue(
                function.Declaration.Id,
                out FunctionDeclaration? stored)
            || !ReferenceEquals(stored, function.Declaration)
            || stored.ResultTypeParameter is null)
        {
            throw new InvalidOperationException(
                "The generated generic function token does not belong to this registry.");
        }

        return stored;
    }

    private FunctionDeclaration RequireResultTypeParameter(
        BamlGeneratedResultTypeParameter parameter)
    {
        if (!ReferenceEquals(parameter.Owner, _owner)
            || !_functions.TryGetValue(
                parameter.Function.Id,
                out FunctionDeclaration? stored)
            || !ReferenceEquals(stored, parameter.Function)
            || !ReferenceEquals(
                stored.ResultTypeParameter,
                parameter.Declaration))
        {
            throw new InvalidOperationException(
                "The generated result type parameter does not belong to this registry.");
        }

        return stored;
    }
}

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed class BamlGeneratedArgumentsBuilder<TResult>
{
    private readonly BamlGeneratedRegistry _registry;
    private readonly FunctionDeclaration _function;
    private readonly Dictionary<ArgumentDeclaration, BamlGeneratedValue> _values = [];
    private bool _built;

    internal BamlGeneratedArgumentsBuilder(
        BamlGeneratedRegistry registry,
        FunctionDeclaration function)
    {
        _registry = registry;
        _function = function;
    }

    public void Set<TArgument>(
        BamlGeneratedArgument<TResult, TArgument> argument,
        TArgument value)
    {
        EnsureMutable();
        RequireArgument(argument.Owner, argument.Function, argument.Declaration);
        if (!_values.TryAdd(
                argument.Declaration,
                _registry.Encode(argument.Declaration.Type, value)))
        {
            throw new InvalidOperationException(
                "The generated argument was already supplied.");
        }
    }

    public void Omit<TArgument>(
        BamlGeneratedArgument<TResult, TArgument> argument)
    {
        EnsureMutable();
        RequireArgument(argument.Owner, argument.Function, argument.Declaration);
        if (!argument.Declaration.Optional)
        {
            throw new InvalidOperationException(
                "A required generated argument cannot be omitted.");
        }

        if (_values.ContainsKey(argument.Declaration))
        {
            throw new InvalidOperationException(
                "A supplied generated argument cannot also be omitted.");
        }
    }

    public BamlGeneratedArguments<TResult> Build()
    {
        EnsureMutable();
        _function.RequireRequiredArguments(_values);
        _built = true;
        return new BamlGeneratedArguments<TResult>(
            _registry,
            _function,
            new Dictionary<ArgumentDeclaration, BamlGeneratedValue>(_values));
    }

    private void RequireArgument(
        RegistryOwner owner,
        FunctionDeclaration function,
        ArgumentDeclaration argument)
    {
        if (!ReferenceEquals(function, _function))
        {
            throw new InvalidOperationException(
                "The generated argument token belongs to another function.");
        }

        _ = owner;
        _registry.RequireArgument(function, argument);
    }

    private void EnsureMutable()
    {
        if (_built)
        {
            throw new InvalidOperationException(
                "The generated arguments builder is already frozen.");
        }
    }
}

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed class BamlGeneratedStreamArgumentsBuilder<TPartial, TFinal>
{
    private readonly BamlGeneratedRegistry _registry;
    private readonly FunctionDeclaration _function;
    private readonly Dictionary<ArgumentDeclaration, BamlGeneratedValue> _values = [];
    private bool _built;

    internal BamlGeneratedStreamArgumentsBuilder(
        BamlGeneratedRegistry registry,
        FunctionDeclaration function)
    {
        _registry = registry;
        _function = function;
    }

    public void Set<TArgument>(
        BamlGeneratedStreamArgument<TPartial, TFinal, TArgument> argument,
        TArgument value)
    {
        EnsureMutable();
        if (!ReferenceEquals(argument.Function, _function))
        {
            throw new InvalidOperationException(
                "The generated stream argument token belongs to another function.");
        }

        _ = argument.Owner;
        _registry.RequireArgument(_function, argument.Declaration);
        if (!_values.TryAdd(
                argument.Declaration,
                _registry.Encode(argument.Declaration.Type, value)))
        {
            throw new InvalidOperationException(
                "The generated stream argument was already supplied.");
        }
    }

    public BamlGeneratedStreamArguments<TPartial, TFinal> Build()
    {
        EnsureMutable();
        _function.RequireRequiredArguments(_values);
        _built = true;
        return new BamlGeneratedStreamArguments<TPartial, TFinal>(
            _registry,
            _function,
            new Dictionary<ArgumentDeclaration, BamlGeneratedValue>(_values));
    }

    private void EnsureMutable()
    {
        if (_built)
        {
            throw new InvalidOperationException(
                "The generated stream arguments builder is already frozen.");
        }
    }
}

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed class BamlGeneratedArguments<TResult>
{
    private readonly IReadOnlyDictionary<ArgumentDeclaration, BamlGeneratedValue> _values;

    internal BamlGeneratedArguments(
        BamlGeneratedRegistry registry,
        FunctionDeclaration function,
        Dictionary<ArgumentDeclaration, BamlGeneratedValue> values)
    {
        Registry = registry;
        Function = function;
        _values =
            new ReadOnlyDictionary<ArgumentDeclaration, BamlGeneratedValue>(values);
    }

    internal BamlGeneratedRegistry Registry { get; }

    internal FunctionDeclaration Function { get; }

    internal bool TryGet(int index, out BamlGeneratedValue? value)
    {
        ArgumentDeclaration argument = Function.Arguments[index];
        return _values.TryGetValue(argument, out value);
    }

    internal BamlGeneratedValue Required(int index) =>
        TryGet(index, out BamlGeneratedValue? value)
            ? value!
            : throw new InvalidOperationException(
                "A required generated argument was not supplied.");
}

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed class BamlGeneratedStreamArguments<TPartial, TFinal>
{
    private readonly IReadOnlyDictionary<ArgumentDeclaration, BamlGeneratedValue> _values;

    internal BamlGeneratedStreamArguments(
        BamlGeneratedRegistry registry,
        FunctionDeclaration function,
        Dictionary<ArgumentDeclaration, BamlGeneratedValue> values)
    {
        Registry = registry;
        Function = function;
        _values =
            new ReadOnlyDictionary<ArgumentDeclaration, BamlGeneratedValue>(values);
    }

    internal BamlGeneratedRegistry Registry { get; }

    internal FunctionDeclaration Function { get; }

    internal BamlGeneratedValue Required(int index)
    {
        ArgumentDeclaration argument = Function.Arguments[index];
        return _values.TryGetValue(argument, out BamlGeneratedValue? value)
            ? value
            : throw new InvalidOperationException(
                "A required generated stream argument was not supplied.");
    }
}

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed class BamlGeneratedProgram
{
    private readonly BamlGeneratedRegistry _registry;

    internal BamlGeneratedProgram(
        BamlGeneratedRegistry registry,
        string fingerprint,
        string runtimePackageVersion,
        string bridgeVersion)
    {
        _registry = registry;
        Fingerprint = fingerprint;
        RuntimePackageVersion = runtimePackageVersion;
        BridgeVersion = bridgeVersion;
    }

    public string Fingerprint { get; }

    public string RuntimePackageVersion { get; }

    public string BridgeVersion { get; }

    public TResult Call<TResult>(
        BamlGeneratedFunction<TResult> function,
        BamlGeneratedArguments<TResult> arguments,
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        FunctionDeclaration declaration = _registry.RequireFunction(function);
        RequireArguments(arguments.Registry, arguments.Function, declaration);
        BamlGeneratedValue result = Dispatch(
            declaration,
            function.ResultDeclaration,
            arguments);
        object? decoded = _registry.Decode(
            function.ResultDeclaration,
            result);
        return (TResult?)decoded
            ?? (default(TResult) is null
                ? (TResult)decoded!
                : throw new InvalidOperationException(
                    "The generated result unexpectedly decoded to null."));
    }

    public async Task<TResult> CallAsync<TResult>(
        BamlGeneratedFunction<TResult> function,
        BamlGeneratedArguments<TResult> arguments,
        CancellationToken cancellationToken = default)
    {
        await Task.Yield();
        cancellationToken.ThrowIfCancellationRequested();
        return Call(function, arguments, cancellationToken);
    }

    public BamlGeneratedStream<TPartial, TFinal> Stream<TPartial, TFinal>(
        BamlGeneratedStreamFunction<TPartial, TFinal> function,
        BamlGeneratedStreamArguments<TPartial, TFinal> arguments,
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        FunctionDeclaration declaration =
            _registry.RequireStreamFunction(function);
        RequireArguments(arguments.Registry, arguments.Function, declaration);

        if (!StringComparer.Ordinal.Equals(
                declaration.Identity,
                "probe.stream_person")
            || !StringComparer.Ordinal.Equals(
                declaration.Variant,
                "stream"))
        {
            throw new InvalidOperationException(
                "The evidence runtime does not implement this stream token.");
        }

        object? decodedPerson = _registry.Decode(
            function.FinalDeclaration,
            arguments.Required(0));
        BamlGeneratedValue finalValue = _registry.Encode(
            function.FinalDeclaration,
            decodedPerson);
        var partials = new[]
        {
            _registry.Encode(
                function.PartialDeclaration,
                "A"),
            _registry.Encode(
                function.PartialDeclaration,
                "Ad"),
            _registry.Encode(
                function.PartialDeclaration,
                "Ada"),
        };
        return new BamlGeneratedStream<TPartial, TFinal>(
            partials
                .Select(value => (TPartial)_registry.Decode(
                    function.PartialDeclaration,
                    value)!)
                .ToArray(),
            (TFinal)_registry.Decode(
                function.FinalDeclaration,
                finalValue)!);
    }

    private BamlGeneratedValue Dispatch<TResult>(
        FunctionDeclaration declaration,
        TypeDeclaration<TResult> resultDeclaration,
        BamlGeneratedArguments<TResult> arguments)
    {
        if (StringComparer.Ordinal.Equals(
                declaration.Identity,
                "probe.echo_person")
            && StringComparer.Ordinal.Equals(
                declaration.Variant,
                "call"))
        {
            return arguments.Required(0);
        }

        if (StringComparer.Ordinal.Equals(
                declaration.Identity,
                "probe.optional_state")
            && StringComparer.Ordinal.Equals(
                declaration.Variant,
                "call"))
        {
            if (!arguments.TryGet(0, out BamlGeneratedValue? optional))
            {
                return BamlGeneratedValue.CreateString("omitted");
            }

            return BamlGeneratedValue.CreateString(
                optional!.IsNull
                    ? "explicit-null"
                    : "value");
        }

        if (StringComparer.Ordinal.Equals(
                declaration.Identity,
                "probe.person_label")
            && StringComparer.Ordinal.Equals(
                declaration.Variant,
                "method"))
        {
            return BamlGeneratedValue.CreateString("self-ok");
        }

        if (StringComparer.Ordinal.Equals(
                declaration.Identity,
                "probe.echo_person")
            && StringComparer.Ordinal.Equals(
                declaration.Variant,
                "build_request"))
        {
            return _registry.Encode(
                declaration.ResultDeclaration!,
                new BamlGeneratedRequest(
                    "POST",
                    "/v1/call/probe.echo_person",
                    true));
        }

        if (StringComparer.Ordinal.Equals(
                declaration.Identity,
                "probe.generic_default")
            && StringComparer.Ordinal.Equals(
                declaration.Variant,
                "call"))
        {
            return _registry.Encode(
                declaration.ResultTypeParameter is null
                    ? throw new InvalidOperationException(
                        "The generic declaration is contradictory.")
                    : ReferenceEquals(arguments.Function, declaration)
                        ? resultDeclaration
                        : throw new InvalidOperationException(
                            "The generic arguments are contradictory."),
                "generic-string");
        }

        throw new InvalidOperationException(
            "The evidence runtime does not implement this generated function token.");
    }

    private void RequireArguments(
        BamlGeneratedRegistry registry,
        FunctionDeclaration argumentFunction,
        FunctionDeclaration function)
    {
        if (!ReferenceEquals(registry, _registry)
            || !ReferenceEquals(argumentFunction, function))
        {
            throw new InvalidOperationException(
                "Generated arguments belong to another function.");
        }
    }
}

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed class BamlGeneratedStream<TPartial, TFinal>
{
    private readonly IReadOnlyList<TPartial> _partials;
    private readonly TFinal _final;

    internal BamlGeneratedStream(
        IReadOnlyList<TPartial> partials,
        TFinal final)
    {
        _partials = partials;
        _final = final;
    }

    public IReadOnlyList<TPartial> Partials => _partials;

    public async Task<TFinal> GetFinalAsync(
        CancellationToken cancellationToken = default)
    {
        await Task.Yield();
        cancellationToken.ThrowIfCancellationRequested();
        return _final;
    }
}

[EditorBrowsable(EditorBrowsableState.Never)]
public readonly struct BamlGeneratedCodecContext
{
    private readonly BamlGeneratedRegistry _registry;

    internal BamlGeneratedCodecContext(BamlGeneratedRegistry registry)
    {
        _registry = registry;
    }

    public BamlGeneratedValue Encode<T>(
        BamlGeneratedType<T> type,
        T value) =>
        _registry.Encode(type, value);

    public T Decode<T>(
        BamlGeneratedType<T> type,
        BamlGeneratedValue value) =>
        _registry.Decode(type, value);

    public BamlGeneratedValue Null() => BamlGeneratedValue.CreateNull();

    public BamlGeneratedValue Boolean(bool value) =>
        BamlGeneratedValue.CreateBoolean(value);

    public bool ReadBoolean(BamlGeneratedValue value) =>
        value.RequireBoolean();

    public BamlGeneratedValue Integer(long value)
    {
        if (value is < BamlGeneratedContract.MinimumInteger
            or > BamlGeneratedContract.MaximumInteger)
        {
            throw new OverflowException(
                "The generated BAML integer is outside the exact V1 range.");
        }

        return BamlGeneratedValue.CreateInteger(value);
    }

    public long ReadInteger(BamlGeneratedValue value) =>
        value.RequireInteger();

    public BamlGeneratedValue Float(double value)
    {
        if (!double.IsFinite(value))
        {
            throw new ArgumentOutOfRangeException(
                nameof(value),
                "Generated BAML floats must be finite.");
        }

        return BamlGeneratedValue.CreateFloat(value);
    }

    public double ReadFloat(BamlGeneratedValue value) =>
        value.RequireFloat();

    public BamlGeneratedValue BigInteger(BigInteger value) =>
        BamlGeneratedValue.CreateBigInteger(value);

    public BigInteger ReadBigInteger(BamlGeneratedValue value) =>
        value.RequireBigInteger();

    public BamlGeneratedValue String(string value)
    {
        ArgumentNullException.ThrowIfNull(value);
        return BamlGeneratedValue.CreateString(value);
    }

    public string ReadString(BamlGeneratedValue value) =>
        value.RequireString();

    public BamlGeneratedValue Bytes(ReadOnlySpan<byte> value) =>
        BamlGeneratedValue.CreateBytes(value);

    public byte[] ReadBytes(BamlGeneratedValue value) =>
        value.RequireBytes();

    public BamlGeneratedValue List(
        IReadOnlyList<BamlGeneratedValue> values) =>
        BamlGeneratedValue.CreateList(values);

    public IReadOnlyList<BamlGeneratedValue> ReadList(
        BamlGeneratedValue value) =>
        value.RequireList();

    public BamlGeneratedValue Map(
        IReadOnlyList<KeyValuePair<string, BamlGeneratedValue>> values) =>
        BamlGeneratedValue.CreateMap(values);

    public BamlGeneratedMap ReadMap(BamlGeneratedValue value) =>
        value.RequireMap();

    public BamlGeneratedValue Enum<T>(
        BamlGeneratedType<T> type,
        string wireVariant)
    {
        TypeDeclaration<T> declaration = _registry.RequireType(type);
        return BamlGeneratedValue.CreateEnum(
            declaration,
            wireVariant);
    }

    public string ReadEnum<T>(
        BamlGeneratedType<T> type,
        BamlGeneratedValue value)
    {
        TypeDeclaration<T> declaration = _registry.RequireType(type);
        return value.RequireEnum(declaration);
    }

    public BamlGeneratedValue Object<T>(
        BamlGeneratedType<T> type,
        IReadOnlyList<KeyValuePair<string, BamlGeneratedValue>> fields)
    {
        TypeDeclaration<T> declaration = _registry.RequireType(type);
        return BamlGeneratedValue.CreateObject(declaration, fields);
    }

    public BamlGeneratedObject ReadObject<T>(
        BamlGeneratedType<T> type,
        BamlGeneratedValue value)
    {
        TypeDeclaration<T> declaration = _registry.RequireType(type);
        return value.RequireObject(declaration);
    }

    public BamlGeneratedValue Union<T>(
        BamlGeneratedType<T> type,
        int activeCase,
        BamlGeneratedValue value)
    {
        TypeDeclaration<T> declaration = _registry.RequireType(type);
        return BamlGeneratedValue.CreateUnion(
            declaration,
            activeCase,
            value);
    }

    public BamlGeneratedUnion ReadUnion<T>(
        BamlGeneratedType<T> type,
        BamlGeneratedValue value)
    {
        TypeDeclaration<T> declaration = _registry.RequireType(type);
        return value.RequireUnion(declaration);
    }

    public BamlGeneratedValue Dynamic(BamlGeneratedValue value) =>
        BamlGeneratedValue.CreateDynamic(value);

    public BamlGeneratedValue ReadDynamic(BamlGeneratedValue value) =>
        value.RequireDynamic();

    public BamlGeneratedValue Media(BamlGeneratedMedia value) =>
        BamlGeneratedValue.CreateMedia(value);

    public BamlGeneratedMedia ReadMedia(BamlGeneratedValue value) =>
        value.RequireMedia();

    public BamlGeneratedValue Handle(BamlGeneratedHandle value) =>
        BamlGeneratedValue.CreateHandle(value);

    public BamlGeneratedHandle ReadHandle(BamlGeneratedValue value) =>
        value.RequireHandle();
}

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed class BamlGeneratedObject
{
    private readonly IReadOnlyDictionary<string, BamlGeneratedValue> _fields;

    internal BamlGeneratedObject(
        IReadOnlyDictionary<string, BamlGeneratedValue> fields)
    {
        _fields = fields;
    }

    public BamlGeneratedValue Required(string wireIdentity)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(wireIdentity);
        if (!_fields.TryGetValue(
                wireIdentity,
                out BamlGeneratedValue? value))
        {
            throw new InvalidOperationException(
                $"Missing required generated wire field {wireIdentity}.");
        }

        return value;
    }

    public bool TryGet(
        string wireIdentity,
        out BamlGeneratedValue? value)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(wireIdentity);
        return _fields.TryGetValue(wireIdentity, out value);
    }
}

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed class BamlGeneratedMap
{
    private readonly IReadOnlyDictionary<string, BamlGeneratedValue> _values;

    internal BamlGeneratedMap(
        IReadOnlyDictionary<string, BamlGeneratedValue> values)
    {
        _values = values;
    }

    public BamlGeneratedValue Required(string key) =>
        _values.TryGetValue(key, out BamlGeneratedValue? value)
            ? value
            : throw new InvalidOperationException(
                $"Missing generated BAML map key {key}.");
}

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed class BamlGeneratedUnion
{
    internal BamlGeneratedUnion(
        int activeCase,
        BamlGeneratedValue value)
    {
        ActiveCase = activeCase;
        Value = value;
    }

    public int ActiveCase { get; }

    public BamlGeneratedValue Value { get; }
}

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed class BamlGeneratedValue
{
    private readonly GeneratedValueKind _kind;
    private readonly TypeDeclaration? _type;
    private readonly object? _payload;

    private BamlGeneratedValue(
        GeneratedValueKind kind,
        TypeDeclaration? type,
        object? payload)
    {
        _kind = kind;
        _type = type;
        _payload = payload;
    }

    public bool IsNull => _kind == GeneratedValueKind.Null;

    internal static BamlGeneratedValue CreateNull() =>
        new(GeneratedValueKind.Null, null, null);

    internal static BamlGeneratedValue CreateBoolean(bool value) =>
        new(GeneratedValueKind.Boolean, null, value);

    internal static BamlGeneratedValue CreateInteger(long value) =>
        new(GeneratedValueKind.Integer, null, value);

    internal static BamlGeneratedValue CreateFloat(double value) =>
        new(GeneratedValueKind.Float, null, value);

    internal static BamlGeneratedValue CreateBigInteger(BigInteger value) =>
        new(GeneratedValueKind.BigInteger, null, value);

    internal static BamlGeneratedValue CreateString(string value) =>
        new(GeneratedValueKind.String, null, value);

    internal static BamlGeneratedValue CreateBytes(ReadOnlySpan<byte> value) =>
        new(GeneratedValueKind.Bytes, null, value.ToArray());

    internal static BamlGeneratedValue CreateList(
        IReadOnlyList<BamlGeneratedValue> values)
    {
        ArgumentNullException.ThrowIfNull(values);
        BamlGeneratedValue[] snapshot = values.ToArray();
        if (snapshot.Any(static value => value is null))
        {
            throw new ArgumentException(
                "Generated BAML lists cannot contain null carriers.",
                nameof(values));
        }

        return new(
            GeneratedValueKind.List,
            null,
            Array.AsReadOnly(snapshot));
    }

    internal static BamlGeneratedValue CreateMap(
        IReadOnlyList<KeyValuePair<string, BamlGeneratedValue>> values)
    {
        ArgumentNullException.ThrowIfNull(values);
        var snapshot = new Dictionary<string, BamlGeneratedValue>(
            values.Count,
            StringComparer.Ordinal);
        foreach ((string key, BamlGeneratedValue value) in values)
        {
            ArgumentNullException.ThrowIfNull(key);
            ArgumentNullException.ThrowIfNull(value);
            if (!snapshot.TryAdd(key, value))
            {
                throw new InvalidOperationException(
                    $"Duplicate generated BAML map key {key}.");
            }
        }

        return new(
            GeneratedValueKind.Map,
            null,
            new ReadOnlyDictionary<string, BamlGeneratedValue>(snapshot));
    }

    internal static BamlGeneratedValue CreateEnum(
        TypeDeclaration type,
        string wireVariant)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(wireVariant);
        return new(GeneratedValueKind.Enum, type, wireVariant);
    }

    internal static BamlGeneratedValue CreateObject(
        TypeDeclaration type,
        IReadOnlyList<KeyValuePair<string, BamlGeneratedValue>> fields)
    {
        ArgumentNullException.ThrowIfNull(fields);
        var snapshot = new Dictionary<string, BamlGeneratedValue>(
            fields.Count,
            StringComparer.Ordinal);
        foreach ((string wireIdentity, BamlGeneratedValue value) in fields)
        {
            ArgumentException.ThrowIfNullOrWhiteSpace(wireIdentity);
            ArgumentNullException.ThrowIfNull(value);
            if (!snapshot.TryAdd(wireIdentity, value))
            {
                throw new InvalidOperationException(
                    $"Duplicate generated wire field {wireIdentity} for {type.Identity}.");
            }
        }

        return new(
            GeneratedValueKind.Object,
            type,
            new ReadOnlyDictionary<string, BamlGeneratedValue>(snapshot));
    }

    internal static BamlGeneratedValue CreateUnion(
        TypeDeclaration type,
        int activeCase,
        BamlGeneratedValue value)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(activeCase);
        ArgumentNullException.ThrowIfNull(value);
        return new(
            GeneratedValueKind.Union,
            type,
            new BamlGeneratedUnion(activeCase, value));
    }

    internal static BamlGeneratedValue CreateDynamic(
        BamlGeneratedValue value)
    {
        ArgumentNullException.ThrowIfNull(value);
        return new(GeneratedValueKind.Dynamic, null, value);
    }

    internal static BamlGeneratedValue CreateMedia(
        BamlGeneratedMedia value)
    {
        ArgumentNullException.ThrowIfNull(value);
        return new(
            GeneratedValueKind.Media,
            null,
            value.Snapshot());
    }

    internal static BamlGeneratedValue CreateHandle(
        BamlGeneratedHandle value)
    {
        ArgumentNullException.ThrowIfNull(value);
        return new(
            GeneratedValueKind.Handle,
            null,
            value.Snapshot());
    }

    internal bool RequireBoolean() =>
        _kind == GeneratedValueKind.Boolean
            ? (bool)_payload!
            : throw WrongKind("boolean");

    internal long RequireInteger() =>
        _kind == GeneratedValueKind.Integer
            ? (long)_payload!
            : throw WrongKind("integer");

    internal double RequireFloat() =>
        _kind == GeneratedValueKind.Float
            ? (double)_payload!
            : throw WrongKind("float");

    internal BigInteger RequireBigInteger() =>
        _kind == GeneratedValueKind.BigInteger
            ? (BigInteger)_payload!
            : throw WrongKind("big integer");

    internal string RequireString() =>
        _kind == GeneratedValueKind.String
            ? (string)_payload!
            : throw WrongKind("string");

    internal byte[] RequireBytes() =>
        _kind == GeneratedValueKind.Bytes
            ? ((byte[])_payload!).ToArray()
            : throw WrongKind("bytes");

    internal IReadOnlyList<BamlGeneratedValue> RequireList() =>
        _kind == GeneratedValueKind.List
            ? (IReadOnlyList<BamlGeneratedValue>)_payload!
            : throw WrongKind("list");

    internal BamlGeneratedMap RequireMap() =>
        _kind == GeneratedValueKind.Map
            ? new BamlGeneratedMap(
                (IReadOnlyDictionary<string, BamlGeneratedValue>)_payload!)
            : throw WrongKind("map");

    internal string RequireEnum(TypeDeclaration type) =>
        _kind == GeneratedValueKind.Enum
            && ReferenceEquals(_type, type)
            ? (string)_payload!
            : throw WrongTypedKind("enum", type);

    internal BamlGeneratedObject RequireObject(
        TypeDeclaration type) =>
        _kind == GeneratedValueKind.Object
            && ReferenceEquals(_type, type)
            ? new BamlGeneratedObject(
                (IReadOnlyDictionary<string, BamlGeneratedValue>)_payload!)
            : throw WrongTypedKind("object", type);

    internal BamlGeneratedUnion RequireUnion(
        TypeDeclaration type) =>
        _kind == GeneratedValueKind.Union
            && ReferenceEquals(_type, type)
            ? (BamlGeneratedUnion)_payload!
            : throw WrongTypedKind("union", type);

    internal BamlGeneratedValue RequireDynamic() =>
        _kind == GeneratedValueKind.Dynamic
            ? (BamlGeneratedValue)_payload!
            : throw WrongKind("dynamic");

    internal BamlGeneratedMedia RequireMedia() =>
        _kind == GeneratedValueKind.Media
            ? ((BamlGeneratedMedia)_payload!).Snapshot()
            : throw WrongKind("media");

    internal BamlGeneratedHandle RequireHandle() =>
        _kind == GeneratedValueKind.Handle
            ? ((BamlGeneratedHandle)_payload!).Snapshot()
            : throw WrongKind("handle");

    private InvalidOperationException WrongKind(string expected) =>
        new($"Expected a generated BAML {expected}, received {_kind}.");

    private InvalidOperationException WrongTypedKind(
        string expected,
        TypeDeclaration type) =>
        new(
            $"Expected generated BAML {expected} {type.Identity}, received {_type?.Identity ?? _kind.ToString()}.");

    private enum GeneratedValueKind
    {
        Null,
        Boolean,
        Integer,
        Float,
        BigInteger,
        String,
        Bytes,
        List,
        Map,
        Enum,
        Object,
        Union,
        Dynamic,
        Media,
        Handle,
    }
}

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed class BamlGeneratedMedia
{
    private readonly byte[] _data;

    public BamlGeneratedMedia(
        string mediaKind,
        string mimeType,
        ReadOnlySpan<byte> data)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(mediaKind);
        ArgumentException.ThrowIfNullOrWhiteSpace(mimeType);
        MediaKind = mediaKind;
        MimeType = mimeType;
        _data = data.ToArray();
    }

    public string MediaKind { get; }

    public string MimeType { get; }

    public byte[] Data => _data.ToArray();

    internal BamlGeneratedMedia Snapshot() =>
        new(MediaKind, MimeType, _data);
}

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed class BamlGeneratedHandle
{
    private readonly IReadOnlyDictionary<string, string> _metadata;

    public BamlGeneratedHandle(
        string kind,
        string identifier,
        IReadOnlyDictionary<string, string> metadata)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(kind);
        ArgumentException.ThrowIfNullOrWhiteSpace(identifier);
        ArgumentNullException.ThrowIfNull(metadata);
        Kind = kind;
        Identifier = identifier;
        _metadata = new ReadOnlyDictionary<string, string>(
            new Dictionary<string, string>(
                metadata,
                StringComparer.Ordinal));
    }

    public string Kind { get; }

    public string Identifier { get; }

    public IReadOnlyDictionary<string, string> Metadata => _metadata;

    internal BamlGeneratedHandle Snapshot() =>
        new(Kind, Identifier, _metadata);
}

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed record BamlGeneratedRequest(
    string Method,
    string Path,
    bool HasBody);

internal sealed class RegistryOwner;

internal abstract class TypeDeclaration(
    int id,
    string identity)
{
    internal int Id { get; } = id;

    internal string Identity { get; } = identity;
}

internal sealed class TypeDeclaration<T>(
    int id,
    string identity)
    : TypeDeclaration(id, identity);

internal sealed class ArgumentDeclaration<T>(
    string wireIdentity,
    TypeDeclaration<T> type,
    bool optional,
    bool isSelf)
    : ArgumentDeclaration(wireIdentity, type, optional, isSelf);

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

internal sealed class ResultTypeParameterDeclaration(
    string identity)
{
    internal string Identity { get; } = identity;
}

internal sealed class FunctionDeclaration(
    int id,
    string identity,
    string variant,
    TypeDeclaration? resultDeclaration,
    TypeDeclaration? partialDeclaration,
    TypeDeclaration? finalDeclaration)
{
    private readonly List<ArgumentDeclaration> _arguments = [];
    private readonly HashSet<string> _argumentIdentities =
        new(StringComparer.Ordinal);
    private TypeDeclaration? _boundResult;

    internal int Id { get; } = id;

    internal string Identity { get; } = identity;

    internal string Variant { get; } = variant;

    internal TypeDeclaration? ResultDeclaration { get; } = resultDeclaration;

    internal TypeDeclaration? PartialDeclaration { get; } = partialDeclaration;

    internal TypeDeclaration? FinalDeclaration { get; } = finalDeclaration;

    internal ResultTypeParameterDeclaration? ResultTypeParameter { get; set; }

    internal IReadOnlyList<ArgumentDeclaration> Arguments => _arguments;

    internal void AddArgument(ArgumentDeclaration argument)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(argument.WireIdentity);
        if (!_argumentIdentities.Add(argument.WireIdentity))
        {
            throw new InvalidOperationException(
                $"Generated argument {argument.WireIdentity} is already declared for {Identity} variant {Variant}.");
        }

        if (argument.IsSelf
            && _arguments.Any(static candidate => candidate.IsSelf))
        {
            throw new InvalidOperationException(
                $"Generated function {Identity} variant {Variant} already has a receiver.");
        }

        _arguments.Add(argument);
    }

    internal bool ContainsArgument(ArgumentDeclaration argument) =>
        _arguments.Any(candidate => ReferenceEquals(candidate, argument));

    internal void RequireRequiredArguments(
        IReadOnlyDictionary<ArgumentDeclaration, BamlGeneratedValue> values)
    {
        foreach (ArgumentDeclaration argument in _arguments)
        {
            if (!argument.Optional
                && !values.ContainsKey(argument))
            {
                throw new InvalidOperationException(
                    $"Missing required generated argument {argument.WireIdentity}.");
            }
        }
    }

    internal TypeDeclaration BoundResultOrThrow() =>
        _boundResult
        ?? throw new InvalidOperationException(
            "The result-only generic function was not bound.");

    internal void BindResult(TypeDeclaration declaration)
    {
        if (_boundResult is not null
            && !ReferenceEquals(_boundResult, declaration))
        {
            throw new InvalidOperationException(
                "The result-only generic function already has a contradictory binding.");
        }

        _boundResult = declaration;
    }
}

internal readonly record struct FunctionIdentity(
    string Identity,
    string Variant);

internal interface ICodecBox
{
    BamlGeneratedValue EncodeObject(
        BamlGeneratedCodecContext context,
        object? value);

    object? DecodeObject(
        BamlGeneratedCodecContext context,
        BamlGeneratedValue value);
}

internal sealed class CodecBox<T>(
    IBamlGeneratedCodec<T> codec)
    : ICodecBox
{
    internal BamlGeneratedValue Encode(
        BamlGeneratedCodecContext context,
        T value) =>
        codec.Encode(context, value);

    internal T Decode(
        BamlGeneratedCodecContext context,
        BamlGeneratedValue value) =>
        codec.Decode(context, value);

    BamlGeneratedValue ICodecBox.EncodeObject(
        BamlGeneratedCodecContext context,
        object? value)
    {
        if (value is null && default(T) is not null)
        {
            throw new InvalidOperationException(
                "A non-nullable generated value cannot be null.");
        }

        return codec.Encode(context, (T)value!);
    }

    object? ICodecBox.DecodeObject(
        BamlGeneratedCodecContext context,
        BamlGeneratedValue value) =>
        codec.Decode(context, value);
}
