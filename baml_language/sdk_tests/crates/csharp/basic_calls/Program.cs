using CsharpBasicCalls;

const string Text = "héllo\0雪";

string synchronous = Functions.BasicCalls(
    flag: true,
    count: 42,
    ratio: 1.25,
    text: Text,
    nullable: null);
if (synchronous != Text)
{
    throw new InvalidOperationException("synchronous primitive result changed");
}

string asynchronous = await Functions.BasicCallsAsync(
    flag: false,
    count: -17,
    ratio: -2.5,
    text: Text,
    nullable: "present");
if (asynchronous != Text)
{
    throw new InvalidOperationException("asynchronous primitive result changed");
}

Console.WriteLine("csharp_basic_calls=ok");
