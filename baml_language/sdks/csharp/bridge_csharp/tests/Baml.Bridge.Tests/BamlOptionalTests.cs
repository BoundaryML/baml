using Baml;

namespace Baml.Bridge.Tests;

public sealed class BamlOptionalTests
{
    [Fact]
    public void ZeroStateIsUnset()
    {
        Assert.False(default(BamlOptional<string>).IsSet);
        Assert.False(new BamlOptional<string>().IsSet);
        Assert.False(BamlOptional<string>.Unset.IsSet);
    }

    [Fact]
    public void SetDefaultValuesRemainSet()
    {
        BamlOptional<string?> nullValue = BamlOptional<string?>.FromValue(null);
        BamlOptional<long> zero = 0;
        BamlOptional<bool> falseValue = false;

        Assert.True(nullValue.IsSet);
        Assert.Null(nullValue.Value);
        Assert.True(zero.IsSet);
        Assert.Equal(0L, zero.Value);
        Assert.True(falseValue.IsSet);
        Assert.False(falseValue.Value);
    }

    [Fact]
    public void StateParticipatesInEqualityHashAndText()
    {
        var unset = BamlOptional<string?>.Unset;
        var setNull = BamlOptional<string?>.FromValue(null);

        Assert.NotEqual(unset, setNull);
        Assert.Equal(0, unset.GetHashCode());
        Assert.Equal("<unset>", unset.ToString());
        Assert.Equal("<null>", setNull.ToString());
        Assert.Throws<InvalidOperationException>(() => unset.Value);
    }
}
