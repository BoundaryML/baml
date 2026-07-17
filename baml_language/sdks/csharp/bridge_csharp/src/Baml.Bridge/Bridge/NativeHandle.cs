using System.Runtime.InteropServices;

namespace Baml.Bridge;

internal sealed class NativeHandle : SafeHandle
{
    private NativeHandle(ulong key, int handleType)
        : base(IntPtr.Zero, ownsHandle: true)
    {
        HandleType = handleType;
        SetHandle(unchecked((nint)key));
    }

    public override bool IsInvalid => handle == IntPtr.Zero;

    internal int HandleType { get; }

    internal ulong Key => unchecked((ulong)handle);

    internal static NativeHandle FromOwned(ulong key, int handleType)
    {
        if (key == 0)
        {
            throw new BamlBridgeException("The native runtime returned an invalid zero handle key.");
        }

        return new NativeHandle(key, handleType);
    }

    internal NativeHandle Clone(string operation)
    {
        var addedReference = false;
        try
        {
            DangerousAddRef(ref addedReference);
            return FromOwned(NativeApi.CloneHandle(Key, operation), HandleType);
        }
        finally
        {
            if (addedReference)
            {
                DangerousRelease();
            }
        }
    }

    internal TResult Use<TResult>(Func<ulong, int, TResult> operation)
    {
        ObjectDisposedException.ThrowIf(IsClosed || IsInvalid, this);
        var addedReference = false;
        try
        {
            DangerousAddRef(ref addedReference);
            return operation(Key, HandleType);
        }
        finally
        {
            if (addedReference)
            {
                DangerousRelease();
            }
        }
    }

    protected override bool ReleaseHandle() => NativeApi.ReleaseHandle(unchecked((ulong)handle));
}
