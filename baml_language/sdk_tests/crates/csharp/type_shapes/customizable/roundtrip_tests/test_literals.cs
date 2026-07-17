namespace CSharpParity;

using System;
using System.Collections.Generic;
using BamlSdk.Literals;

internal static class RoundTripLiterals
{
    internal static void Run()
    {
        AssertEqual(42L, Functions.ReturnLiteral42());
        AssertEqual(-1L, Functions.ReturnLiteralNegOne());
        AssertEqual("draft", Functions.ReturnLiteralDraft());
        AssertEqual("has \"quotes\"", Functions.ReturnLiteralEscaped());
        AssertEqual(true, Functions.ReturnLiteralTrue());
        AssertEqual(false, Functions.ReturnLiteralFalse());

        AssertEqual("draft", Functions.RoundTripStatus("draft"));
        AssertEqual("published", Functions.RoundTripStatus("published"));
        AssertEqual(1L, Functions.RoundTripPriority(1));
        AssertEqual(2L, Functions.RoundTripPriority(2));
    }

    private static void AssertEqual<T>(T expected, T actual)
    {
        if (!EqualityComparer<T>.Default.Equals(expected, actual))
        {
            throw new InvalidOperationException($"Expected {expected}, received {actual}.");
        }
    }
}
