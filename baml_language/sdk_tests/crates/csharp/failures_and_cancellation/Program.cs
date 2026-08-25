using System.Diagnostics;
using System.Reflection;

using Baml;
using CsharpFailuresAndCancellation;

const string ExitChildArgument = "--exit-child";
const string SensitiveMarker = "sensitive";

if (args.Length == 2 && args[0] == ExitChildArgument)
{
    long exitCode = long.Parse(
        args[1],
        System.Globalization.CultureInfo.InvariantCulture);
    Console.WriteLine("failures_and_cancellation_exit_before");
    Console.Out.Flush();
    try
    {
        _ = Functions.DoExit(exitCode);
    }
    finally
    {
        Console.WriteLine("failures_and_cancellation_exit_unreachable_finally");
    }

    return 99;
}

BamlErrorException error = Expect<BamlErrorException>(() => _ = Functions.ThrowFailure());
Require(
    error.ErrorName == "user.csharp_failures_and_cancellation.Failure"
        && error.BamlFunction == "user.csharp_failures_and_cancellation.throw_failure"
        && error.ThrownValue is not null
        && error.Trace.Lines.Count != 0,
    "native typed error metadata changed");
RequireRedacted(error);

BamlTypeMismatchException mismatch =
    Expect<BamlTypeMismatchException>(() => _ = Functions.ThrowTypeMismatch());
Require(
    mismatch.ErrorName == "baml.errors.TypeMismatch"
        && mismatch.BamlFunction == "user.csharp_failures_and_cancellation.throw_type_mismatch",
    "native type mismatch classification changed");
RequireRedacted(mismatch);

BamlPanicException panic = Expect<BamlPanicException>(() =>
    _ = Functions.DoPanic("sensitive panic body"));
Require(
    panic.BamlFunction == "user.csharp_failures_and_cancellation.do_panic"
        && panic.Panic.Value is not null
        && !panic.Panic.IsExitPanic
        && panic.Panic.ExitCode is null
        && panic.Trace.Lines.Count != 0,
    "native panic metadata changed");
RequireRedacted(panic);

using (var caller = new CancellationTokenSource())
{
    Task<long> canceled = Functions.SleepMsAsync(10_000L, caller.Token);
    caller.CancelAfter(TimeSpan.FromMilliseconds(50));
    BamlOperationCanceledException cancellation = await ExpectCanceled(canceled);
    Require(
        cancellation.Origin == BamlCancellationOrigin.Caller
            && cancellation.BamlFunction == "user.csharp_failures_and_cancellation.sleep_ms"
            && cancellation.CancellationToken == caller.Token
            && cancellation.Trace is null,
        "native caller cancellation metadata changed");
}


using (var caller = new CancellationTokenSource(TimeSpan.FromMilliseconds(50)))
{
    BamlOperationCanceledException cancellation =
        Expect<BamlOperationCanceledException>(() =>
            _ = Functions.SleepMs(10_000L, caller.Token));
    Require(
        cancellation.Origin == BamlCancellationOrigin.Caller
            && cancellation.CancellationToken == caller.Token,
        "sync caller cancellation metadata changed");
}

using (var uncanceledCaller = new CancellationTokenSource())
{
    Task<long> canceled = Functions.EngineCancelAsync(uncanceledCaller.Token);
    BamlOperationCanceledException cancellation = await ExpectCanceled(canceled);
    Require(
        cancellation.Origin == BamlCancellationOrigin.Engine
            && cancellation.BamlFunction == "user.csharp_failures_and_cancellation.engine_cancel"
            && cancellation.CancellationToken.IsCancellationRequested
            && cancellation.CancellationToken != uncanceledCaller.Token
            && cancellation.Trace is not null,
        "native engine cancellation metadata changed");
}

BamlOperationCanceledException syncEngineCancellation =
    Expect<BamlOperationCanceledException>(() => _ = Functions.EngineCancel());
Require(
    syncEngineCancellation.Origin == BamlCancellationOrigin.Engine
        && syncEngineCancellation.CancellationToken.IsCancellationRequested,
    "sync engine cancellation metadata changed");

Require(Functions.Ping() == 42L, "catchable failures poisoned later native calls");

await VerifyHardExitChildAsync(0);
await VerifyHardExitChildAsync(37);

Console.WriteLine("csharp_failures_and_cancellation=ok");
return 0;

static TException Expect<TException>(Action action)
    where TException : Exception
{
    try
    {
        action();
    }
    catch (TException exception)
    {
        return exception;
    }

    throw new InvalidOperationException($"expected {typeof(TException).Name}");
}

static async Task<BamlOperationCanceledException> ExpectCanceled(Task task)
{
    try
    {
        await task;
    }
    catch (BamlOperationCanceledException exception)
    {
        Require(task.Status == TaskStatus.Canceled, "operation task was not Canceled");
        Require(task.Exception is null, "canceled operation retained an AggregateException");
        return exception;
    }

    throw new InvalidOperationException("expected BamlOperationCanceledException");
}

static void RequireRedacted(Exception exception)
{
    Require(
        !exception.Message.Contains(SensitiveMarker, StringComparison.OrdinalIgnoreCase)
            && !exception.ToString().Contains(SensitiveMarker, StringComparison.OrdinalIgnoreCase),
        $"{exception.GetType().Name} leaked structured payload data");
}

static async Task VerifyHardExitChildAsync(int exitCode)
{
    string executable = Environment.ProcessPath
        ?? throw new InvalidOperationException("current process path is unavailable");
    var start = new ProcessStartInfo
    {
        FileName = executable,
        RedirectStandardOutput = true,
        RedirectStandardError = true,
        UseShellExecute = false,
    };
    if (StringComparer.OrdinalIgnoreCase.Equals(
        Path.GetFileNameWithoutExtension(executable),
        "dotnet"))
    {
        start.ArgumentList.Add(
            Assembly.GetEntryAssembly()?.Location
                ?? throw new InvalidOperationException("entry assembly path is unavailable"));
    }

    start.ArgumentList.Add(ExitChildArgument);
    start.ArgumentList.Add(exitCode.ToString(System.Globalization.CultureInfo.InvariantCulture));
    using Process child = Process.Start(start)
        ?? throw new InvalidOperationException("failed to start hard-exit child");
    Task<string> stdout = child.StandardOutput.ReadToEndAsync();
    Task<string> stderr = child.StandardError.ReadToEndAsync();
    using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(10));
    try
    {
        await child.WaitForExitAsync(timeout.Token);
    }
    catch (OperationCanceledException) when (timeout.IsCancellationRequested)
    {
        child.Kill(entireProcessTree: true);
        await child.WaitForExitAsync();
        throw new TimeoutException("hard-exit child exceeded its ten-second bound");
    }
    string output = await stdout;
    string error = await stderr;
    Require(
        child.ExitCode == exitCode,
        $"hard-exit child returned {child.ExitCode}: {output}\n{error}");
    Require(output.Contains("failures_and_cancellation_exit_before", StringComparison.Ordinal),
        "hard-exit child omitted its pre-exit marker");
    Require(!output.Contains("failures_and_cancellation_exit_unreachable_finally", StringComparison.Ordinal),
        "hard-exit child ran finally cleanup");
}

static void Require(bool condition, string message)
{
    if (!condition)
    {
        throw new InvalidOperationException(message);
    }
}
