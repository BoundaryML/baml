using BamlBridge.Cffi.V1;

namespace Baml.Cffi;

internal readonly record struct NativeMediaSnapshot(
    string Url,
    string Base64,
    string Name,
    string MimeType);

internal sealed unsafe partial class NativeApi
{
    internal NativeMediaSnapshot ReadMedia(
        BamlSafeHandle owner,
        BamlHandleType handleType,
        MediaTypeEnum mediaType)
    {
        ArgumentNullException.ThrowIfNull(owner);
        BamlHandleType expectedHandleType = MediaHandleType(mediaType);
        if (handleType != expectedHandleType)
        {
            throw new BamlProtocolException(
                "The native bridge returned the wrong BAML media handle type.",
                $"Expected {expectedHandleType}, received {handleType}.");
        }

        using var lease = new BamlSafeHandleLease(owner);
        return new NativeMediaSnapshot(
            ReadMediaField(lease.Key, handleType, table->MediaUrl, "URL"),
            ReadMediaField(lease.Key, handleType, table->MediaBase64, "base64"),
            ReadMediaField(lease.Key, handleType, table->MediaName, "name"),
            ReadMediaField(lease.Key, handleType, table->MediaMimeType, "MIME type"));
    }

    private string ReadMediaField(
        ulong key,
        BamlHandleType handleType,
        delegate* unmanaged[Cdecl]<ulong, int, BamlBuffer*, BamlCffiStatus> accessor,
        string field)
    {
        BamlBuffer buffer = default;
        BamlCffiStatus status = accessor(key, (int)handleType, &buffer);
        if (status != BamlCffiStatus.Ok)
        {
            table->FreeBuffer(buffer);
            throw new BamlProtocolException(
                "The native bridge could not restore a BAML media value.",
                $"The media {field} accessor returned {status} for handle {key}.");
        }

        return NativeBuffer.ReadUtf8AndFree(table, buffer);
    }

    private static BamlHandleType MediaHandleType(MediaTypeEnum mediaType) => mediaType switch
    {
        MediaTypeEnum.Image => BamlHandleType.AdtMediaImage,
        MediaTypeEnum.Audio => BamlHandleType.AdtMediaAudio,
        MediaTypeEnum.Pdf => BamlHandleType.AdtMediaPdf,
        MediaTypeEnum.Video => BamlHandleType.AdtMediaVideo,
        _ => throw new BamlProtocolException(
            "The managed bridge encountered an unsupported BAML media kind.",
            $"Media kind {mediaType} has no public managed projection."),
    };
}
