using System.Buffers.Binary;
using System.Security.Cryptography;
using System.Text;

internal static class Program
{
    private static readonly UTF8Encoding StrictUtf8 = new(
        encoderShouldEmitUTF8Identifier: false,
        throwOnInvalidBytes: true);

    private static readonly EnumIdentity[] GoldenVectors =
    [
        new(
            [],
            [],
            "Status",
            "Ok",
            "8d456dc2675796e473082e4c7db9de6b362b80926a658d5953bc41abd35ab861",
            956_291_177_610_974_948L),
        new(
            ["acme"],
            ["billing", "v1"],
            "PaymentStatus",
            "AwaitingPayment",
            "0bcfcb1fba494e88b1f313df6ecff6bfd9125bf4cc650dbc470ff06fe557affc",
            851_122_191_726_104_200L),
        new(
            ["a", "bc"],
            [],
            "E",
            "V",
            "364af088fe2a9c68024bf2cd2e4686eefcb48bac9dbd53bd618bcc9fb343e3eb",
            3_912_203_697_495_121_000L),
        new(
            ["ab", "c"],
            [],
            "E",
            "V",
            "de4cd125297c104ec36614e28129a4037e151a30b35927f44f5df876c98ae899",
            6_795_035_895_335_227_470L),
    ];

    public static int Main()
    {
        foreach (EnumIdentity vector in GoldenVectors)
        {
            Discriminant result = Compute(vector);
            Require(
                StringComparer.Ordinal.Equals(result.Sha256, vector.ExpectedSha256),
                $"SHA-256 mismatch for {vector.EnumSymbol}.{vector.VariantSymbol}");
            Require(
                result.Value == vector.ExpectedValue,
                $"discriminant mismatch for {vector.EnumSymbol}.{vector.VariantSymbol}");
            Require(result.Value > 0, "discriminant must be positive");
        }

        Require(
            GoldenVectors[2].ExpectedValue != GoldenVectors[3].ExpectedValue,
            "length-delimited package segments aliased");

        EnumIdentity inserted = new(
            ["acme"],
            ["billing", "v1"],
            "PaymentStatus",
            "Inserted",
            ExpectedSha256: string.Empty,
            ExpectedValue: 0);
        Dictionary<string, long> before = ComputeMembers(
            [GoldenVectors[0], GoldenVectors[1]]);
        Dictionary<string, long> after = ComputeMembers(
            [inserted, GoldenVectors[1], GoldenVectors[0]]);
        foreach ((string identity, long value) in before)
        {
            Require(
                after.TryGetValue(identity, out long afterValue)
                && afterValue == value,
                $"member insertion/reordering changed {identity}");
        }

        Expect<InvalidOperationException>(
            () => ValidateDistinct(
                [
                    ("pkg.E.A", 42L),
                    ("pkg.E.B", 42L),
                ]),
            "duplicate generated enum discriminant");
        Expect<InvalidOperationException>(
            () => ValidateDistinct([("pkg.E.Zero", 0L)]),
            "zero generated enum discriminant");

        Console.WriteLine("enum_discriminant_golden_vectors=4/4");
        Console.WriteLine("enum_discriminant_segment_boundaries=distinct");
        Console.WriteLine("enum_discriminant_reorder_insert=stable");
        Console.WriteLine("enum_discriminant_zero_collision=fail_closed");
        return 0;
    }

    private static Dictionary<string, long> ComputeMembers(
        IEnumerable<EnumIdentity> identities)
    {
        var values = new Dictionary<string, long>(StringComparer.Ordinal);
        foreach (EnumIdentity identity in identities)
        {
            string key = IdentityKey(identity);
            if (!values.TryAdd(key, Compute(identity).Value))
            {
                throw new InvalidOperationException(
                    $"duplicate typed enum identity {key}");
            }
        }

        ValidateDistinct(values.Select(pair => (pair.Key, pair.Value)));
        return values;
    }

    private static void ValidateDistinct(
        IEnumerable<(string Identity, long Value)> members)
    {
        var byValue = new Dictionary<long, string>();
        foreach ((string identity, long value) in members)
        {
            if (value == 0)
            {
                throw new InvalidOperationException(
                    $"zero generated enum discriminant for {identity}");
            }

            if (value < 0)
            {
                throw new InvalidOperationException(
                    $"negative generated enum discriminant for {identity}");
            }

            if (!byValue.TryAdd(value, identity))
            {
                throw new InvalidOperationException(
                    $"duplicate generated enum discriminant {value} for "
                    + $"{byValue[value]} and {identity}");
            }
        }
    }

    private static Discriminant Compute(EnumIdentity identity)
    {
        byte[] input = Encode(identity);
        byte[] digest = SHA256.HashData(input);
        ulong unsigned = BinaryPrimitives.ReadUInt64BigEndian(digest);
        long value = checked((long)(unsigned & 0x7fff_ffff_ffff_ffffUL));
        return new Discriminant(
            Convert.ToHexString(digest).ToLowerInvariant(),
            value);
    }

    private static byte[] Encode(EnumIdentity identity)
    {
        ArgumentNullException.ThrowIfNull(identity);
        using var bytes = new MemoryStream();
        WriteField(
            bytes,
            0x00,
            "baml-csharp-enum-discriminant-v1");
        WriteCount(bytes, 0x10, identity.PackageSegments.Count);
        foreach (string segment in identity.PackageSegments)
        {
            WriteField(bytes, 0x11, segment);
        }

        WriteCount(bytes, 0x20, identity.NamespaceSegments.Count);
        foreach (string segment in identity.NamespaceSegments)
        {
            WriteField(bytes, 0x21, segment);
        }

        WriteField(bytes, 0x30, identity.EnumSymbol);
        WriteField(bytes, 0x31, identity.VariantSymbol);
        return bytes.ToArray();
    }

    private static void WriteField(
        Stream destination,
        byte tag,
        string value)
    {
        ArgumentException.ThrowIfNullOrEmpty(value);
        byte[] encoded = StrictUtf8.GetBytes(value);
        destination.WriteByte(tag);
        WriteUInt32(destination, checked((uint)encoded.Length));
        destination.Write(encoded);
    }

    private static void WriteCount(
        Stream destination,
        byte tag,
        int count)
    {
        if (count < 0)
        {
            throw new ArgumentOutOfRangeException(nameof(count));
        }

        destination.WriteByte(tag);
        WriteUInt32(destination, checked((uint)count));
    }

    private static void WriteUInt32(Stream destination, uint value)
    {
        Span<byte> encoded = stackalloc byte[sizeof(uint)];
        BinaryPrimitives.WriteUInt32BigEndian(encoded, value);
        destination.Write(encoded);
    }

    private static string IdentityKey(EnumIdentity identity) =>
        string.Join(
            "|",
            string.Join("/", identity.PackageSegments),
            string.Join("/", identity.NamespaceSegments),
            identity.EnumSymbol,
            identity.VariantSymbol);

    private static void Expect<TException>(
        Action action,
        string expectedMessage)
        where TException : Exception
    {
        try
        {
            action();
            throw new InvalidOperationException(
                $"expected {typeof(TException).Name}");
        }
        catch (TException exception)
        {
            Require(
                exception.Message.Contains(
                    expectedMessage,
                    StringComparison.Ordinal),
                $"unexpected {typeof(TException).Name}: {exception.Message}");
        }
    }

    private static void Require(bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidOperationException(message);
        }
    }

    private sealed record EnumIdentity(
        IReadOnlyList<string> PackageSegments,
        IReadOnlyList<string> NamespaceSegments,
        string EnumSymbol,
        string VariantSymbol,
        string ExpectedSha256,
        long ExpectedValue);

    private readonly record struct Discriminant(string Sha256, long Value);
}
