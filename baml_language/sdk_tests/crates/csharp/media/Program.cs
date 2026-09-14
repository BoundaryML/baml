using Baml;
using CsharpMedia;

const string Url = "https://example.com/media/%E9%9B%AA?token=fixture";

RequireImageUrl(Functions.ReturnImage(Url, "image/png"), Url, "image/png");
RequireAudioUrl(await Functions.ReturnAudioAsync(Url, "audio/wav"), Url, "audio/wav");
RequireVideoUrl(Functions.ReturnVideo(Url, "video/mp4"), Url, "video/mp4");
RequirePdfUrl(await Functions.ReturnPdfAsync(Url, "application/pdf"), Url, "application/pdf");

byte[] original = [0x00, 0x01, 0x02, 0xff, 0x7f, 0x80];
BamlImage image = BamlImage.FromBytes(original, "image/png");
BamlAudio audio = BamlAudio.FromBytes(original, "audio/wav");
BamlVideo video = BamlVideo.FromBase64(Convert.ToBase64String(original), "video/mp4");
BamlPdf pdf = BamlPdf.FromBytes(original);
original[0] = 0x42;

RequireImageBytes(Functions.RoundTripImage(image), [0x00, 0x01, 0x02, 0xff, 0x7f, 0x80], "image/png");
RequireAudioBytes(await Functions.RoundTripAudioAsync(audio), [0x00, 0x01, 0x02, 0xff, 0x7f, 0x80], "audio/wav");
RequireVideoBytes(Functions.RoundTripVideo(video), [0x00, 0x01, 0x02, 0xff, 0x7f, 0x80], "video/mp4");
RequirePdfBytes(await Functions.RoundTripPdfAsync(pdf), [0x00, 0x01, 0x02, 0xff, 0x7f, 0x80], "application/pdf");

var dynamicPayload = new DynamicPayload { Name = "typed", Count = 7 };
DynamicPayload typedPayload = Functions.RoundTripPayload(dynamicPayload);
if (typedPayload.Name != "typed" || typedPayload.Count != 7)
{
    throw new InvalidOperationException("typed dynamic payload setup changed");
}

BamlValue dynamicValue = BamlValue.From(dynamicPayload);
BamlValue returnedDynamic = await Functions.RoundTripUnknownAsync(dynamicValue);
DynamicPayload restoredDynamic = returnedDynamic.As<DynamicPayload>();
if (restoredDynamic.Name != "typed" || restoredDynamic.Count != 7)
{
    throw new InvalidOperationException("registered dynamic nominal round trip changed");
}

Console.WriteLine("csharp_media=ok");

static void RequireImageUrl(BamlImage value, string url, string mediaType) =>
    RequireUrlCore(value.TryGetUrl, value.TryGetBytes, url, mediaType);

static void RequireAudioUrl(BamlAudio value, string url, string mediaType) =>
    RequireUrlCore(value.TryGetUrl, value.TryGetBytes, url, mediaType);

static void RequireVideoUrl(BamlVideo value, string url, string mediaType) =>
    RequireUrlCore(value.TryGetUrl, value.TryGetBytes, url, mediaType);

static void RequirePdfUrl(BamlPdf value, string url, string mediaType) =>
    RequireUrlCore(value.TryGetUrl, value.TryGetBytes, url, mediaType);

static void RequireUrlCore(
    TryGetUrl tryGetUrl,
    TryGetBytes tryGetBytes,
    string expectedUrl,
    string expectedMediaType)
{
    if (!tryGetUrl(out string? actualUrl)
        || actualUrl != expectedUrl
        || tryGetBytes(out _, out _))
    {
        throw new InvalidOperationException(
            $"URL media round trip changed for {expectedMediaType}");
    }
}

static void RequireImageBytes(BamlImage value, byte[] bytes, string mediaType) =>
    RequireBytesCore(value.TryGetBytes, bytes, mediaType);

static void RequireAudioBytes(BamlAudio value, byte[] bytes, string mediaType) =>
    RequireBytesCore(value.TryGetBytes, bytes, mediaType);

static void RequireVideoBytes(BamlVideo value, byte[] bytes, string mediaType) =>
    RequireBytesCore(value.TryGetBytes, bytes, mediaType);

static void RequirePdfBytes(BamlPdf value, byte[] bytes, string mediaType) =>
    RequireBytesCore(value.TryGetBytes, bytes, mediaType);

static void RequireBytesCore(
    TryGetBytes tryGetBytes,
    byte[] expected,
    string expectedMediaType)
{
    if (!tryGetBytes(out ReadOnlyMemory<byte> actual, out string? actualMediaType)
        || !actual.Span.SequenceEqual(expected)
        || actualMediaType != expectedMediaType)
    {
        throw new InvalidOperationException(
            $"byte media round trip changed for {expectedMediaType}");
    }
}

delegate bool TryGetUrl(out string? url);
delegate bool TryGetBytes(out ReadOnlyMemory<byte> data, out string? mediaType);
