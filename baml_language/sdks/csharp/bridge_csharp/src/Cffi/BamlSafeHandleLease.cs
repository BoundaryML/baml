namespace Baml.Cffi;

internal ref struct BamlSafeHandleLease
{
    private BamlSafeHandle? owner;

    internal BamlSafeHandleLease(BamlSafeHandle owner)
    {
        ArgumentNullException.ThrowIfNull(owner);
        bool added = false;
        try
        {
            owner.DangerousAddRef(ref added);
            if (!added)
            {
                throw new ObjectDisposedException(nameof(BamlSafeHandle));
            }

            this.owner = owner;
            Key = unchecked((ulong)(nuint)owner.DangerousGetHandle());
        }
        catch
        {
            if (added)
            {
                owner.DangerousRelease();
            }

            throw;
        }
    }

    internal ulong Key { get; }

    public void Dispose()
    {
        BamlSafeHandle? current = owner;
        owner = null;
        current?.DangerousRelease();
    }
}
