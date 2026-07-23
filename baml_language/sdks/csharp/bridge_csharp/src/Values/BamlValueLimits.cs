namespace Baml;

internal static class BamlValueLimits
{
    internal const int MaxDepth = 64;
    internal const int MaxCollectionItems = 1_000_000;
    internal const int MaxBytes = 64 * 1024 * 1024;
    internal const int MaxNodes = 2_000_000;

    internal static void RequireBytes(long length, string path, Type clrType)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(path);
        ArgumentNullException.ThrowIfNull(clrType);
        if (length > MaxBytes)
        {
            throw new BamlTypeMappingException(
                clrType,
                "dynamic value",
                path,
                $"The value at {path} exceeds the {MaxBytes}-byte BAML limit.");
        }
    }

    internal static void RequireCollection(int count, string path, Type clrType)
    {
        if (count > MaxCollectionItems)
        {
            throw new BamlTypeMappingException(
                clrType,
                "dynamic value",
                path,
                $"The collection at {path} exceeds the {MaxCollectionItems}-item BAML limit.");
        }
    }

    internal static void ValidateGraph(BamlValue root)
    {
        int nodes = 0;
        Visit(root, "$", depth: 0, ref nodes);
    }

    private static void Visit(BamlValue value, string path, int depth, ref int nodes)
    {
        if (depth > MaxDepth)
        {
            throw Limit(path, nameof(MaxDepth));
        }

        nodes++;
        if (nodes > MaxNodes)
        {
            throw Limit(path, nameof(MaxNodes));
        }

        if (value.Kind == BamlValueKind.List)
        {
            IReadOnlyList<BamlValue> values = value.ReadListValues();
            for (int index = 0; index < values.Count; index++)
            {
                Visit(values[index], $"{path}[{index}]", depth + 1, ref nodes);
            }
        }
        else if (value.Kind == BamlValueKind.Map)
        {
            foreach ((string key, BamlValue child) in value.ReadMapValues())
            {
                Visit(child, $"{path}[{key}]", depth + 1, ref nodes);
            }
        }
        else if (value.Kind == BamlValueKind.Class)
        {
            foreach ((string key, BamlValue child) in value.ReadClassValues())
            {
                Visit(child, $"{path}.{key}", depth + 1, ref nodes);
            }
        }
        else if (value.Kind == BamlValueKind.Union)
        {
            Visit(value.ReadUnionValue(), $"{path}<union>", depth + 1, ref nodes);
        }
    }

    private static BamlTypeMappingException Limit(string path, string limit) =>
        new(
            typeof(BamlValue),
            "dynamic value",
            path,
            $"The BAML value exceeded {limit}.");
}

internal sealed class BamlDecodeBudget
{
    private int nodes;

    internal void Visit(string path, int depth)
    {
        if (depth > BamlValueLimits.MaxDepth)
        {
            throw Limit(path, nameof(BamlValueLimits.MaxDepth));
        }

        nodes++;
        if (nodes > BamlValueLimits.MaxNodes)
        {
            throw Limit(path, nameof(BamlValueLimits.MaxNodes));
        }
    }

    internal static void RequireCollection(int count, string path)
    {
        if (count > BamlValueLimits.MaxCollectionItems)
        {
            throw Limit(path, nameof(BamlValueLimits.MaxCollectionItems));
        }
    }

    internal static void RequireBytes(long length, string path)
    {
        if (length > BamlValueLimits.MaxBytes)
        {
            throw Limit(path, nameof(BamlValueLimits.MaxBytes));
        }
    }

    private static BamlProtocolException Limit(string path, string limit) =>
        new(
            "The native bridge returned a BAML value that exceeds managed resource limits.",
            $"The value at {path} exceeded {limit}.");
}
