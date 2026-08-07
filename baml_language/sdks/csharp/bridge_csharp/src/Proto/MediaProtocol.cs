using Baml.Cffi;
using Baml.Generated.V1;
using BamlBridge.Cffi.V1;

namespace Baml.Proto;

internal readonly record struct MediaContract(
    string ClassIdentity,
    MediaTypeEnum MediaType,
    BamlHandleType HandleType);

internal static class MediaProtocol
{
    internal static bool TryContract(object value, out MediaContract contract, out MediaPayload payload)
    {
        switch (value)
        {
            case BamlImage image:
                contract = Contract(MediaTypeEnum.Image);
                payload = image.Payload;
                return true;
            case BamlAudio audio:
                contract = Contract(MediaTypeEnum.Audio);
                payload = audio.Payload;
                return true;
            case BamlVideo video:
                contract = Contract(MediaTypeEnum.Video);
                payload = video.Payload;
                return true;
            case BamlPdf pdf:
                contract = Contract(MediaTypeEnum.Pdf);
                payload = pdf.Payload;
                return true;
            default:
                contract = default;
                payload = null!;
                return false;
        }
    }

    internal static bool TryContract(string classIdentity, out MediaContract contract)
    {
        contract = classIdentity switch
        {
            "baml.media.Image" => Contract(MediaTypeEnum.Image),
            "baml.media.Audio" => Contract(MediaTypeEnum.Audio),
            "baml.media.Video" => Contract(MediaTypeEnum.Video),
            "baml.media.Pdf" => Contract(MediaTypeEnum.Pdf),
            _ => default,
        };
        return contract.MediaType != MediaTypeEnum.MediaTypeUnspecified;
    }

    internal static bool TryContract(BamlHandleType handleType, out MediaContract contract)
    {
        contract = handleType switch
        {
            BamlHandleType.AdtMediaImage => Contract(MediaTypeEnum.Image),
            BamlHandleType.AdtMediaAudio => Contract(MediaTypeEnum.Audio),
            BamlHandleType.AdtMediaVideo => Contract(MediaTypeEnum.Video),
            BamlHandleType.AdtMediaPdf => Contract(MediaTypeEnum.Pdf),
            _ => default,
        };
        return contract.MediaType != MediaTypeEnum.MediaTypeUnspecified;
    }

    internal static BamlGeneratedValue DecodeInline(BamlValueMedia wire, string path)
    {
        ArgumentNullException.ThrowIfNull(wire);
        MediaContract contract = Contract(wire.Media);
        string? mimeType = wire.HasMimeType ? wire.MimeType : null;
        MediaPayload payload = Materialize(
            () => wire.ValueCase switch
            {
                BamlValueMedia.ValueOneofCase.Url => MediaPayload.FromUrl(wire.Url, mimeType),
                BamlValueMedia.ValueOneofCase.Base64 => MediaPayload.FromBytes(
                    DecodeBase64(wire.Base64, path),
                    RequireMimeType(mimeType, path)),
                BamlValueMedia.ValueOneofCase.File => MediaPayload.FromBytes(
                    ReadFile(wire.File, path),
                    RequireMimeType(mimeType, path)),
                _ => throw Invalid(path, "The inline media value has no representation."),
            },
            path);
        return BamlGeneratedValue.CreateMedia(CreateManaged(contract, payload), path);
    }

    internal static BamlGeneratedValue DecodeHandle(
        NativeApi api,
        BamlSafeHandle owner,
        BamlHandleType handleType,
        MediaContract contract,
        string path)
    {
        NativeMediaSnapshot snapshot = api.ReadMedia(owner, handleType, contract.MediaType);
        int representations = (snapshot.Url.Length == 0 ? 0 : 1)
            + (snapshot.Base64.Length == 0 ? 0 : 1)
            + (snapshot.File.Length == 0 ? 0 : 1);
        if (representations != 1)
        {
            throw Invalid(path, $"The media handle exposed {representations} representations.");
        }

        string? mimeType = snapshot.MimeType.Length == 0 ? null : snapshot.MimeType;
        MediaPayload payload = Materialize(
            () => snapshot.Url.Length != 0
                ? MediaPayload.FromUrl(snapshot.Url, mimeType)
                : MediaPayload.FromBytes(
                    snapshot.Base64.Length != 0
                        ? DecodeBase64(snapshot.Base64, path)
                        : ReadFile(snapshot.File, path),
                    RequireMimeType(mimeType, path)),
            path);
        return BamlGeneratedValue.CreateMedia(CreateManaged(contract, payload), path);
    }

    internal static InboundValue Encode(
        NativeApi api,
        EncodedCallArguments ownership,
        object media)
    {
        if (!TryContract(media, out MediaContract contract, out MediaPayload payload))
        {
            throw new BamlProtocolException(
                "A generated media codec produced an unsupported managed value.",
                $"Managed media type {media.GetType()} is not canonical.");
        }

        using BamlSafeHandle original = api.CreateMediaOwner(contract.MediaType, payload);
        BamlSafeHandle transferred = original.CloneOwned();
        ownership.AddTransfer(transferred);
        var @class = new InboundClassValue();
        @class.Fields.Add(new InboundMapEntry
        {
            StringKey = "_data",
            Value = new InboundValue
            {
                Handle = new global::BamlBridge.Cffi.V1.BamlHandle
                {
                    Key = transferred.Key,
                    HandleType = contract.HandleType,
                },
            },
        });
        return new InboundValue
        {
            ValueType = new BamlTy
            {
                Media = new BamlTyMedia { Kind = MediaKind(contract.MediaType) },
            },
            ClassValue = @class,
        };
    }

    private static BamlTyMediaKind MediaKind(MediaTypeEnum mediaType) => mediaType switch
    {
        MediaTypeEnum.Image => BamlTyMediaKind.Image,
        MediaTypeEnum.Audio => BamlTyMediaKind.Audio,
        MediaTypeEnum.Video => BamlTyMediaKind.Video,
        MediaTypeEnum.Pdf => BamlTyMediaKind.Pdf,
        _ => throw new BamlProtocolException(
            "A generated media codec produced an unsupported media type.",
            $"Managed media type {mediaType} has no exact BAML media kind."),
    };

    private static MediaContract Contract(MediaTypeEnum mediaType) => mediaType switch
    {
        MediaTypeEnum.Image => new(
            "baml.media.Image",
            mediaType,
            BamlHandleType.AdtMediaImage),
        MediaTypeEnum.Audio => new(
            "baml.media.Audio",
            mediaType,
            BamlHandleType.AdtMediaAudio),
        MediaTypeEnum.Video => new(
            "baml.media.Video",
            mediaType,
            BamlHandleType.AdtMediaVideo),
        MediaTypeEnum.Pdf => new(
            "baml.media.Pdf",
            mediaType,
            BamlHandleType.AdtMediaPdf),
        _ => throw new BamlProtocolException(
            "The native bridge returned an unsupported BAML media kind.",
            $"Media kind {mediaType} has no public managed projection."),
    };

    private static object CreateManaged(MediaContract contract, MediaPayload payload) =>
        contract.MediaType switch
        {
            MediaTypeEnum.Image => BamlImage.FromPayload(payload),
            MediaTypeEnum.Audio => BamlAudio.FromPayload(payload),
            MediaTypeEnum.Video => BamlVideo.FromPayload(payload),
            MediaTypeEnum.Pdf => BamlPdf.FromPayload(payload),
            _ => throw new InvalidOperationException(),
        };

    private static string RequireMimeType(string? mimeType, string path)
    {
        if (string.IsNullOrWhiteSpace(mimeType))
        {
            throw Invalid(path, "Byte-backed media has no MIME type.");
        }

        return mimeType;
    }

    private static byte[] DecodeBase64(string value, string path)
    {
        if (value.Length % 4 != 0 || value.Any(char.IsWhiteSpace))
        {
            throw Invalid(path, "The media base64 is not in canonical form.");
        }

        int padding = value.EndsWith("==", StringComparison.Ordinal)
            ? 2
            : value.EndsWith('=') ? 1 : 0;
        long decodedLength = (long)(value.Length / 4) * 3 - padding;
        if (decodedLength > BamlValueLimits.MaxBytes)
        {
            throw Invalid(path, $"The media value exceeds the {BamlValueLimits.MaxBytes}-byte limit.");
        }

        try
        {
            return Convert.FromBase64String(value);
        }
        catch (FormatException error)
        {
            throw new BamlProtocolException(
                "The native bridge returned malformed BAML media.",
                $"Invalid base64 media at {path}: {error.Message}");
        }
    }

    private static byte[] ReadFile(string pathValue, string path)
    {
        try
        {
            using var stream = new FileStream(
                pathValue,
                FileMode.Open,
                FileAccess.Read,
                FileShare.Read,
                bufferSize: 81920,
                FileOptions.SequentialScan);
            if (stream.Length > BamlValueLimits.MaxBytes)
            {
                throw Invalid(path, $"The media file exceeds the {BamlValueLimits.MaxBytes}-byte limit.");
            }

            byte[] bytes = new byte[checked((int)stream.Length)];
            stream.ReadExactly(bytes);
            if (stream.ReadByte() != -1)
            {
                throw Invalid(path, "The media file grew beyond its declared bounded length.");
            }

            return bytes;
        }
        catch (BamlProtocolException)
        {
            throw;
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
            throw new BamlProtocolException(
                "The native bridge returned an unreadable BAML media file.",
                $"Could not eagerly read media at {path}: {error.Message}");
        }
    }

    private static MediaPayload Materialize(Func<MediaPayload> factory, string path)
    {
        try
        {
            return factory();
        }
        catch (BamlProtocolException)
        {
            throw;
        }
        catch (Exception error) when (
            error is ArgumentException
            or BamlTypeMappingException
            or FormatException
            or NotSupportedException
            or OverflowException)
        {
            throw new BamlProtocolException(
                "The native bridge returned malformed BAML media.",
                $"Could not materialize media at {path}: {error.Message}");
        }
    }

    private static BamlProtocolException Invalid(string path, string diagnostic) =>
        new(
            "The native bridge returned malformed BAML media.",
            $"{diagnostic} Path: {path}");
}
