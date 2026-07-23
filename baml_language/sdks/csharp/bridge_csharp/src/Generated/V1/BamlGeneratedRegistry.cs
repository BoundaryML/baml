using System.Collections.ObjectModel;
using System.Collections.Concurrent;
using System.ComponentModel;
using System.Diagnostics.CodeAnalysis;
using System.Reflection;
using System.Runtime.ExceptionServices;

using Baml;
using BamlBridge.Cffi.V1;

namespace Baml.Generated.V1;

[EditorBrowsable(EditorBrowsableState.Never)]
public interface IBamlGeneratedCodec<T>
{
    BamlGeneratedValue Encode(BamlGeneratedCodecContext context, T value);

    T Decode(BamlGeneratedCodecContext context, BamlGeneratedValue value);
}

internal sealed class BamlGeneratedEncodeBudget
{
    private readonly HashSet<object> activeReferences =
        new(ReferenceEqualityComparer.Instance);
    private int depth;
    private int nodes;

    internal IDisposable Enter<T>(T value, Type declaredType)
    {
        ArgumentNullException.ThrowIfNull(declaredType);
        if (depth > global::Baml.BamlValueLimits.MaxDepth)
        {
            throw Limit(declaredType, nameof(global::Baml.BamlValueLimits.MaxDepth));
        }
        if (++nodes > global::Baml.BamlValueLimits.MaxNodes)
        {
            throw Limit(declaredType, nameof(global::Baml.BamlValueLimits.MaxNodes));
        }

        object? reference = value;
        bool tracked = reference is not null
            && !declaredType.IsValueType
            && reference is not string
            and not BamlValue
            and not BamlImage
            and not BamlAudio
            and not BamlVideo
            and not BamlPdf
            and not BamlHandle
            and not Delegate;
        if (tracked && !activeReferences.Add(reference!))
        {
            throw new BamlTypeMappingException(
                declaredType,
                "dynamic value",
                "$",
                $"The CLR value graph for {declaredType} contains a reference cycle.");
        }

        depth++;
        return new Scope(this, tracked ? reference : null);
    }

    private void Exit(object? reference)
    {
        depth--;
        if (reference is not null && !activeReferences.Remove(reference))
        {
            throw new InvalidOperationException(
                "The generated encode cycle guard lost an active reference.");
        }
    }

    private static BamlTypeMappingException Limit(Type declaredType, string limit) =>
        new(
            declaredType,
            "dynamic value",
            "$",
            $"The CLR value graph exceeded {limit}.");

    private sealed class Scope(BamlGeneratedEncodeBudget owner, object? reference) : IDisposable
    {
        private BamlGeneratedEncodeBudget? owner = owner;

        public void Dispose()
        {
            BamlGeneratedEncodeBudget? current = Interlocked.Exchange(ref owner, null);
            current?.Exit(reference);
        }
    }
}

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed class BamlGeneratedRegistryBuilder
{
    private readonly RegistryOwner owner;
    private readonly Dictionary<int, TypeDeclaration> types = [];
    private readonly Dictionary<int, ICodecBox> codecs = [];
    private readonly Dictionary<int, FunctionDeclaration> functions = [];
    private readonly Dictionary<int, GenericFunctionDeclaration> genericFunctions = [];
    private readonly Dictionary<Type, TypeDeclaration> genericBindings = [];
    private readonly Dictionary<Type, GenericTypeFactoryDeclaration> genericTypeFactories = [];
    private readonly HashSet<string> typeIdentities = new(StringComparer.Ordinal);
    private readonly HashSet<FunctionIdentity> functionIdentities = [];
    private int nextTypeId = 1;
    private int nextFunctionId = 1;
    private bool built;

    internal BamlGeneratedRegistryBuilder(RegistryOwner owner)
    {
        this.owner = owner;
    }

    public BamlGeneratedType<T> DeclareType<T>(
        string bamlIdentity,
        byte[]? typeMetadata = null)
    {
        EnsureMutable();
        ArgumentException.ThrowIfNullOrWhiteSpace(bamlIdentity);
        if (!typeIdentities.Add(bamlIdentity))
        {
            throw new InvalidOperationException(
                $"Generated BAML type {bamlIdentity} is already declared.");
        }

        int id = checked(nextTypeId++);
        var declaration = new TypeDeclaration<T>(id, bamlIdentity, typeMetadata);
        types.Add(id, declaration);
        return new BamlGeneratedType<T>(owner, declaration);
    }

    public void RegisterGenericBinding<T>(BamlGeneratedType<T> type)
    {
        EnsureMutable();
        TypeDeclaration<T> declaration = RequireType(type);
        if (declaration.Metadata.Length == 0)
        {
            throw new InvalidOperationException(
                $"Generated generic type {declaration.Identity} has no BAML metadata.");
        }

        if (!genericBindings.TryAdd(typeof(T), declaration))
        {
            throw new InvalidOperationException(
                $"CLR type {typeof(T)} already has a canonical generated BAML binding.");
        }
    }

    public void RegisterGenericTypeFactory(
        Type genericTypeDefinition,
        MethodInfo factory)
    {
        EnsureMutable();
        ArgumentNullException.ThrowIfNull(genericTypeDefinition);
        ArgumentNullException.ThrowIfNull(factory);
        if (!genericTypeDefinition.IsGenericTypeDefinition
            || !factory.IsStatic
            || !factory.IsGenericMethodDefinition
            || factory.GetGenericArguments().Length
                != genericTypeDefinition.GetGenericArguments().Length
            || factory.ReturnType != typeof(BamlGeneratedTypeFactoryResult))
        {
            throw new ArgumentException(
                "A generated generic type factory must be a static generic method with matching arity and the canonical result type.",
                nameof(factory));
        }

        ParameterInfo[] parameters = factory.GetParameters();
        if (parameters.Length != 1
            || parameters[0].ParameterType != typeof(BamlGeneratedRegistry))
        {
            throw new ArgumentException(
                "A generated generic type factory must accept exactly one BamlGeneratedRegistry.",
                nameof(factory));
        }

        if (!genericTypeFactories.TryAdd(
                genericTypeDefinition,
                new GenericTypeFactoryDeclaration(genericTypeDefinition, factory)))
        {
            throw new InvalidOperationException(
                $"CLR generic type {genericTypeDefinition} already has a generated BAML factory.");
        }
    }

    public void RegisterCodec<T>(BamlGeneratedType<T> type, IBamlGeneratedCodec<T> codec)
    {
        EnsureMutable();
        ArgumentNullException.ThrowIfNull(codec);
        TypeDeclaration<T> declaration = RequireType(type);
        if (!codecs.TryAdd(declaration.Id, new CodecBox<T>(codec)))
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
        ArgumentException.ThrowIfNullOrWhiteSpace(bamlIdentity);
        ArgumentException.ThrowIfNullOrWhiteSpace(variant);
        TypeDeclaration<TResult> result = RequireType(resultType);
        var identity = new FunctionIdentity(bamlIdentity, variant);
        if (!functionIdentities.Add(identity))
        {
            throw new InvalidOperationException(
                $"Generated BAML function {bamlIdentity} variant {variant} is already declared.");
        }

        int id = checked(nextFunctionId++);
        var declaration = new FunctionDeclaration(id, bamlIdentity, variant, result);
        functions.Add(id, declaration);
        return new BamlGeneratedFunction<TResult>(owner, declaration, result);
    }

    public BamlGeneratedArgument<TResult, TArgument> DeclareArgument<TResult, TArgument>(
        BamlGeneratedFunction<TResult> function,
        string wireIdentity,
        BamlGeneratedType<TArgument> type,
        bool optional = false,
        bool isSelf = false)
    {
        EnsureMutable();
        FunctionDeclaration functionDeclaration = RequireFunction(function);
        TypeDeclaration<TArgument> typeDeclaration = RequireType(type);
        var argument = new ArgumentDeclaration<TArgument>(
            wireIdentity,
            typeDeclaration,
            optional,
            isSelf);
        functionDeclaration.AddArgument(argument);
        return new BamlGeneratedArgument<TResult, TArgument>(
            owner,
            functionDeclaration,
            argument);
    }

    public BamlGeneratedGenericFunction DeclareGenericFunction(
        string bamlIdentity,
        string variant)
    {
        EnsureMutable();
        ArgumentException.ThrowIfNullOrWhiteSpace(bamlIdentity);
        ArgumentException.ThrowIfNullOrWhiteSpace(variant);
        var identity = new FunctionIdentity(bamlIdentity, variant);
        if (!functionIdentities.Add(identity))
        {
            throw new InvalidOperationException(
                $"Generated BAML function {bamlIdentity} variant {variant} is already declared.");
        }

        int id = checked(nextFunctionId++);
        var declaration = new GenericFunctionDeclaration(id, bamlIdentity, variant);
        genericFunctions.Add(id, declaration);
        return new BamlGeneratedGenericFunction(owner, declaration);
    }

    public BamlGeneratedTypeParameter DeclareTypeParameter(
        BamlGeneratedGenericFunction function,
        string wireIdentity)
    {
        EnsureMutable();
        GenericFunctionDeclaration declaration = RequireGenericFunction(function);
        TypeParameterDeclaration parameter = declaration.AddTypeParameter(wireIdentity);
        return new BamlGeneratedTypeParameter(owner, declaration, parameter);
    }

    public BamlGeneratedGenericArgument DeclareGenericArgument(
        BamlGeneratedGenericFunction function,
        string wireIdentity,
        bool optional = false,
        bool isSelf = false)
    {
        EnsureMutable();
        GenericFunctionDeclaration declaration = RequireGenericFunction(function);
        GenericArgumentDeclaration argument = declaration.AddArgument(
            wireIdentity,
            optional,
            isSelf);
        return new BamlGeneratedGenericArgument(owner, declaration, argument);
    }

    public BamlGeneratedRegistry Build()
    {
        EnsureMutable();
        foreach (TypeDeclaration declaration in types.Values)
        {
            if (!codecs.ContainsKey(declaration.Id))
            {
                throw new InvalidOperationException(
                    $"No generated codec was registered for {declaration.Identity}.");
            }
        }

        built = true;
        var registry = new BamlGeneratedRegistry(
            owner,
            new Dictionary<int, TypeDeclaration>(types),
            new Dictionary<int, ICodecBox>(codecs),
            new Dictionary<int, FunctionDeclaration>(functions),
            new Dictionary<int, GenericFunctionDeclaration>(genericFunctions),
            new Dictionary<Type, TypeDeclaration>(genericBindings),
            new Dictionary<Type, GenericTypeFactoryDeclaration>(genericTypeFactories));
        registry.RegisterDynamicTypes(types.Values);
        return registry;
    }

    private TypeDeclaration<T> RequireType<T>(BamlGeneratedType<T> type)
    {
        if (!ReferenceEquals(type.Owner, owner)
            || !types.TryGetValue(type.Declaration.Id, out TypeDeclaration? stored)
            || !ReferenceEquals(stored, type.Declaration))
        {
            throw new InvalidOperationException(
                "The generated type token does not belong to this registry builder.");
        }

        return type.Declaration;
    }

    private FunctionDeclaration RequireFunction<TResult>(BamlGeneratedFunction<TResult> function)
    {
        if (!ReferenceEquals(function.Owner, owner)
            || !functions.TryGetValue(function.Declaration.Id, out FunctionDeclaration? stored)
            || !ReferenceEquals(stored, function.Declaration)
            || !ReferenceEquals(stored.Result, function.Result))
        {
            throw new InvalidOperationException(
                "The generated function token does not belong to this registry builder.");
        }

        return stored;
    }

    private GenericFunctionDeclaration RequireGenericFunction(
        BamlGeneratedGenericFunction function)
    {
        if (!ReferenceEquals(function.Owner, owner)
            || !genericFunctions.TryGetValue(
                function.Declaration.Id,
                out GenericFunctionDeclaration? stored)
            || !ReferenceEquals(stored, function.Declaration))
        {
            throw new InvalidOperationException(
                "The generated generic function token does not belong to this registry builder.");
        }

        return stored;
    }

    private void EnsureMutable()
    {
        if (built)
        {
            throw new InvalidOperationException(
                "The generated registry builder is already frozen.");
        }
    }
}

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed class BamlGeneratedRegistry
{
    private static readonly MethodInfo CreateNullableValueTypeMethod =
        RequireFactory(nameof(CreateNullableValueType));
    private static readonly MethodInfo CreateNullableTypeMethod =
        RequireFactory(nameof(CreateNullableType));
    private static readonly MethodInfo CreateListTypeMethod =
        RequireFactory(nameof(CreateListType));
    private static readonly MethodInfo CreateMapTypeMethod =
        RequireFactory(nameof(CreateMapType));
    private static readonly MethodInfo CreateUnion2TypeMethod =
        RequireFactory(nameof(CreateUnion2Type));

    private readonly RegistryOwner owner;
    private readonly IReadOnlyDictionary<int, TypeDeclaration> types;
    private readonly IReadOnlyDictionary<int, ICodecBox> codecs;
    private readonly IReadOnlyDictionary<int, FunctionDeclaration> functions;
    private readonly IReadOnlyDictionary<int, GenericFunctionDeclaration> genericFunctions;
    private readonly IReadOnlyDictionary<Type, TypeDeclaration> genericBindings;
    private readonly IReadOnlyDictionary<Type, GenericTypeFactoryDeclaration> genericTypeFactories;
    private readonly ConcurrentDictionary<Type, IResolvedGeneratedType> dynamicTypes = [];
    private int nextDynamicTypeId;

    internal BamlGeneratedRegistry(
        RegistryOwner owner,
        Dictionary<int, TypeDeclaration> types,
        Dictionary<int, ICodecBox> codecs,
        Dictionary<int, FunctionDeclaration> functions,
        Dictionary<int, GenericFunctionDeclaration> genericFunctions,
        Dictionary<Type, TypeDeclaration> genericBindings,
        Dictionary<Type, GenericTypeFactoryDeclaration> genericTypeFactories)
    {
        this.owner = owner;
        this.types = new ReadOnlyDictionary<int, TypeDeclaration>(types);
        this.codecs = new ReadOnlyDictionary<int, ICodecBox>(codecs);
        this.functions = new ReadOnlyDictionary<int, FunctionDeclaration>(functions);
        this.genericFunctions =
            new ReadOnlyDictionary<int, GenericFunctionDeclaration>(genericFunctions);
        this.genericBindings =
            new ReadOnlyDictionary<Type, TypeDeclaration>(genericBindings);
        this.genericTypeFactories =
            new ReadOnlyDictionary<Type, GenericTypeFactoryDeclaration>(genericTypeFactories);
    }

    public BamlGeneratedValue Encode<T>(BamlGeneratedType<T> type, T value)
    {
        TypeDeclaration<T> declaration = RequireType(type);
        return Encode(declaration, value, new BamlGeneratedEncodeBudget());
    }

    public T Decode<T>(BamlGeneratedType<T> type, BamlGeneratedValue value)
    {
        ArgumentNullException.ThrowIfNull(value);
        TypeDeclaration<T> declaration = RequireType(type);
        return GetCodec(declaration).Decode(new BamlGeneratedCodecContext(this), value);
    }

    public BamlGeneratedArgumentsBuilder<TResult> CreateArgumentsBuilder<TResult>(
        BamlGeneratedFunction<TResult> function) =>
        new(this, RequireFunction(function));

    public BamlGeneratedGenericArgumentsBuilder<TResult> CreateArgumentsBuilder<TResult>(
        BamlGeneratedBoundFunction<TResult> function) =>
        new(this, RequireBoundFunction(function));

    public BamlGeneratedType<T> ResolveType<T>(string position)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(position);
        if (genericBindings.TryGetValue(typeof(T), out TypeDeclaration? declaration))
        {
            if (declaration is not TypeDeclaration<T> typedRegistered)
            {
                throw new InvalidOperationException(
                    $"Generated type registration for {typeof(T)} is contradictory.");
            }

            _ = GetCodec(typedRegistered);
            return new BamlGeneratedType<T>(owner, typedRegistered);
        }

        IResolvedGeneratedType resolved = dynamicTypes.GetOrAdd(
            typeof(T),
            type => CreateCanonicalType(type, position, position));
        if (resolved.Declaration is not TypeDeclaration<T> typed)
        {
            throw new InvalidOperationException(
                $"Canonical generated type resolution for {typeof(T)} is contradictory.");
        }

        BamlDynamicCodecRegistry.Register(this, typed, GetCodec(typed));
        return new BamlGeneratedType<T>(owner, typed);
    }

    public BamlGeneratedTypeBinding BindType<T>(
        BamlGeneratedTypeParameter parameter,
        BamlGeneratedType<T> type)
    {
        GenericFunctionDeclaration function = RequireTypeParameter(parameter);
        TypeDeclaration<T> declaration = RequireType(type);
        if (declaration.Metadata.Length == 0)
        {
            throw new InvalidOperationException(
                $"Generated type {declaration.Identity} has no BAML metadata for {parameter.Declaration.WireIdentity}.");
        }

        return new BamlGeneratedTypeBinding(
            owner,
            function,
            parameter.Declaration,
            declaration);
    }

    public BamlGeneratedTypeArgument TypeArgument<T>(BamlGeneratedType<T> type) =>
        new(owner, RequireType(type));

    public BamlGeneratedTypeFactoryResult CreateClassTypeFactoryResult<T>(
        string bamlIdentity,
        IBamlGeneratedCodec<T> codec,
        params BamlGeneratedTypeArgument[] typeArguments)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(bamlIdentity);
        ArgumentNullException.ThrowIfNull(codec);
        ArgumentNullException.ThrowIfNull(typeArguments);
        var declarations = new TypeDeclaration[typeArguments.Length];
        for (int index = 0; index < typeArguments.Length; index++)
        {
            BamlGeneratedTypeArgument argument = typeArguments[index];
            if (!ReferenceEquals(argument.Owner, owner))
            {
                throw new InvalidOperationException(
                    $"Generated class type argument {index} belongs to another registry.");
            }

            declarations[index] = argument.Declaration;
        }

        var declaration = new TypeDeclaration<T>(
            Interlocked.Decrement(ref nextDynamicTypeId),
            bamlIdentity,
            BamlGeneratedTypeMetadata.Class(bamlIdentity, declarations));
        var resolved = new ResolvedGeneratedType<T>(declaration, new CodecBox<T>(codec));
        return new BamlGeneratedTypeFactoryResult(owner, resolved);
    }

    public BamlGeneratedBoundFunction<TResult> BindFunction<TResult>(
        BamlGeneratedGenericFunction function,
        BamlGeneratedType<TResult> resultType,
        params BamlGeneratedTypeBinding[] typeBindings)
    {
        GenericFunctionDeclaration declaration = RequireGenericFunction(function);
        TypeDeclaration<TResult> result = RequireType(resultType);
        ArgumentNullException.ThrowIfNull(typeBindings);
        if (typeBindings.Length != declaration.TypeParameters.Count)
        {
            throw new InvalidOperationException(
                $"Generated function {declaration.Identity} ({declaration.Variant}) expected {declaration.TypeParameters.Count} type binding(s), received {typeBindings.Length}.");
        }

        var snapshot = new BamlGeneratedTypeBinding[typeBindings.Length];
        for (int index = 0; index < typeBindings.Length; index++)
        {
            BamlGeneratedTypeBinding binding = typeBindings[index];
            TypeParameterDeclaration expected = declaration.TypeParameters[index];
            if (!ReferenceEquals(binding.Owner, owner)
                || !ReferenceEquals(binding.Function, declaration)
                || !ReferenceEquals(binding.Parameter, expected))
            {
                throw new InvalidOperationException(
                    $"Generated type binding {index} does not belong to {declaration.Identity} ({declaration.Variant}).");
            }

            snapshot[index] = binding;
        }

        var bound = new BoundGenericFunctionDeclaration<TResult>(
            declaration,
            result,
            Array.AsReadOnly(snapshot));
        return new BamlGeneratedBoundFunction<TResult>(owner, bound);
    }

    internal RegistryOwner Owner => owner;

    internal TypeDeclaration<T> RequireType<T>(BamlGeneratedType<T> type)
    {
        if (!ReferenceEquals(type.Owner, owner)
            || (!types.TryGetValue(type.Declaration.Id, out TypeDeclaration? stored)
                || !ReferenceEquals(stored, type.Declaration))
            && (!dynamicTypes.TryGetValue(typeof(T), out IResolvedGeneratedType? dynamic)
                || !ReferenceEquals(dynamic.Declaration, type.Declaration)))
        {
            throw new InvalidOperationException(
                "The generated type token does not belong to this registry.");
        }

        return type.Declaration;
    }

    internal FunctionDeclaration RequireFunction<TResult>(BamlGeneratedFunction<TResult> function)
    {
        if (!ReferenceEquals(function.Owner, owner)
            || !functions.TryGetValue(function.Declaration.Id, out FunctionDeclaration? stored)
            || !ReferenceEquals(stored, function.Declaration)
            || !ReferenceEquals(stored.Result, function.Result))
        {
            throw new InvalidOperationException(
                "The generated function token does not belong to this registry or has a contradictory result type.");
        }

        _ = GetCodec(function.Result);
        return stored;
    }

    internal BoundGenericFunctionDeclaration<TResult> RequireBoundFunction<TResult>(
        BamlGeneratedBoundFunction<TResult> function)
    {
        BoundGenericFunctionDeclaration<TResult> bound = function.Declaration;
        if (!ReferenceEquals(function.Owner, owner)
            || !genericFunctions.TryGetValue(
                bound.Definition.Id,
                out GenericFunctionDeclaration? stored)
            || !ReferenceEquals(stored, bound.Definition))
        {
            throw new InvalidOperationException(
                "The bound generated function token does not belong to this registry.");
        }

        _ = GetCodec(bound.Result);
        return bound;
    }

    internal void RequireGenericArgument(
        GenericFunctionDeclaration function,
        GenericArgumentDeclaration argument)
    {
        if (!genericFunctions.TryGetValue(
                function.Id,
                out GenericFunctionDeclaration? stored)
            || !ReferenceEquals(stored, function)
            || !stored.Contains(argument))
        {
            throw new InvalidOperationException(
                "The generated generic argument token does not belong to this function.");
        }
    }

    internal byte[] RequireTypeArgumentMetadata(BamlGeneratedTypeArgument argument)
    {
        if (!ReferenceEquals(argument.Owner, owner)
            || argument.Declaration.Metadata.Length == 0)
        {
            throw new InvalidOperationException(
                "The generated type-argument token does not belong to this registry or has no metadata.");
        }

        return argument.Declaration.Metadata.ToArray();
    }

    internal void RegisterDynamicTypes(IEnumerable<TypeDeclaration> declarations)
    {
        foreach (TypeDeclaration declaration in declarations)
        {
            if (codecs.TryGetValue(declaration.Id, out ICodecBox? codec))
            {
                BamlDynamicCodecRegistry.Register(this, declaration, codec);
            }
        }
    }

    internal BamlGeneratedValue Encode<T>(TypeDeclaration<T> type, T value) =>
        Encode(type, value, new BamlGeneratedEncodeBudget());

    internal BamlGeneratedValue Encode<T>(
        BamlGeneratedType<T> type,
        T value,
        BamlGeneratedEncodeBudget budget) =>
        Encode(RequireType(type), value, budget);

    internal BamlGeneratedValue EncodeForwarded<T>(
        BamlGeneratedType<T> type,
        T value,
        BamlGeneratedEncodeBudget budget) =>
        GetCodec(RequireType(type)).Encode(
            new BamlGeneratedCodecContext(this, budget),
            value);

    internal BamlGeneratedValue Encode<T>(
        TypeDeclaration<T> type,
        T value,
        BamlGeneratedEncodeBudget budget)
    {
        using IDisposable scope = budget.Enter(value, typeof(T));
        return GetCodec(type).Encode(new BamlGeneratedCodecContext(this, budget), value);
    }

    internal T Decode<T>(TypeDeclaration<T> type, BamlGeneratedValue value) =>
        GetCodec(type).Decode(new BamlGeneratedCodecContext(this), value);

    internal void RequireArgument(FunctionDeclaration function, ArgumentDeclaration argument)
    {
        if (!functions.TryGetValue(function.Id, out FunctionDeclaration? stored)
            || !ReferenceEquals(stored, function)
            || !stored.ContainsArgument(argument))
        {
            throw new InvalidOperationException(
                "The generated argument token does not belong to this function.");
        }
    }

    private CodecBox<T> GetCodec<T>(TypeDeclaration<T> declaration)
    {
        if (codecs.TryGetValue(declaration.Id, out ICodecBox? untyped)
            && untyped is CodecBox<T> typed
            && types.TryGetValue(declaration.Id, out TypeDeclaration? stored)
            && ReferenceEquals(stored, declaration))
        {
            return typed;
        }

        if (dynamicTypes.TryGetValue(typeof(T), out IResolvedGeneratedType? dynamic)
            && ReferenceEquals(dynamic.Declaration, declaration)
            && dynamic.Codec is CodecBox<T> dynamicCodec)
        {
            return dynamicCodec;
        }

        throw new InvalidOperationException(
            $"No compatible generated codec is registered for {declaration.Identity}.");
    }

    [UnconditionalSuppressMessage(
        "AotAnalysis",
        "IL2060",
        Justification = "Generated generic type factories and their closed codec members are emitted together and retained by the generated registry contract.")]
    private IResolvedGeneratedType CreateCanonicalType(
        Type type,
        string position,
        string path)
    {
        Type? nullableInner = Nullable.GetUnderlyingType(type);
        if (nullableInner is not null)
        {
            IResolvedGeneratedType inner = ResolveCanonicalType(
                nullableInner,
                position,
                $"{path}.nullable");
            return InvokeFactory(
                CreateNullableValueTypeMethod,
                [nullableInner],
                [inner]);
        }

        if (!type.IsGenericType)
        {
            throw UnsupportedGenericType(type, position, path);
        }

        Type definition = type.GetGenericTypeDefinition();
        Type[] arguments = type.GetGenericArguments();
        if (definition == typeof(global::Baml.BamlNullable<>))
        {
            Type innerType = arguments[0];
            if (Nullable.GetUnderlyingType(innerType) is not null
                || innerType.IsGenericType
                && innerType.GetGenericTypeDefinition()
                    == typeof(global::Baml.BamlNullable<>))
            {
                throw UnsupportedGenericType(type, position, $"{path}.nullable");
            }

            IResolvedGeneratedType inner = ResolveCanonicalType(
                innerType,
                position,
                $"{path}.nullable");
            return InvokeFactory(CreateNullableTypeMethod, arguments, [inner]);
        }

        if (definition == typeof(IReadOnlyList<>))
        {
            IResolvedGeneratedType item = ResolveCanonicalType(
                arguments[0],
                position,
                $"{path}.item");
            return InvokeFactory(CreateListTypeMethod, arguments, [item]);
        }

        if (definition == typeof(IReadOnlyDictionary<,>))
        {
            IResolvedGeneratedType key = ResolveCanonicalType(
                arguments[0],
                position,
                $"{path}.key");
            IResolvedGeneratedType value = ResolveCanonicalType(
                arguments[1],
                position,
                $"{path}.value");
            BamlTy keyMetadata = BamlGeneratedTypeMetadata.Parse(
                key.Declaration.Metadata,
                "generic map key");
            if (keyMetadata.TyCase is not BamlTy.TyOneofCase.Primitive
                and not BamlTy.TyOneofCase.Enum)
            {
                throw UnsupportedGenericType(arguments[0], position, $"{path}.key");
            }

            if (keyMetadata.TyCase == BamlTy.TyOneofCase.Primitive
                && keyMetadata.Primitive.Kind
                    != BamlTyPrimitiveKind.BamlTyPrimitiveString)
            {
                throw UnsupportedGenericType(arguments[0], position, $"{path}.key");
            }

            return InvokeFactory(CreateMapTypeMethod, arguments, [key, value]);
        }

        if (definition == typeof(global::Baml.BamlUnion<,>))
        {
            IResolvedGeneratedType option0 = ResolveCanonicalType(
                arguments[0],
                position,
                $"{path}.option0");
            IResolvedGeneratedType option1 = ResolveCanonicalType(
                arguments[1],
                position,
                $"{path}.option1");
            return InvokeFactory(
                CreateUnion2TypeMethod,
                arguments,
                [option0, option1]);
        }

        if (genericTypeFactories.TryGetValue(
                definition,
                out GenericTypeFactoryDeclaration? generatedFactory))
        {
            BamlGeneratedTypeFactoryResult result;
            try
            {
                result = (BamlGeneratedTypeFactoryResult)(generatedFactory.Factory
                    .MakeGenericMethod(arguments)
                    .Invoke(null, [this])
                    ?? throw new InvalidOperationException(
                        $"Generated generic type factory for {definition} returned null."));
            }
            catch (TargetInvocationException error) when (error.InnerException is not null)
            {
                ExceptionDispatchInfo.Capture(error.InnerException).Throw();
                throw;
            }

            if (!ReferenceEquals(result.Owner, owner)
                || result.Resolved.Declaration.ClrType != type)
            {
                throw new InvalidOperationException(
                    $"Generated generic type factory for {definition} returned a contradictory CLR type.");
            }

            return result.Resolved;
        }

        throw UnsupportedGenericType(type, position, path);
    }

    private IResolvedGeneratedType ResolveCanonicalType(
        Type type,
        string position,
        string path)
    {
        if (genericBindings.TryGetValue(type, out TypeDeclaration? declaration))
        {
            if (!codecs.TryGetValue(declaration.Id, out ICodecBox? codec))
            {
                throw new InvalidOperationException(
                    $"Canonical generated type {declaration.Identity} has no codec.");
            }

            return new UntypedResolvedGeneratedType(declaration, codec);
        }

        return dynamicTypes.GetOrAdd(
            type,
            candidate => CreateCanonicalType(candidate, position, path));
    }

    private IResolvedGeneratedType CreateNullableValueType<T>(
        IResolvedGeneratedType inner)
        where T : struct
    {
        TypeDeclaration<T> innerDeclaration = RequireResolved<T>(inner);
        var declaration = new TypeDeclaration<T?>(
            Interlocked.Decrement(ref nextDynamicTypeId),
            $"{innerDeclaration.Identity}?",
            BamlGeneratedTypeMetadata.Optional(innerDeclaration.Metadata));
        var codec = new CodecBox<T?>(new BamlGeneratedNullableValueCodec<T>(
            new BamlGeneratedType<T>(owner, innerDeclaration)));
        return new ResolvedGeneratedType<T?>(declaration, codec);
    }

    private IResolvedGeneratedType CreateNullableType<T>(IResolvedGeneratedType inner)
    {
        TypeDeclaration<T> innerDeclaration = RequireResolved<T>(inner);
        var declaration = new TypeDeclaration<global::Baml.BamlNullable<T>>(
            Interlocked.Decrement(ref nextDynamicTypeId),
            $"{innerDeclaration.Identity}?",
            BamlGeneratedTypeMetadata.Optional(innerDeclaration.Metadata));
        var codec = new CodecBox<global::Baml.BamlNullable<T>>(
            new BamlGeneratedNullableCodec<T>(
                new BamlGeneratedType<T>(owner, innerDeclaration)));
        return new ResolvedGeneratedType<global::Baml.BamlNullable<T>>(declaration, codec);
    }

    private IResolvedGeneratedType CreateListType<T>(IResolvedGeneratedType item)
    {
        TypeDeclaration<T> itemDeclaration = RequireResolved<T>(item);
        var declaration = new TypeDeclaration<IReadOnlyList<T>>(
            Interlocked.Decrement(ref nextDynamicTypeId),
            $"list<{itemDeclaration.Identity}>",
            BamlGeneratedTypeMetadata.List(itemDeclaration.Metadata));
        var codec = new CodecBox<IReadOnlyList<T>>(new BamlGeneratedListCodec<T>(
            new BamlGeneratedType<T>(owner, itemDeclaration),
            itemDeclaration.Metadata));
        return new ResolvedGeneratedType<IReadOnlyList<T>>(declaration, codec);
    }

    private IResolvedGeneratedType CreateMapType<TKey, TValue>(
        IResolvedGeneratedType key,
        IResolvedGeneratedType value)
        where TKey : notnull
    {
        TypeDeclaration<TKey> keyDeclaration = RequireResolved<TKey>(key);
        TypeDeclaration<TValue> valueDeclaration = RequireResolved<TValue>(value);
        BamlTy keyMetadata = BamlGeneratedTypeMetadata.Parse(
            keyDeclaration.Metadata,
            "generic map key");
        if (keyMetadata.TyCase != BamlTy.TyOneofCase.Primitive
            || keyMetadata.Primitive.Kind != BamlTyPrimitiveKind.BamlTyPrimitiveString)
        {
            throw new NotSupportedException(
                $"Generated BAML map keys must resolve to string, received {keyDeclaration.Identity}.");
        }
        var declaration = new TypeDeclaration<IReadOnlyDictionary<TKey, TValue>>(
            Interlocked.Decrement(ref nextDynamicTypeId),
            $"map<{keyDeclaration.Identity},{valueDeclaration.Identity}>",
            BamlGeneratedTypeMetadata.Map(
                keyDeclaration.Metadata,
                valueDeclaration.Metadata));
        var codec = new CodecBox<IReadOnlyDictionary<TKey, TValue>>(
            new BamlGeneratedMapCodec<TKey, TValue>(
                new BamlGeneratedType<TKey>(owner, keyDeclaration),
                new BamlGeneratedType<TValue>(owner, valueDeclaration),
                keyDeclaration.Metadata,
                valueDeclaration.Metadata));
        return new ResolvedGeneratedType<IReadOnlyDictionary<TKey, TValue>>(
            declaration,
            codec);
    }

    private IResolvedGeneratedType CreateUnion2Type<T0, T1>(
        IResolvedGeneratedType option0,
        IResolvedGeneratedType option1)
    {
        TypeDeclaration<T0> declaration0 = RequireResolved<T0>(option0);
        TypeDeclaration<T1> declaration1 = RequireResolved<T1>(option1);
        byte[] metadata = BamlGeneratedTypeMetadata.Union(
            [declaration0.Metadata, declaration1.Metadata]);
        string optionName0 = BamlGeneratedTypeMetadata.OptionName(declaration0.Metadata);
        string optionName1 = BamlGeneratedTypeMetadata.OptionName(declaration1.Metadata);
        var declaration = new TypeDeclaration<global::Baml.BamlUnion<T0, T1>>(
            Interlocked.Decrement(ref nextDynamicTypeId),
            $"{optionName0} | {optionName1}",
            metadata);
        var codec = new CodecBox<global::Baml.BamlUnion<T0, T1>>(
            new BamlGeneratedUnionCodec<T0, T1>(
                new BamlGeneratedType<T0>(owner, declaration0),
                new BamlGeneratedType<T1>(owner, declaration1),
                metadata,
                declaration0.Metadata,
                declaration1.Metadata,
                optionName0,
                optionName1));
        return new ResolvedGeneratedType<global::Baml.BamlUnion<T0, T1>>(
            declaration,
            codec);
    }

    private static TypeDeclaration<T> RequireResolved<T>(IResolvedGeneratedType resolved)
    {
        if (resolved.Declaration is not TypeDeclaration<T> typed)
        {
            throw new InvalidOperationException(
                $"Resolved generated type {resolved.Declaration.Identity} is not {typeof(T)}.");
        }

        return typed;
    }

    [UnconditionalSuppressMessage(
        "AotAnalysis",
        "IL2060",
        Justification = "Canonical collection factories are private closed generic methods retained by this registry type.")]
    private IResolvedGeneratedType InvokeFactory(
        MethodInfo factory,
        Type[] typeArguments,
        object?[] arguments)
    {
        try
        {
            return (IResolvedGeneratedType)(factory
                .MakeGenericMethod(typeArguments)
                .Invoke(this, arguments)
                ?? throw new InvalidOperationException(
                    $"Generated generic type factory {factory.Name} returned null."));
        }
        catch (TargetInvocationException error) when (error.InnerException is not null)
        {
            ExceptionDispatchInfo.Capture(error.InnerException).Throw();
            throw;
        }
    }

    private static MethodInfo RequireFactory(string name) =>
        typeof(BamlGeneratedRegistry).GetMethod(
            name,
            BindingFlags.Instance | BindingFlags.NonPublic)
        ?? throw new InvalidOperationException(
            $"Generated generic type factory {name} is missing.");

    private GenericFunctionDeclaration RequireGenericFunction(
        BamlGeneratedGenericFunction function)
    {
        if (!ReferenceEquals(function.Owner, owner)
            || !genericFunctions.TryGetValue(
                function.Declaration.Id,
                out GenericFunctionDeclaration? stored)
            || !ReferenceEquals(stored, function.Declaration))
        {
            throw new InvalidOperationException(
                "The generated generic function token does not belong to this registry.");
        }

        return stored;
    }

    private GenericFunctionDeclaration RequireTypeParameter(
        BamlGeneratedTypeParameter parameter)
    {
        if (!ReferenceEquals(parameter.Owner, owner)
            || !genericFunctions.TryGetValue(
                parameter.Function.Id,
                out GenericFunctionDeclaration? stored)
            || !ReferenceEquals(stored, parameter.Function)
            || !stored.Contains(parameter.Declaration))
        {
            throw new InvalidOperationException(
                "The generated type-parameter token does not belong to this registry.");
        }

        return stored;
    }

    private static BamlTypeMappingException UnsupportedGenericType(
        Type type,
        string position,
        string path)
    {
        string? replacement = type == typeof(int)
            ? "long"
            : type == typeof(float)
                ? "double"
                : null;
        return new BamlTypeMappingException(
            type,
            position,
            path,
            replacement is null
                ? $"CLR type {type} at {path} is not a registered canonical BAML type."
                : $"CLR type {type} at {path} is noncanonical; use {replacement}.",
            replacement);
    }
}
