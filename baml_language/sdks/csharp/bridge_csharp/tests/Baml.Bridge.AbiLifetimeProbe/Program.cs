using System.Collections.Concurrent;
using System.Runtime.CompilerServices;
using System.Runtime.ExceptionServices;
using System.Runtime.InteropServices;
using System.Text;

internal static unsafe partial class Program
{
    private const string NativeLibraryName = "bridge_cffi";
    private const uint BamlApiV1AbiVersion = 2;
    private const uint BamlBridgeLanguageCSharp = 5;
    private const uint BamlBridgeLanguageRust = 4;
    private const uint StatusOk = 0;
    private const uint StatusInvalidHandle = 1;
    private const uint StatusTypeMismatch = 2;
    private const uint StatusUnsupportedHandleType = 3;
    private const uint StatusInternalError = 4;
    private const uint StatusUnexpectedNullPointer = 5;
    private const int MediaKindImage = 1;
    private const int HandleTypeMediaImage = 6;
    private const int HandleTypeMediaAudio = 7;

    private static readonly nuint RequiredV1PrefixSize = ApiFieldEnd(
        nameof(BamlApiV1.RegisterBridge));

    private static readonly ConcurrentDictionary<
        uint,
        TaskCompletionSource<byte[]>> PendingCalls = new();

    private static int _nextCallbackId;
    private static int _borrowedResultCopies;
    private static int _releasedBuffers;
    private static int _lateOrDuplicateResults;
    private static ExceptionDispatchInfo? _callbackFailure;

    public static int Main(string[] args)
    {
        if (args.Length != 3)
        {
            Console.Error.WriteLine(
                "usage: Baml.Bridge.AbiLifetimeProbe <absolute-native-library-path|package-default> <expected-product-version> <bytecode-path>");
            return 2;
        }

        string? nativeLibraryPath = StringComparer.Ordinal.Equals(
                args[0],
                "package-default")
            ? null
            : RequireExistingAbsoluteFile(args[0], "native library");
        string bytecodePath = RequireExistingAbsoluteFile(args[2], "bytecode");

        if (nativeLibraryPath is not null)
        {
            NativeLibrary.SetDllImportResolver(
                typeof(Program).Assembly,
                (libraryName, assembly, searchPath) =>
                {
                    if (!StringComparer.Ordinal.Equals(
                            libraryName,
                            NativeLibraryName))
                    {
                        return IntPtr.Zero;
                    }

                    return NativeLibrary.Load(
                        nativeLibraryPath,
                        assembly,
                        searchPath);
                });
        }

        BamlApiV1* api = NativeMethods.GetApiV1();
        Require(api is not null, "baml_get_api_v1 returned null");
        Require(
            api->AbiVersion == BamlApiV1AbiVersion,
            $"unexpected ABI version {api->AbiVersion}");
        Require(
            api->StructSize >= RequiredV1PrefixSize,
            $"truncated BamlApiV1 prefix: {api->StructSize} < {RequiredV1PrefixSize}");
        ValidateRequiredFunctions(api);

        string nativeVersion = ConsumeUtf8Buffer(api, api->Version());
        Require(
            StringComparer.Ordinal.Equals(nativeVersion, args[1]),
            $"product version mismatch: native={nativeVersion}, expected={args[1]}");

        string wrongVersionDiagnostic = RegisterBridge(
            api,
            BamlBridgeLanguageCSharp,
            $"{args[1]}-wrong");
        Require(
            wrongVersionDiagnostic.Length > 0,
            "wrong bridge product version was accepted");
        Require(
            RegisterBridge(api, BamlBridgeLanguageCSharp, args[1]).Length == 0,
            "matching C# bridge registration failed");
        Require(
            RegisterBridge(api, BamlBridgeLanguageCSharp, args[1]).Length == 0,
            "identical C# bridge registration was not idempotent");
        Require(
            RegisterBridge(api, BamlBridgeLanguageRust, args[1]).Length > 0,
            "conflicting bridge language registration was accepted");

        string emptyBytecodeDiagnostic = ConsumeUtf8Buffer(
            api,
            api->InitializeRuntimeFromBytecode(null, 0));
        Require(
            emptyBytecodeDiagnostic.Length > 0,
            "empty bytecode unexpectedly initialized the runtime");

        byte[] invalidBytecode = [0xff, 0x00, 0x7f, 0x80];
        fixed (byte* invalid = invalidBytecode)
        {
            string diagnostic = ConsumeUtf8Buffer(
                api,
                api->InitializeRuntimeFromBytecode(
                    invalid,
                    (nuint)invalidBytecode.Length));
            Require(
                diagnostic.Length > 0,
                "invalid bytecode unexpectedly initialized the runtime");
        }

        byte[] bytecode = File.ReadAllBytes(bytecodePath);
        fixed (byte* bytes = bytecode)
        {
            string diagnostic = ConsumeUtf8Buffer(
                api,
                api->InitializeRuntimeFromBytecode(
                    bytes,
                    (nuint)bytecode.Length));
            Require(
                diagnostic.Length == 0,
                $"valid bytecode initialization failed: {diagnostic}");
        }

        api->RegisterCallback(&OnResult);
        api->RegisterHostDispatchCallback(&OnHostDispatch);
        api->RegisterHostReleaseCallback(&OnHostRelease);

        VerifyCallIdentifiers(api);

        ulong helloCallId = api->NewFunctionCall();
        Require(helloCallId != 0, "new_function_call returned zero");
        byte[] helloResult = CallFunction(
            api,
            "hello_world",
            EncodeCallArguments(helloCallId, null, null));
        Require(
            StringComparer.Ordinal.Equals(
                DecodeSuccessfulString(helloResult),
                "hello world"),
            "hello_world returned an unexpected value");
        Require(
            api->CancelFunctionCall(helloCallId) == 0,
            "completed call ID was not reserved against reuse");

        const string utf8Boundary = "héllo\0雪\u0001";
        ulong stringCallId = api->NewFunctionCall();
        byte[] stringResult = CallFunction(
            api,
            "round_trip_string",
            EncodeCallArguments(stringCallId, "value", utf8Boundary));
        Require(
            StringComparer.Ordinal.Equals(
                DecodeSuccessfulString(stringResult),
                utf8Boundary),
            "UTF-8/NUL string changed across the ABI");

        ulong missingCallId = api->NewFunctionCall();
        byte[] missingResult = CallFunction(
            api,
            "function_that_does_not_exist",
            EncodeCallArguments(missingCallId, null, null));
        RequireNonSuccess(
            missingResult,
            "unknown function did not return a structured failure");
        Expect<InvalidDataException>(
            () => _ = DecodeSuccessfulString([0xff]));

        VerifyCancellation(api);
        VerifyMediaAndHandleOwnership(api);

        api->CompleteHostCall(uint.MaxValue, 0, null, 0);
        VerifySyntheticCallbackContainment(helloResult);
        _callbackFailure?.Throw();
        Require(PendingCalls.IsEmpty, "pending result registry was not drained");
        Require(
            Volatile.Read(ref _lateOrDuplicateResults) == 1,
            "synthetic duplicate result was not contained");
        Require(
            Volatile.Read(ref _borrowedResultCopies) == 6,
            "result callback did not copy every borrowed payload");
        Require(
            Volatile.Read(ref _releasedBuffers) == 15,
            $"expected exactly 15 released native buffers, received {_releasedBuffers}");

        Console.WriteLine($"api_v1_size={api->StructSize}");
        Console.WriteLine($"product_version={nativeVersion}");
        Console.WriteLine($"bytecode_bytes={bytecode.Length}");
        Console.WriteLine(
            $"hello_result_wire={Convert.ToHexString(helloResult).ToLowerInvariant()}");
        Console.WriteLine("registration_version_conflict=fail_closed");
        Console.WriteLine("bytecode_invalid_then_valid=ok");
        Console.WriteLine("ordinary_calls=2/2");
        Console.WriteLine("utf8_binary_boundary=ok");
        Console.WriteLine("structured_error_and_decode_failure=ok");
        Console.WriteLine("pre_and_inflight_cancellation=ok");
        Console.WriteLine("media_handle_clone_release=ok");
        Console.WriteLine($"owned_buffers_released={_releasedBuffers}");
        Console.WriteLine("callback_boundary=contained");
        Console.WriteLine("host_callback_abi=v1");
        return 0;
    }

    private static void VerifySyntheticCallbackContainment(byte[] result)
    {
        delegate* unmanaged[Cdecl]<
            uint,
            byte*,
            nuint,
            void> callback = &OnResult;
        fixed (byte* content = result)
        {
            callback(1, content, (nuint)result.Length);
        }

        Require(
            Volatile.Read(ref _lateOrDuplicateResults) == 1,
            "duplicate callback was not classified as late");

        callback(uint.MaxValue, null, 1);
        ExceptionDispatchInfo? captured = Interlocked.Exchange(
            ref _callbackFailure,
            null);
        Require(
            captured?.SourceException is InvalidDataException,
            "callback exception was not captured inside the unmanaged boundary");
    }

    private static void VerifyCancellation(BamlApiV1* api)
    {
        ulong preCancelledId = api->NewFunctionCall();
        Require(preCancelledId != 0, "pre-cancel call ID allocation failed");
        Require(
            api->CancelFunctionCall(preCancelledId) == 0,
            "pre-registration cancellation was rejected");
        byte[] preCancelled = CallFunction(
            api,
            "slow_cancel_probe",
            EncodeCallArguments(preCancelledId, null, null));
        RequireNonSuccess(
            preCancelled,
            "pre-cancelled call returned success");

        ulong inFlightId = api->NewFunctionCall();
        Require(inFlightId != 0, "in-flight call ID allocation failed");
        Task<byte[]> inFlight = DispatchFunction(
            api,
            "slow_cancel_probe",
            EncodeCallArguments(inFlightId, null, null));
        Thread.Sleep(TimeSpan.FromMilliseconds(100));
        var started = System.Diagnostics.Stopwatch.StartNew();
        Require(
            api->CancelFunctionCall(inFlightId) == 0,
            "in-flight cancellation was rejected");
        byte[] cancelled = inFlight
            .WaitAsync(TimeSpan.FromSeconds(3))
            .GetAwaiter()
            .GetResult();
        started.Stop();
        RequireNonSuccess(
            cancelled,
            "in-flight cancelled call returned success");
        Require(
            started.Elapsed < TimeSpan.FromSeconds(2),
            $"in-flight cancellation completed too slowly: {started.Elapsed}");
    }

    private static string RequireExistingAbsoluteFile(
        string value,
        string description)
    {
        if (!Path.IsPathFullyQualified(value))
        {
            throw new FileNotFoundException(
                $"{description} does not exist at an absolute path",
                value);
        }

        string path = Path.GetFullPath(value);
        if (!File.Exists(path))
        {
            throw new FileNotFoundException(
                $"{description} does not exist at an absolute path",
                path);
        }

        return path;
    }

    private static string RegisterBridge(
        BamlApiV1* api,
        uint language,
        string version)
    {
        byte[] encodedVersion = Encoding.UTF8.GetBytes(version);
        fixed (byte* encoded = encodedVersion)
        {
            BamlBridgeInfoV1 info = new()
            {
                StructSize = (nuint)sizeof(BamlBridgeInfoV1),
                Language = language,
                SdkVersion = encoded,
                SdkVersionLength = (nuint)encodedVersion.Length,
            };
            return ConsumeUtf8Buffer(api, api->RegisterBridge(&info));
        }
    }

    private static void VerifyCallIdentifiers(BamlApiV1* api)
    {
        Require(
            api->CancelFunctionCall(0) == 1,
            "zero call ID was accepted for cancellation");
        Require(
            api->CancelFunctionCall(ulong.MaxValue) == 0,
            "nonzero pre-registration cancellation was rejected");

        var identifiers = new HashSet<ulong>();
        for (int index = 0; index < 1_024; index++)
        {
            ulong identifier = api->NewFunctionCall();
            Require(identifier != 0, "new_function_call returned zero");
            Require(
                identifiers.Add(identifier),
                $"new_function_call repeated {identifier}");
        }
    }

    private static byte[] CallFunction(
        BamlApiV1* api,
        string functionName,
        byte[] encodedArguments)
        => DispatchFunction(api, functionName, encodedArguments)
            .WaitAsync(TimeSpan.FromSeconds(15))
            .GetAwaiter()
            .GetResult();

    private static Task<byte[]> DispatchFunction(
        BamlApiV1* api,
        string functionName,
        byte[] encodedArguments)
    {
        uint callbackId = checked((uint)Interlocked.Increment(
            ref _nextCallbackId));
        var completion = new TaskCompletionSource<byte[]>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        Require(
            PendingCalls.TryAdd(callbackId, completion),
            $"duplicate managed callback ID {callbackId}");

        var encodedCall = new List<byte>(encodedArguments);
        WriteLengthDelimited(
            encodedCall,
            4,
            Encoding.UTF8.GetBytes(functionName));
        byte[] encodedCallBytes = [.. encodedCall];
        fixed (byte* arguments = encodedCallBytes)
        {
            api->CallFunction(
                arguments,
                (nuint)encodedCallBytes.Length,
                callbackId);
        }

        return completion.Task;
    }

    private static byte[] EncodeCallArguments(
        ulong callId,
        string? argumentName,
        string? argumentValue)
    {
        var output = new List<byte>();
        if (argumentName is not null)
        {
            string suppliedValue = argumentValue
                ?? throw new InvalidOperationException(
                    "a named string argument requires a value");
            var inbound = new List<byte>();
            WriteLengthDelimited(
                inbound,
                2,
                Encoding.UTF8.GetBytes(suppliedValue));

            var entry = new List<byte>();
            WriteLengthDelimited(
                entry,
                1,
                Encoding.UTF8.GetBytes(argumentName));
            WriteLengthDelimited(entry, 6, inbound);
            WriteLengthDelimited(output, 1, entry);
        }

        WriteTag(output, 2, 0);
        WriteVarint(output, callId);
        return [.. output];
    }

    private static string DecodeSuccessfulString(byte[] encodedResult)
    {
        ReadOnlySpan<byte> outer = encodedResult;
        if (!TryGetLengthDelimited(outer, 1, out ReadOnlySpan<byte> value))
        {
            string caseName = TryGetLengthDelimited(
                outer,
                2,
                out _)
                ? "error"
                : TryGetLengthDelimited(outer, 3, out _)
                    ? "panic"
                    : "missing";
            throw new InvalidDataException(
                $"expected successful BamlOutboundResult, received {caseName}: {Convert.ToHexString(encodedResult)}");
        }

        if (TryGetLengthDelimited(value, 3, out ReadOnlySpan<byte> plain))
        {
            return DecodeUtf8(plain, "string result");
        }

        if (TryGetLengthDelimited(value, 9, out ReadOnlySpan<byte> literal)
            && TryGetLengthDelimited(
                literal,
                1,
                out ReadOnlySpan<byte> literalString))
        {
            return DecodeUtf8(literalString, "string literal result");
        }

        throw new InvalidDataException(
            $"successful result is not a string or string literal: {Convert.ToHexString(encodedResult)}");
    }

    private static void RequireNonSuccess(
        byte[] encodedResult,
        string message)
    {
        ReadOnlySpan<byte> outer = encodedResult;
        Require(
            !TryGetLengthDelimited(outer, 1, out _),
            message);
        Require(
            TryGetLengthDelimited(outer, 2, out _)
                || TryGetLengthDelimited(outer, 3, out _),
            $"failure result had no error or panic case: {Convert.ToHexString(encodedResult)}");
    }

    private static string DecodeUtf8(
        ReadOnlySpan<byte> bytes,
        string description)
    {
        try
        {
            return new UTF8Encoding(
                encoderShouldEmitUTF8Identifier: false,
                throwOnInvalidBytes: true).GetString(bytes);
        }
        catch (DecoderFallbackException error)
        {
            throw new InvalidDataException(
                $"{description} is not valid UTF-8",
                error);
        }
    }

    private static bool TryGetLengthDelimited(
        ReadOnlySpan<byte> message,
        int wantedField,
        out ReadOnlySpan<byte> value)
    {
        int offset = 0;
        while (offset < message.Length)
        {
            ulong tag = ReadVarint(message, ref offset);
            int field = checked((int)(tag >> 3));
            int wireType = checked((int)(tag & 7));
            if (wireType == 2)
            {
                int length = checked((int)ReadVarint(message, ref offset));
                RequireProtocol(
                    length >= 0 && offset <= message.Length - length,
                    "truncated length-delimited protobuf field");
                ReadOnlySpan<byte> candidate = message.Slice(offset, length);
                offset += length;
                if (field == wantedField)
                {
                    value = candidate;
                    return true;
                }

                continue;
            }

            SkipField(message, ref offset, wireType);
        }

        value = default;
        return false;
    }

    private static ulong ReadVarint(
        ReadOnlySpan<byte> message,
        ref int offset)
    {
        ulong result = 0;
        for (int shift = 0; shift < 70; shift += 7)
        {
            RequireProtocol(
                offset < message.Length,
                "truncated protobuf varint");
            byte current = message[offset++];
            result |= (ulong)(current & 0x7f) << shift;
            if ((current & 0x80) == 0)
            {
                return result;
            }
        }

        throw new InvalidDataException("protobuf varint exceeds 10 bytes");
    }

    private static void SkipField(
        ReadOnlySpan<byte> message,
        ref int offset,
        int wireType)
    {
        switch (wireType)
        {
            case 0:
                _ = ReadVarint(message, ref offset);
                break;
            case 1:
                RequireProtocol(
                    offset <= message.Length - 8,
                    "truncated fixed64 protobuf field");
                offset += 8;
                break;
            case 5:
                RequireProtocol(
                    offset <= message.Length - 4,
                    "truncated fixed32 protobuf field");
                offset += 4;
                break;
            default:
                throw new InvalidDataException(
                    $"unsupported protobuf wire type {wireType}");
        }
    }

    private static void WriteLengthDelimited(
        List<byte> output,
        int field,
        IReadOnlyCollection<byte> bytes)
    {
        WriteTag(output, field, 2);
        WriteVarint(output, checked((ulong)bytes.Count));
        output.AddRange(bytes);
    }

    private static void WriteTag(
        List<byte> output,
        int field,
        int wireType) =>
        WriteVarint(
            output,
            checked(((ulong)field << 3) | (uint)wireType));

    private static void WriteVarint(List<byte> output, ulong value)
    {
        while (value >= 0x80)
        {
            output.Add((byte)(value | 0x80));
            value >>= 7;
        }

        output.Add((byte)value);
    }

    private static void VerifyMediaAndHandleOwnership(BamlApiV1* api)
    {
        byte[] invalidUtf8 = [0xff, 0x00];
        ulong invalidKey = 0;
        int invalidType = 0;
        fixed (byte* invalid = invalidUtf8)
        {
            Require(
                api->MediaFromUrl(
                    MediaKindImage,
                    invalid,
                    null,
                    &invalidKey,
                    &invalidType)
                    == StatusInternalError,
                "invalid UTF-8 media URL did not fail");
        }

        byte[] validUrl = NullTerminatedUtf8("https://example.com/image.png");
        fixed (byte* url = validUrl)
        {
            Require(
                api->MediaFromUrl(
                    0,
                    url,
                    null,
                    &invalidKey,
                    &invalidType)
                    == StatusUnsupportedHandleType,
                "unsupported media kind did not fail");
            Require(
                api->MediaFromUrl(
                    MediaKindImage,
                    url,
                    null,
                    null,
                    &invalidType)
                    == StatusUnexpectedNullPointer,
                "null media output pointer did not fail");
        }

        const string expectedUrl = "https://example.com/雪.png";
        const string expectedMimeType = "image/png";
        ulong original = CreateMedia(
            api->MediaFromUrl,
            MediaKindImage,
            expectedUrl,
            expectedMimeType,
            out int handleType);
        ulong clone = 0;
        try
        {
            Require(
                handleType == HandleTypeMediaImage,
                $"unexpected media handle type {handleType}");
            Require(
                ReadMedia(
                    api,
                    api->MediaUrl,
                    original,
                    handleType)
                    == expectedUrl,
                "media URL changed");
            Require(
                ReadMedia(
                    api,
                    api->MediaMimeType,
                    original,
                    handleType)
                    == expectedMimeType,
                "media MIME type changed");
            Require(
                ReadMedia(
                    api,
                    api->MediaFile,
                    original,
                    handleType).Length == 0,
                "URL media unexpectedly had a file representation");
            Require(
                api->MediaUrl(
                    original,
                    HandleTypeMediaAudio,
                    null)
                    == StatusUnexpectedNullPointer,
                "null accessor output pointer did not fail first");

            BamlBuffer mismatchOutput = default;
            Require(
                api->MediaUrl(
                    original,
                    HandleTypeMediaAudio,
                    &mismatchOutput)
                    == StatusTypeMismatch,
                "wrong media handle type did not fail");

            Require(
                api->HandleClone(original, &clone) == StatusOk,
                "media handle clone failed");
            Require(clone != 0 && clone != original, "clone key was not distinct");
            Require(
                ReadMedia(api, api->MediaUrl, clone, 0) == expectedUrl,
                "cloned media handle did not retain the URL");

            Require(
                api->HandleRelease(original) == StatusOk,
                "original media handle release failed");
            original = 0;
            Require(
                api->HandleRelease(original) == StatusInvalidHandle,
                "zero media handle release was accepted");
            Require(
                ReadMedia(api, api->MediaUrl, clone, 0) == expectedUrl,
                "clone did not outlive original release");
        }
        finally
        {
            if (original != 0)
            {
                Require(
                    api->HandleRelease(original) == StatusOk,
                    "fallback original media release failed");
            }

            if (clone != 0)
            {
                Require(
                    api->HandleRelease(clone) == StatusOk,
                    "cloned media handle release failed");
            }
        }

        BamlBuffer releasedOutput = default;
        Require(
            api->MediaUrl(clone, 0, &releasedOutput)
                == StatusInvalidHandle,
            "released cloned handle remained readable");
        Require(
            api->HandleClone(clone, &original) == StatusInvalidHandle,
            "released cloned handle remained cloneable");

        const string expectedBase64 = "AAEC/w==";
        ulong base64 = CreateMedia(
            api->MediaFromBase64,
            MediaKindImage,
            expectedBase64,
            null,
            out int base64Type);
        try
        {
            Require(
                ReadMedia(
                    api,
                    api->MediaBase64,
                    base64,
                    base64Type)
                    == expectedBase64,
                "media base64 changed");
            Require(
                ReadMedia(
                    api,
                    api->MediaMimeType,
                    base64,
                    base64Type).Length == 0,
                "null media MIME type was not preserved");
        }
        finally
        {
            Require(
                api->HandleRelease(base64) == StatusOk,
                "base64 media handle release failed");
        }
    }

    private static ulong CreateMedia(
        delegate* unmanaged[Cdecl]<
            int,
            byte*,
            byte*,
            ulong*,
            int*,
            uint> constructor,
        int mediaKind,
        string value,
        string? mimeType,
        out int handleType)
    {
        byte[] encodedValue = NullTerminatedUtf8(value);
        byte[]? encodedMimeType = mimeType is null
            ? null
            : NullTerminatedUtf8(mimeType);
        ulong key = 0;
        int returnedType = 0;
        fixed (byte* valuePointer = encodedValue)
        fixed (byte* mimeTypePointer = encodedMimeType)
        {
            uint status = constructor(
                mediaKind,
                valuePointer,
                mimeTypePointer,
                &key,
                &returnedType);
            Require(status == StatusOk, $"media constructor failed with {status}");
        }

        Require(key != 0, "media constructor returned key zero");
        handleType = returnedType;
        return key;
    }

    private static string ReadMedia(
        BamlApiV1* api,
        delegate* unmanaged[Cdecl]<ulong, int, BamlBuffer*, uint> accessor,
        ulong key,
        int handleType)
    {
        BamlBuffer output = default;
        uint status = accessor(key, handleType, &output);
        Require(status == StatusOk, $"media accessor failed with {status}");
        return Encoding.UTF8.GetString(ConsumeBuffer(api, output));
    }

    private static byte[] NullTerminatedUtf8(string value)
    {
        byte[] encoded = new byte[Encoding.UTF8.GetByteCount(value) + 1];
        Encoding.UTF8.GetBytes(value, encoded);
        return encoded;
    }

    private static string ConsumeUtf8Buffer(
        BamlApiV1* api,
        BamlBuffer buffer) =>
        DecodeUtf8(ConsumeBuffer(api, buffer), "native buffer");

    private static byte[] ConsumeBuffer(
        BamlApiV1* api,
        BamlBuffer buffer)
    {
        try
        {
            if (buffer.Length == 0)
            {
                return [];
            }

            Require(
                buffer.Pointer is not null,
                "non-empty BamlBuffer has a null pointer");
            Require(
                buffer.Length <= int.MaxValue,
                $"BamlBuffer is too large: {buffer.Length}");
            return new ReadOnlySpan<byte>(
                    buffer.Pointer,
                    checked((int)buffer.Length))
                .ToArray();
        }
        finally
        {
            api->FreeBuffer(buffer);
            Interlocked.Increment(ref _releasedBuffers);
        }
    }

    private static void ValidateRequiredFunctions(BamlApiV1* api)
    {
        Require(api->Version is not null, "version is null");
        Require(
            api->InitializeRuntimeFromBytecode is not null,
            "initialize_runtime_from_bytecode is null");
        Require(api->FreeBuffer is not null, "free_buffer is null");
        Require(api->RegisterCallback is not null, "register_callback is null");
        Require(api->CallFunction is not null, "call_function is null");
        Require(
            api->NewFunctionCall is not null,
            "new_function_call is null");
        Require(
            api->CancelFunctionCall is not null,
            "cancel_function_call is null");
        Require(
            api->RegisterHostDispatchCallback is not null,
            "register_host_dispatch_callback is null");
        Require(
            api->RegisterHostReleaseCallback is not null,
            "register_host_release_callback is null");
        Require(
            api->CompleteHostCall is not null,
            "complete_host_call is null");
        Require(api->HandleClone is not null, "handle_clone is null");
        Require(api->HandleRelease is not null, "handle_release is null");
        Require(api->MediaFromUrl is not null, "media_from_url is null");
        Require(api->MediaFromFile is not null, "media_from_file is null");
        Require(
            api->MediaFromBase64 is not null,
            "media_from_base64 is null");
        Require(api->MediaUrl is not null, "media_url is null");
        Require(api->MediaFile is not null, "media_file is null");
        Require(api->MediaBase64 is not null, "media_base64 is null");
        Require(
            api->MediaMimeType is not null,
            "media_mime_type is null");
        Require(api->RegisterBridge is not null, "register_bridge is null");
    }

    private static nuint ApiFieldEnd(string field) =>
        checked((nuint)Marshal.OffsetOf<BamlApiV1>(field) + (nuint)IntPtr.Size);

    private static void Require(bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidOperationException(message);
        }
    }

    private static void RequireProtocol(bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidDataException(message);
        }
    }

    private static void Expect<TException>(Action action)
        where TException : Exception
    {
        try
        {
            action();
        }
        catch (TException)
        {
            return;
        }

        throw new InvalidOperationException(
            $"expected {typeof(TException).Name}");
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void OnResult(
        uint callId,
        byte* content,
        nuint length)
    {
        try
        {
            if (length > int.MaxValue
                || (length != 0 && content is null))
            {
                throw new InvalidDataException(
                    $"invalid borrowed result buffer for call {callId}");
            }

            byte[] copy = length == 0
                ? []
                : new ReadOnlySpan<byte>(
                    content,
                    checked((int)length)).ToArray();
            Interlocked.Increment(ref _borrowedResultCopies);
            if (!PendingCalls.TryRemove(callId, out var completion))
            {
                Interlocked.Increment(ref _lateOrDuplicateResults);
                return;
            }

            completion.TrySetResult(copy);
        }
        catch (Exception error)
        {
            Interlocked.CompareExchange(
                ref _callbackFailure,
                ExceptionDispatchInfo.Capture(error),
                null);
        }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void OnHostDispatch(
        ulong hostValueKey,
        uint callId,
        byte* args,
        nuint length)
    {
        try
        {
            if (hostValueKey == 0
                || callId == 0
                || length > int.MaxValue
                || (length != 0 && args is null))
            {
                throw new InvalidDataException(
                    "invalid borrowed host-dispatch callback arguments");
            }
        }
        catch (Exception error)
        {
            Interlocked.CompareExchange(
                ref _callbackFailure,
                ExceptionDispatchInfo.Capture(error),
                null);
        }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void OnHostRelease(ulong hostValueKey)
    {
        try
        {
            if (hostValueKey == 0)
            {
                throw new InvalidDataException(
                    "host-release callback supplied key zero");
            }
        }
        catch (Exception error)
        {
            Interlocked.CompareExchange(
                ref _callbackFailure,
                ExceptionDispatchInfo.Capture(error),
                null);
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    private readonly struct BamlBuffer
    {
        public readonly byte* Pointer;
        public readonly nuint Length;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct BamlBridgeInfoV1
    {
        public nuint StructSize;
        public uint Language;
        public byte* SdkVersion;
        public nuint SdkVersionLength;
    }

    [StructLayout(LayoutKind.Sequential)]
    private readonly struct BamlApiV1
    {
        public readonly uint AbiVersion;
        public readonly nuint StructSize;
        public readonly delegate* unmanaged[Cdecl]<BamlBuffer> Version;
        public readonly delegate* unmanaged[Cdecl]<
            byte*,
            nuint,
            BamlBuffer> InitializeRuntimeFromBytecode;
        public readonly delegate* unmanaged[Cdecl]<BamlBuffer, void> FreeBuffer;
        public readonly delegate* unmanaged[Cdecl]<
            delegate* unmanaged[Cdecl]<uint, byte*, nuint, void>,
            void> RegisterCallback;
        public readonly delegate* unmanaged[Cdecl]<
            byte*,
            nuint,
            uint,
            void> CallFunction;
        public readonly delegate* unmanaged[Cdecl]<ulong> NewFunctionCall;
        public readonly delegate* unmanaged[Cdecl]<ulong, int> CancelFunctionCall;
        public readonly delegate* unmanaged[Cdecl]<
            delegate* unmanaged[Cdecl]<ulong, uint, byte*, nuint, void>,
            void> RegisterHostDispatchCallback;
        public readonly delegate* unmanaged[Cdecl]<
            delegate* unmanaged[Cdecl]<ulong, void>,
            void> RegisterHostReleaseCallback;
        public readonly delegate* unmanaged[Cdecl]<
            uint,
            int,
            byte*,
            nuint,
            void> CompleteHostCall;
        public readonly delegate* unmanaged[Cdecl]<
            ulong,
            ulong*,
            uint> HandleClone;
        public readonly delegate* unmanaged[Cdecl]<ulong, uint> HandleRelease;
        public readonly delegate* unmanaged[Cdecl]<
            int,
            byte*,
            byte*,
            ulong*,
            int*,
            uint> MediaFromUrl;
        public readonly delegate* unmanaged[Cdecl]<
            int,
            byte*,
            byte*,
            ulong*,
            int*,
            uint> MediaFromFile;
        public readonly delegate* unmanaged[Cdecl]<
            int,
            byte*,
            byte*,
            ulong*,
            int*,
            uint> MediaFromBase64;
        public readonly delegate* unmanaged[Cdecl]<
            ulong,
            int,
            BamlBuffer*,
            uint> MediaUrl;
        public readonly delegate* unmanaged[Cdecl]<
            ulong,
            int,
            BamlBuffer*,
            uint> MediaFile;
        public readonly delegate* unmanaged[Cdecl]<
            ulong,
            int,
            BamlBuffer*,
            uint> MediaBase64;
        public readonly delegate* unmanaged[Cdecl]<
            ulong,
            int,
            BamlBuffer*,
            uint> MediaMimeType;
        public readonly delegate* unmanaged[Cdecl]<
            BamlBridgeInfoV1*,
            BamlBuffer> RegisterBridge;
    }

    private static partial class NativeMethods
    {
        [LibraryImport(
            NativeLibraryName,
            EntryPoint = "baml_get_api_v1")]
        [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
        internal static partial BamlApiV1* GetApiV1();
    }
}
