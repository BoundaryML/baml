using System.Diagnostics;
using System.Diagnostics.CodeAnalysis;
using System.Runtime.ExceptionServices;

using Baml.Generated.V1;

namespace Baml.Cffi;

/// <summary>
/// The host's table of objects the engine references by host key. Each key
/// counts the references the host has handed the engine: a registration
/// starts at one, for the send it is registered for, and the engine's release
/// callback gives references back. An entry is dropped once the engine holds
/// none, its own send is settled, and no dispatch of it is running.
/// </summary>
internal sealed class HostValueRegistry
{
    private readonly object gate = new();
    private readonly List<Slot> slots = [];
    private readonly Stack<int> freeSlots = [];
    private readonly Dictionary<ulong, Operation> operations = [];
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
        return Register(new CallableObject(
            callable,
            ExecutionContext.Capture(),
            parentFunctionCallId));
    }

    internal HostValueRegistration RegisterException(ExceptionDispatchInfo exception)
    {
        ArgumentNullException.ThrowIfNull(exception);
        return Register(new ExceptionObject(exception));
    }

    internal void BeginFunctionCall(ulong functionCallId, CancellationToken cancellationToken)
    {
        functionCallId = NativeApi.RequireFunctionCallIdentifier(functionCallId);
        lock (gate)
        {
            if (!operations.TryAdd(functionCallId, new Operation(cancellationToken, ExecutionContext.Capture())))
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
        }
    }

    /// <summary>
    /// Start a dispatched invocation of the callable registered at
    /// <paramref name="hostKey"/>. The started invocation owns
    /// <paramref name="arguments"/>; when none starts, the caller still does.
    /// </summary>
    internal HostInvocation? TryStartInvocation(
        ulong hostKey,
        uint hostCallId,
        Baml.Proto.HostCallArguments arguments,
        out string? diagnostic,
        ulong? hostEnvironment = null)
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

            // The engine dispatches only a callable it holds a reference to.
            if (!TryGetEntry(hostKey, out Entry? entry)
                || entry.Object is not CallableObject callable
                || entry.References == 0)
            {
                diagnostic = $"The native host dispatch referenced stale callable key {hostKey}.";
                Interlocked.Increment(ref staleDispatches);
                return null;
            }

            entry.ActiveLeases = checked(entry.ActiveLeases + 1);
            ulong functionCallId = hostEnvironment ?? callable.ParentFunctionCallId;
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
                callable.Callable,
                hostEnvironment.HasValue && operation is not null
                    ? operation.ExecutionContext
                    : callable.ExecutionContext,
                cancellation);
            invocations.Add(hostCallId, invocation);
        }

        return invocation;
    }

    internal void CancelInvocation(uint callbackId)
    {
        HostInvocation? invocation;
        lock (gate) { invocations.TryGetValue(callbackId, out invocation); }
        invocation?.Cancel();
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

    /// <summary>
    /// The engine giving back <paramref name="count"/> references to
    /// <paramref name="key"/>. A release of an unknown key, of no references,
    /// or of more references than the engine holds is counted as stale and
    /// changes nothing.
    /// </summary>
    internal void Release(ulong key, ulong count)
    {
        lock (gate)
        {
            if (count == 0
                || !TryGetEntry(key, out Entry? entry)
                || count > entry.References)
            {
                Interlocked.Increment(ref staleReleases);
                return;
            }

            entry.References -= count;
            TryCollect(key, entry);
        }
    }

    /// <summary>
    /// The exception registered at <paramref name="key"/>, claimed once: a
    /// second restore of the same registration finds nothing, however long
    /// the engine still holds it.
    /// </summary>
    internal bool TryRestoreException(ulong key, [NotNullWhen(true)] out ExceptionDispatchInfo? exception)
    {
        lock (gate)
        {
            if (!TryGetEntry(key, out Entry? entry)
                || entry.Object is not ExceptionObject registered
                || registered.Claimed)
            {
                exception = null;
                return false;
            }

            registered.Claimed = true;
            exception = registered.Exception;
            return true;
        }
    }

    private HostValueRegistration Register(HostObject hostObject)
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

            slot.Entry = new Entry(hostObject);
            ulong key = Pack(index, slot.Generation);
            return new HostValueRegistration(this, key);
        }
    }

    /// <summary>The registration's send reached the engine, which now holds its reference.</summary>
    internal void Commit(ulong key)
    {
        lock (gate)
        {
            if (!TryGetEntry(key, out Entry? entry) || !entry.SendPending)
            {
                throw new InvalidOperationException(
                    "The managed host-value ownership transaction is no longer pending.");
            }

            entry.SendPending = false;
            TryCollect(key, entry);
        }
    }

    /// <summary>The registration's send never reached the engine: its one reference comes back.</summary>
    internal void Abort(ulong key)
    {
        lock (gate)
        {
            if (!TryGetEntry(key, out Entry? entry) || !entry.SendPending)
            {
                return;
            }

            Debug.Assert(
                entry.References != 0,
                "The engine released a reference to a host value whose send never reached it.");
            entry.SendPending = false;
            if (entry.References != 0)
            {
                entry.References--;
            }

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
        if (entry.References != 0 || entry.SendPending || entry.ActiveLeases != 0)
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

    private sealed class Entry(HostObject hostObject)
    {
        internal HostObject Object { get; } = hostObject;

        /// <summary>
        /// References the host has handed the engine and not had back,
        /// including the one of a send not yet settled.
        /// </summary>
        internal ulong References { get; set; } = 1;

        /// <summary>The registration's own send has neither reached the engine nor been rolled back.</summary>
        internal bool SendPending { get; set; } = true;

        internal int ActiveLeases { get; set; }
    }

    private abstract class HostObject;

    private sealed class CallableObject(
        BamlGeneratedHostCallable callable,
        ExecutionContext? executionContext,
        ulong parentFunctionCallId) : HostObject
    {
        internal BamlGeneratedHostCallable Callable { get; } = callable;

        internal ExecutionContext? ExecutionContext { get; } = executionContext;

        internal ulong ParentFunctionCallId { get; } = parentFunctionCallId;
    }

    private sealed class ExceptionObject(ExceptionDispatchInfo exception) : HostObject
    {
        internal ExceptionDispatchInfo Exception { get; } = exception;

        /// <summary>The exception has been rethrown once; it is never restored again.</summary>
        internal bool Claimed { get; set; }
    }

    private sealed class Operation(CancellationToken cancellationToken, ExecutionContext? executionContext)
    {
        internal CancellationToken CancellationToken { get; } = cancellationToken;

        internal ExecutionContext? ExecutionContext { get; } = executionContext;
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
        Baml.Proto.HostCallArguments arguments,
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

    /// <summary>The invocation's arguments and the engine references they carry, owned until bound or completed.</summary>
    internal Baml.Proto.HostCallArguments Arguments { get; }

    internal BamlGeneratedHostCallable Callable { get; }

    internal ExecutionContext? ExecutionContext { get; }

    internal CancellationToken CancellationToken => cancellation.Token;

    internal void Complete() => registry.CompleteInvocation(this);

    internal BamlSafeHandle? EffectiveState { get; set; }

    internal Baml.Generated.V1.BamlInvocationCapture? Frame { get; set; }

    internal Baml.Proto.OutboundOwnershipScope? Controls { get; set; }

    internal void Cancel()
    {
        try { cancellation.Cancel(); } catch (ObjectDisposedException) { }
    }

    internal void DisposeCancellation()
    {
        cancellation.Dispose();
        Arguments.Dispose();
        Controls?.Dispose();
        EffectiveState?.Dispose();
    }
}

internal static class InvocationFrame
{
    internal static readonly AsyncLocal<Baml.Generated.V1.BamlInvocationCapture?> Current = new();
}
