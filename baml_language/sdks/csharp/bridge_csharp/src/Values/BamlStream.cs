using System.Runtime.ExceptionServices;

namespace Baml;

public sealed class BamlStream<T>
    : IAsyncEnumerable<T>, IAsyncDisposable
{
    private readonly BamlStreamController<T> controller;

    internal BamlStream(
        Func<IBamlStreamDriver<T>> driverFactory,
        string? bamlFunction,
        CancellationToken cancellationToken)
    {
        controller = new BamlStreamController<T>(
            driverFactory,
            bamlFunction,
            cancellationToken);
    }

    internal BamlStream(
        IBamlStreamDriver<T> driver,
        string? bamlFunction,
        CancellationToken cancellationToken)
    {
        controller = new BamlStreamController<T>(
            driver,
            bamlFunction,
            cancellationToken);
    }

    public IAsyncEnumerator<T> GetAsyncEnumerator(
        CancellationToken cancellationToken = default) =>
        controller.GetAsyncEnumerator(cancellationToken);

    public Task<T> GetFinalResponseAsync(
        CancellationToken cancellationToken = default) =>
        controller.GetFinalResponseAsync(cancellationToken);

    public ValueTask DisposeAsync() => controller.DisposeAsync();
}

internal static class BamlStreamFactory
{
    internal static BamlStream<T> Create<T>(
        Func<IBamlStreamDriver<T>> driverFactory,
        string? bamlFunction = null,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(driverFactory);
        return new BamlStream<T>(
            driverFactory,
            bamlFunction,
            cancellationToken);
    }

    internal static BamlStream<T> Create<T>(
        IBamlStreamDriver<T> driver,
        string? bamlFunction = null,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(driver);
        return new BamlStream<T>(
            driver,
            bamlFunction,
            cancellationToken);
    }
}

internal interface IBamlStreamDriver<T> : IAsyncDisposable
{
    Task StartAsync(CancellationToken cancellationToken);

    Task<BamlStreamPull<T>> PullAsync(CancellationToken cancellationToken);

    Task<T> GetFinalResponseAsync(CancellationToken cancellationToken);
}

internal readonly struct BamlStreamPull<T>
{
    private readonly T partial;

    private BamlStreamPull(bool hasPartial, T partial)
    {
        HasPartial = hasPartial;
        this.partial = partial;
    }

    internal bool HasPartial { get; }

    internal T Partial => HasPartial
        ? partial
        : throw new InvalidOperationException(
            "A finished stream pull has no partial value.");

    internal static BamlStreamPull<T> FromPartial(T partial) =>
        new(hasPartial: true, partial);

    internal static BamlStreamPull<T> Finished { get; } =
        new(hasPartial: false, default!);
}

internal sealed class BamlStreamController<T>
{
    private readonly object gate = new();
    private readonly SemaphoreSlim driverGate = new(initialCount: 1, maxCount: 1);
    private readonly Func<IBamlStreamDriver<T>> driverFactory;
    private readonly string? bamlFunction;
    private readonly CancellationTokenSource operationCancellation = new();
    private readonly List<CancellationTokenRegistration> operationRegistrations = [];
    private readonly TaskCompletionSource<TerminalOutcome> terminalSignal =
        new(TaskCreationOptions.RunContinuationsAsynchronously);
    private readonly Task<T> finalResponseTask;

    private StreamMode mode;
    private IBamlStreamDriver<T>? driver;
    private bool driverStarted;
    private bool driverDisposed;
    private bool disposed;
    private bool enumeratorClaimed;
    private bool operationActivated;
    private bool partialStartScheduled;
    private bool terminalSelected;
    private bool registrationsClosed;
    private CancellationToken? pendingCallerCancellation;
    private Task terminalFinalization = Task.CompletedTask;

    internal BamlStreamController(
        Func<IBamlStreamDriver<T>> driverFactory,
        string? bamlFunction,
        CancellationToken factoryCancellationToken)
    {
        ArgumentNullException.ThrowIfNull(driverFactory);
        this.driverFactory = driverFactory;
        this.bamlFunction = bamlFunction;
        finalResponseTask = ObserveTerminalAsync(terminalSignal.Task);
        RegisterOperationCancellation(factoryCancellationToken);
    }

    internal BamlStreamController(
        IBamlStreamDriver<T> driver,
        string? bamlFunction,
        CancellationToken factoryCancellationToken)
        : this(
            () => driver,
            bamlFunction,
            factoryCancellationToken)
    {
        ArgumentNullException.ThrowIfNull(driver);
        this.driver = driver;
    }

    internal IAsyncEnumerator<T> GetAsyncEnumerator(
        CancellationToken cancellationToken)
    {
        lock (gate)
        {
            ObjectDisposedException.ThrowIf(disposed, this);
            if (enumeratorClaimed || mode != StreamMode.Created)
            {
                throw new InvalidOperationException(
                    "A BAML stream permits exactly one partial enumerator, and final-only streams cannot be enumerated.");
            }

            enumeratorClaimed = true;
            mode = StreamMode.PartialConsumer;
        }

        RegisterOperationCancellation(cancellationToken);
        return new Enumerator(this);
    }

    internal Task<T> GetFinalResponseAsync(CancellationToken cancellationToken)
    {
        bool startFinalOnly = false;
        bool startPartial = false;

        lock (gate)
        {
            if (cancellationToken.IsCancellationRequested
                && mode == StreamMode.Created
                && !terminalSelected
                && !disposed)
            {
                return Task.FromCanceled<T>(cancellationToken);
            }
        }

        ActivateOperation();

        lock (gate)
        {
            if (!terminalSelected)
            {
                if (mode == StreamMode.Created)
                {
                    mode = StreamMode.FinalOnly;
                    startFinalOnly = true;
                }
                else if (mode == StreamMode.PartialConsumer && !partialStartScheduled)
                {
                    partialStartScheduled = true;
                    startPartial = true;
                }
            }
        }

        if (startFinalOnly)
        {
            _ = RunFinalOnlyAsync();
        }
        else if (startPartial)
        {
            _ = StartPartialOperationAsync();
        }

        return cancellationToken.CanBeCanceled
            ? finalResponseTask.WaitAsync(cancellationToken)
            : finalResponseTask;
    }

    internal ValueTask DisposeAsync()
    {
        ActivateOperation();

        bool cancelOperation = false;
        Task finalization;
        lock (gate)
        {
            disposed = true;
            if (!terminalSelected)
            {
                CancellationToken cancellationToken = CreateCanceledToken();
                var cancellation = new BamlOperationCanceledException(
                    "The BAML stream was canceled because it was disposed.",
                    BamlCancellationOrigin.StreamDisposed,
                    cancellationToken,
                    bamlFunction,
                    trace: null);
                cancelOperation = SelectTerminalLocked(
                    TerminalOutcome.FromException(cancellation));
            }

            finalization = terminalFinalization;
        }

        if (cancelOperation)
        {
            CancelDriverOperation();
        }

        return new ValueTask(finalization);
    }

    private async ValueTask<MoveResult> MoveNextAsync()
    {
        ActivateOperation();

        if (IsTerminalSelected())
        {
            return await ObserveTerminalForMoveAsync().ConfigureAwait(false);
        }

        BamlStreamPull<T> pull;
        try
        {
            pull = await InvokeDriverAsync(
                    static (streamDriver, cancellationToken) =>
                        streamDriver.PullAsync(cancellationToken))
                .ConfigureAwait(false);
        }
        catch (TerminalAlreadySelectedException)
        {
            return await ObserveTerminalForMoveAsync().ConfigureAwait(false);
        }
        catch (Exception error)
        {
            SelectFailure(error);
            return await ObserveTerminalForMoveAsync().ConfigureAwait(false);
        }

        if (pull.HasPartial)
        {
            lock (gate)
            {
                if (!terminalSelected)
                {
                    return MoveResult.FromPartial(pull.Partial);
                }
            }

            return await ObserveTerminalForMoveAsync().ConfigureAwait(false);
        }

        T finalResponse;
        try
        {
            finalResponse = await InvokeDriverAsync(
                    static (streamDriver, cancellationToken) =>
                        streamDriver.GetFinalResponseAsync(cancellationToken))
                .ConfigureAwait(false);
        }
        catch (TerminalAlreadySelectedException)
        {
            return await ObserveTerminalForMoveAsync().ConfigureAwait(false);
        }
        catch (Exception error)
        {
            SelectFailure(error);
            return await ObserveTerminalForMoveAsync().ConfigureAwait(false);
        }

        SelectSuccess(finalResponse);
        return await ObserveTerminalForMoveAsync().ConfigureAwait(false);
    }

    private async Task RunFinalOnlyAsync()
    {
        try
        {
            T finalResponse = await InvokeDriverAsync(
                    static (streamDriver, cancellationToken) =>
                        streamDriver.GetFinalResponseAsync(cancellationToken))
                .ConfigureAwait(false);
            SelectSuccess(finalResponse);
        }
        catch (TerminalAlreadySelectedException)
        {
        }
        catch (Exception error)
        {
            SelectFailure(error);
        }
    }

    private async Task StartPartialOperationAsync()
    {
        try
        {
            await EnsureDriverStartedAsync().ConfigureAwait(false);
        }
        catch (TerminalAlreadySelectedException)
        {
        }
        catch (Exception error)
        {
            SelectFailure(error);
        }
    }

    private async Task<TResult> InvokeDriverAsync<TResult>(
        Func<IBamlStreamDriver<T>, CancellationToken, Task<TResult>> operation)
    {
        await driverGate.WaitAsync().ConfigureAwait(false);
        try
        {
            ThrowIfTerminalSelected();
            IBamlStreamDriver<T> streamDriver =
                await EnsureDriverStartedWhileLockedAsync().ConfigureAwait(false);
            ThrowIfTerminalSelected();
            return await operation(streamDriver, operationCancellation.Token)
                .ConfigureAwait(ConfigureAwaitOptions.ForceYielding);
        }
        finally
        {
            driverGate.Release();
        }
    }

    private async Task EnsureDriverStartedAsync()
    {
        await driverGate.WaitAsync().ConfigureAwait(false);
        try
        {
            ThrowIfTerminalSelected();
            _ = await EnsureDriverStartedWhileLockedAsync().ConfigureAwait(false);
        }
        finally
        {
            driverGate.Release();
        }
    }

    private async Task<IBamlStreamDriver<T>>
        EnsureDriverStartedWhileLockedAsync()
    {
        if (driver is null)
        {
            driver = driverFactory()
                ?? throw new InvalidOperationException(
                    "The BAML stream driver factory returned null.");
        }

        if (!driverStarted)
        {
            await driver.StartAsync(operationCancellation.Token)
                .ConfigureAwait(ConfigureAwaitOptions.ForceYielding);
            driverStarted = true;
        }

        return driver;
    }

    private void ActivateOperation()
    {
        bool cancelOperation = false;
        lock (gate)
        {
            if (operationActivated)
            {
                return;
            }

            operationActivated = true;
            if (pendingCallerCancellation is CancellationToken cancellationToken
                && !terminalSelected)
            {
                cancelOperation = SelectTerminalLocked(
                    TerminalOutcome.FromException(
                        CreateCallerCancellation(cancellationToken)));
            }
        }

        if (cancelOperation)
        {
            CancelDriverOperation();
        }
    }

    private void RegisterOperationCancellation(CancellationToken cancellationToken)
    {
        if (!cancellationToken.CanBeCanceled)
        {
            return;
        }

        CancellationTokenRegistration registration = cancellationToken.UnsafeRegister(
            static (state, token) =>
                ((BamlStreamController<T>)state!).OnCallerCancellation(token),
            this);

        bool disposeRegistration;
        lock (gate)
        {
            disposeRegistration = registrationsClosed;
            if (!disposeRegistration)
            {
                operationRegistrations.Add(registration);
            }
        }

        if (disposeRegistration)
        {
            registration.Dispose();
        }
    }

    private void OnCallerCancellation(CancellationToken cancellationToken)
    {
        bool cancelOperation = false;
        lock (gate)
        {
            if (terminalSelected)
            {
                return;
            }

            if (!operationActivated)
            {
                pendingCallerCancellation ??= cancellationToken;
                return;
            }

            cancelOperation = SelectTerminalLocked(
                TerminalOutcome.FromException(
                    CreateCallerCancellation(cancellationToken)));
        }

        if (cancelOperation)
        {
            CancelDriverOperation();
        }
    }

    private BamlOperationCanceledException CreateCallerCancellation(
        CancellationToken cancellationToken) =>
        new(
            "The BAML stream was canceled by the caller.",
            BamlCancellationOrigin.Caller,
            cancellationToken,
            bamlFunction,
            trace: null);

    private void SelectSuccess(T finalResponse)
    {
        lock (gate)
        {
            _ = SelectTerminalLocked(TerminalOutcome.FromValue(finalResponse));
        }
    }

    private void SelectFailure(Exception error)
    {
        ArgumentNullException.ThrowIfNull(error);
        bool cancelOperation;
        lock (gate)
        {
            cancelOperation = SelectTerminalLocked(
                TerminalOutcome.FromException(error));
        }

        if (cancelOperation)
        {
            CancelDriverOperation();
        }
    }

    private bool SelectTerminalLocked(TerminalOutcome outcome)
    {
        if (terminalSelected)
        {
            return false;
        }

        terminalSelected = true;
        mode = StreamMode.Terminal;
        terminalFinalization = FinalizeTerminalAsync(outcome);
        return outcome.HasException;
    }

    private async Task FinalizeTerminalAsync(TerminalOutcome outcome)
    {
        await Task.Yield();

        await driverGate.WaitAsync().ConfigureAwait(false);
        try
        {
            if (driver is not null && !driverDisposed)
            {
                driverDisposed = true;
                try
                {
                    await driver.DisposeAsync().ConfigureAwait(false);
                }
                catch
                {
                    // A release failure cannot replace the already selected
                    // stream result, error, or cancellation outcome.
                }
            }
        }
        finally
        {
            driverGate.Release();
        }

        CancellationTokenRegistration[] registrations;
        lock (gate)
        {
            registrationsClosed = true;
            registrations = [.. operationRegistrations];
            operationRegistrations.Clear();
        }

        foreach (CancellationTokenRegistration registration in registrations)
        {
            registration.Dispose();
        }

        operationCancellation.Dispose();
        terminalSignal.TrySetResult(outcome);
    }

    private void CancelDriverOperation()
    {
        try
        {
            operationCancellation.Cancel();
        }
        catch (AggregateException)
        {
            // Cancellation callbacks belong to the internal driver. A callback
            // failure cannot replace the stream's already selected terminal state.
        }
        catch (ObjectDisposedException)
        {
            // Terminal cleanup won the race and has already released the driver.
        }
    }

    private bool IsTerminalSelected()
    {
        lock (gate)
        {
            return terminalSelected;
        }
    }

    private void ThrowIfTerminalSelected()
    {
        lock (gate)
        {
            if (terminalSelected)
            {
                throw new TerminalAlreadySelectedException();
            }
        }
    }

    private async ValueTask<MoveResult> ObserveTerminalForMoveAsync()
    {
        _ = await finalResponseTask.ConfigureAwait(false);
        return MoveResult.Finished;
    }

    private static async Task<T> ObserveTerminalAsync(
        Task<TerminalOutcome> terminalTask)
    {
        TerminalOutcome outcome = await terminalTask.ConfigureAwait(false);
        return outcome.GetValueOrThrow();
    }

    private static CancellationToken CreateCanceledToken()
    {
        using var cancellation = new CancellationTokenSource();
        cancellation.Cancel();
        return cancellation.Token;
    }

    private enum StreamMode
    {
        Created,
        PartialConsumer,
        FinalOnly,
        Terminal,
    }

    private sealed class TerminalAlreadySelectedException : Exception;

    private sealed class TerminalOutcome
    {
        private readonly T value = default!;
        private readonly ExceptionDispatchInfo? error;

        private TerminalOutcome(T value)
        {
            this.value = value;
        }

        private TerminalOutcome(Exception exception)
        {
            error = ExceptionDispatchInfo.Capture(exception);
        }

        internal bool HasException => error is not null;

        internal static TerminalOutcome FromValue(T value) => new(value);

        internal static TerminalOutcome FromException(Exception error) => new(error);

        internal T GetValueOrThrow()
        {
            error?.Throw();
            return value;
        }
    }

    private readonly struct MoveResult
    {
        private MoveResult(bool hasPartial, T partial)
        {
            HasPartial = hasPartial;
            Partial = partial;
        }

        internal bool HasPartial { get; }

        internal T Partial { get; }

        internal static MoveResult FromPartial(T partial) =>
            new(hasPartial: true, partial);

        internal static MoveResult Finished { get; } =
            new(hasPartial: false, default!);
    }

    private sealed class Enumerator : IAsyncEnumerator<T>
    {
        private readonly BamlStreamController<T> owner;
        private int moveInProgress;
        private bool disposed;
        private bool finished;

        internal Enumerator(BamlStreamController<T> owner)
        {
            this.owner = owner;
        }

        public T Current { get; private set; } = default!;

        public ValueTask<bool> MoveNextAsync()
        {
            ObjectDisposedException.ThrowIf(disposed, this);
            if (finished)
            {
                return ValueTask.FromResult(false);
            }

            if (Interlocked.CompareExchange(ref moveInProgress, 1, 0) != 0)
            {
                return ValueTask.FromException<bool>(
                    new InvalidOperationException(
                        "Concurrent MoveNextAsync calls are not permitted on a BAML stream."));
            }

            return MoveNextCoreAsync();
        }

        public ValueTask DisposeAsync()
        {
            if (disposed)
            {
                return ValueTask.CompletedTask;
            }

            disposed = true;
            return owner.DisposeAsync();
        }

        private async ValueTask<bool> MoveNextCoreAsync()
        {
            try
            {
                MoveResult result = await owner.MoveNextAsync().ConfigureAwait(false);
                if (!result.HasPartial)
                {
                    finished = true;
                    return false;
                }

                Current = result.Partial;
                return true;
            }
            finally
            {
                Volatile.Write(ref moveInProgress, 0);
            }
        }
    }
}
