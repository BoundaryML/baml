namespace CSharpParity;

using System;
using System.Collections.Generic;
using System.Linq;
using AliasFunctions = BamlSdk.Aliases.Functions;
using AliasEdgeFunctions = BamlSdk.GoCodegen.AliasEdges.Functions;
using AliasState = BamlSdk.GoCodegen.EnumEdges.ResponseState;
using LeftFunctions = BamlSdk.GoCodegen.Left.Functions;
using RightFunctions = BamlSdk.GoCodegen.Right.Functions;

internal static class RoundTripAliases
{
    internal static void Run()
    {
        AssertSequenceEqual(
            new[] { "a", "b" },
            AliasFunctions.RoundTripStringList(new() { "a", "b" }));
        AssertEqual("alias-chain", AliasEdgeFunctions.RoundTripText("alias-chain"));

        var states = AliasEdgeFunctions.RoundTripStatesByKey(new()
        {
            ["first"] = AliasState.PendingReview,
            ["second"] = AliasState.Accepted,
        });
        AssertEqual(AliasState.PendingReview, states["first"]);
        AssertEqual(AliasState.Accepted, states["second"]);

        AssertEqual("left", LeftFunctions.Echo("left"));
        AssertEqual("right", RightFunctions.Echo("right"));
    }

    private static void AssertEqual<T>(T expected, T actual)
    {
        if (!EqualityComparer<T>.Default.Equals(expected, actual))
        {
            throw new InvalidOperationException($"Expected {expected}, received {actual}.");
        }
    }

    private static void AssertSequenceEqual<T>(IEnumerable<T> expected, IEnumerable<T> actual)
    {
        if (!expected.SequenceEqual(actual))
        {
            throw new InvalidOperationException(
                $"Expected [{string.Join(", ", expected)}], received [{string.Join(", ", actual)}].");
        }
    }
}
