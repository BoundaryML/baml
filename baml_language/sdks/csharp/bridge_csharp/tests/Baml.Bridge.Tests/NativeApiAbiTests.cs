using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;

namespace Baml.Bridge.Tests;

public sealed unsafe class NativeApiAbiTests
{
    private static int _releaseCount;
    private static NativeBuffer _lastReleased;

    [Fact]
    public void ManagedDeclarationsMatchTheCompleteV1Layout()
    {
        var pointerSize = IntPtr.Size;
        Assert.Equal(2 * pointerSize, sizeof(NativeBuffer));
        Assert.Equal(0, OffsetOf<NativeBuffer>(nameof(NativeBuffer.Pointer)));
        Assert.Equal(pointerSize, OffsetOf<NativeBuffer>(nameof(NativeBuffer.Length)));

        Assert.Equal(4 * pointerSize, sizeof(BridgeInfoV1));
        Assert.Equal(0, OffsetOf<BridgeInfoV1>(nameof(BridgeInfoV1.StructSize)));
        Assert.Equal(pointerSize, OffsetOf<BridgeInfoV1>(nameof(BridgeInfoV1.Language)));
        Assert.Equal(2 * pointerSize, OffsetOf<BridgeInfoV1>(nameof(BridgeInfoV1.SdkVersion)));
        Assert.Equal(3 * pointerSize, OffsetOf<BridgeInfoV1>(nameof(BridgeInfoV1.SdkVersionLength)));

        Assert.Equal(23 * pointerSize, sizeof(ApiV1));
        Assert.Equal(0, OffsetOf<ApiV1>(nameof(ApiV1.AbiVersion)));
        Assert.Equal(pointerSize, OffsetOf<ApiV1>(nameof(ApiV1.StructSize)));

        string[] functionFields =
        [
            nameof(ApiV1.Version),
            nameof(ApiV1.InitializeRuntimeFromBytecode),
            nameof(ApiV1.FreeBuffer),
            nameof(ApiV1.RegisterCallback),
            nameof(ApiV1.CallFunction),
            nameof(ApiV1.NewFunctionCall),
            nameof(ApiV1.CancelFunctionCall),
            nameof(ApiV1.RegisterHostDispatchCallback),
            nameof(ApiV1.RegisterHostReleaseCallback),
            nameof(ApiV1.CompleteHostCall),
            nameof(ApiV1.HandleClone),
            nameof(ApiV1.HandleRelease),
            nameof(ApiV1.MediaFromUrl),
            nameof(ApiV1.MediaFromFile),
            nameof(ApiV1.MediaFromBase64),
            nameof(ApiV1.MediaUrl),
            nameof(ApiV1.MediaFile),
            nameof(ApiV1.MediaBase64),
            nameof(ApiV1.MediaMimeType),
            nameof(ApiV1.RegisterBridge),
            nameof(ApiV1.FlushEvents),
        ];
        for (var index = 0; index < functionFields.Length; index++)
        {
            Assert.Equal((index + 2) * pointerSize, OffsetOf<ApiV1>(functionFields[index]));
        }

        Assert.Equal((nuint)sizeof(ApiV1), NativeApiContract.RequiredSize);
        Assert.True(
            OffsetOf<ApiV1>(nameof(ApiV1.RegisterBridge)) + pointerSize
            < checked((int)NativeApiContract.RequiredSize));
        Assert.Equal(
            checked((int)NativeApiContract.RequiredSize),
            OffsetOf<ApiV1>(nameof(ApiV1.FlushEvents)) + pointerSize);
    }

    [Fact]
    public void ManagedEnumsMatchThePublicCAbi()
    {
        Assert.Equal(4, sizeof(BamlCffiStatus));
        Assert.Equal(0U, (uint)BamlCffiStatus.Ok);
        Assert.Equal(1U, (uint)BamlCffiStatus.InvalidHandle);
        Assert.Equal(2U, (uint)BamlCffiStatus.TypeMismatch);
        Assert.Equal(3U, (uint)BamlCffiStatus.UnsupportedHandleType);
        Assert.Equal(4U, (uint)BamlCffiStatus.InternalError);
        Assert.Equal(5U, (uint)BamlCffiStatus.UnexpectedNullPointer);

        Assert.Equal(4, sizeof(NativeBridgeLanguage));
        Assert.Equal(1U, (uint)NativeBridgeLanguage.NodeJs);
        Assert.Equal(2U, (uint)NativeBridgeLanguage.Python);
        Assert.Equal(3U, (uint)NativeBridgeLanguage.Go);
        Assert.Equal(4U, (uint)NativeBridgeLanguage.Rust);
        Assert.Equal(5U, (uint)NativeBridgeLanguage.CSharp);
        Assert.Equal(6U, (uint)NativeBridgeLanguage.Cpp);

        Assert.Equal(4, sizeof(NativeMediaKind));
        Assert.Equal(0, (int)NativeMediaKind.Unspecified);
        Assert.Equal(1, (int)NativeMediaKind.Image);
        Assert.Equal(2, (int)NativeMediaKind.Audio);
        Assert.Equal(3, (int)NativeMediaKind.Pdf);
        Assert.Equal(4, (int)NativeMediaKind.Video);
        Assert.Equal(5, (int)NativeMediaKind.Generic);

        Assert.Equal(4, sizeof(NativeHandleType));
        Assert.Equal(0, (int)NativeHandleType.Unspecified);
        Assert.Equal(1, (int)NativeHandleType.UntaggedRustData);
        Assert.Equal(2, (int)NativeHandleType.UntaggedBexHeap);
        Assert.Equal(5, (int)NativeHandleType.FunctionRef);
        Assert.Equal(6, (int)NativeHandleType.MediaImage);
        Assert.Equal(7, (int)NativeHandleType.MediaAudio);
        Assert.Equal(8, (int)NativeHandleType.MediaVideo);
        Assert.Equal(9, (int)NativeHandleType.MediaPdf);
        Assert.Equal(10, (int)NativeHandleType.MediaGeneric);
        Assert.Equal(11, (int)NativeHandleType.PromptAst);
        Assert.Equal(12, (int)NativeHandleType.Collector);
        Assert.Equal(13, (int)NativeHandleType.Type);
        Assert.Equal(14, (int)NativeHandleType.TaggedHeapHandle);
        Assert.Equal(15, (int)NativeHandleType.HostValueCallable);
        Assert.Equal(16, (int)NativeHandleType.HostValueOpaque);
    }

    [Fact]
    public void ApiCompatibilityRequiresTheCSharpExtensionAndAcceptsFutureTails()
    {
        var api = CompleteApi((nuint)sizeof(ApiV1) + 64);
        ValidateApi(api);

        api.StructSize = (nuint)OffsetOf<ApiV1>(nameof(ApiV1.FlushEvents));
        var truncated = Assert.Throws<BamlBridgeException>(() => ValidateApi(api));
        Assert.Contains("truncated", truncated.Message, StringComparison.OrdinalIgnoreCase);

        api = CompleteApi((nuint)sizeof(ApiV1));
        api.FlushEvents = null;
        var missing = Assert.Throws<BamlBridgeException>(() => ValidateApi(api));
        Assert.Contains("function pointers", missing.Message, StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public void EveryManagedFunctionPointerUsesTheCCallingConvention()
    {
        var api = default(ApiV1);
        delegate* unmanaged[Cdecl]<NativeBuffer> version = api.Version;
        delegate* unmanaged[Cdecl]<byte*, nuint, NativeBuffer> initialize = api.InitializeRuntimeFromBytecode;
        delegate* unmanaged[Cdecl]<NativeBuffer, void> free = api.FreeBuffer;
        delegate* unmanaged[Cdecl]<delegate* unmanaged[Cdecl]<uint, sbyte*, nuint, void>, void> register = api.RegisterCallback;
        delegate* unmanaged[Cdecl]<byte*, byte*, nuint, uint, void> call = api.CallFunction;
        delegate* unmanaged[Cdecl]<ulong> newCall = api.NewFunctionCall;
        delegate* unmanaged[Cdecl]<ulong, int> cancel = api.CancelFunctionCall;
        delegate* unmanaged[Cdecl]<delegate* unmanaged[Cdecl]<ulong, uint, byte*, nuint, void>, void> dispatch = api.RegisterHostDispatchCallback;
        delegate* unmanaged[Cdecl]<delegate* unmanaged[Cdecl]<ulong, void>, void> release = api.RegisterHostReleaseCallback;
        delegate* unmanaged[Cdecl]<uint, int, sbyte*, nuint, void> complete = api.CompleteHostCall;
        delegate* unmanaged[Cdecl]<ulong, ulong*, BamlCffiStatus> clone = api.HandleClone;
        delegate* unmanaged[Cdecl]<ulong, BamlCffiStatus> releaseHandle = api.HandleRelease;
        delegate* unmanaged[Cdecl]<int, byte*, byte*, ulong*, int*, BamlCffiStatus> media = api.MediaFromUrl;
        delegate* unmanaged[Cdecl]<ulong, int, NativeBuffer*, BamlCffiStatus> mediaRead = api.MediaUrl;
        delegate* unmanaged[Cdecl]<BridgeInfoV1*, NativeBuffer> bridge = api.RegisterBridge;
        delegate* unmanaged[Cdecl]<void> flush = api.FlushEvents;

        Assert.True(
            version == null && initialize == null && free == null && register == null
            && call == null && newCall == null && cancel == null && dispatch == null
            && release == null && complete == null && clone == null && releaseHandle == null
            && media == null && mediaRead == null && bridge == null && flush == null);
    }

    [Fact]
    public void OwnedBuffersAreReleasedExactlyOnceForEveryPointerShape()
    {
        AssertBufferRelease(new NativeBuffer { Pointer = null, Length = 0 }, []);

        sbyte zeroLengthStorage = 42;
        AssertBufferRelease(
            new NativeBuffer { Pointer = &zeroLengthStorage, Length = 0 },
            []);

        var content = stackalloc sbyte[] { 65, 66, 67 };
        AssertBufferRelease(
            new NativeBuffer { Pointer = content, Length = 3 },
            "ABC"u8.ToArray());
    }

    [Fact]
    public void OptionalMediaAbsenceUsesLengthAndStillReleases()
    {
        ResetReleases();
        var fromNull = NativeBufferMarshaller.ReadUtf8AndFree(
            new NativeBuffer { Pointer = null, Length = 0 },
            optional: true,
            &RecordRelease);
        Assert.Null(fromNull);
        Assert.Equal(1, _releaseCount);

        ResetReleases();
        sbyte zeroLengthStorage = 42;
        var fromNonNull = NativeBufferMarshaller.ReadUtf8AndFree(
            new NativeBuffer { Pointer = &zeroLengthStorage, Length = 0 },
            optional: true,
            &RecordRelease);
        Assert.Null(fromNonNull);
        Assert.Equal(1, _releaseCount);

        ResetReleases();
        var required = NativeBufferMarshaller.ReadUtf8AndFree(
            new NativeBuffer { Pointer = null, Length = 0 },
            optional: false,
            &RecordRelease);
        Assert.Equal(string.Empty, required);
        Assert.Equal(1, _releaseCount);
    }

    [Fact]
    public void InvalidOwnedBufferIsStillReleasedExactlyOnce()
    {
        ResetReleases();
        var buffer = new NativeBuffer { Pointer = null, Length = 1 };
        Assert.Throws<BamlBridgeException>(
            () => NativeBufferMarshaller.CopyAndFree(buffer, &RecordRelease));
        Assert.Equal(1, _releaseCount);
    }

    [Fact]
    public void RuntimePathAliasesMustAgree()
    {
        var values = new Dictionary<string, string?>
        {
            [NativeLibraryOverride.CanonicalVariable] = "/runtime/current",
            [NativeLibraryOverride.LibraryCompatibilityVariable] = "/runtime/current",
            [NativeLibraryOverride.CSharpCompatibilityVariable] = null,
        };
        var resolved = NativeLibraryOverride.Resolve(name => values[name]);
        Assert.Equal(NativeLibraryOverride.CanonicalVariable, resolved?.Variable);
        Assert.Equal("/runtime/current", resolved?.Path);

        values[NativeLibraryOverride.CSharpCompatibilityVariable] = "/runtime/other";
        var conflict = Assert.Throws<BamlBridgeException>(
            () => NativeLibraryOverride.Resolve(name => values[name]));
        Assert.Contains(NativeLibraryOverride.CanonicalVariable, conflict.Message, StringComparison.Ordinal);
        Assert.Contains(NativeLibraryOverride.CSharpCompatibilityVariable, conflict.Message, StringComparison.Ordinal);
    }

    private static void AssertBufferRelease(NativeBuffer buffer, byte[] expected)
    {
        ResetReleases();
        var actual = NativeBufferMarshaller.CopyAndFree(buffer, &RecordRelease);
        Assert.Equal(expected, actual);
        Assert.Equal(1, _releaseCount);
        Assert.Equal((nint)buffer.Pointer, (nint)_lastReleased.Pointer);
        Assert.Equal(buffer.Length, _lastReleased.Length);
    }

    private static ApiV1 CompleteApi(nuint size)
    {
        var address = (void*)1;
        return new ApiV1
        {
            AbiVersion = NativeApiContract.ExpectedAbiVersion,
            StructSize = size,
            Version = (delegate* unmanaged[Cdecl]<NativeBuffer>)address,
            InitializeRuntimeFromBytecode = (delegate* unmanaged[Cdecl]<byte*, nuint, NativeBuffer>)address,
            FreeBuffer = (delegate* unmanaged[Cdecl]<NativeBuffer, void>)address,
            RegisterCallback = (delegate* unmanaged[Cdecl]<delegate* unmanaged[Cdecl]<uint, sbyte*, nuint, void>, void>)address,
            CallFunction = (delegate* unmanaged[Cdecl]<byte*, byte*, nuint, uint, void>)address,
            NewFunctionCall = (delegate* unmanaged[Cdecl]<ulong>)address,
            CancelFunctionCall = (delegate* unmanaged[Cdecl]<ulong, int>)address,
            RegisterHostDispatchCallback = (delegate* unmanaged[Cdecl]<delegate* unmanaged[Cdecl]<ulong, uint, byte*, nuint, void>, void>)address,
            RegisterHostReleaseCallback = (delegate* unmanaged[Cdecl]<delegate* unmanaged[Cdecl]<ulong, void>, void>)address,
            CompleteHostCall = (delegate* unmanaged[Cdecl]<uint, int, sbyte*, nuint, void>)address,
            HandleClone = (delegate* unmanaged[Cdecl]<ulong, ulong*, BamlCffiStatus>)address,
            HandleRelease = (delegate* unmanaged[Cdecl]<ulong, BamlCffiStatus>)address,
            MediaFromUrl = (delegate* unmanaged[Cdecl]<int, byte*, byte*, ulong*, int*, BamlCffiStatus>)address,
            MediaFromFile = (delegate* unmanaged[Cdecl]<int, byte*, byte*, ulong*, int*, BamlCffiStatus>)address,
            MediaFromBase64 = (delegate* unmanaged[Cdecl]<int, byte*, byte*, ulong*, int*, BamlCffiStatus>)address,
            MediaUrl = (delegate* unmanaged[Cdecl]<ulong, int, NativeBuffer*, BamlCffiStatus>)address,
            MediaFile = (delegate* unmanaged[Cdecl]<ulong, int, NativeBuffer*, BamlCffiStatus>)address,
            MediaBase64 = (delegate* unmanaged[Cdecl]<ulong, int, NativeBuffer*, BamlCffiStatus>)address,
            MediaMimeType = (delegate* unmanaged[Cdecl]<ulong, int, NativeBuffer*, BamlCffiStatus>)address,
            RegisterBridge = (delegate* unmanaged[Cdecl]<BridgeInfoV1*, NativeBuffer>)address,
            FlushEvents = (delegate* unmanaged[Cdecl]<void>)address,
        };
    }

    private static int OffsetOf<T>(string field)
        where T : struct => Marshal.OffsetOf<T>(field).ToInt32();

    private static void ValidateApi(ApiV1 api) => NativeApiContract.Validate(&api);

    private static void ResetReleases()
    {
        _releaseCount = 0;
        _lastReleased = default;
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void RecordRelease(NativeBuffer buffer)
    {
        _lastReleased = buffer;
        _releaseCount++;
    }
}
