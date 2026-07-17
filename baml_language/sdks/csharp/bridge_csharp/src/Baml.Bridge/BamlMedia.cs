using Baml.Bridge;

namespace Baml;

public abstract class BamlMedia : IDisposable
{
    private NativeHandle? _handle;

    private protected BamlMedia(NativeHandle handle)
    {
        _handle = handle;
    }

    public string? Url => UseHandle(static (key, handleType) => NativeApi.ReadMediaUrl(key, handleType));

    public string? File => UseHandle(static (key, handleType) => NativeApi.ReadMediaFile(key, handleType));

    public string Base64 => UseHandle(static (key, handleType) => NativeApi.ReadMediaBase64(key, handleType));

    public string? MimeType => UseHandle(static (key, handleType) => NativeApi.ReadMediaMimeType(key, handleType));

    internal (ulong Key, int HandleType) CloneForWire()
    {
        var clone = GetHandle().Clone("clone media for BAML argument");
        var key = clone.Key;
        var handleType = clone.HandleType;
        clone.SetHandleAsInvalid();
        clone.Dispose();
        return (key, handleType);
    }

    internal NativeHandle CloneHandle(string operation) => GetHandle().Clone(operation);

    public void Dispose()
    {
        Interlocked.Exchange(ref _handle, null)?.Dispose();
        GC.SuppressFinalize(this);
    }

    private protected static NativeHandle Create(
        NativeMediaKind kind,
        NativeMediaSource source,
        string value,
        string? mimeType) => NativeApi.CreateMedia(kind, source, value, mimeType);

    private TResult UseHandle<TResult>(Func<ulong, int, TResult> operation) => GetHandle().Use(operation);

    private NativeHandle GetHandle() => Volatile.Read(ref _handle)
        ?? throw new ObjectDisposedException(GetType().FullName);
}

public sealed class BamlImage : BamlMedia
{
    private BamlImage(NativeHandle handle)
        : base(handle)
    {
    }

    public static BamlImage FromUrl(string url, string? mimeType = null) =>
        new(Create(NativeMediaKind.Image, NativeMediaSource.Url, url, mimeType));

    public static BamlImage FromFile(string file, string? mimeType = null) =>
        new(Create(NativeMediaKind.Image, NativeMediaSource.File, file, mimeType));

    public static BamlImage FromBase64(string base64, string? mimeType = null) =>
        new(Create(NativeMediaKind.Image, NativeMediaSource.Base64, base64, mimeType));

    public BamlImage Clone() => new(CloneHandle("clone BamlImage"));

    internal static BamlImage FromOwnedHandle(NativeHandle handle) => new(handle);
}

public sealed class BamlAudio : BamlMedia
{
    private BamlAudio(NativeHandle handle)
        : base(handle)
    {
    }

    public static BamlAudio FromUrl(string url, string? mimeType = null) =>
        new(Create(NativeMediaKind.Audio, NativeMediaSource.Url, url, mimeType));

    public static BamlAudio FromFile(string file, string? mimeType = null) =>
        new(Create(NativeMediaKind.Audio, NativeMediaSource.File, file, mimeType));

    public static BamlAudio FromBase64(string base64, string? mimeType = null) =>
        new(Create(NativeMediaKind.Audio, NativeMediaSource.Base64, base64, mimeType));

    public BamlAudio Clone() => new(CloneHandle("clone BamlAudio"));

    internal static BamlAudio FromOwnedHandle(NativeHandle handle) => new(handle);
}

public sealed class BamlVideo : BamlMedia
{
    private BamlVideo(NativeHandle handle)
        : base(handle)
    {
    }

    public static BamlVideo FromUrl(string url, string? mimeType = null) =>
        new(Create(NativeMediaKind.Video, NativeMediaSource.Url, url, mimeType));

    public static BamlVideo FromFile(string file, string? mimeType = null) =>
        new(Create(NativeMediaKind.Video, NativeMediaSource.File, file, mimeType));

    public static BamlVideo FromBase64(string base64, string? mimeType = null) =>
        new(Create(NativeMediaKind.Video, NativeMediaSource.Base64, base64, mimeType));

    public BamlVideo Clone() => new(CloneHandle("clone BamlVideo"));

    internal static BamlVideo FromOwnedHandle(NativeHandle handle) => new(handle);
}

public sealed class BamlPdf : BamlMedia
{
    private BamlPdf(NativeHandle handle)
        : base(handle)
    {
    }

    public static BamlPdf FromUrl(string url, string? mimeType = null) =>
        new(Create(NativeMediaKind.Pdf, NativeMediaSource.Url, url, mimeType));

    public static BamlPdf FromFile(string file, string? mimeType = null) =>
        new(Create(NativeMediaKind.Pdf, NativeMediaSource.File, file, mimeType));

    public static BamlPdf FromBase64(string base64, string? mimeType = null) =>
        new(Create(NativeMediaKind.Pdf, NativeMediaSource.Base64, base64, mimeType));

    public BamlPdf Clone() => new(CloneHandle("clone BamlPdf"));

    internal static BamlPdf FromOwnedHandle(NativeHandle handle) => new(handle);
}
