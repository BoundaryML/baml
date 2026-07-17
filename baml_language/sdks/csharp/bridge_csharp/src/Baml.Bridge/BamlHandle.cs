using Baml.Bridge;

namespace Baml;

public sealed class BamlHandle : IDisposable
{
    private NativeHandle? _handle;

    private BamlHandle(NativeHandle handle)
    {
        _handle = handle;
    }

    public BamlHandle Clone() => new(GetHandle().Clone("clone BamlHandle"));

    public void Dispose()
    {
        Interlocked.Exchange(ref _handle, null)?.Dispose();
        GC.SuppressFinalize(this);
    }

    internal static BamlHandle FromOwnedHandle(NativeHandle handle) => new(handle);

    internal (ulong Key, int HandleType) CloneForWire()
    {
        var clone = GetHandle().Clone("clone BamlHandle for BAML argument");
        var key = clone.Key;
        var handleType = clone.HandleType;
        clone.SetHandleAsInvalid();
        clone.Dispose();
        return (key, handleType);
    }

    private NativeHandle GetHandle() => Volatile.Read(ref _handle)
        ?? throw new ObjectDisposedException(GetType().FullName);
}
