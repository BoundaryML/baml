using System.Runtime.ExceptionServices;
using System.Diagnostics.CodeAnalysis;

using Baml.Generated.V1;

namespace Baml.Cffi;

internal sealed class HostValueRegistry
{
    private readonly object gate = new();
    private readonly List<Slot> slots = [];
    private readonly Stack<int> freeSlots = [];
    private readonly Dictionary<ulong, Operation> operations = [];
    private readonly Dictionary<ulong, HashSet<ulong>> operationExceptions = [];
    private readonly Dictionary<uint, HostInvocation> invocations = [];
    private long staleReleases;
    private long staleDispatches;

    internal static HostValueRegistry Shared { get; } = new();

    internal int EntryCount
    {
        get
        {
            lock (gate)
            {
                return slots.Count(slot => slot.Entry is not null);
            }
        }
    }

    internal int InvocationCount
    {
        get
        {
            lock (gate)
            {
                return invocations.Count;
            }
        }
    }

    internal long StaleReleases => Interlocked.Read(ref staleReleases);

    internal long StaleDispatches => Interlocked.Read(ref staleDispatches);

    internal HostValueRegistration RegisterCallable(
        BamlGeneratedHostCallable callable,
        ulong parentFunctionCallId)
    {
        ArgumentNullException.ThrowIfNull(callable);
        parentFunctionCallId =
            NativeApi.RequireFunctionCallIdentifier(parentFunctionCallId);
        var entry = new Entry(
            EntryKind.Callable,
            callable,
            ExecutionContext.Capture(),
            exception: null,
            parentFunctionCallId,
            operationSettled: false);
        return Register(entry);
    }

    internal HostValueRegistration RegisterException(
        ExceptionDispatchInfo exception,
        ulong parentFunctionCallId)
    {
        ArgumentNullException.ThrowIfNull(exception);
        bool operationSettled;
        lock (gate)
        {
            operationSettled = parentFunctionCallId == 0
                || !operations.ContainsKey(parentFunctionCallId);
        }

        var entry = new Entry(
            EntryKind.Exception,
            callable: null,
            executionContext: null,
            exception,
            parentFunctionCallId,
            operationSettled);
        HostValueRegistration registration = Register(entry);
        if (!operationSettled)
        {
            lock (gate)
            {
                if (TryGetEntry(registration.Key, out Entry? registered)
                    && ReferenceEquals(registered, entry)
                    && operations.ContainsKey(parentFunctionCallId))
                {
                    if (!operationExceptions.TryGetValue(
                            parentFunctionCallId,
                            out HashSet<ulong>? keys))
                    {
                        keys = [];
                        operationExceptions.Add(parentFunctionCallId, keys);
                    }

                    keys.Add(registration.Key);
                }
                else
                {
                    entry.OperationSettled = true;
                }
            }
        }

        return registration;
    }

    internal void BeginFunctionCall(ulong functionCallId, CancellationToken cancellationToken)
    {
        functionCallId = NativeApi.RequireFunctionCallIdentifier(functionCallId);
        lock (gate)
        {
            if (!operations.TryAdd(functionCallId, new Operation(cancellationToken)))
            {
                throw new BamlProtocolException(
                    "A native function-call identifier was unexpectedly reused.",
                    $"Function-call identifier {functionCallId} already exists in the managed host-value registry.");
            }
        }
    }

    internal void CompleteFunctionCall(ulong functionCallId)
    {
        if (functionCallId == 0)
        {
            return;
        }

        lock (gate)
        {
            _ = operations.Remove(functionCallId);
            if (!operationExceptions.Remove(functionCallId, out HashSet<ulong>? keys))
            {
                return;
            }

            foreach (ulong key in keys)
            {
                if (TryGetEntry(key, out Entry? entry)
                    && entry.Kind == EntryKind.Exception
                    && entry.ParentFunctionCallId == functionCallId)
                {
                    entry.OperationSettled = true;
                    TryCollect(key, entry);
                }
            }
        }
    }

    internal HostInvocation? TryStartInvocation(
        ulong hostKey,
        uint hostCallId,
        byte[] arguments,
        out string? diagnostic)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        diagnostic = null;
        HostInvocation? invocation;
        lock (gate)
        {
            if (hostCallId == 0)
            {
                diagnostic = "The native host dispatch supplied call identifier zero.";
                Interlocked.Increment(ref staleDispatches);
                return null;
            }

            if (invocations.ContainsKey(hostCallId))
            {
                diagnostic = $"The native host dispatch reused active call identifier {hostCallId}.";
                Interlocked.Increment(ref staleDispatches);
                return null;
            }

            if (!TryGetEntry(hostKey, out Entry? entry)
                || entry.Kind != EntryKind.Callable
                || entry.Aborted
                || entry.ReleaseObserved)
            {
                diagnostic = $"The native host dispatch referenced stale callable key {hostKey}.";
                Interlocked.Increment(ref staleDispatches);
                return null;
            }

            entry.ActiveLeases = checked(entry.ActiveLeases + 1);
            ulong functionCallId = entry.ParentFunctionCallId;
            CancellationToken callerToken = operations.TryGetValue(
                functionCallId,
                out Operation? operation)
                ? operation.CancellationToken
                : CancellationToken.None;
            var cancellation = CancellationTokenSource.CreateLinkedTokenSource(callerToken);
            invocation = new HostInvocation(
                this,
                hostKey,
                hostCallId,
                functionCallId,
                arguments,
                entry.Callable!,
                entry.ExecutionContext,
                cancellation);
            invocations.Add(hostCallId, invocation);
        }

        return invocation;
    }

    internal void CompleteInvocation(HostInvocation invocation)
    {
        ArgumentNullException.ThrowIfNull(invocation);
        lock (gate)
        {
            if (!invocations.Remove(invocation.HostCallId, out HostInvocation? stored)
                || !ReferenceEquals(stored, invocation))
            {
                return;
            }

            if (TryGetEntry(invocation.HostKey, out Entry? entry))
            {
                entry.ActiveLeases = checked(entry.ActiveLeases - 1);
                TryCollect(invocation.HostKey, entry);
            }
        }

        invocation.DisposeCancellation();
    }

    internal void Release(ulong key)
    {
        lock (gate)
        {
            if (!TryGetEntry(key, out Entry? entry) || entry.ReleaseObserved)
            {
                Interlocked.Increment(ref staleReleases);
                return;
            }

            entry.ReleaseObserved = true;
            TryCollect(key, entry);
        }
    }

    internal bool TryRestoreException(ulong key, out ExceptionDispatchInfo? exception)
    {
        lock (gate)
        {
            if (!TryGetEntry(key, out Entry? entry)
                || entry.Kind != EntryKind.Exception
                || entry.Aborted
                || entry.ExceptionClaimed)
            {
                exception = null;
                return false;
            }

            entry.ExceptionClaimed = true;
            exception = entry.Exception;
            TryCollect(key, entry);
            return true;
        }
    }

    private HostValueRegistration Register(Entry entry)
    {
        lock (gate)
        {
            int index;
            Slot slot;
            if (freeSlots.TryPop(out index))
            {
                slot = slots[index];
            }
            else
            {
                index = slots.Count;
                slot = new Slot(generation: 1);
                slots.Add(slot);
            }

            if (slot.Entry is not null || slot.Retired)
            {
                throw new InvalidOperationException(
                    "The managed host-value free list is corrupt.");
            }

            slot.Entry = entry;
            ulong key = Pack(index, slot.Generation);
            return new HostValueRegistration(this, key);
        }
    }

    internal void Commit(ulong key)
    {
        lock (gate)
        {
            if (!TryGetEntry(key, out Entry? entry) || entry.Aborted)
            {
                throw new InvalidOperationException(
                    "The managed host-value ownership transaction is no longer pending.");
            }

            entry.Committed = true;
            TryCollect(key, entry);
        }
    }

    internal void Abort(ulong key)
    {
        lock (gate)
        {
            if (!TryGetEntry(key, out Entry? entry) || entry.Committed)
            {
                return;
            }

            entry.Aborted = true;
            TryCollect(key, entry);
        }
    }

    private bool TryGetEntry(
        ulong key,
        [NotNullWhen(true)] out Entry? entry)
    {
        uint oneBasedIndex = (uint)key;
        uint generation = (uint)(key >> 32);
        if (oneBasedIndex == 0
            || generation == 0
            || oneBasedIndex > slots.Count)
        {
            entry = null;
            return false;
        }

        Slot slot = slots[checked((int)oneBasedIndex - 1)];
        if (slot.Generation != generation || slot.Entry is null)
        {
            entry = null;
            return false;
        }

        entry = slot.Entry;
        return true;
    }

    private void TryCollect(ulong key, Entry entry)
    {
        if (entry.ActiveLeases != 0)
        {
            return;
        }

        bool collect = entry.Aborted
            || entry.Committed
                && (entry.Kind == EntryKind.Callable
                    ? entry.ReleaseObserved
                    : entry.OperationSettled
                        || entry.ReleaseObserved && entry.ExceptionClaimed);
        if (!collect)
        {
            return;
        }

        uint oneBasedIndex = (uint)key;
        int index = checked((int)oneBasedIndex - 1);
        Slot slot = slots[index];
        if (!ReferenceEquals(slot.Entry, entry))
        {
            return;
        }

        slot.Entry = null;
        if (entry.Kind == EntryKind.Exception
            && entry.ParentFunctionCallId != 0
            && operationExceptions.TryGetValue(
                entry.ParentFunctionCallId,
                out HashSet<ulong>? keys))
        {
            _ = keys.Remove(key);
            if (keys.Count == 0)
            {
                _ = operationExceptions.Remove(entry.ParentFunctionCallId);
            }
        }

        if (slot.Generation == uint.MaxValue)
        {
            slot.Retired = true;
        }
        else
        {
            slot.Generation++;
            freeSlots.Push(index);
        }
    }

    private static ulong Pack(int index, uint generation) =>
        ((ulong)generation << 32) | checked((uint)index + 1U);

    private sealed class Slot(uint generation)
    {
        internal uint Generation { get; set; } = generation;

        internal Entry? Entry { get; set; }

        internal bool Retired { get; set; }
    }

    private sealed class Entry(
        EntryKind kind,
        BamlGeneratedHostCallable? callable,
        ExecutionContext? executionContext,
        ExceptionDispatchInfo? exception,
        ulong parentFunctionCallId,
        bool operationSettled)
    {
        internal EntryKind Kind { get; } = kind;

        internal BamlGeneratedHostCallable? Callable { get; } = callable;

        internal ExecutionContext? ExecutionContext { get; } = executionContext;

        internal ExceptionDispatchInfo? Exception { get; } = exception;

        internal ulong ParentFunctionCallId { get; } = parentFunctionCallId;

        internal bool OperationSettled { get; set; } = operationSettled;

        internal bool Committed { get; set; }

        internal bool Aborted { get; set; }

        internal bool ReleaseObserved { get; set; }

        internal bool ExceptionClaimed { get; set; }

        internal int ActiveLeases { get; set; }
    }

    private sealed class Operation(CancellationToken cancellationToken)
    {
        internal CancellationToken CancellationToken { get; } = cancellationToken;
    }

    private enum EntryKind
    {
        Callable,
        Exception,
    }
}

internal sealed class HostValueRegistration : IDisposable
{
    private readonly HostValueRegistry registry;
    private readonly object gate = new();
    private int state;

    internal HostValueRegistration(HostValueRegistry registry, ulong key)
    {
        this.registry = registry;
        Key = key;
    }

    internal ulong Key { get; }

    internal void Commit()
    {
        lock (gate)
        {
            if (state != 0)
            {
                return;
            }

            registry.Commit(Key);
            state = 1;
        }
    }

    public void Dispose()
    {
        lock (gate)
        {
            if (state == 0)
            {
                registry.Abort(Key);
                state = 2;
            }
        }
    }
}

internal sealed class HostInvocation
{
    private readonly HostValueRegistry registry;
    private readonly CancellationTokenSource cancellation;

    internal HostInvocation(
        HostValueRegistry registry,
        ulong hostKey,
        uint hostCallId,
        ulong functionCallId,
        byte[] arguments,
        BamlGeneratedHostCallable callable,
        ExecutionContext? executionContext,
        CancellationTokenSource cancellation)
    {
        this.registry = registry;
        this.cancellation = cancellation;
        HostKey = hostKey;
        HostCallId = hostCallId;
        FunctionCallId = functionCallId;
        Arguments = arguments;
        Callable = callable;
        ExecutionContext = executionContext;
    }

    internal ulong HostKey { get; }

    internal uint HostCallId { get; }

    internal ulong FunctionCallId { get; }

    internal byte[] Arguments { get; }

    internal BamlGeneratedHostCallable Callable { get; }

    internal ExecutionContext? ExecutionContext { get; }

    internal CancellationToken CancellationToken => cancellation.Token;

    internal void Complete() => registry.CompleteInvocation(this);

    internal void DisposeCancellation() => cancellation.Dispose();
}
