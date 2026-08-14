namespace Baml.Cffi;

internal sealed class CallbackIdAllocator
{
    private long lastAllocated;

    internal CallbackIdAllocator(uint lastAllocated = 0)
    {
        this.lastAllocated = lastAllocated;
    }

    internal uint Next()
    {
        long value = Interlocked.Increment(ref lastAllocated);
        if (value is <= 0 or > uint.MaxValue)
        {
            Interlocked.Exchange(ref lastAllocated, (long)uint.MaxValue + 1);
            throw new BamlProtocolException(
                "The managed callback identifier space is exhausted.",
                "The nonzero UInt32 callback identifier domain was exhausted without reuse.");
        }

        return checked((uint)value);
    }
}
