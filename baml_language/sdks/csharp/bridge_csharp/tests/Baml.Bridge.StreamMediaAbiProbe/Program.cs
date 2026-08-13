using System.Collections.Concurrent;
using System.Diagnostics;
using System.Security.Cryptography;
using System.Text;
using BamlBridge.Cffi.V1;

internal static class Program
{
    private const string ReplayAddressPrefix = "BAML_REPLAY_ADDRESS=";
    private const int TaggedStreamHandleType = 14;
    private const int ExpectedStreamFinalUtf8Length = 789;
    private const string ExpectedStreamFinalSha256 =
        "2e950ddbdb0c2e12f64c09bc6e4a72f687367894cdea17d632529fd6719d2ef2";
    private const string StreamPartialOptionName = "string | null";
    private const string StreamClassName = "ai.stream.Stream";
    private const string StreamDoneClassName = "ai.stream.Done";

    public static async Task<int> Main(string[] args)
    {
        if (args.Length is < 4 or > 5)
        {
            Console.Error.WriteLine(
                "usage: Baml.Bridge.StreamMediaAbiProbe <native-library|package-default> <version> <media|stream> <bytecode> [recording]");
            return 2;
        }

        if (StringComparer.Ordinal.Equals(args[2], "media"))
        {
            Require(args.Length == 4, "media mode does not accept a recording");
            using NativeBridge bridge = new(
                args[0],
                args[1],
                args[3]);
            await VerifyMediaAsync(bridge).ConfigureAwait(false);
            WriteBridgeSummary(bridge);
        }
        else if (StringComparer.Ordinal.Equals(args[2], "stream"))
        {
            Require(args.Length == 5, "stream mode requires a recording");
            string recording = Path.GetFullPath(args[4]);
            Require(
                File.Exists(recording),
                $"recording does not exist: {recording}");
            ReplayServerProcess server = await StartReplayServerProcessAsync(
                    args[0],
                    args[1],
                    args[3],
                    recording)
                .ConfigureAwait(false);
            try
            {
                return await RunStreamConsumerProcessAsync(
                        args[0],
                        args[1],
                        args[3],
                        recording,
                        server.BaseUrl)
                    .ConfigureAwait(false);
            }
            finally
            {
                await server.DisposeAsync().ConfigureAwait(false);
            }
        }
        else if (StringComparer.Ordinal.Equals(
                     args[2],
                     "stream-consumer"))
        {
            Require(
                args.Length == 5,
                "stream-consumer mode requires a recording");
            Require(
                !String.IsNullOrWhiteSpace(
                    Environment.GetEnvironmentVariable(
                        "BAML_REPLAY_BASE_URL")),
                "stream-consumer must receive BAML_REPLAY_BASE_URL at process start");
            using NativeBridge bridge = new(
                    args[0],
                    args[1],
                    args[3]);
            await VerifyStreamAsync(bridge).ConfigureAwait(false);
            WriteBridgeSummary(bridge);
        }
        else if (StringComparer.Ordinal.Equals(
                     args[2],
                     "stream-server"))
        {
            Require(
                args.Length == 5,
                "stream-server mode requires a recording");
            return await RunReplayServerAsync(
                    args[0],
                    args[1],
                    args[3],
                    Path.GetFullPath(args[4]))
                .ConfigureAwait(false);
        }
        else
        {
            throw new ArgumentException(
                $"unknown probe mode {args[2]}",
                nameof(args));
        }

        return 0;
    }

    private static void WriteBridgeSummary(NativeBridge bridge)
    {
        Console.WriteLine($"product_version={bridge.ProductVersion}");
        Console.WriteLine($"bytecode_bytes={bridge.BytecodeLength}");
        Console.WriteLine($"native_callbacks={bridge.CallbackCount}");
        Console.WriteLine($"owned_buffers_released={bridge.ReleasedBuffers}");
    }

    private static async Task VerifyMediaAsync(NativeBridge bridge)
    {
        MediaCase[] cases =
        [
            new("image", NativeKind: 1, HandleType: 6, "image/png"),
            new("audio", NativeKind: 2, HandleType: 7, "audio/wav"),
            new("pdf", NativeKind: 3, HandleType: 9, "application/pdf"),
            new("video", NativeKind: 4, HandleType: 8, "video/mp4"),
        ];
        byte[] expectedBytes = [0x00, 0x01, 0x02, 0xff, 0x7f, 0x80];
        string base64 = Convert.ToBase64String(expectedBytes);
        MediaRestoreCounter handlesRestored = new();

        foreach (MediaCase mediaCase in cases)
        {
            string url =
                $"https://example.com/{mediaCase.Name}/雪?token=fixture";
            BamlOutboundValue returned = RequireOk(
                await bridge.CallAsync(
                        $"user.media.return_{mediaCase.Name}",
                        Arguments(
                            ("url", new InboundValue
                            {
                                StringValue = url,
                            }),
                            ("mime", new InboundValue
                            {
                                StringValue = mediaCase.MimeType,
                            })))
                    .ConfigureAwait(false));
            ManagedMedia returnedMedia = RestoreMedia(
                bridge,
                returned,
                mediaCase,
                handlesRestored);
            Require(
                returnedMedia.IsUrl
                && StringComparer.Ordinal.Equals(
                    returnedMedia.Url,
                    url)
                && StringComparer.Ordinal.Equals(
                    returnedMedia.MimeType,
                    mediaCase.MimeType),
                $"{mediaCase.Name} BAML-created URL restoration failed");

            await VerifyMediaRoundTripAsync(
                    bridge,
                    mediaCase,
                    MediaSource.Url,
                    url,
                    expectedBytes: null,
                    handlesRestored)
                .ConfigureAwait(false);
            await VerifyMediaRoundTripAsync(
                    bridge,
                    mediaCase,
                    MediaSource.Base64,
                    base64,
                    expectedBytes,
                    handlesRestored)
                .ConfigureAwait(false);

            string filePath = Path.Combine(
                Path.GetTempPath(),
                $"baml-csharp-b9-{Environment.ProcessId}-{mediaCase.Name}.bin");
            await File.WriteAllBytesAsync(filePath, expectedBytes)
                .ConfigureAwait(false);
            try
            {
                ManagedMedia restoredFile =
                    await RoundTripMediaAsync(
                            bridge,
                            mediaCase,
                            MediaSource.File,
                            filePath,
                            handlesRestored)
                        .ConfigureAwait(false);
                File.Delete(filePath);
                Require(
                    !restoredFile.IsUrl
                    && restoredFile.Bytes.Span.SequenceEqual(expectedBytes)
                    && StringComparer.Ordinal.Equals(
                        restoredFile.MimeType,
                        mediaCase.MimeType),
                    $"{mediaCase.Name} file-backed media was not eagerly copied");
            }
            finally
            {
                if (File.Exists(filePath))
                {
                    File.Delete(filePath);
                }
            }
        }

        BamlOutboundValue cleanupValue = RequireOk(
            await bridge.CallAsync(
                    "user.media.return_image",
                    Arguments(
                        ("url", new InboundValue
                        {
                            StringValue = "https://example.com/cleanup.png",
                        }),
                        ("mime", new InboundValue
                        {
                            StringValue = "image/png",
                        })))
                .ConfigureAwait(false));
        ulong cleanupKey = ExtractMediaHandle(cleanupValue).Key;
        try
        {
            _ = RestoreMedia(
                bridge,
                cleanupValue,
                cases[1],
                handlesRestored);
            throw new InvalidOperationException(
                "wrong expected media kind was accepted");
        }
        catch (InvalidDataException)
        {
            Require(
                bridge.IsReleasedHandle(cleanupKey),
                "media decode failure leaked its native output handle");
        }

        VerifyInlineMediaProtocol(cases[0], expectedBytes);
        Require(
            handlesRestored.Value == 17,
            $"unexpected restored native media handle count {handlesRestored.Value}");
        Require(
            bridge.ReleasedBuffers == 79,
            $"expected exactly 79 released native buffers, received {bridge.ReleasedBuffers}");

        Console.WriteLine("media_kinds=url_base64_file_4x");
        Console.WriteLine("media_actual_envelope=handle_eagerly_restored");
        Console.WriteLine("media_file=eager_owned_bytes");
        Console.WriteLine("media_decode_failure=handle_released");
        Console.WriteLine("media_inline_protocol=url_base64_file");
        Console.WriteLine($"media_handles_restored={handlesRestored.Value}");
    }

    private static async Task VerifyMediaRoundTripAsync(
        NativeBridge bridge,
        MediaCase mediaCase,
        MediaSource source,
        string value,
        byte[]? expectedBytes,
        MediaRestoreCounter handlesRestored)
    {
        ManagedMedia restored = await RoundTripMediaAsync(
                bridge,
                mediaCase,
                source,
                value,
                handlesRestored)
            .ConfigureAwait(false);
        if (source == MediaSource.Url)
        {
            Require(
                restored.IsUrl
                && StringComparer.Ordinal.Equals(restored.Url, value),
                $"{mediaCase.Name} URL round trip changed representation");
        }
        else
        {
            Require(
                !restored.IsUrl
                && restored.Bytes.Span.SequenceEqual(
                    expectedBytes
                    ?? throw new InvalidOperationException(
                        "expected bytes are required")),
                $"{mediaCase.Name} byte round trip changed data");
        }

        Require(
            StringComparer.Ordinal.Equals(
                restored.MimeType,
                mediaCase.MimeType),
            $"{mediaCase.Name} round trip changed MIME type");
    }

    private static async Task<ManagedMedia> RoundTripMediaAsync(
        NativeBridge bridge,
        MediaCase mediaCase,
        MediaSource source,
        string value,
        MediaRestoreCounter handlesRestored)
    {
        NativeHandle original = bridge.CreateMedia(
            mediaCase.NativeKind,
            source,
            value,
            mediaCase.MimeType);
        try
        {
            Require(
                original.HandleType == mediaCase.HandleType,
                $"{mediaCase.Name} constructor returned handle type {original.HandleType}");
            ulong transferredClone = bridge.CloneHandle(original.Key);
            BamlOutboundValue output = RequireOk(
                await bridge.CallAsync(
                        $"user.media.round_trip_{mediaCase.Name}",
                        Arguments(
                            ("x", HandleValue(
                                transferredClone,
                                original.HandleType,
                                mediaCase))))
                    .ConfigureAwait(false));
            ManagedMedia restored = RestoreMedia(
                bridge,
                output,
                mediaCase,
                handlesRestored);
            Require(
                source switch
                {
                    MediaSource.Url => StringComparer.Ordinal.Equals(
                        bridge.ReadMedia(original, MediaSource.Url),
                        value),
                    MediaSource.Base64 => StringComparer.Ordinal.Equals(
                        bridge.ReadMedia(original, MediaSource.Base64),
                        value),
                    MediaSource.File => StringComparer.Ordinal.Equals(
                        bridge.ReadMedia(original, MediaSource.File),
                        value),
                    _ => false,
                },
                $"{mediaCase.Name} inbound clone drained the managed-owned original");
            return restored;
        }
        finally
        {
            bridge.ReleaseHandle(original.Key);
        }
    }

    private static ManagedMedia RestoreMedia(
        NativeBridge bridge,
        BamlOutboundValue value,
        MediaCase expected,
        MediaRestoreCounter handlesRestored)
    {
        string? mediaClassName = null;
        if (value.ValueCase
            == BamlOutboundValue.ValueOneofCase.ClassValue)
        {
            BamlValueClass mediaClass = value.ClassValue;
            mediaClassName = mediaClass.Name;
            if (mediaClass.Fields.Count != 1
                || !StringComparer.Ordinal.Equals(
                    mediaClass.Fields[0].Key,
                    "_data")
                || mediaClass.Fields[0].Value is null)
            {
                throw new InvalidDataException(
                    $"malformed media class envelope: {mediaClass}");
            }

            value = mediaClass.Fields[0].Value;
        }

        if (value.ValueCase
            == BamlOutboundValue.ValueOneofCase.MediaValue)
        {
            return RestoreInlineMedia(value.MediaValue, expected);
        }

        if (value.ValueCase
            != BamlOutboundValue.ValueOneofCase.HandleValue)
        {
            throw new InvalidDataException(
                $"expected media handle/inline value, received {value.ValueCase}: {value}");
        }

        BamlOutboundHandle wire = value.HandleValue;
        NativeHandle handle = new(
            wire.Key,
            (int)wire.HandleType,
            wire.Ty);
        try
        {
            if (mediaClassName is not null
                && !StringComparer.Ordinal.Equals(
                    mediaClassName,
                    MediaClassName(expected)))
            {
                throw new InvalidDataException(
                    $"expected {MediaClassName(expected)}, received {mediaClassName}");
            }

            if (handle.HandleType != expected.HandleType)
            {
                throw new InvalidDataException(
                    $"expected {expected.Name} handle type {expected.HandleType}, received {handle.HandleType}");
            }

            string url = bridge.ReadMedia(handle, MediaSource.Url);
            string base64 = bridge.ReadMedia(handle, MediaSource.Base64);
            string file = bridge.ReadMedia(handle, MediaSource.File);
            string mime = bridge.ReadMediaMimeType(handle);
            int representationCount =
                (url.Length == 0 ? 0 : 1)
                + (base64.Length == 0 ? 0 : 1)
                + (file.Length == 0 ? 0 : 1);
            if (representationCount != 1)
            {
                throw new InvalidDataException(
                    $"media handle has {representationCount} representations");
            }

            if (url.Length != 0)
            {
                return ManagedMedia.FromUrl(url, EmptyToNull(mime));
            }

            if (String.IsNullOrWhiteSpace(mime))
            {
                throw new InvalidDataException(
                    "byte-backed media has no MIME type");
            }

            byte[] bytes = base64.Length != 0
                ? Convert.FromBase64String(base64)
                : File.ReadAllBytes(file);
            return ManagedMedia.FromBytes(bytes, mime);
        }
        finally
        {
            bridge.ReleaseHandle(handle.Key);
            handlesRestored.Increment();
        }
    }

    private static BamlOutboundHandle ExtractMediaHandle(
        BamlOutboundValue value)
    {
        if (value.ValueCase
            == BamlOutboundValue.ValueOneofCase.ClassValue)
        {
            BamlValueClass mediaClass = value.ClassValue;
            Require(
                mediaClass.Fields.Count == 1
                && StringComparer.Ordinal.Equals(
                    mediaClass.Fields[0].Key,
                    "_data")
                && mediaClass.Fields[0].Value is not null,
                $"malformed media class envelope: {mediaClass}");
            value = mediaClass.Fields[0].Value;
        }

        Require(
            value.ValueCase
            == BamlOutboundValue.ValueOneofCase.HandleValue,
            $"actual CFFI media did not use its current class/handle envelope: {value}");
        return value.HandleValue;
    }

    private static ManagedMedia RestoreInlineMedia(
        BamlValueMedia wire,
        MediaCase expected)
    {
        MediaTypeEnum expectedKind = expected.Name switch
        {
            "image" => MediaTypeEnum.Image,
            "audio" => MediaTypeEnum.Audio,
            "pdf" => MediaTypeEnum.Pdf,
            "video" => MediaTypeEnum.Video,
            _ => throw new InvalidOperationException(),
        };
        if (wire.Media != expectedKind)
        {
            throw new InvalidDataException(
                $"inline media kind mismatch: {wire.Media}");
        }

        string? mime = wire.HasMimeType
            ? wire.MimeType
            : null;
        return wire.ValueCase switch
        {
            BamlValueMedia.ValueOneofCase.Url =>
                ManagedMedia.FromUrl(wire.Url, mime),
            BamlValueMedia.ValueOneofCase.Base64 =>
                ManagedMedia.FromBytes(
                    Convert.FromBase64String(wire.Base64),
                    mime
                    ?? throw new InvalidDataException(
                        "inline base64 media has no MIME type")),
            BamlValueMedia.ValueOneofCase.File =>
                ManagedMedia.FromBytes(
                    File.ReadAllBytes(wire.File),
                    mime
                    ?? throw new InvalidDataException(
                        "inline file media has no MIME type")),
            _ => throw new InvalidDataException(
                "inline media has no representation"),
        };
    }

    private static void VerifyInlineMediaProtocol(
        MediaCase mediaCase,
        byte[] expectedBytes)
    {
        ManagedMedia url = RestoreInlineMedia(
            new BamlValueMedia
            {
                Media = MediaTypeEnum.Image,
                MimeType = mediaCase.MimeType,
                Url = "https://example.com/inline.png",
            },
            mediaCase);
        Require(url.IsUrl, "inline URL media did not restore as URL");

        ManagedMedia base64 = RestoreInlineMedia(
            new BamlValueMedia
            {
                Media = MediaTypeEnum.Image,
                MimeType = mediaCase.MimeType,
                Base64 = Convert.ToBase64String(expectedBytes),
            },
            mediaCase);
        Require(
            base64.Bytes.Span.SequenceEqual(expectedBytes),
            "inline base64 media changed bytes");

        string file = Path.Combine(
            Path.GetTempPath(),
            $"baml-csharp-b9-inline-{Environment.ProcessId}.bin");
        File.WriteAllBytes(file, expectedBytes);
        try
        {
            ManagedMedia fromFile = RestoreInlineMedia(
                new BamlValueMedia
                {
                    Media = MediaTypeEnum.Image,
                    MimeType = mediaCase.MimeType,
                    File = file,
                },
                mediaCase);
            File.Delete(file);
            Require(
                fromFile.Bytes.Span.SequenceEqual(expectedBytes),
                "inline file media was not eagerly copied");
        }
        finally
        {
            if (File.Exists(file))
            {
                File.Delete(file);
            }
        }
    }

    private static async Task VerifyStreamAsync(NativeBridge bridge)
    {
        VerifyStreamUnionMetadataNegatives();

        int streamFactoryCalls = 0;
        int callbacksBeforeColdFactory = bridge.CallbackCount;
        Lazy<Task<NativeHandle>> coldFactory = new(
            () =>
            {
                Interlocked.Increment(ref streamFactoryCalls);
                return StartStreamAsync(bridge);
            },
            LazyThreadSafetyMode.ExecutionAndPublication);
        await Task.Delay(TimeSpan.FromMilliseconds(75))
            .ConfigureAwait(false);
        Require(
            streamFactoryCalls == 0
            && bridge.CallbackCount == callbacksBeforeColdFactory,
            "cold managed stream started native execution");

        NativeHandle stream = await coldFactory.Value
            .ConfigureAwait(false);
        Require(
            streamFactoryCalls == 1,
            "cold stream factory did not start exactly once");
        try
        {
            int partialCount = 0;
            List<string?> partialValues = [];
            while (true)
            {
                int callbacksBeforePull = bridge.CallbackCount;
                PullResult pulled = await PullNextAsync(
                        bridge,
                        stream)
                    .ConfigureAwait(false);
                Require(
                    bridge.CallbackCount
                    == callbacksBeforePull + 1,
                    "one pull did not produce exactly one native completion");
                int callbacksAfterPull = bridge.CallbackCount;
                await Task.Delay(TimeSpan.FromMilliseconds(75))
                    .ConfigureAwait(false);
                Require(
                    bridge.CallbackCount == callbacksAfterPull,
                    "native stream pushed an unsolicited completion while the consumer was idle");

                if (pulled.IsFinished)
                {
                    break;
                }

                partialCount++;
                partialValues.Add(pulled.Value);
                Require(
                    partialCount < 10_000,
                    "stream pull did not terminate");
            }

            Require(
                partialCount > 0,
                "stream returned no partials");
            Require(
                partialValues.All(value => value is not null),
                "stream returned a null partial");
            for (int index = 1; index < partialValues.Count; index++)
            {
                string previous = partialValues[index - 1]!;
                string current = partialValues[index]!;
                Require(
                    current.Length > previous.Length
                    && current.StartsWith(
                        previous,
                        StringComparison.Ordinal),
                    $"stream partial {index + 1} did not strictly extend partial {index}");
            }

            Task<string>[] finalWaiters = Enumerable.Range(0, 8)
                .Select(_ => GetFinalAsync(bridge, stream))
                .ToArray();
            string[] finals = await Task.WhenAll(finalWaiters)
                .ConfigureAwait(false);
            Require(
                finals.All(
                    value => StringComparer.Ordinal.Equals(
                        value,
                        finals[0])),
                "multiple final calls returned different cached values");
            Require(
                finals[0].StartsWith(
                    partialValues[^1]!,
                    StringComparison.Ordinal),
                "final response did not extend the last ordered partial");
            string contentSha256 = RequireCanonicalStreamContent(
                partialValues,
                finals[0]);
            Console.WriteLine($"stream_partials={partialCount}");
            Console.WriteLine(
                "stream_partial_order=initial_prefix_then_strict_extensions_exact_canonical_final");
            Console.WriteLine(
                $"stream_content_utf8_bytes={ExpectedStreamFinalUtf8Length}");
            Console.WriteLine(
                $"stream_content_sha256={contentSha256}");
        }
        finally
        {
            bridge.ReleaseHandle(stream.Key);
        }

        await VerifyFinalOnlyAndWaitCancellationAsync(bridge)
            .ConfigureAwait(false);
        await VerifyPreCanceledPullAndEarlyReleaseAsync(bridge)
            .ConfigureAwait(false);

        Require(
            bridge.MaxPendingCalls <= 8,
            $"managed callback registry grew unexpectedly: {bridge.MaxPendingCalls}");
        Console.WriteLine("stream_pull=one_demand_one_completion");
        Console.WriteLine("stream_idle=zero_unsolicited_completions");
        Console.WriteLine("stream_cold_start=exactly_once");
        Console.WriteLine("stream_final=multi_waiter_and_final_only");
        Console.WriteLine("stream_wait_token=wait_only");
        Console.WriteLine("stream_precancel_and_release=exact");
        Console.WriteLine(
            $"stream_max_pending_calls={bridge.MaxPendingCalls}");
    }

    private static async Task<NativeHandle> StartStreamAsync(
        NativeBridge bridge)
    {
        BamlOutboundValue value = RequireOk(
            await bridge.CallAsync(
                    "user.lorem.stream_e2e_extract$stream",
                    Arguments(
                        ("text", new InboundValue
                        {
                            StringValue = "ignored-by-replay-server",
                        })))
                .ConfigureAwait(false));
        if (value.ValueCase
            != BamlOutboundValue.ValueOneofCase.HandleValue)
        {
            throw new InvalidDataException(
                $"stream factory returned {value.ValueCase}");
        }

        BamlOutboundHandle handle = value.HandleValue;
        if ((int)handle.HandleType != TaggedStreamHandleType
            || handle.Ty?.TyCase != BamlTy.TyOneofCase.ClassTy
            || !StringComparer.Ordinal.Equals(
                handle.Ty.ClassTy.Name,
                StreamClassName)
            || handle.Ty.ClassTy.TypeArgs.Count != 2)
        {
            throw new InvalidDataException(
                $"stream handle descriptor was invalid: {handle}");
        }

        return new NativeHandle(
            handle.Key,
            (int)handle.HandleType,
            handle.Ty);
    }

    private static async Task<PullResult> PullNextAsync(
        NativeBridge bridge,
        NativeHandle stream)
    {
        ulong clone = bridge.CloneHandle(stream.Key);
        BamlOutboundValue value = RequireOk(
            await bridge.CallAsync(
                    "ai.stream.Stream.next",
                    Arguments(
                        ("self", HandleValue(
                            clone,
                            stream.HandleType))))
                .ConfigureAwait(false));
        return DecodePull(value);
    }

    private static PullResult DecodePull(BamlOutboundValue value)
    {
        if (value.ValueCase
            != BamlOutboundValue.ValueOneofCase.UnionVariantValue)
        {
            throw new InvalidDataException(
                "stream pull must be encoded as its typed union");
        }

        BamlValueUnionVariant union = value.UnionVariantValue;
        ValidateStreamPullUnionDescriptor(union);
        BamlOutboundValue selected = union.Value
            ?? throw new InvalidDataException(
                "stream pull union omitted its selected value");

        if (StringComparer.Ordinal.Equals(
                union.ValueOptionName,
                StreamPartialOptionName))
        {
            return selected.ValueCase switch
            {
                BamlOutboundValue.ValueOneofCase.None =>
                    new PullResult(false, null),
                BamlOutboundValue.ValueOneofCase.StringValue =>
                    new PullResult(false, selected.StringValue),
                _ => throw new InvalidDataException(
                    "stream pull selected the partial arm but carried "
                    + selected.ValueCase),
            };
        }

        if (StringComparer.Ordinal.Equals(
                union.ValueOptionName,
                StreamDoneClassName))
        {
            if (selected.ValueCase
                != BamlOutboundValue.ValueOneofCase.ClassValue)
            {
                throw new InvalidDataException(
                    "stream pull selected the finished arm but carried "
                    + selected.ValueCase);
            }

            if (!StringComparer.Ordinal.Equals(
                    selected.ClassValue.Name,
                    StreamDoneClassName)
                || selected.ClassValue.Fields.Count != 0
                || selected.ClassValue.TypeArgs.Count != 0)
            {
                throw new InvalidDataException(
                    "stream pull finished arm carried the wrong nominal "
                    + "class shape");
            }

            return PullResult.Finished;
        }

        throw new InvalidDataException(
            "stream pull selected unknown union arm "
            + union.ValueOptionName);
    }

    private static void ValidateStreamPullUnionDescriptor(
        BamlValueUnionVariant union)
    {
        BamlTy? selfType = union.SelfType;
        bool valid = union.Name.Length == 0
            && !union.IsOptional
            && !union.IsSinglePattern
            && selfType is not null
            && selfType.TyCase
                == BamlTy.TyOneofCase.Union
            && selfType.Union.Options.Count == 2;
        if (!valid || selfType is null)
        {
            throw new InvalidDataException(
                "stream pull union descriptor did not exactly match "
                + "(string | null) | ai.stream.Done");
        }

        BamlTy partial = selfType.Union.Options[0];
        BamlTy finished = selfType.Union.Options[1];
        valid = partial.TyCase
                == BamlTy.TyOneofCase.Optional
            && partial.Optional.Inner is not null
            && partial.Optional.Inner.TyCase
                == BamlTy.TyOneofCase.Primitive
            && partial.Optional.Inner.Primitive.Kind
                == BamlTyPrimitiveKind.BamlTyPrimitiveString
            && finished.TyCase
                == BamlTy.TyOneofCase.ClassTy
            && StringComparer.Ordinal.Equals(
                finished.ClassTy.Name,
                StreamDoneClassName)
            && finished.ClassTy.TypeArgs.Count == 0;

        if (!valid)
        {
            throw new InvalidDataException(
                "stream pull union descriptor did not exactly match "
                + "(string | null) | ai.stream.Done");
        }
    }

    private static void VerifyStreamUnionMetadataNegatives()
    {
        PullResult partial = DecodePull(
            CreateStreamPullUnion(
                StreamPartialOptionName,
                new BamlOutboundValue
                {
                    StringValue = "fixture",
                }));
        Require(
            !partial.IsFinished
            && StringComparer.Ordinal.Equals(
                partial.Value,
                "fixture"),
            "canonical stream partial metadata did not decode");
        Require(
            DecodePull(
                CreateStreamPullUnion(
                    StreamDoneClassName,
                    CreateStreamDoneValue()))
                .IsFinished,
            "canonical stream-finished metadata did not decode");

        RequireDecodePullRejected(
            new BamlOutboundValue
            {
                StringValue = "fixture",
            },
            "must be encoded as its typed union",
            "bare payload");

        BamlOutboundValue wrongDescriptor = CreateStreamPullUnion(
            StreamPartialOptionName,
            new BamlOutboundValue
            {
                StringValue = "fixture",
            });
        wrongDescriptor.UnionVariantValue.SelfType
            .Union.Options[0].Optional.Inner.Primitive.Kind =
            BamlTyPrimitiveKind.BamlTyPrimitiveInt;
        RequireDecodePullRejected(
            wrongDescriptor,
            "union descriptor did not exactly match",
            "contradictory union descriptor");

        RequireDecodePullRejected(
            CreateStreamPullUnion(
                String.Empty,
                new BamlOutboundValue
                {
                    StringValue = "fixture",
                }),
            "selected unknown union arm",
            "missing selected arm");
        RequireDecodePullRejected(
            CreateStreamPullUnion(
                "unknown.arm",
                new BamlOutboundValue
                {
                    StringValue = "fixture",
                }),
            "selected unknown union arm",
            "unknown selected arm");
        RequireDecodePullRejected(
            CreateStreamPullUnion(
                StreamDoneClassName,
                new BamlOutboundValue
                {
                    StringValue = "fixture",
                }),
            "selected the finished arm but carried StringValue",
            "finished name with partial payload");
        RequireDecodePullRejected(
            CreateStreamPullUnion(
                StreamPartialOptionName,
                CreateStreamDoneValue()),
            "selected the partial arm but carried ClassValue",
            "partial name with finished payload");

        BamlOutboundValue wrongFinishedClass = CreateStreamPullUnion(
            StreamDoneClassName,
            CreateStreamDoneValue());
        wrongFinishedClass.UnionVariantValue.Value.ClassValue.Name =
            "baml.stream.NotFinished";
        RequireDecodePullRejected(
            wrongFinishedClass,
            "finished arm carried the wrong nominal class shape",
            "finished name with wrong nominal class");

        Console.WriteLine(
            "stream_union_metadata=exact_2_positive_7_negative");
    }

    private static BamlOutboundValue CreateStreamPullUnion(
        string selectedOption,
        BamlOutboundValue selectedValue)
    {
        BamlTy selfType = new()
        {
            Union = new BamlTyUnion(),
        };
        selfType.Union.Options.Add(
            new BamlTy
            {
                Optional = new BamlTyOptional
                {
                    Inner = new BamlTy
                    {
                        Primitive = new BamlTyPrimitive
                        {
                            Kind = BamlTyPrimitiveKind
                                .BamlTyPrimitiveString,
                        },
                    },
                },
            });
        selfType.Union.Options.Add(
            new BamlTy
            {
                ClassTy = new BamlTyClass
                {
                    Name = StreamDoneClassName,
                },
            });
        return new BamlOutboundValue
        {
            UnionVariantValue = new BamlValueUnionVariant
            {
                SelfType = selfType,
                ValueOptionName = selectedOption,
                Value = selectedValue,
            },
        };
    }

    private static BamlOutboundValue CreateStreamDoneValue() =>
        new()
        {
            ClassValue = new BamlValueClass
            {
                Name = StreamDoneClassName,
            },
        };

    private static void RequireDecodePullRejected(
        BamlOutboundValue value,
        string expectedMessage,
        string caseName)
    {
        try
        {
            _ = DecodePull(value);
        }
        catch (InvalidDataException exception)
            when (exception.Message.Contains(
                expectedMessage,
                StringComparison.Ordinal))
        {
            return;
        }
        catch (InvalidDataException exception)
        {
            throw new InvalidOperationException(
                $"{caseName} failed with an unexpected diagnostic",
                exception);
        }

        throw new InvalidOperationException(
            $"{caseName} was accepted");
    }

    private static string RequireCanonicalStreamContent(
        IReadOnlyList<string?> partials,
        string final)
    {
        string contentSha256 = RequireCanonicalStreamFinal(
            final,
            "drained stream");
        StringBuilder normalized = new();
        string previous = String.Empty;
        bool first = true;
        foreach (string? current in partials)
        {
            if (current is null
                || (first
                    ? current.Length < previous.Length
                    : current.Length <= previous.Length)
                || !current.StartsWith(
                    previous,
                    StringComparison.Ordinal))
            {
                throw new InvalidOperationException(
                    "stream partial sequence was not a strict ordered prefix");
            }

            normalized.Append(current.AsSpan(previous.Length));
            previous = current;
            first = false;
        }

        Require(
            final.StartsWith(
                previous,
                StringComparison.Ordinal),
            "canonical final did not extend the ordered partial sequence");
        normalized.Append(final.AsSpan(previous.Length));
        Require(
            StringComparer.Ordinal.Equals(
                normalized.ToString(),
                final),
            "normalized ordered partial deltas did not reconstruct "
            + "the canonical final");
        return contentSha256;
    }

    private static string RequireCanonicalStreamFinal(
        string final,
        string context)
    {
        byte[] utf8 = Encoding.UTF8.GetBytes(final);
        string sha256 = Convert.ToHexString(
                SHA256.HashData(utf8))
            .ToLowerInvariant();
        Require(
            utf8.Length == ExpectedStreamFinalUtf8Length
            && StringComparer.Ordinal.Equals(
                sha256,
                ExpectedStreamFinalSha256),
            $"{context} content identity changed: "
            + $"utf8_bytes={utf8.Length}, sha256={sha256}");
        return sha256;
    }

    private static async Task<string> GetFinalAsync(
        NativeBridge bridge,
        NativeHandle stream)
    {
        ulong clone = bridge.CloneHandle(stream.Key);
        return DecodeString(
            RequireOk(
                await bridge.CallAsync(
                        "ai.stream.Stream.final",
                        Arguments(
                            ("self", HandleValue(
                                clone,
                                stream.HandleType))))
                    .ConfigureAwait(false)));
    }

    private static async Task VerifyFinalOnlyAndWaitCancellationAsync(
        NativeBridge bridge)
    {
        NativeHandle stream = await StartStreamAsync(bridge)
            .ConfigureAwait(false);
        try
        {
            int finalDispatches = 0;
            object gate = new();
            Task<string>? cached = null;
            Task<string> GetCachedFinal()
            {
                lock (gate)
                {
                    cached ??= DispatchFinal();
                    return cached;
                }
            }

            Task<string> DispatchFinal()
            {
                Interlocked.Increment(ref finalDispatches);
                return GetFinalAsync(bridge, stream);
            }

            ConcurrentBag<Task<string>> acquiredFinalTasks = new();
            Task[] acquisitionWorkers = Enumerable.Range(0, 16)
                .Select(
                    _ => Task.Run(
                        () => acquiredFinalTasks.Add(
                            GetCachedFinal())))
                .ToArray();
            await Task.WhenAll(acquisitionWorkers)
                .ConfigureAwait(false);
            Task<string>[] resolvedTasks = acquiredFinalTasks.ToArray();
            Require(
                resolvedTasks.Length == 16
                &&
                finalDispatches == 1
                && resolvedTasks.All(
                    task => ReferenceEquals(task, resolvedTasks[0])),
                "multiple final waiters did not share one terminal task");

            Task<string> underlying = resolvedTasks[0];
            using CancellationTokenSource waitCancellation = new();
            waitCancellation.Cancel();
            bool waitCanceled = false;
            try
            {
                _ = await underlying
                    .WaitAsync(waitCancellation.Token)
                    .ConfigureAwait(false);
            }
            catch (OperationCanceledException exception)
            {
                waitCanceled = true;
                Require(
                    exception.CancellationToken
                    == waitCancellation.Token,
                    "final wait cancellation changed the wait token");
            }

            string final = await underlying
                .WaitAsync(TimeSpan.FromSeconds(30))
                .ConfigureAwait(false);
            _ = RequireCanonicalStreamFinal(
                final,
                "final-only stream");
            Require(
                waitCanceled,
                "final wait token did not cancel only the caller's wait");
        }
        finally
        {
            bridge.ReleaseHandle(stream.Key);
        }
    }

    private static async Task VerifyPreCanceledPullAndEarlyReleaseAsync(
        NativeBridge bridge)
    {
        NativeHandle stream = await StartStreamAsync(bridge)
            .ConfigureAwait(false);
        ulong originalKey = stream.Key;
        ulong transferredClone = bridge.CloneHandle(stream.Key);
        ulong callId = bridge.AllocateCallId();
        Require(
            bridge.Cancel(callId) == 0,
            "pre-cancel stream call ID was rejected");
        NativeCall call = bridge.Dispatch(
            "ai.stream.Stream.next",
            ArgumentsWithCallId(
                callId,
                ("self", HandleValue(
                    transferredClone,
                    stream.HandleType))));
        BamlOutboundResult result = await call.Completion
            .WaitAsync(TimeSpan.FromSeconds(10))
            .ConfigureAwait(false);
        Require(
            result.ResultCase
            is BamlOutboundResult.ResultOneofCase.Error
                or BamlOutboundResult.ResultOneofCase.Panic,
            "pre-canceled stream pull returned success");

        bridge.ReleaseHandle(originalKey);
        Require(
            bridge.IsReleasedHandle(originalKey),
            "early stream release left the handle live");
    }

    private static async Task<int> RunReplayServerAsync(
        string nativeLibrary,
        string version,
        string bytecode,
        string recordingPath)
    {
        Require(
            File.Exists(recordingPath),
            $"recording does not exist: {recordingPath}");
        using NativeBridge bridge = new(
            nativeLibrary,
            version,
            bytecode);
        string address = DecodeString(
            RequireOk(
                await bridge.CallAsync(
                        "user.replay.replay_serve_detached",
                        Arguments(
                            ("recording_path", new InboundValue
                            {
                                StringValue = recordingPath,
                            })))
                    .ConfigureAwait(false)));
        string baseUrl = $"http://{address}";
        Console.WriteLine($"{ReplayAddressPrefix}{address}");
        Console.Out.Flush();
        try
        {
            _ = await Console.In.ReadLineAsync().ConfigureAwait(false);
        }
        finally
        {
            await ShutdownReplayServerAsync(baseUrl).ConfigureAwait(false);
        }

        return 0;
    }

    private static async Task<ReplayServerProcess>
        StartReplayServerProcessAsync(
            string nativeLibrary,
            string version,
            string bytecode,
            string recordingPath)
    {
        ProcessStartInfo startInfo = CreateSelfProcessStartInfo();
        startInfo.ArgumentList.Add(nativeLibrary);
        startInfo.ArgumentList.Add(version);
        startInfo.ArgumentList.Add("stream-server");
        startInfo.ArgumentList.Add(bytecode);
        startInfo.ArgumentList.Add(recordingPath);
        startInfo.Environment.Remove("BAML_REPLAY_BASE_URL");
        startInfo.Environment.Remove("BAML_REPLAY_API_KEY");

        Process process = new()
        {
            StartInfo = startInfo,
        };
        if (!process.Start())
        {
            process.Dispose();
            throw new InvalidOperationException(
                "failed to start the replay server process");
        }

        Task<string> standardError = process.StandardError.ReadToEndAsync();
        try
        {
            using CancellationTokenSource startupTimeout =
                new(TimeSpan.FromSeconds(30));
            string? addressLine = await process.StandardOutput
                .ReadLineAsync(startupTimeout.Token)
                .ConfigureAwait(false);
            if (addressLine is null
                || !addressLine.StartsWith(
                    ReplayAddressPrefix,
                    StringComparison.Ordinal))
            {
                if (!process.HasExited)
                {
                    process.Kill(entireProcessTree: true);
                    await process.WaitForExitAsync().ConfigureAwait(false);
                }

                string error = await standardError.ConfigureAwait(false);
                throw new InvalidOperationException(
                    "replay server exited without an address: "
                    + error);
            }

            string address = addressLine[ReplayAddressPrefix.Length..];
            Require(
                address.Length != 0,
                "replay server returned an empty address");
            return new ReplayServerProcess(
                process,
                standardError,
                $"http://{address}");
        }
        catch
        {
            if (!process.HasExited)
            {
                process.Kill(entireProcessTree: true);
            }

            process.Dispose();
            throw;
        }
    }

    private static async Task<int> RunStreamConsumerProcessAsync(
        string nativeLibrary,
        string version,
        string bytecode,
        string recordingPath,
        string baseUrl)
    {
        ProcessStartInfo startInfo = CreateSelfProcessStartInfo();
        startInfo.ArgumentList.Add(nativeLibrary);
        startInfo.ArgumentList.Add(version);
        startInfo.ArgumentList.Add("stream-consumer");
        startInfo.ArgumentList.Add(bytecode);
        startInfo.ArgumentList.Add(recordingPath);
        startInfo.Environment["BAML_REPLAY_BASE_URL"] = baseUrl;
        startInfo.Environment["BAML_REPLAY_API_KEY"] =
            "csharp-abi-probe-key";

        using Process process = new()
        {
            StartInfo = startInfo,
        };
        Require(
            process.Start(),
            "failed to start the stream consumer process");
        Task<string> standardOutput = process.StandardOutput.ReadToEndAsync();
        Task<string> standardError = process.StandardError.ReadToEndAsync();
        using CancellationTokenSource timeout =
            new(TimeSpan.FromMinutes(2));
        try
        {
            await process.WaitForExitAsync(timeout.Token)
                .ConfigureAwait(false);
        }
        catch (OperationCanceledException)
        {
            process.Kill(entireProcessTree: true);
            await process.WaitForExitAsync().ConfigureAwait(false);
            throw new TimeoutException(
                "stream consumer process did not finish within two minutes");
        }

        string output = await standardOutput.ConfigureAwait(false);
        string error = await standardError.ConfigureAwait(false);
        Console.Out.Write(output);
        Console.Error.Write(error);
        Require(
            process.ExitCode == 0,
            $"stream consumer exited with {process.ExitCode}");
        return 0;
    }

    private static ProcessStartInfo CreateSelfProcessStartInfo()
    {
        string executable = Environment.ProcessPath
            ?? throw new InvalidOperationException(
                "the stream probe requires a process executable");
        ProcessStartInfo startInfo = new()
        {
            FileName = executable,
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
        };
        if (StringComparer.OrdinalIgnoreCase.Equals(
                Path.GetFileNameWithoutExtension(executable),
                "dotnet"))
        {
            string entryPath = Environment.GetCommandLineArgs()[0];
            Require(
                Path.IsPathFullyQualified(entryPath)
                && File.Exists(entryPath),
                $"cannot relaunch managed entry assembly: {entryPath}");
            startInfo.ArgumentList.Add(entryPath);
        }

        return startInfo;
    }

    private static async Task ShutdownReplayServerAsync(string baseUrl)
    {
        using HttpClient client = new()
        {
            Timeout = TimeSpan.FromSeconds(10),
        };
        using HttpResponseMessage response = await client.PostAsync(
                $"{baseUrl}/__replay__/shutdown",
                content: null)
            .ConfigureAwait(false);
        Require(
            response.IsSuccessStatusCode,
            $"replay shutdown returned {(int)response.StatusCode}");
    }

    private static CallFunctionArgs Arguments(
        params (string Name, InboundValue Value)[] values) =>
        ArgumentsWithCallId(callId: 0, values);

    private static CallFunctionArgs ArgumentsWithCallId(
        ulong callId,
        params (string Name, InboundValue Value)[] values)
    {
        CallFunctionArgs arguments = new()
        {
            CallId = callId,
        };
        foreach ((string name, InboundValue value) in values)
        {
            arguments.Kwargs.Add(
                new InboundMapEntry
                {
                    StringKey = name,
                    Value = value,
                });
        }

        return arguments;
    }

    private static InboundValue HandleValue(
        ulong key,
        int handleType,
        MediaCase? mediaCase = null)
    {
        InboundValue handle = new()
        {
            Handle = new BamlHandle
            {
                Key = key,
                HandleType = (BamlHandleType)handleType,
            },
        };
        if (mediaCase is null)
        {
            return handle;
        }

        InboundClassValue media = new()
        {
            ClassTy = new BamlTyClass
            {
                Name = MediaClassName(mediaCase),
            },
        };
        media.Fields.Add(
            new InboundMapEntry
            {
                StringKey = "_data",
                Value = handle,
            });
        return new InboundValue
        {
            ClassValue = media,
        };
    }

    private static string MediaClassName(MediaCase mediaCase) =>
        mediaCase.Name switch
        {
            "image" => "baml.media.Image",
            "audio" => "baml.media.Audio",
            "pdf" => "baml.media.Pdf",
            "video" => "baml.media.Video",
            _ => throw new InvalidOperationException(
                $"unknown media kind {mediaCase.Name}"),
        };

    private static BamlOutboundValue RequireOk(
        BamlOutboundResult result)
    {
        if (result.ResultCase
            != BamlOutboundResult.ResultOneofCase.Ok)
        {
            throw new InvalidDataException(
                $"native call failed with {result.ResultCase}: {result}");
        }

        return result.Ok;
    }

    private static string DecodeString(BamlOutboundValue value)
    {
        if (value.ValueCase
            == BamlOutboundValue.ValueOneofCase.StringValue)
        {
            return value.StringValue;
        }

        if (value.ValueCase
                == BamlOutboundValue.ValueOneofCase.LiteralValue
            && value.LiteralValue.LiteralCase
                == BamlLiteralValue.LiteralOneofCase.StringValue)
        {
            return value.LiteralValue.StringValue;
        }

        throw new InvalidDataException(
            $"expected string, received {value.ValueCase}");
    }

    private static string? EmptyToNull(string value) =>
        value.Length == 0 ? null : value;

    private static void Require(bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidOperationException(message);
        }
    }

    private sealed record MediaCase(
        string Name,
        int NativeKind,
        int HandleType,
        string MimeType);

    private sealed class ManagedMedia
    {
        private ManagedMedia(
            string? url,
            byte[]? bytes,
            string? mimeType)
        {
            Url = url;
            Bytes = bytes is null
                ? ReadOnlyMemory<byte>.Empty
                : new ReadOnlyMemory<byte>(bytes.ToArray());
            MimeType = mimeType;
        }

        public ReadOnlyMemory<byte> Bytes { get; }

        public bool IsUrl => Url is not null;

        public string? MimeType { get; }

        public string? Url { get; }

        public static ManagedMedia FromUrl(
            string url,
            string? mimeType)
        {
            ArgumentException.ThrowIfNullOrWhiteSpace(url);
            return new ManagedMedia(url, bytes: null, mimeType);
        }

        public static ManagedMedia FromBytes(
            ReadOnlySpan<byte> bytes,
            string mimeType)
        {
            ArgumentException.ThrowIfNullOrWhiteSpace(mimeType);
            return new ManagedMedia(
                url: null,
                bytes.ToArray(),
                mimeType);
        }
    }

    private sealed record PullResult(
        bool IsFinished,
        string? Value)
    {
        public static PullResult Finished { get; } =
            new(IsFinished: true, Value: null);
    }

    private sealed class ReplayServerProcess : IAsyncDisposable
    {
        private readonly Process process;
        private readonly Task<string> standardError;
        private bool disposed;

        internal ReplayServerProcess(
            Process process,
            Task<string> standardError,
            string baseUrl)
        {
            this.process = process;
            this.standardError = standardError;
            BaseUrl = baseUrl;
        }

        internal string BaseUrl { get; }

        public async ValueTask DisposeAsync()
        {
            if (disposed)
            {
                return;
            }

            disposed = true;
            try
            {
                if (!process.HasExited)
                {
                    await process.StandardInput.WriteLineAsync("stop")
                        .ConfigureAwait(false);
                    process.StandardInput.Close();
                    using CancellationTokenSource shutdownTimeout =
                        new(TimeSpan.FromSeconds(20));
                    try
                    {
                        await process.WaitForExitAsync(
                                shutdownTimeout.Token)
                            .ConfigureAwait(false);
                    }
                    catch (OperationCanceledException)
                    {
                        process.Kill(entireProcessTree: true);
                        await process.WaitForExitAsync()
                            .ConfigureAwait(false);
                        throw new TimeoutException(
                            "replay server process did not stop within 20 seconds");
                    }
                }

                string error = await standardError.ConfigureAwait(false);
                Require(
                    process.ExitCode == 0,
                    $"replay server exited with {process.ExitCode}: {error}");
            }
            finally
            {
                process.Dispose();
            }
        }
    }

    private sealed class MediaRestoreCounter
    {
        private int value;

        public int Value => Volatile.Read(ref value);

        public void Increment() =>
            Interlocked.Increment(ref value);
    }
}
