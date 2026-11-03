using System.Collections.Concurrent;

using Baml.Generated.V1;
using BamlBridge.Cffi.V1;
using Google.Protobuf;

namespace Baml;

internal static class BamlDynamicCodecRegistry
{
    private static readonly ConcurrentDictionary<Type, EntrySet> Entries = [];

    // Tuple and list declarations whose slot or item types name unions. A
    // union is not context-free on its own (its CLR arms are one ordering of
    // its options), so these never decode a value by its own type alone; they
    // serve tuples that carry their type: a tuple identity names its union
    // slots exactly, and a list of the join of its slot types is the fallback
    // representation when no tuple type is registered.
    private static readonly ConcurrentDictionary<Type, IReadOnlyList<Entry>> Structural = [];

    internal static void Register(
        BamlGeneratedRegistry registry,
        TypeDeclaration declaration,
        ICodecBox codec)
    {
        ArgumentNullException.ThrowIfNull(registry);
        ArgumentNullException.ThrowIfNull(declaration);
        ArgumentNullException.ThrowIfNull(codec);
        if (declaration.Metadata.Length == 0 || IsBuiltIn(declaration.ClrType))
        {
            return;
        }

        if (!IsContextIndependent(declaration.Metadata))
        {
            if (IsStructuralWithUnions(declaration.Metadata))
            {
                RegisterStructural(registry, declaration, codec);
            }

            return;
        }

        var candidate = new Entry(
            registry,
            declaration,
            codec,
            BamlTypeDescriptor.FromMetadata(declaration.Metadata));
        while (true)
        {
            if (!Entries.TryGetValue(declaration.ClrType, out EntrySet? stored))
            {
                if (Entries.TryAdd(declaration.ClrType, new EntrySet(candidate)))
                {
                    return;
                }

                continue;
            }

            EntrySet updated = stored.Add(declaration.ClrType, candidate);
            if (ReferenceEquals(updated, stored)
                || Entries.TryUpdate(declaration.ClrType, updated, stored))
            {
                return;
            }
        }
    }

    internal static bool TryEncode<T>(T value, out BamlValue? encoded)
    {
        if (!Entries.TryGetValue(typeof(T), out EntrySet? entries))
        {
            encoded = null;
            return false;
        }

        Entry entry = entries.Canonical;
        BamlGeneratedValue generated = entry.Codec.Encode(
            new BamlGeneratedCodecContext(entry.Registry),
            value);
        encoded = new BamlValue(
            generated.WithDeclaredType(entry.Declaration.Metadata),
            entry.Registry);
        BamlValueLimits.ValidateGraph(encoded);
        return true;
    }

    internal static bool TryDecode<T>(BamlValue value, out T result)
    {
        if (!Entries.TryGetValue(typeof(T), out EntrySet? entries)
            || !entries.TryGetValue(value.Type, out Entry entry))
        {
            result = default!;
            return false;
        }

        object? decoded = entry.Codec.Decode(
            new BamlGeneratedCodecContext(entry.Registry),
            value.GeneratedValue);
        if (decoded is T typed)
        {
            result = typed;
            return true;
        }

        result = default!;
        return false;
    }

    /// <summary>
    /// Decode a tuple that carries no type, or a plain list, as the registered
    /// tuple type <typeparamref name="T"/>: the requested type supplies the
    /// shape. The arity must match exactly and every slot must decode through
    /// its own slot codec; anything else is no match.
    /// </summary>
    internal static bool TryDecodeTupleShape<T>(BamlValue value, out T result)
    {
        result = default!;
        if (!Entries.TryGetValue(typeof(T), out EntrySet? entries))
        {
            return false;
        }

        Entry entry = entries.Canonical;
        IReadOnlyList<BamlGeneratedValue> items = value.GeneratedValue.Kind switch
        {
            PrimitiveCarrierKind.Tuple => value.GeneratedValue.ReadTuple(),
            PrimitiveCarrierKind.List => value.GeneratedValue.ReadList(),
            _ => Array.Empty<BamlGeneratedValue>(),
        };
        if (entry.Descriptor.Kind != BamlTypeDescriptorKind.Tuple
            || entry.Descriptor.IsNullable
            || entry.Descriptor.Arguments.Count != items.Count)
        {
            return false;
        }

        object? decoded;
        try
        {
            decoded = entry.Codec.Decode(
                new BamlGeneratedCodecContext(entry.Registry),
                BamlGeneratedValue.CreateTuple(items, tupleTypeMetadata: null, "$"));
        }
        catch (Exception error) when (error is BamlProtocolException or BamlTypeMappingException)
        {
            return false;
        }

        if (decoded is T typed)
        {
            result = typed;
            return true;
        }

        return false;
    }

    /// <summary>
    /// Decode a tuple that carries its type as a registered tuple type whose
    /// slots name unions: the carried identity must be exactly the registered
    /// one. Tuple types without unions are found by <see cref="TryDecode{T}"/>.
    /// </summary>
    internal static bool TryDecodeCarriedTuple<T>(BamlValue value, out T result)
    {
        result = default!;
        if (value.GeneratedValue.TupleTypeMetadata is not { Length: > 0 }
            || !Structural.TryGetValue(typeof(T), out IReadOnlyList<Entry>? entries))
        {
            return false;
        }

        foreach (Entry entry in entries)
        {
            if (entry.Descriptor.Kind != BamlTypeDescriptorKind.Tuple
                || !entry.Descriptor.Equals(value.Type))
            {
                continue;
            }

            if (entry.Codec.Decode(
                    new BamlGeneratedCodecContext(entry.Registry),
                    value.GeneratedValue) is T typed)
            {
                result = typed;
                return true;
            }
        }

        return false;
    }

    /// <summary>
    /// Decode a tuple that carries its type as a registered list of the join
    /// of its carried slot types, <c>(T1 | T2 | …)[]</c>: the fallback when its
    /// tuple type has no registered C# type. The list's item type must name
    /// exactly the join's members (in any order); each slot crosses as the
    /// union arm of its own carried slot type, never one guessed from its
    /// value. Every slot is kept, in order.
    /// </summary>
    internal static bool TryDecodeTupleJoinList<T>(BamlValue value, out T result)
    {
        result = default!;
        if (value.GeneratedValue.Kind != PrimitiveCarrierKind.Tuple
            || value.GeneratedValue.TupleTypeMetadata is not { Length: > 0 } carried)
        {
            return false;
        }

        BamlTy carriedType = BamlGeneratedTypeMetadata.Parse(carried, "carried tuple");
        if (carriedType.TyCase != BamlTy.TyOneofCase.Tuple)
        {
            return false;
        }

        var candidates = new List<Entry>();
        if (Entries.TryGetValue(typeof(T), out EntrySet? set))
        {
            candidates.Add(set.Canonical);
        }

        if (Structural.TryGetValue(typeof(T), out IReadOnlyList<Entry>? structural))
        {
            candidates.AddRange(structural);
        }

        IReadOnlyList<BamlGeneratedValue> items = value.GeneratedValue.ReadTuple();
        var join = new HashSet<BamlTypeDescriptor>();
        foreach (BamlTy slot in carriedType.Tuple.Items)
        {
            join.UnionWith(Members(slot).Select(Descriptor));
        }

        foreach (Entry entry in candidates)
        {
            BamlTy listType = BamlGeneratedTypeMetadata.Parse(
                entry.Declaration.Metadata,
                "join list");
            if (listType.TyCase != BamlTy.TyOneofCase.List)
            {
                continue;
            }

            IReadOnlyList<BamlTy> options = Members(listType.List.Item);
            if (!join.SetEquals(options.Select(Descriptor)))
            {
                continue;
            }

            byte[] itemMetadata = listType.List.Item.ToByteArray();
            BamlTy[] arms = options
                .Where(option => !IsNull(option))
                .ToArray();
            var restated = new BamlGeneratedValue[items.Count];
            bool representable = true;
            for (int index = 0; index < items.Count && representable; index++)
            {
                BamlGeneratedValue? item = arms.Length < 2
                    ? items[index]
                    : AsJoinArm(items[index], carriedType.Tuple.Items[index], arms, itemMetadata);
                representable = item is not null;
                restated[index] = item!;
            }

            if (!representable)
            {
                continue;
            }

            try
            {
                if (entry.Codec.Decode(
                        new BamlGeneratedCodecContext(entry.Registry),
                        BamlGeneratedValue.CreateList(
                            Array.AsReadOnly(restated),
                            itemMetadata,
                            "$")) is T typed)
                {
                    result = typed;
                    return true;
                }
            }
            catch (Exception error) when (error is BamlProtocolException or BamlTypeMappingException)
            {
                // Not this representation; try the next one.
            }
        }

        return false;
    }

    /// <summary>
    /// A tuple slot as an item of a list of the join: the union arm of the
    /// slot's carried type (or, for a union slot, of the arm the engine
    /// selected), with the join as its union type.
    /// </summary>
    private static BamlGeneratedValue? AsJoinArm(
        BamlGeneratedValue item,
        BamlTy slotType,
        IReadOnlyList<BamlTy> arms,
        byte[] joinMetadata)
    {
        if (item.Kind == PrimitiveCarrierKind.Null)
        {
            return item;
        }

        BamlGeneratedValue payload = item;
        BamlTy[] slotArms = Members(slotType).Where(member => !IsNull(member)).ToArray();
        BamlTypeDescriptor? selected = null;
        if (slotArms.Length == 1)
        {
            selected = Descriptor(slotArms[0]);
        }
        else if (item.Kind == PrimitiveCarrierKind.Union)
        {
            payload = item.ReadUnionPayload();
            if (item.UnionSelectedTypeMetadata is { Length: > 0 } selectedMetadata)
            {
                selected = Descriptor(
                    BamlGeneratedTypeMetadata.Parse(selectedMetadata, "union selected"));
            }
            else
            {
                string optionName = item.ReadUnionOptionName();
                BamlTy[] named = slotArms
                    .Where(arm => StringComparer.Ordinal.Equals(
                        BamlGeneratedTypeMetadata.OptionName(arm.ToByteArray()),
                        optionName))
                    .ToArray();
                selected = named.Length == 1 ? Descriptor(named[0]) : null;
            }
        }

        if (selected is null)
        {
            return null;
        }

        BamlTy[] matching = arms.Where(arm => Descriptor(arm).Equals(selected)).ToArray();
        if (matching.Length != 1)
        {
            return null;
        }

        byte[] arm = matching[0].ToByteArray();
        return BamlGeneratedValue.CreateUnion(
            joinMetadata,
            arm,
            BamlGeneratedTypeMetadata.OptionName(arm),
            payload,
            "$");
    }

    /// <summary>
    /// The members a type contributes to a join: a union's options and an
    /// optional's inner type plus <c>null</c>, flattened.
    /// </summary>
    private static IReadOnlyList<BamlTy> Members(BamlTy type)
    {
        var members = new List<BamlTy>();
        void Visit(BamlTy current, int depth)
        {
            if (depth > BamlValueLimits.MaxDepth)
            {
                throw new BamlProtocolException(
                    "A carried BAML tuple type is too deep.",
                    "A join of tuple slot types exceeded the BAML value depth limit.");
            }

            switch (current.TyCase)
            {
                case BamlTy.TyOneofCase.Union:
                    foreach (BamlTy option in current.Union.Options)
                    {
                        Visit(option, depth + 1);
                    }
                    break;
                case BamlTy.TyOneofCase.Optional:
                    Visit(current.Optional.Inner, depth + 1);
                    members.Add(new BamlTy
                    {
                        Primitive = new BamlTyPrimitive
                        {
                            Kind = BamlTyPrimitiveKind.BamlTyPrimitiveNull,
                        },
                    });
                    break;
                default:
                    members.Add(current);
                    break;
            }
        }

        Visit(type, depth: 0);
        return members;
    }

    private static bool IsNull(BamlTy type) =>
        type.TyCase == BamlTy.TyOneofCase.Primitive
        && type.Primitive.Kind == BamlTyPrimitiveKind.BamlTyPrimitiveNull;

    private static BamlTypeDescriptor Descriptor(BamlTy type) =>
        BamlTypeDescriptor.FromMetadata(type.ToByteArray());

    internal static bool IsRegistered(Type type) =>
        Entries.ContainsKey(type) || Structural.ContainsKey(type);

    private static void RegisterStructural(
        BamlGeneratedRegistry registry,
        TypeDeclaration declaration,
        ICodecBox codec)
    {
        var candidate = new Entry(
            registry,
            declaration,
            codec,
            BamlTypeDescriptor.FromMetadata(declaration.Metadata));
        Structural.AddOrUpdate(
            declaration.ClrType,
            _ => [candidate],
            (_, stored) => stored.Any(entry => entry.Descriptor.Equals(candidate.Descriptor))
                ? stored
                : [.. stored, candidate]);
    }

    /// <summary>
    /// A tuple whose slots, or a list whose item, name unions of otherwise
    /// context-free types: the declarations a carried tuple type can reach
    /// through <see cref="Structural"/>.
    /// </summary>
    private static bool IsStructuralWithUnions(byte[] metadata)
    {
        BamlTy type = BamlGeneratedTypeMetadata.Parse(metadata, "generated");
        return type.TyCase switch
        {
            BamlTy.TyOneofCase.Tuple => type.Tuple.Items.All(
                item => IsContextIndependent(item, depth: 1, allowUnions: true)),
            BamlTy.TyOneofCase.List =>
                IsContextIndependent(type.List.Item, depth: 1, allowUnions: true),
            _ => false,
        };
    }

    private static bool IsBuiltIn(Type type) =>
        type == typeof(BamlValue)
        || type == typeof(bool)
        || type == typeof(long)
        || type == typeof(double)
        || type == typeof(System.Numerics.BigInteger)
        || type == typeof(string)
        || type == typeof(ReadOnlyMemory<byte>)
        || type == typeof(BamlImage)
        || type == typeof(BamlAudio)
        || type == typeof(BamlVideo)
        || type == typeof(BamlPdf)
        || type == typeof(BamlHandle);

    private static bool IsContextIndependent(byte[] metadata)
    {
        BamlTy type;
        try
        {
            type = BamlTy.Parser.ParseFrom(metadata);
        }
        catch (InvalidProtocolBufferException error)
        {
            throw new BamlProtocolException(
                "Generated BAML type metadata is malformed.",
                error.Message);
        }

        return IsContextIndependent(type, depth: 0);
    }

    private static bool IsContextIndependent(BamlTy? type, int depth, bool allowUnions = false)
    {
        if (depth > BamlValueLimits.MaxDepth
            || type is null
            || type.TyCase == BamlTy.TyOneofCase.None)
        {
            return false;
        }

        return type.TyCase switch
        {
            BamlTy.TyOneofCase.Primitive =>
                type.Primitive.Kind != BamlTyPrimitiveKind.BamlTyPrimitiveNull,
            BamlTy.TyOneofCase.ClassTy => type.ClassTy.TypeArgs.All(
                item => IsContextIndependent(item, depth + 1, allowUnions)),
            BamlTy.TyOneofCase.Enum => true,
            BamlTy.TyOneofCase.List => IsContextIndependent(type.List.Item, depth + 1, allowUnions),
            BamlTy.TyOneofCase.Map =>
                IsContextIndependent(type.Map.Key, depth + 1, allowUnions)
                && IsContextIndependent(type.Map.Value, depth + 1, allowUnions),
            BamlTy.TyOneofCase.Optional =>
                IsContextIndependent(type.Optional.Inner, depth + 1, allowUnions),
            BamlTy.TyOneofCase.Tuple => type.Tuple.Items.All(
                item => IsContextIndependent(item, depth + 1, allowUnions)),
            BamlTy.TyOneofCase.Unknown or BamlTy.TyOneofCase.Media
                or BamlTy.TyOneofCase.Resource => true,
            BamlTy.TyOneofCase.Union => allowUnions && type.Union.Options.All(
                item => IsNull(item) || IsContextIndependent(item, depth + 1, allowUnions)),
            BamlTy.TyOneofCase.Literal or BamlTy.TyOneofCase.TypeAlias
                or BamlTy.TyOneofCase.EnumVariant => false,
            _ => false,
        };
    }

    private sealed record Entry(
        BamlGeneratedRegistry Registry,
        TypeDeclaration Declaration,
        ICodecBox Codec,
        BamlTypeDescriptor Descriptor);

    private sealed class EntrySet
    {
        // Nullable reference annotations erase to the same CLR Type. Keep both
        // exact occurrence descriptors for decode and prefer the non-null one
        // for context-free encoding.
        private readonly IReadOnlyDictionary<BamlTypeDescriptor, Entry> entries;

        internal EntrySet(Entry entry)
            : this(entry, new Dictionary<BamlTypeDescriptor, Entry> { [entry.Descriptor] = entry })
        {
        }

        private EntrySet(
            Entry canonical,
            IReadOnlyDictionary<BamlTypeDescriptor, Entry> entries)
        {
            Canonical = canonical;
            this.entries = entries;
        }

        internal Entry Canonical { get; }

        internal EntrySet Add(Type clrType, Entry candidate)
        {
            if (entries.ContainsKey(candidate.Descriptor))
            {
                return this;
            }

            if (clrType.IsValueType
                || entries.Keys.Any(descriptor =>
                    !AreNullableReferenceAliases(descriptor, candidate.Descriptor)))
            {
                throw new InvalidOperationException(
                    $"CLR type {clrType} has contradictory context-free BAML descriptors "
                    + $"{Canonical.Descriptor} and {candidate.Descriptor}.");
            }

            var updated = new Dictionary<BamlTypeDescriptor, Entry>(entries)
            {
                [candidate.Descriptor] = candidate,
            };
            Entry canonical = Canonical.Descriptor.IsNullable
                && !candidate.Descriptor.IsNullable
                ? candidate
                : Canonical;
            return new EntrySet(canonical, updated);
        }

        internal bool TryGetValue(BamlTypeDescriptor descriptor, out Entry entry) =>
            entries.TryGetValue(descriptor, out entry!);

        private static bool AreNullableReferenceAliases(
            BamlTypeDescriptor left,
            BamlTypeDescriptor right) =>
            left.IsNullable != right.IsNullable
            && left.Kind == right.Kind
            && StringComparer.Ordinal.Equals(left.Fqn, right.Fqn)
            && left.Arguments.SequenceEqual(right.Arguments)
            && StringComparer.Ordinal.Equals(left.Alias, right.Alias)
            && StringComparer.Ordinal.Equals(left.Literal, right.Literal);
    }
}
