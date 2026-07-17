using Baml;

namespace Baml.Bridge.Tests;

public sealed class BamlNullableTests
{
    [Fact]
    public void ZeroStateIsNull()
    {
        Assert.True(default(BamlNullable<long>).IsNull);
        Assert.True(new BamlNullable<long>().IsNull);
        Assert.True(BamlNullable<long>.Null.IsNull);
        Assert.True(BamlNullable.Null<long>().IsNull);
    }

    [Fact]
    public void ValueTypeDefaultsRemainValues()
    {
        BamlNullable<long> zero = 0;
        BamlNullable<bool> falseValue = false;

        Assert.False(zero.IsNull);
        Assert.Equal(0L, zero.Value);
        Assert.False(falseValue.IsNull);
        Assert.False(falseValue.Value);
    }

    [Fact]
    public void ReferenceNullMapsToNullCase()
    {
        var value = BamlNullable<string?>.FromValue(null);

        Assert.True(value.IsNull);
        Assert.False(value.TryGetValue(out _));
        Assert.Equal("null", value.Match(() => "null", _ => "value"));
        Assert.Throws<InvalidOperationException>(() => value.Value);
    }
}
