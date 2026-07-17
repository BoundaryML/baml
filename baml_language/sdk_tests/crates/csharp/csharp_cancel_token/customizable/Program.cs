using Baml;
using Functions = BamlSdk.Functions;

var original = Functions.NewCancelToken();
using var token = original.Clone();
original.Dispose();

if (await token.IsCancelledAsync())
{
    throw new InvalidOperationException("A new BAML cancel token was already canceled.");
}

try
{
    _ = original.IsCancelled();
    throw new InvalidOperationException("A disposed BAML cancel token remained usable.");
}
catch (ObjectDisposedException)
{
}

using var roundTripped = Functions.RoundTripCancelToken(token);
if (roundTripped.Cancel() != 1
    || token.Cancel() != 0
    || !token.IsCancelled()
    || !await roundTripped.IsCancelledAsync())
{
    throw new InvalidOperationException("BAML cancel token clones did not share one-shot state.");
}

using var left = Functions.NewCancelToken();
using var right = Functions.NewCancelToken();
using var combined = Functions.AnyCancelToken([left, right]);
if (combined.IsCancelled() || right.Cancel() != 1 || !combined.IsCancelled())
{
    throw new InvalidOperationException("BAML CancelToken.any did not observe an input token.");
}

try
{
    _ = await Functions.CancelSpawnAndPropagateAsync();
    throw new InvalidOperationException("An engine-originated cancellation unexpectedly returned.");
}
catch (BamlCancelledException error)
{
    if (error.ClassName != "baml.panics.Cancelled"
        || error.CancellationToken.CanBeCanceled
        || error.Value is not IReadOnlyDictionary<string, object?>)
    {
        throw new InvalidOperationException("Engine cancellation lost its BAML panic metadata.", error);
    }
}

Console.WriteLine("C# BAML cancel-token integration passed.");
