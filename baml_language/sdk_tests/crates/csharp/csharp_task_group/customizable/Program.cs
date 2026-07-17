using Baml;
using Functions = BamlSdk.Functions;

var original = Functions.NewTaskGroup(2, "csharp-group");
using var group = original.Clone();
original.Dispose();

if (group.Limit() != 2
    || await group.NameAsync() != "csharp-group"
    || group.ActiveCount() != 0
    || await group.QueuedCountAsync() != 0)
{
    throw new InvalidOperationException("A new BAML task group exposed invalid state.");
}

try
{
    _ = original.Limit();
    throw new InvalidOperationException("A disposed BAML task group remained usable.");
}
catch (ObjectDisposedException)
{
}

using var roundTripped = Functions.RoundTripTaskGroup(group);
await roundTripped.SetLimitAsync(5);
if (group.Limit() != 5
    || roundTripped.Cancel() != 0
    || await group.CancelAsync(pending: false, active: true) != 0)
{
    throw new InvalidOperationException("BAML task group clones did not share state.");
}

Console.WriteLine("C# BAML task-group integration passed.");
