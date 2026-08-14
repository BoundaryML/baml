using Baml;

internal static class Program
{
    private static readonly TimeSpan Timeout = TimeSpan.FromSeconds(10);

    private static async Task<int> Main()
    {
        VerifyPublicShape();
        await VerifyColdPullAndNaturalCompletionAsync().ConfigureAwait(false);
        await VerifyFinalOnlyAndWaitCancellationAsync().ConfigureAwait(false);
        await VerifyPreCanceledWaitDoesNotStartAsync().ConfigureAwait(false);
        await VerifyFactoryAndEnumeratorCancellationAsync().ConfigureAwait(false);
        await VerifyConcurrentMoveAndEarlyDisposeAsync().ConfigureAwait(false);
        await VerifyDisposeBeforeStartAsync().ConfigureAwait(false);
        await VerifyFailureIsSharedAsync().ConfigureAwait(false);
        await VerifyContinuationsDoNotRunInlineAsync().ConfigureAwait(false);
        Console.WriteLine("managed_stream_lifecycle=ok");
        return 0;
    }

    private static void VerifyPublicShape()
    {
        Type streamType = typeof(BamlStream<int, string>);
        Require(streamType.IsSealed, "BamlStream must remain sealed");
        Require(streamType.GetConstructors().Length == 0, "BamlStream exposed public construction");
        Require(
            typeof(IAsyncEnumerable<int>).IsAssignableFrom(streamType),
            "BamlStream lost IAsyncEnumerable");
        Require(
            typeof(IAsyncDisposable).IsAssignableFrom(streamType),
            "BamlStream lost IAsyncDisposable");
        Require(
            streamType.GetMethod(nameof(BamlStream<int, string>.GetFinalResponseAsync))?.ReturnType
                == typeof(Task<string>),
            "BamlStream final response is not Task<TFinal>");
    }

    private static async Task VerifyColdPullAndNaturalCompletionAsync()
    {
        var driver = new ScriptedDriver<int, string>("final");
        driver.EnqueuePartial(10);
        driver.EnqueuePartial(20);
        driver.EnqueueFinished();
        int factoryCalls = 0;
        BamlStream<int, string> stream = BamlStreamFactory.Create<int, string>(
            () =>
            {
                Interlocked.Increment(ref factoryCalls);
                return driver;
            },
            "fixture.cold");

        Require(factoryCalls == 0 && driver.StartCount == 0, "stream factory was not cold");
        IAsyncEnumerator<int> enumerator = stream.GetAsyncEnumerator();
        Require(factoryCalls == 0 && driver.StartCount == 0, "enumerator acquisition started the stream");

        Task<string> finalBeforePull = stream.GetFinalResponseAsync();
        Task<string> secondFinalWaiter = stream.GetFinalResponseAsync();
        Require(
            ReferenceEquals(finalBeforePull, secondFinalWaiter),
            "default final waiters did not share one task");
        await WaitUntilAsync(() => driver.StartCount == 1).ConfigureAwait(false);
        Require(driver.PullCount == 0, "final attachment pulled a partial");

        Require(await enumerator.MoveNextAsync().ConfigureAwait(false), "first partial was missing");
        Require(enumerator.Current == 10, "first partial changed");
        Require(driver.PullCount == 1, "first demand dispatched more than one pull");
        await Task.Yield();
        Require(driver.PullCount == 1, "an idle consumer caused an unsolicited pull");

        Require(await enumerator.MoveNextAsync().ConfigureAwait(false), "second partial was missing");
        Require(enumerator.Current == 20, "second partial changed");
        Require(driver.PullCount == 2, "second demand did not map to exactly one pull");

        Require(!await enumerator.MoveNextAsync().ConfigureAwait(false), "finished pull produced a partial");
        Require(driver.PullCount == 3, "finished demand did not map to exactly one pull");
        Require(!await enumerator.MoveNextAsync().ConfigureAwait(false), "completed enumerator restarted");
        Require(driver.PullCount == 3, "completed enumerator dispatched another pull");
        Require(await finalBeforePull.WaitAsync(Timeout).ConfigureAwait(false) == "final", "final response changed");
        Require(await secondFinalWaiter.ConfigureAwait(false) == "final", "second final waiter changed");
        Require(driver.FinalCount == 1, "partial mode dispatched final more than once");
        Require(driver.DisposeCount == 1, "natural completion did not release exactly once");

        Expect<InvalidOperationException>(() => stream.GetAsyncEnumerator());
        await enumerator.DisposeAsync().ConfigureAwait(false);
        await stream.DisposeAsync().ConfigureAwait(false);
        Require(driver.DisposeCount == 1, "post-terminal disposal released twice");
    }

    private static async Task VerifyFinalOnlyAndWaitCancellationAsync()
    {
        var finalSource = new TaskCompletionSource<string>();
        var driver = new ScriptedDriver<int, string>(
            cancellationToken => finalSource.Task.WaitAsync(cancellationToken));
        BamlStream<int, string> stream = BamlStreamFactory.Create<int, string>(
            () => driver,
            "fixture.final_only");

        Task<string> first = stream.GetFinalResponseAsync();
        Task<string> second = stream.GetFinalResponseAsync();
        Require(ReferenceEquals(first, second), "final-only waiters did not share the cached task");
        await WaitUntilAsync(() => driver.FinalCount == 1).ConfigureAwait(false);
        Require(driver.StartCount == 1, "final-only mode did not start exactly once");
        Require(driver.PullCount == 0, "final-only mode exposed a partial pull");
        Expect<InvalidOperationException>(() => stream.GetAsyncEnumerator());

        using var waitCancellation = new CancellationTokenSource();
        Task<string> canceledWaiter = stream.GetFinalResponseAsync(waitCancellation.Token);
        waitCancellation.Cancel();
        OperationCanceledException waitError =
            await ExpectAsync<OperationCanceledException>(canceledWaiter).ConfigureAwait(false);
        Require(
            waitError is not BamlOperationCanceledException,
            "a final-wait token became an operation cancellation");
        Require(
            waitError.CancellationToken == waitCancellation.Token,
            "a final-wait cancellation lost its token");
        Require(canceledWaiter.Status == TaskStatus.Canceled, "final waiter was not canceled");
        Require(!first.IsCompleted, "canceling one waiter canceled the shared operation");

        finalSource.SetResult("final-only");
        Require(await first.WaitAsync(Timeout).ConfigureAwait(false) == "final-only", "final-only result changed");
        Require(await second.ConfigureAwait(false) == "final-only", "shared final-only result changed");
        Require(driver.FinalCount == 1, "final-only mode dispatched final more than once");
        Require(driver.DisposeCount == 1, "final-only completion did not release exactly once");
        Require(
            ReferenceEquals(first, stream.GetFinalResponseAsync()),
            "completed final response was not cached");
        await stream.DisposeAsync().ConfigureAwait(false);
        Require(driver.DisposeCount == 1, "final-only disposal released twice");
    }

    private static async Task VerifyPreCanceledWaitDoesNotStartAsync()
    {
        var driver = new ScriptedDriver<int, string>("eventual");
        int factoryCalls = 0;
        BamlStream<int, string> stream = BamlStreamFactory.Create<int, string>(() =>
        {
            Interlocked.Increment(ref factoryCalls);
            return driver;
        });
        using var waitCancellation = new CancellationTokenSource();
        waitCancellation.Cancel();

        Task<string> canceledWait = stream.GetFinalResponseAsync(waitCancellation.Token);
        OperationCanceledException error =
            await ExpectAsync<OperationCanceledException>(canceledWait).ConfigureAwait(false);
        Require(error.CancellationToken == waitCancellation.Token, "pre-canceled wait lost its token");
        Require(error is not BamlOperationCanceledException, "pre-canceled wait changed cancellation domain");
        Require(factoryCalls == 0 && driver.StartCount == 0, "pre-canceled wait started the stream");

        Require(
            await stream.GetFinalResponseAsync().WaitAsync(Timeout).ConfigureAwait(false) == "eventual",
            "pre-canceled wait selected a terminal stream mode");
        Require(factoryCalls == 1 && driver.FinalCount == 1, "later final-only start was not singular");
        await stream.DisposeAsync().ConfigureAwait(false);
    }

    private static async Task VerifyFactoryAndEnumeratorCancellationAsync()
    {
        using (var factoryCancellation = new CancellationTokenSource())
        {
            factoryCancellation.Cancel();
            int factoryCalls = 0;
            BamlStream<int, string> stream = BamlStreamFactory.Create<int, string>(
                () =>
                {
                    Interlocked.Increment(ref factoryCalls);
                    return new ScriptedDriver<int, string>("unreachable");
                },
                "fixture.factory_cancel",
                factoryCancellation.Token);

            IAsyncEnumerator<int> enumerator = stream.GetAsyncEnumerator();
            Task<bool> move = enumerator.MoveNextAsync().AsTask();
            BamlOperationCanceledException moveError =
                await ExpectAsync<BamlOperationCanceledException>(move).ConfigureAwait(false);
            Require(move.Status == TaskStatus.Canceled, "factory cancellation did not cancel MoveNext");
            Require(moveError.Origin == BamlCancellationOrigin.Caller, "factory cancellation origin changed");
            Require(
                moveError.CancellationToken == factoryCancellation.Token,
                "factory cancellation lost the exact token");
            Task<string> final = stream.GetFinalResponseAsync();
            BamlOperationCanceledException finalError =
                await ExpectAsync<BamlOperationCanceledException>(final).ConfigureAwait(false);
            Require(final.Status == TaskStatus.Canceled, "factory cancellation did not cancel final task");
            Require(ReferenceEquals(moveError, finalError), "factory cancellation outcome was not shared");
            Require(factoryCalls == 0, "pre-canceled factory created a driver");
            await stream.DisposeAsync().ConfigureAwait(false);
            await enumerator.DisposeAsync().ConfigureAwait(false);
        }

        using var factorySource = new CancellationTokenSource();
        using var enumeratorSource = new CancellationTokenSource();
        var pullStarted = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var driverDuringCancellation = new ScriptedDriver<int, string>("unreachable");
        driverDuringCancellation.EnqueuePull(async cancellationToken =>
        {
            pullStarted.TrySetResult();
            await Task.Delay(System.Threading.Timeout.InfiniteTimeSpan, cancellationToken).ConfigureAwait(false);
            return BamlStreamPull<int>.Finished;
        });
        BamlStream<int, string> runningStream = BamlStreamFactory.Create<int, string>(
            () => driverDuringCancellation,
            "fixture.enumerator_cancel",
            factorySource.Token);
        IAsyncEnumerator<int> runningEnumerator =
            runningStream.GetAsyncEnumerator(enumeratorSource.Token);
        Task<bool> runningMove = runningEnumerator.MoveNextAsync().AsTask();
        await pullStarted.Task.WaitAsync(Timeout).ConfigureAwait(false);

        enumeratorSource.Cancel();
        factorySource.Cancel();
        BamlOperationCanceledException enumeratorError =
            await ExpectAsync<BamlOperationCanceledException>(runningMove).ConfigureAwait(false);
        Require(
            enumeratorError.CancellationToken == enumeratorSource.Token,
            "the first accepted source cancellation did not win");
        Require(enumeratorError.Origin == BamlCancellationOrigin.Caller, "enumerator origin changed");
        BamlOperationCanceledException cachedError =
            await ExpectAsync<BamlOperationCanceledException>(
                    runningStream.GetFinalResponseAsync())
                .ConfigureAwait(false);
        Require(ReferenceEquals(enumeratorError, cachedError), "enumerator cancellation was not cached");
        Require(driverDuringCancellation.DisposeCount == 1, "canceled driver was not released once");
        await runningEnumerator.DisposeAsync().ConfigureAwait(false);
        await runningStream.DisposeAsync().ConfigureAwait(false);
        Require(driverDuringCancellation.DisposeCount == 1, "canceled driver was released twice");
    }

    private static async Task VerifyConcurrentMoveAndEarlyDisposeAsync()
    {
        var pullStarted = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var driver = new ScriptedDriver<int, string>("unreachable");
        driver.EnqueuePull(async cancellationToken =>
        {
            pullStarted.TrySetResult();
            await Task.Delay(System.Threading.Timeout.InfiniteTimeSpan, cancellationToken).ConfigureAwait(false);
            return BamlStreamPull<int>.Finished;
        });
        BamlStream<int, string> stream = BamlStreamFactory.Create<int, string>(
            () => driver,
            "fixture.dispose");
        IAsyncEnumerator<int> enumerator = stream.GetAsyncEnumerator();
        Task<string> final = stream.GetFinalResponseAsync();
        Task<bool> firstMove = enumerator.MoveNextAsync().AsTask();
        await pullStarted.Task.WaitAsync(Timeout).ConfigureAwait(false);

        Task<bool> concurrentMove = enumerator.MoveNextAsync().AsTask();
        await ExpectAsync<InvalidOperationException>(concurrentMove).ConfigureAwait(false);
        Require(driver.PullCount == 1, "concurrent MoveNext dispatched another pull");

        await stream.DisposeAsync().ConfigureAwait(false);
        BamlOperationCanceledException moveError =
            await ExpectAsync<BamlOperationCanceledException>(firstMove).ConfigureAwait(false);
        BamlOperationCanceledException finalError =
            await ExpectAsync<BamlOperationCanceledException>(final).ConfigureAwait(false);
        Require(moveError.Origin == BamlCancellationOrigin.StreamDisposed, "early dispose origin changed");
        Require(moveError.CancellationToken.IsCancellationRequested, "dispose token was not canceled");
        Require(ReferenceEquals(moveError, finalError), "dispose cancellation was not shared");
        Require(final.Status == TaskStatus.Canceled, "disposed final task was not canceled");
        Require(driver.DisposeCount == 1, "early disposal did not release exactly once");

        Require(
            ReferenceEquals(final, stream.GetFinalResponseAsync()),
            "disposed stream did not retain its final task");
        await stream.DisposeAsync().ConfigureAwait(false);
        await enumerator.DisposeAsync().ConfigureAwait(false);
        Require(driver.DisposeCount == 1, "idempotent disposal released twice");
    }

    private static async Task VerifyDisposeBeforeStartAsync()
    {
        int factoryCalls = 0;
        BamlStream<int, string> stream = BamlStreamFactory.Create<int, string>(() =>
        {
            Interlocked.Increment(ref factoryCalls);
            return new ScriptedDriver<int, string>("unreachable");
        });

        await stream.DisposeAsync().ConfigureAwait(false);
        Require(factoryCalls == 0, "dispose-before-start created a driver");
        Task<string> final = stream.GetFinalResponseAsync();
        BamlOperationCanceledException error =
            await ExpectAsync<BamlOperationCanceledException>(final).ConfigureAwait(false);
        Require(error.Origin == BamlCancellationOrigin.StreamDisposed, "dispose-before-start origin changed");
        Require(final.Status == TaskStatus.Canceled, "dispose-before-start final was not canceled");
        Expect<ObjectDisposedException>(() => stream.GetAsyncEnumerator());
        await stream.DisposeAsync().ConfigureAwait(false);
    }

    private static async Task VerifyFailureIsSharedAsync()
    {
        var failure = new InvalidDataException("partial decode failed");
        var driver = new ScriptedDriver<int, string>("unreachable");
        driver.EnqueuePull(_ => Task.FromException<BamlStreamPull<int>>(failure));
        BamlStream<int, string> stream = BamlStreamFactory.Create<int, string>(() => driver);
        IAsyncEnumerator<int> enumerator = stream.GetAsyncEnumerator();
        Task<string> final = stream.GetFinalResponseAsync();

        InvalidDataException moveError =
            await ExpectAsync<InvalidDataException>(enumerator.MoveNextAsync().AsTask())
                .ConfigureAwait(false);
        InvalidDataException finalError =
            await ExpectAsync<InvalidDataException>(final).ConfigureAwait(false);
        Require(ReferenceEquals(failure, moveError), "partial failure instance changed");
        Require(ReferenceEquals(moveError, finalError), "partial failure was not the shared terminal outcome");
        Require(final.Status == TaskStatus.Faulted, "partial failure did not fault the final task");
        Require(driver.DisposeCount == 1, "failed stream did not release exactly once");
        await stream.DisposeAsync().ConfigureAwait(false);
        await enumerator.DisposeAsync().ConfigureAwait(false);
    }

    private static async Task VerifyContinuationsDoNotRunInlineAsync()
    {
        var finalSource = new TaskCompletionSource<string>();
        var driver = new ScriptedDriver<int, string>(_ => finalSource.Task);
        BamlStream<int, string> stream = BamlStreamFactory.Create<int, string>(() => driver);
        Task<string> final = stream.GetFinalResponseAsync();
        await WaitUntilAsync(() => driver.FinalCount == 1).ConfigureAwait(false);

        bool ranOnCompletingStack = false;
        Task continuation = final.ContinueWith(
            _ => ranOnCompletingStack = CompletionThreadMarker.IsCompleting,
            CancellationToken.None,
            TaskContinuationOptions.ExecuteSynchronously,
            TaskScheduler.Default);
        CompletionThreadMarker.IsCompleting = true;
        finalSource.SetResult("done");
        CompletionThreadMarker.IsCompleting = false;

        Require(await final.WaitAsync(Timeout).ConfigureAwait(false) == "done", "async final changed");
        await continuation.WaitAsync(Timeout).ConfigureAwait(false);
        Require(!ranOnCompletingStack, "a public continuation ran inline on the driver completion stack");
        await stream.DisposeAsync().ConfigureAwait(false);
    }

    private static async Task<TException> ExpectAsync<TException>(Task task)
        where TException : Exception
    {
        try
        {
            await task.WaitAsync(Timeout).ConfigureAwait(false);
        }
        catch (TException error)
        {
            return error;
        }

        throw new InvalidOperationException($"expected {typeof(TException).Name}");
    }

    private static void Expect<TException>(Action action)
        where TException : Exception
    {
        try
        {
            action();
        }
        catch (TException)
        {
            return;
        }

        throw new InvalidOperationException($"expected {typeof(TException).Name}");
    }

    private static async Task WaitUntilAsync(Func<bool> condition)
    {
        using var timeout = new CancellationTokenSource(Timeout);
        while (!condition())
        {
            await Task.Delay(1, timeout.Token).ConfigureAwait(false);
        }
    }

    private static void Require(bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidOperationException(message);
        }
    }

    private static class CompletionThreadMarker
    {
        [ThreadStatic]
        internal static bool IsCompleting;
    }

    private sealed class ScriptedDriver<TPartial, TFinal>
        : IBamlStreamDriver<TPartial, TFinal>
    {
        private readonly object gate = new();
        private readonly Queue<Func<CancellationToken, Task<BamlStreamPull<TPartial>>>> pulls = [];
        private readonly Func<CancellationToken, Task<TFinal>> final;
        private int startCount;
        private int pullCount;
        private int finalCount;
        private int disposeCount;

        internal ScriptedDriver(TFinal final) : this(_ => Task.FromResult(final))
        {
        }

        internal ScriptedDriver(Func<CancellationToken, Task<TFinal>> final)
        {
            this.final = final;
        }

        internal int StartCount => Volatile.Read(ref startCount);

        internal int PullCount => Volatile.Read(ref pullCount);

        internal int FinalCount => Volatile.Read(ref finalCount);

        internal int DisposeCount => Volatile.Read(ref disposeCount);

        internal void EnqueuePartial(TPartial partial) =>
            EnqueuePull(_ => Task.FromResult(BamlStreamPull<TPartial>.FromPartial(partial)));

        internal void EnqueueFinished() =>
            EnqueuePull(_ => Task.FromResult(BamlStreamPull<TPartial>.Finished));

        internal void EnqueuePull(
            Func<CancellationToken, Task<BamlStreamPull<TPartial>>> pull)
        {
            lock (gate)
            {
                pulls.Enqueue(pull);
            }
        }

        public Task StartAsync(CancellationToken cancellationToken)
        {
            Interlocked.Increment(ref startCount);
            return Task.CompletedTask;
        }

        public Task<BamlStreamPull<TPartial>> PullAsync(
            CancellationToken cancellationToken)
        {
            Interlocked.Increment(ref pullCount);
            Func<CancellationToken, Task<BamlStreamPull<TPartial>>> pull;
            lock (gate)
            {
                pull = pulls.Count == 0
                    ? throw new InvalidOperationException("unexpected pull")
                    : pulls.Dequeue();
            }

            return pull(cancellationToken);
        }

        public Task<TFinal> GetFinalResponseAsync(CancellationToken cancellationToken)
        {
            Interlocked.Increment(ref finalCount);
            return final(cancellationToken);
        }

        public ValueTask DisposeAsync()
        {
            Interlocked.Increment(ref disposeCount);
            return ValueTask.CompletedTask;
        }
    }
}
