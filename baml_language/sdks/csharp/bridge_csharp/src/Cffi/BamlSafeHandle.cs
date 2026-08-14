using System.Runtime.InteropServices;

namespace Baml.Cffi;

internal sealed unsafe class BamlSafeHandle : SafeHandle
{
    private readonly delegate* unmanaged[Cdecl]<ulong, ulong*, BamlCffiStatus> clone;
    private readonly delegate* unmanaged[Cdecl]<ulong, BamlCffiStatus> release;

    internal BamlSafeHandle(
        ulong key,
        delegate* unmanaged[Cdecl]<ulong, ulong*, BamlCffiStatus> clone,
        delegate* unmanaged[Cdecl]<ulong, BamlCffiStatus> release)
        : base(IntPtr.Zero, ownsHandle: true)
    {
        if (key == 0)
        {
            throw new ArgumentOutOfRangeException(nameof(key));
        }

        if (clone is null)
        {
            throw new ArgumentNullException(nameof(clone));
        }

        if (release is null)
        {
            throw new ArgumentNullException(nameof(release));
        }

        this.clone = clone;
        this.release = release;
        SetHandle(unchecked((nint)(nuint)key));
    }

    public override bool IsInvalid => handle == IntPtr.Zero;

    internal ulong Key => unchecked((ulong)(nuint)handle);

    internal BamlSafeHandle CloneOwned()
    {
        using var lease = new BamlSafeHandleLease(this);
        ulong clonedKey = 0;
        BamlCffiStatus status = clone(lease.Key, &clonedKey);
        if (status != BamlCffiStatus.Ok || clonedKey == 0 || clonedKey == lease.Key)
        {
            throw new BamlProtocolException(
                "The native bridge could not clone an owned handle.",
                $"handle_clone returned {status} with source key {lease.Key} and clone key {clonedKey}.");
        }

        return new BamlSafeHandle(clonedKey, clone, release);
    }

    internal void TransferOwnership()
    {
        ObjectDisposedException.ThrowIf(IsClosed || IsInvalid, this);
        SetHandleAsInvalid();
    }

    protected override bool ReleaseHandle() =>
        release(unchecked((ulong)(nuint)handle)) == BamlCffiStatus.Ok;
}
