using Baml.Cffi;
using BamlBridge.Cffi.V1;
using Google.Protobuf;

namespace Baml.Proto;

/// <summary>
/// A host invocation's arguments as dispatch delivers them, together with the
/// engine references they carry. The adapter owns those references from
/// dispatch on: binding hands each one an argument keeps to its decoded value
/// (a BAML closure, an engine handle, a type value's rows), and disposing
/// releases every one nothing claimed, so a callback that never runs or fails
/// to bind leaks none.
/// </summary>
internal sealed class HostCallArguments : IDisposable
{
    private HostCallArguments(BamlToHostCall call, OutboundOwnershipScope ownership, NativeApi api)
    {
        Call = call;
        Ownership = ownership;
        Api = api;
    }

    internal BamlToHostCall Call { get; }

    internal OutboundOwnershipScope Ownership { get; }

    internal NativeApi Api { get; }

    /// <summary>Parse borrowed dispatch bytes and take ownership of the references they carry.</summary>
    internal static HostCallArguments Parse(ReadOnlySpan<byte> bytes, NativeApi api)
    {
        ArgumentNullException.ThrowIfNull(api);
        BamlToHostCall call;
        try
        {
            call = BamlToHostCall.Parser.ParseFrom(bytes);
        }
        catch (InvalidProtocolBufferException error)
        {
            throw new BamlProtocolException(
                "The native bridge supplied malformed host callback arguments.",
                error.Message);
        }

        return new HostCallArguments(call, OutboundOwnershipScope.Create(call, api), api);
    }

    public void Dispose() => Ownership.Dispose();
}
