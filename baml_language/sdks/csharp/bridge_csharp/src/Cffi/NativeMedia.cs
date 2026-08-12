using System.Text;

using BamlBridge.Cffi.V1;

namespace Baml.Cffi;

internal readonly record struct NativeMediaSnapshot(
    string Url,
    string Base64,
    string File,
    string MimeType);

internal sealed unsafe partial class NativeApi
{
    private static readonly UTF8Encoding StrictMediaUtf8 = new(
        encoderShouldEmitUTF8Identifier: false,
        throwOnInvalidBytes: true);

    internal BamlSafeHandle CreateMediaOwner(MediaTypeEnum mediaType, MediaPayload payload)
    {
        ArgumentNullException.ThrowIfNull(payload);
        BamlHandleType expectedHandleType = MediaHandleType(mediaType);
        string representation = payload.IsUrl
            ? payload.Url!
            : Convert.ToBase64String(payload.Bytes.Span);
        byte[] encodedRepresentation = NullTerminatedMediaUtf8(representation, "media value");
        byte[]? encodedMimeType = payload.MediaType is null
            ? null
            : NullTerminatedMediaUtf8(payload.MediaType, "media MIME type");
        ulong key = 0;
        int rawHandleType = 0;
        BamlCffiStatus status;
        fixed (byte* representationPointer = encodedRepresentation)
        fixed (byte* mimeTypePointer = encodedMimeType)
        {
            status = payload.IsUrl
                ? table->MediaFromUrl(
                    (int)mediaType,
                    representationPointer,
                    mimeTypePointer,
                    &key,
                    &rawHandleType)
                : table->MediaFromBase64(
                    (int)mediaType,
                    representationPointer,
                    mimeTypePointer,
                    &key,
                    &rawHandleType);
        }

        if (status != BamlCffiStatus.Ok
            || key == 0
            || rawHandleType != (int)expectedHandleType)
        {
            if (key != 0)
            {
                _ = table->HandleRelease(key);
            }

            throw new BamlProtocolException(
                "The native bridge could not create a BAML media value.",
                $"Media constructor returned {status}, key {key}, and handle type {rawHandleType}; expected {(int)expectedHandleType}.");
        }

        return new BamlSafeHandle(key, table->HandleClone, table->HandleRelease);
    }

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
            ReadMediaField(lease.Key, handleType, table->MediaFile, "file"),
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

    private static byte[] NullTerminatedMediaUtf8(string value, string description)
    {
        ArgumentNullException.ThrowIfNull(value);
        if (value.Contains('\0'))
        {
            throw new BamlTypeMappingException(
                typeof(string),
                "media",
                "$",
                $"The {description} contains an interior NUL byte.");
        }

        try
        {
            byte[] bytes = new byte[StrictMediaUtf8.GetByteCount(value) + 1];
            StrictMediaUtf8.GetBytes(value, bytes);
            return bytes;
        }
        catch (EncoderFallbackException error)
        {
            throw new BamlTypeMappingException(
                typeof(string),
                "media",
                "$",
                $"The {description} is not valid Unicode: {error.Message}");
        }
    }
}
