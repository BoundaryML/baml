namespace Baml;

internal static class BamlInteger
{
    internal const long Minimum = -4_611_686_018_427_387_904;
    internal const long Maximum = 4_611_686_018_427_387_903;

    internal static long Require(long value, string path)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(path);
        if (value is < Minimum or > Maximum)
        {
            throw new BamlProtocolException(
                $"BAML integer at {path} is outside the supported range.",
                $"BAML integer {value} at {path} is outside [{Minimum}, {Maximum}].");
        }

        return value;
    }
}
