namespace Baml.ProtocolProbe;

/// <summary>
/// Repository-only public marker used to prove that consuming the packed
/// fixture neither restores Grpc.Tools nor regenerates transport sources.
/// </summary>
public static class ProtocolPackageSurface
{
    /// <summary>
    /// Gets the canonical Protobuf descriptor name without exposing a
    /// generated Protobuf type in the public signature.
    /// </summary>
    public static string OutboundDescriptorName =>
        global::BamlBridge.Cffi.V1.BamlOutboundValue.Descriptor.FullName;
}
