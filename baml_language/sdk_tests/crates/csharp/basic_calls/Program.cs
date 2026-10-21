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

invocation_options.four_call_forms();
await invocation_options.four_call_forms_async();
invocation_options.omitted_argument_is_not_null();
invocation_options.explicit_controls_are_applied();
await invocation_options.explicit_controls_are_applied_async();
Console.WriteLine("csharp_basic_calls=ok");
