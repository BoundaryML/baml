using Baml;

namespace Baml.Bridge.Tests;

public sealed class BamlUnionTests
{
    [Fact]
    public void DefaultHasNoActiveCase()
    {
        var value = default(BamlUnion<string, long>);

        Assert.False(value.IsT0);
        Assert.False(value.IsT1);
        Assert.Throws<InvalidOperationException>(() => value.AsT0);
        Assert.Throws<InvalidOperationException>(() => value.AsT1);
        Assert.Throws<InvalidOperationException>(() => value.Match(static text => text, static number => number.ToString()));
        Assert.Throws<InvalidOperationException>(() => value.Switch(static _ => { }, static _ => { }));
    }

    [Fact]
    public void FactoriesConversionsAndMatchingPreserveTheActiveCase()
    {
        BamlUnion<string, long> text = "value";
        BamlUnion<string, long> number = 42L;

        Assert.True(text.IsT0);
        Assert.Equal("value", text.AsT0);
        Assert.True(number.IsT1);
        Assert.Equal(42, number.AsT1);
        Assert.Equal("text:value", text.Match(static value => $"text:{value}", static value => $"number:{value}"));
        Assert.Equal("number:42", number.Match(static value => $"text:{value}", static value => $"number:{value}"));
        Assert.Equal(text, BamlUnion<string, long>.FromT0("value"));
        Assert.Equal(number, BamlUnion<string, long>.FromT1(42));
    }

    [Fact]
    public void EqualityIncludesTheActiveCase()
    {
        var first = BamlUnion<string, string>.FromT0("same");
        var second = BamlUnion<string, string>.FromT1("same");

        Assert.NotEqual(first, second);
        Assert.Equal(default, default(BamlUnion<string, string>));
        Assert.Equal(0, default(BamlUnion<string, string>).GetHashCode());
    }

    [Fact]
    public void AritySixteenSupportsTheLastCaseAndExhaustiveMatch()
    {
        var value = BamlUnion<
            long, long, long, long, long, long, long, long,
            long, long, long, long, long, long, long, long>.FromT15(16);

        Assert.True(value.IsT15);
        Assert.Equal(16, value.AsT15);
        Assert.Equal(
            16,
            value.Match(
                static item => item,
                static item => item,
                static item => item,
                static item => item,
                static item => item,
                static item => item,
                static item => item,
                static item => item,
                static item => item,
                static item => item,
                static item => item,
                static item => item,
                static item => item,
                static item => item,
                static item => item,
                static item => item));
    }

    [Fact]
    public void ArityThirtyTwoSupportsTheLastCaseAndInvalidDefault()
    {
        var value = BamlUnion<
            long, long, long, long, long, long, long, long,
            long, long, long, long, long, long, long, long,
            long, long, long, long, long, long, long, long,
            long, long, long, long, long, long, long, long>.FromT31(32);
        var same = BamlUnion<
            long, long, long, long, long, long, long, long,
            long, long, long, long, long, long, long, long,
            long, long, long, long, long, long, long, long,
            long, long, long, long, long, long, long, long>.FromT31(32);

        Assert.True(value.IsT31);
        Assert.Equal(32, value.AsT31);
        Assert.Equal(value, same);
        Assert.Throws<InvalidOperationException>(() => default(BamlUnion<
            long, long, long, long, long, long, long, long,
            long, long, long, long, long, long, long, long,
            long, long, long, long, long, long, long, long,
            long, long, long, long, long, long, long, long>).AsT31);
    }
}
