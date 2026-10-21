using CsharpGenerics;

internal static class StaticNullInference
{
    public static void Main() => Box<long>.New(null);
}
