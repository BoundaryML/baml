using System.Runtime.CompilerServices;
using Baml.Bridge;

namespace Baml;

public sealed class BamlStreamFinished
{
    internal static BamlStreamFinished Instance { get; } = new();

    private BamlStreamFinished()
    {
    }

    public override string ToString() => nameof(BamlStreamFinished);
}

public sealed class BamlStream<TPartial, TFinal> :
    IAsyncEnumerable<TPartial>,
    IAsyncDisposable,
    IDisposable,
    IBamlStreamValue
{
    private const int TaggedHeapHandleType = 14;
    private readonly SemaphoreSlim _pullGate = new(1, 1);
    private NativeHandle? _handle;
    private int _enumerationStarted;

    internal BamlStream(NativeHandle handle)
    {
        if (handle.HandleType != TaggedHeapHandleType)
        {
            handle.Dispose();
            throw new BamlBridgeException(
                $"The native runtime returned handle type {handle.HandleType}, but a BAML stream requires {TaggedHeapHandleType}.");
        }

        _handle = handle;
    }

    public BamlUnion<TPartial, BamlStreamFinished> Next() =>
        NextAsync(CancellationToken.None).GetAwaiter().GetResult();

    public Task<BamlUnion<TPartial, BamlStreamFinished>> NextAsync(
        CancellationToken cancellationToken = default) => NextCoreAsync(cancellationToken);

    public TFinal Final() => FinalAsync(CancellationToken.None).GetAwaiter().GetResult();

    public Task<TFinal> FinalAsync(CancellationToken cancellationToken = default) =>
        PullAsync<TFinal>("baml.llm.Stream.final", cancellationToken);

    public IAsyncEnumerator<TPartial> GetAsyncEnumerator(
        CancellationToken cancellationToken = default)
    {
        ObjectDisposedException.ThrowIf(Volatile.Read(ref _handle) is null, this);
        if (Interlocked.Exchange(ref _enumerationStarted, 1) != 0)
        {
            throw new InvalidOperationException("A BAML stream can be enumerated only once.");
        }

        return new Enumerator(this, cancellationToken);
    }

    public void Dispose()
    {
        Interlocked.Exchange(ref _handle, null)?.Dispose();
        GC.SuppressFinalize(this);
    }

    public ValueTask DisposeAsync()
    {
        Dispose();
        return ValueTask.CompletedTask;
    }

    (ulong Key, int HandleType) IBamlStreamValue.CloneForWire()
    {
        var clone = GetHandle().Clone("clone BamlStream for BAML argument");
        var key = clone.Key;
        var handleType = clone.HandleType;
        clone.SetHandleAsInvalid();
        clone.Dispose();
        return (key, handleType);
    }

    private async Task<TResult> PullAsync<TResult>(
        string functionName,
        CancellationToken cancellationToken)
    {
        await _pullGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            ObjectDisposedException.ThrowIf(Volatile.Read(ref _handle) is null, this);
            return await CallDispatcher.CallAsync<TResult>(
                    functionName,
                    [("self", this)],
                    Array.Empty<(string Name, Type Type)>(),
                    cancellationToken)
                .ConfigureAwait(false);
        }
        finally
        {
            _pullGate.Release();
        }
    }

    private async Task<BamlUnion<TPartial, BamlStreamFinished>> NextCoreAsync(
        CancellationToken cancellationToken)
    {
        await _pullGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            ObjectDisposedException.ThrowIf(Volatile.Read(ref _handle) is null, this);
            return await CallDispatcher.CallStreamNextAsync<TPartial>(this, cancellationToken)
                .ConfigureAwait(false);
        }
        finally
        {
            _pullGate.Release();
        }
    }

    private NativeHandle GetHandle() => Volatile.Read(ref _handle)
        ?? throw new ObjectDisposedException(GetType().FullName);

    private sealed class Enumerator(
        BamlStream<TPartial, TFinal> owner,
        CancellationToken cancellationToken) : IAsyncEnumerator<TPartial>
    {
        private bool _finished;

        public TPartial Current { get; private set; } = default!;

        public async ValueTask<bool> MoveNextAsync()
        {
            if (_finished)
            {
                return false;
            }

            var next = await owner.NextAsync(cancellationToken).ConfigureAwait(false);
            if (next.IsT1)
            {
                _finished = true;
                Current = default!;
                return false;
            }

            Current = next.AsT0;
            return true;
        }

        public ValueTask DisposeAsync()
        {
            if (!_finished)
            {
                _finished = true;
                owner.Dispose();
            }

            return ValueTask.CompletedTask;
        }
    }
}
