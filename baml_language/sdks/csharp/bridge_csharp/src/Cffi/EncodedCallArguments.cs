using BamlBridge.Cffi.V1;
using Google.Protobuf;

namespace Baml.Cffi;

internal sealed class EncodedCallArguments : IDisposable
{
    private readonly List<BamlSafeHandle> transfers = [];
    private readonly List<HostValueRegistration> hostTransfers = [];
    private bool committed;
    private bool disposed;

    internal EncodedCallArguments(byte[] bytes)
    {
        ArgumentNullException.ThrowIfNull(bytes);
        Bytes = bytes;
    }

    internal byte[] Bytes { get; private set; }

    internal void SetCallTarget(string functionName)
    {
        CallFunctionArgs call = CallFunctionArgs.Parser.ParseFrom(Bytes);
        call.FunctionName = functionName;
        Bytes = call.ToByteArray();
    }

    internal void SetCallTarget(ulong functionHandle)
    {
        CallFunctionArgs call = CallFunctionArgs.Parser.ParseFrom(Bytes);
        call.FunctionHandle = functionHandle;
        Bytes = call.ToByteArray();
    }

    internal void SetBytes(byte[] bytes)
    {
        ArgumentNullException.ThrowIfNull(bytes);
        ObjectDisposedException.ThrowIf(disposed, this);
        if (Bytes.Length != 0)
        {
            throw new InvalidOperationException("The encoded call payload is already finalized.");
        }

        Bytes = bytes;
    }

    internal void AddTransfer(BamlSafeHandle handle)
    {
        ArgumentNullException.ThrowIfNull(handle);
        ObjectDisposedException.ThrowIf(disposed, this);
        if (committed)
        {
            throw new InvalidOperationException("The native ownership transaction is already committed.");
        }

        transfers.Add(handle);
    }

    internal void AddTransfer(HostValueRegistration registration)
    {
        ArgumentNullException.ThrowIfNull(registration);
        ObjectDisposedException.ThrowIf(disposed, this);
        if (committed)
        {
            throw new InvalidOperationException("The native ownership transaction is already committed.");
        }

        hostTransfers.Add(registration);
    }

    internal void Commit()
    {
        ObjectDisposedException.ThrowIf(disposed, this);
        if (committed)
        {
            return;
        }

        foreach (BamlSafeHandle handle in transfers)
        {
            handle.TransferOwnership();
        }

        foreach (HostValueRegistration registration in hostTransfers)
        {
            registration.Commit();
        }

        committed = true;
    }

    public void Dispose()
    {
        if (disposed)
        {
            return;
        }

        disposed = true;
        foreach (BamlSafeHandle handle in transfers)
        {
            handle.Dispose();
        }
        foreach (HostValueRegistration registration in hostTransfers)
        {
            registration.Dispose();
        }
    }
}
