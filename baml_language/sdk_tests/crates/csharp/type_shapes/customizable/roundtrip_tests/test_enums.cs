namespace CSharpParity;

using System;
using System.Collections.Generic;
using BamlSdk.Enums;

internal static class RoundTripEnums
{
    internal static void Run()
    {
        AssertEqual(Sentiment.Positive, Functions.PickPositive());
        AssertEqual(
            Sentiment.Positive,
            Functions.RoundTripSentimentPositive(Sentiment.Positive));

        var value = new Enums
        {
            BareEnum = Sentiment.Negative,
            VariantAsType = Sentiment.Positive,
        };
        var roundTripped = Functions.RoundTripEnums(value);
        AssertEqual(Sentiment.Negative, roundTripped.BareEnum);
        AssertEqual(Sentiment.Positive, roundTripped.VariantAsType);
    }

    private static void AssertEqual<T>(T expected, T actual)
    {
        if (!EqualityComparer<T>.Default.Equals(expected, actual))
        {
            throw new InvalidOperationException($"Expected {expected}, received {actual}.");
        }
    }
}
