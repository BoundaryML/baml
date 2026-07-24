using CsharpSlice;

const string Text = "héllo\0雪";

string synchronous = Functions.PrimitiveSlice(
    flag: true,
    count: 42,
    ratio: 1.25,
    text: Text,
    nullable: null);
if (synchronous != Text)
{
    throw new InvalidOperationException("packaged synchronous primitive result changed");
}

string asynchronous = await Functions.PrimitiveSliceAsync(
    flag: false,
    count: -17,
    ratio: -2.5,
    text: Text,
    nullable: "present");
if (asynchronous != Text)
{
    throw new InvalidOperationException("packaged asynchronous primitive result changed");
}

Console.WriteLine("csharp_primitive_package=ok");
