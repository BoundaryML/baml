namespace CSharpParity;

using System;
using System.Collections.Generic;
using BamlSdk.Maps;

internal static class RoundTripMaps
{
    internal static void Run()
    {
        var simple = Functions.RoundTripSimpleMap(new()
        {
            ["one"] = 1,
            ["two"] = 2,
        });
        AssertEqual(1L, simple["one"]);
        AssertEqual(2L, simple["two"]);

        var literalKeyed = Functions.RoundTripLiteralKeyedMap(new()
        {
            ["draft"] = 3,
            ["published"] = 5,
        });
        AssertEqual(3L, literalKeyed["draft"]);
        AssertEqual(5L, literalKeyed["published"]);
    }

    private static void AssertEqual<T>(T expected, T actual)
    {
        if (!EqualityComparer<T>.Default.Equals(expected, actual))
        {
            throw new InvalidOperationException($"Expected {expected}, received {actual}.");
        }
    }
}
