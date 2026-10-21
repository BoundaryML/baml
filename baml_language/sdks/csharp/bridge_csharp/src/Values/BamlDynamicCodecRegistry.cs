using System.Collections.Concurrent;

using Baml.Generated.V1;
using BamlBridge.Cffi.V1;
using Google.Protobuf;

namespace Baml;

internal static class BamlDynamicCodecRegistry
{
    private static readonly ConcurrentDictionary<Type, EntrySet> Entries = [];

    internal static void Register(
        BamlGeneratedRegistry registry,
        TypeDeclaration declaration,
        ICodecBox codec)
    {
        ArgumentNullException.ThrowIfNull(registry);
        ArgumentNullException.ThrowIfNull(declaration);
        ArgumentNullException.ThrowIfNull(codec);
        if (declaration.Metadata.Length == 0
            || !IsContextIndependent(declaration.Metadata)
            || IsBuiltIn(declaration.ClrType))
        {
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

    internal static bool IsRegistered(Type type) => Entries.ContainsKey(type);

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

    private static bool IsContextIndependent(BamlTy? type, int depth)
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
                item => IsContextIndependent(item, depth + 1)),
            BamlTy.TyOneofCase.Enum => true,
            BamlTy.TyOneofCase.List => IsContextIndependent(type.List.Item, depth + 1),
            BamlTy.TyOneofCase.Map =>
                IsContextIndependent(type.Map.Key, depth + 1)
                && IsContextIndependent(type.Map.Value, depth + 1),
            BamlTy.TyOneofCase.Optional =>
                IsContextIndependent(type.Optional.Inner, depth + 1),
            BamlTy.TyOneofCase.Unknown or BamlTy.TyOneofCase.Media
                or BamlTy.TyOneofCase.Resource => true,
            BamlTy.TyOneofCase.Literal or BamlTy.TyOneofCase.TypeAlias
                or BamlTy.TyOneofCase.Union or BamlTy.TyOneofCase.EnumVariant => false,
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
