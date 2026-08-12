using System.Diagnostics.CodeAnalysis;

namespace Baml;

internal sealed class MediaPayload : IEquatable<MediaPayload>
{
    private readonly byte[]? bytes;

    private MediaPayload(
        string? url,
        byte[]? bytes,
        string? mediaType)
    {
        Url = url;
        this.bytes = bytes?.ToArray();
        MediaType = mediaType;
    }

    internal ReadOnlyMemory<byte> Bytes =>
        bytes is null
            ? ReadOnlyMemory<byte>.Empty
            : new ReadOnlyMemory<byte>(bytes);

    internal bool IsUrl => Url is not null;

    internal string? MediaType { get; }

    internal string? Url { get; }

    internal static MediaPayload FromUrl(
        string url,
        string? mediaType)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(url);
        if (mediaType is not null)
        {
            ArgumentException.ThrowIfNullOrWhiteSpace(mediaType);
        }

        return new MediaPayload(url, bytes: null, mediaType);
    }

    internal static MediaPayload FromBytes(
        ReadOnlyMemory<byte> data,
        string mediaType)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(mediaType);
        BamlValueLimits.RequireBytes(
            data.Length,
            "$.media");
        return new MediaPayload(
            url: null,
            data.Span.ToArray(),
            mediaType);
    }

    internal static MediaPayload FromBase64(
        string base64,
        string mediaType)
    {
        ArgumentNullException.ThrowIfNull(base64);
        ArgumentException.ThrowIfNullOrWhiteSpace(mediaType);
        return FromBytes(
            Convert.FromBase64String(base64),
            mediaType);
    }

    internal static async Task<MediaPayload> FromFileAsync(
        string path,
        string mediaType,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(path);
        ArgumentException.ThrowIfNullOrWhiteSpace(mediaType);
        FileInfo file = new(path);
        if (file.Length > BamlValueLimits.MaxBytes)
        {
            BamlValueLimits.RequireBytes(
                checked((int)Math.Min(
                    file.Length,
                    Int32.MaxValue)),
                "$.media");
        }

        byte[] contents = await File.ReadAllBytesAsync(
                path,
                cancellationToken)
            .ConfigureAwait(false);
        return FromBytes(contents, mediaType);
    }

    internal bool TryGetUrl(
        [NotNullWhen(true)] out string? url)
    {
        url = Url;
        return url is not null;
    }

    internal bool TryGetBytes(
        out ReadOnlyMemory<byte> data,
        [NotNullWhen(true)] out string? mediaType)
    {
        data = Bytes;
        mediaType = IsUrl ? null : MediaType;
        return !IsUrl;
    }

    public bool Equals(MediaPayload? other)
    {
        if (other is null
            || IsUrl != other.IsUrl
            || !StringComparer.Ordinal.Equals(
                MediaType,
                other.MediaType))
        {
            return false;
        }

        return IsUrl
            ? StringComparer.Ordinal.Equals(Url, other.Url)
            : Bytes.Span.SequenceEqual(other.Bytes.Span);
    }

    public override bool Equals(object? obj) =>
        Equals(obj as MediaPayload);

    public override int GetHashCode()
    {
        HashCode hash = new();
        hash.Add(IsUrl);
        hash.Add(MediaType, StringComparer.Ordinal);
        if (IsUrl)
        {
            hash.Add(Url, StringComparer.Ordinal);
        }
        else
        {
            foreach (byte value in Bytes.Span)
            {
                hash.Add(value);
            }
        }

        return hash.ToHashCode();
    }

    public override string ToString()
    {
        if (!IsUrl)
        {
            return $"bytes(<redacted>, mediaType={MediaType})";
        }

        string url = Url!;
        int sensitive = url.IndexOfAny(['?', '#']);
        string safe = sensitive < 0
            ? url
            : $"{url[..sensitive]}?<redacted>";
        return $"url({safe}, mediaType={MediaType ?? "<none>"})";
    }
}

public sealed class BamlImage : IEquatable<BamlImage>
{
    private readonly MediaPayload payload;

    private BamlImage(MediaPayload payload)
    {
        this.payload = payload;
    }

    public bool IsUrl => payload.IsUrl;

    public static BamlImage FromUrl(
        string url,
        string? mediaType = null) =>
        new(MediaPayload.FromUrl(url, mediaType));

    public static BamlImage FromBytes(
        ReadOnlyMemory<byte> data,
        string mediaType) =>
        new(MediaPayload.FromBytes(data, mediaType));

    public static BamlImage FromBase64(
        string base64,
        string mediaType) =>
        new(MediaPayload.FromBase64(base64, mediaType));

    public static async Task<BamlImage> FromFileAsync(
        string path,
        string mediaType,
        CancellationToken cancellationToken = default) =>
        new(await MediaPayload.FromFileAsync(
                path,
                mediaType,
                cancellationToken)
            .ConfigureAwait(false));

    public bool TryGetUrl(
        [NotNullWhen(true)] out string? url) =>
        payload.TryGetUrl(out url);

    public bool TryGetBytes(
        out ReadOnlyMemory<byte> data,
        [NotNullWhen(true)] out string? mediaType) =>
        payload.TryGetBytes(out data, out mediaType);

    public bool Equals(BamlImage? other) =>
        other is not null && payload.Equals(other.payload);

    public override bool Equals(object? obj) =>
        Equals(obj as BamlImage);

    public override int GetHashCode() => payload.GetHashCode();

    public override string ToString() => $"BamlImage({payload})";
}

public sealed class BamlAudio : IEquatable<BamlAudio>
{
    private readonly MediaPayload payload;

    private BamlAudio(MediaPayload payload)
    {
        this.payload = payload;
    }

    public bool IsUrl => payload.IsUrl;

    public static BamlAudio FromUrl(
        string url,
        string? mediaType = null) =>
        new(MediaPayload.FromUrl(url, mediaType));

    public static BamlAudio FromBytes(
        ReadOnlyMemory<byte> data,
        string mediaType) =>
        new(MediaPayload.FromBytes(data, mediaType));

    public static BamlAudio FromBase64(
        string base64,
        string mediaType) =>
        new(MediaPayload.FromBase64(base64, mediaType));

    public static async Task<BamlAudio> FromFileAsync(
        string path,
        string mediaType,
        CancellationToken cancellationToken = default) =>
        new(await MediaPayload.FromFileAsync(
                path,
                mediaType,
                cancellationToken)
            .ConfigureAwait(false));

    public bool TryGetUrl(
        [NotNullWhen(true)] out string? url) =>
        payload.TryGetUrl(out url);

    public bool TryGetBytes(
        out ReadOnlyMemory<byte> data,
        [NotNullWhen(true)] out string? mediaType) =>
        payload.TryGetBytes(out data, out mediaType);

    public bool Equals(BamlAudio? other) =>
        other is not null && payload.Equals(other.payload);

    public override bool Equals(object? obj) =>
        Equals(obj as BamlAudio);

    public override int GetHashCode() => payload.GetHashCode();

    public override string ToString() => $"BamlAudio({payload})";
}

public sealed class BamlVideo : IEquatable<BamlVideo>
{
    private readonly MediaPayload payload;

    private BamlVideo(MediaPayload payload)
    {
        this.payload = payload;
    }

    public bool IsUrl => payload.IsUrl;

    public static BamlVideo FromUrl(
        string url,
        string? mediaType = null) =>
        new(MediaPayload.FromUrl(url, mediaType));

    public static BamlVideo FromBytes(
        ReadOnlyMemory<byte> data,
        string mediaType) =>
        new(MediaPayload.FromBytes(data, mediaType));

    public static BamlVideo FromBase64(
        string base64,
        string mediaType) =>
        new(MediaPayload.FromBase64(base64, mediaType));

    public static async Task<BamlVideo> FromFileAsync(
        string path,
        string mediaType,
        CancellationToken cancellationToken = default) =>
        new(await MediaPayload.FromFileAsync(
                path,
                mediaType,
                cancellationToken)
            .ConfigureAwait(false));

    public bool TryGetUrl(
        [NotNullWhen(true)] out string? url) =>
        payload.TryGetUrl(out url);

    public bool TryGetBytes(
        out ReadOnlyMemory<byte> data,
        [NotNullWhen(true)] out string? mediaType) =>
        payload.TryGetBytes(out data, out mediaType);

    public bool Equals(BamlVideo? other) =>
        other is not null && payload.Equals(other.payload);

    public override bool Equals(object? obj) =>
        Equals(obj as BamlVideo);

    public override int GetHashCode() => payload.GetHashCode();

    public override string ToString() => $"BamlVideo({payload})";
}

public sealed class BamlPdf : IEquatable<BamlPdf>
{
    private readonly MediaPayload payload;

    private BamlPdf(MediaPayload payload)
    {
        this.payload = payload;
    }

    public bool IsUrl => payload.IsUrl;

    public static BamlPdf FromUrl(
        string url,
        string? mediaType = null) =>
        new(MediaPayload.FromUrl(url, mediaType));

    public static BamlPdf FromBytes(
        ReadOnlyMemory<byte> data,
        string mediaType) =>
        new(MediaPayload.FromBytes(data, mediaType));

    public static BamlPdf FromBase64(
        string base64,
        string mediaType) =>
        new(MediaPayload.FromBase64(base64, mediaType));

    public static async Task<BamlPdf> FromFileAsync(
        string path,
        string mediaType,
        CancellationToken cancellationToken = default) =>
        new(await MediaPayload.FromFileAsync(
                path,
                mediaType,
                cancellationToken)
            .ConfigureAwait(false));

    public bool TryGetUrl(
        [NotNullWhen(true)] out string? url) =>
        payload.TryGetUrl(out url);

    public bool TryGetBytes(
        out ReadOnlyMemory<byte> data,
        [NotNullWhen(true)] out string? mediaType) =>
        payload.TryGetBytes(out data, out mediaType);

    public bool Equals(BamlPdf? other) =>
        other is not null && payload.Equals(other.payload);

    public override bool Equals(object? obj) =>
        Equals(obj as BamlPdf);

    public override int GetHashCode() => payload.GetHashCode();

    public override string ToString() => $"BamlPdf({payload})";
}
