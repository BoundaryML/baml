using Baml.Cffi;
using BamlBridge.Cffi.V1;

namespace Baml;

public sealed class BamlHandle : IDisposable
{
    private readonly BamlSafeHandle handle;

    internal BamlHandle(
        BamlSafeHandle handle,
        BamlTypeDescriptor type,
        BamlHandleType handleType = BamlHandleType.AdtTaggedHeapHandle,
        byte[]? wireTypeMetadata = null)
    {
        ArgumentNullException.ThrowIfNull(handle);
        ArgumentNullException.ThrowIfNull(type);
        if (type.Kind != BamlTypeDescriptorKind.Handle)
        {
            throw new ArgumentException("A BAML handle requires a handle descriptor.", nameof(type));
        }

        this.handle = handle;
        Type = type;
        HandleType = handleType;
        WireTypeMetadata = wireTypeMetadata?.ToArray();
    }

    public bool IsClosed => handle.IsClosed;

    public BamlHandle Clone() => new(handle.CloneOwned(), Type, HandleType, WireTypeMetadata);

    public void Dispose() => handle.Dispose();

    internal BamlTypeDescriptor Type { get; }

    internal BamlHandleType HandleType { get; }

    internal byte[]? WireTypeMetadata { get; }

    internal BamlSafeHandleLease Lease() => new(handle);

    internal BamlSafeHandle CloneOwnedHandle() => handle.CloneOwned();
}
