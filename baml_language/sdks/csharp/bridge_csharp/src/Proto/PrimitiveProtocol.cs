using System.Globalization;
using System.Numerics;

using Baml.Cffi;
using Baml.Generated.V1;
using Baml.Runtime;
using BamlBridge.Cffi.V1;
using Google.Protobuf;

namespace Baml.Proto;

internal sealed class BamlStreamNativeHandle : IDisposable
{
    internal BamlStreamNativeHandle(BamlSafeHandle handle, string classIdentity)
    {
        ArgumentNullException.ThrowIfNull(handle);
        ArgumentException.ThrowIfNullOrWhiteSpace(classIdentity);
        Handle = handle;
        ClassIdentity = classIdentity;
    }

    internal BamlSafeHandle Handle { get; }

    internal string ClassIdentity { get; }

    public void Dispose() => Handle.Dispose();
}

internal static class PrimitiveProtocol
{
    private const string StreamClassIdentity = "ai.stream.Stream";
    private const string StreamDoneIdentity = "ai.stream.Done";

    internal static EncodedCallArguments EncodeOwnedValue(
        BamlGeneratedValue value,
        NativeApi api,
        ulong functionCallId)
    {
        ArgumentNullException.ThrowIfNull(value);
        ArgumentNullException.ThrowIfNull(api);
        var ownership = new EncodedCallArguments([]);
        try
        {
            ownership.SetBytes(Encode(value, api, ownership, functionCallId).ToByteArray());
            return ownership;
        }
        catch
        {
            ownership.Dispose();
            throw;
        }
    }

    internal static EncodedCallArguments EncodeOwnedCallArguments<TResult>(
        BamlGeneratedArguments<TResult> arguments,
        ulong callId,
        NativeApi api)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        ArgumentNullException.ThrowIfNull(api);
        callId = NativeApi.RequireFunctionCallIdentifier(callId);
        var ownership = new EncodedCallArguments([]);
        try
        {
            var call = new CallFunctionArgs { CallId = callId };
            foreach ((ArgumentDeclaration argument, BamlGeneratedValue value) in arguments.Supplied())
            {
                call.Kwargs.Add(new InboundMapEntry
                {
                    StringKey = argument.WireIdentity,
                    Value = Encode(value, api, ownership, callId),
                });
            }

            ownership.SetBytes(call.ToByteArray());
            return ownership;
        }
        catch
        {
            ownership.Dispose();
            throw;
        }
    }

    internal static EncodedCallArguments EncodeOwnedHandleArguments(
        IReadOnlyList<KeyValuePair<string, BamlGeneratedValue>> arguments,
        ulong callId,
        NativeApi api)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        ArgumentNullException.ThrowIfNull(api);
        callId = NativeApi.RequireFunctionCallIdentifier(callId);
        var ownership = new EncodedCallArguments([]);
        try
        {
            var call = new CallFunctionArgs { CallId = callId };
            foreach ((string name, BamlGeneratedValue value) in arguments)
            {
                ArgumentException.ThrowIfNullOrEmpty(name);
                ArgumentNullException.ThrowIfNull(value);
                call.Kwargs.Add(new InboundMapEntry
                {
                    StringKey = name,
                    Value = Encode(value, api, ownership, callId),
                });
            }

            ownership.SetBytes(call.ToByteArray());
            return ownership;
        }
        catch
        {
            ownership.Dispose();
            throw;
        }
    }

    internal static EncodedCallArguments EncodeOwnedCallArguments<TResult>(
        BamlGeneratedGenericArguments<TResult> arguments,
        ulong callId,
        NativeApi api)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        ArgumentNullException.ThrowIfNull(api);
        callId = NativeApi.RequireFunctionCallIdentifier(callId);
        var ownership = new EncodedCallArguments([]);
        try
        {
            var call = new CallFunctionArgs { CallId = callId };
            foreach ((GenericArgumentDeclaration argument, BamlGeneratedValue value) in arguments.Supplied())
            {
                call.Kwargs.Add(new InboundMapEntry
                {
                    StringKey = argument.WireIdentity,
                    Value = Encode(value, api, ownership, callId),
                });
            }

            foreach (BamlGeneratedTypeBinding binding in arguments.Function.TypeBindings)
            {
                call.TypeArgs.Add(new BamlTyArg
                {
                    TypeVar = binding.Parameter.WireIdentity,
                    TypeValue = ParseTypeMetadata(
                        binding.Type.Metadata,
                        $"generic binding {binding.Parameter.WireIdentity}"),
                });
            }

            ownership.SetBytes(call.ToByteArray());
            return ownership;
        }
        catch
        {
            ownership.Dispose();
            throw;
        }
    }

    internal static byte[] EncodeCallArguments<TResult>(
        BamlGeneratedArguments<TResult> arguments,
        ulong callId)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        callId = NativeApi.RequireFunctionCallIdentifier(callId);
        var call = new CallFunctionArgs { CallId = callId };
        foreach ((ArgumentDeclaration argument, BamlGeneratedValue value) in arguments.Supplied())
        {
            call.Kwargs.Add(new InboundMapEntry
            {
                StringKey = argument.WireIdentity,
                Value = Encode(value),
            });
        }

        return call.ToByteArray();
    }

    internal static EncodedCallArguments EncodeStreamHandleArguments(
        BamlSafeHandle stream,
        ulong callId)
    {
        ArgumentNullException.ThrowIfNull(stream);
        callId = NativeApi.RequireFunctionCallIdentifier(callId);
        BamlSafeHandle transferred = stream.CloneOwned();
        var ownership = new EncodedCallArguments([]);
        try
        {
            ownership.AddTransfer(transferred);
            var call = new CallFunctionArgs { CallId = callId };
            call.Kwargs.Add(new InboundMapEntry
            {
                StringKey = "self",
                Value = new InboundValue
                {
                    Handle = new global::BamlBridge.Cffi.V1.BamlHandle
                    {
                        Key = transferred.Key,
                        HandleType = BamlHandleType.AdtTaggedHeapHandle,
                    },
                },
            });
            ownership.SetBytes(call.ToByteArray());
            return ownership;
        }
        catch
        {
            ownership.Dispose();
            throw;
        }
    }

    internal static byte[] EncodeCallArguments<TResult>(
        BamlGeneratedGenericArguments<TResult> arguments,
        ulong callId)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        callId = NativeApi.RequireFunctionCallIdentifier(callId);
        var call = new CallFunctionArgs { CallId = callId };
        foreach ((GenericArgumentDeclaration argument, BamlGeneratedValue value) in arguments.Supplied())
        {
            call.Kwargs.Add(new InboundMapEntry
            {
                StringKey = argument.WireIdentity,
                Value = Encode(value),
            });
        }

        foreach (BamlGeneratedTypeBinding binding in arguments.Function.TypeBindings)
        {
            call.TypeArgs.Add(new BamlTyArg
            {
                TypeVar = binding.Parameter.WireIdentity,
                TypeValue = ParseTypeMetadata(
                    binding.Type.Metadata,
                    $"generic binding {binding.Parameter.WireIdentity}"),
            });
        }

        return call.ToByteArray();
    }

    internal static BamlGeneratedValue DecodeCallResult(
        ReadOnlySpan<byte> bytes,
        string? bamlFunction = null,
        NativeApi? api = null)
    {
        BamlOutboundResult envelope = ParseCallResult(bytes, "BAML result");

        HostCallableProtocol.ThrowIfHostCallbackError(envelope);

        using OutboundOwnershipScope ownership = OutboundOwnershipScope.Create(envelope, api);
        var budget = new BamlDecodeBudget();
        return envelope.ResultCase switch
        {
            BamlOutboundResult.ResultOneofCase.Ok =>
                Decode(envelope.Ok, "$result", ownership, api, budget, depth: 0),
            BamlOutboundResult.ResultOneofCase.Error => throw DecodeError(
                envelope.Error,
                bamlFunction,
                ownership,
                api,
                budget),
            BamlOutboundResult.ResultOneofCase.Panic => throw DecodePanic(
                envelope.Panic,
                bamlFunction,
                ownership,
                api,
                budget),
            _ => throw new BamlProtocolException(
                "The native bridge returned an empty BAML result.",
                "BamlOutboundResult.result was absent."),
        };
    }

    internal static BamlStreamNativeHandle DecodeStreamHandle(
        ReadOnlySpan<byte> bytes,
        ReadOnlySpan<byte> expectedPartialType,
        ReadOnlySpan<byte> expectedFinalType,
        string bamlFunction,
        NativeApi api)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(bamlFunction);
        ArgumentNullException.ThrowIfNull(api);
        RequireExpectedStreamType(expectedPartialType, "partial");
        RequireExpectedStreamType(expectedFinalType, "final");
        BamlOutboundResult envelope = ParseCallResult(bytes, "BAML stream handle result");
        using OutboundOwnershipScope ownership = OutboundOwnershipScope.Create(envelope, api);
        var budget = new BamlDecodeBudget();
        switch (envelope.ResultCase)
        {
            case BamlOutboundResult.ResultOneofCase.Error:
                throw DecodeError(envelope.Error, bamlFunction, ownership, api, budget);
            case BamlOutboundResult.ResultOneofCase.Panic:
                throw DecodePanic(envelope.Panic, bamlFunction, ownership, api, budget);
            case BamlOutboundResult.ResultOneofCase.Ok:
                break;
            default:
                throw new BamlProtocolException(
                    "The native bridge returned an empty BAML stream handle result.",
                    "BamlOutboundResult.result was absent for a stream factory call.");
        }

        BamlOutboundValue value = envelope.Ok;
        if (value.ValueCase != BamlOutboundValue.ValueOneofCase.HandleValue)
        {
            throw new BamlProtocolException(
                "The native bridge returned the wrong BAML stream factory result.",
                $"Expected an owned {StreamClassIdentity} handle, received {value.ValueCase}.");
        }

        BamlOutboundHandle wire = value.HandleValue;
        if (wire.HandleType != BamlHandleType.AdtTaggedHeapHandle
            || wire.Ty?.TyCase != BamlTy.TyOneofCase.ClassTy
            || !StringComparer.Ordinal.Equals(wire.Ty.ClassTy.Name, StreamClassIdentity)
            || wire.Ty.ClassTy.TypeArgs.Count != 2)
        {
            throw new BamlProtocolException(
                "The native bridge returned an invalid BAML stream handle descriptor.",
                $"Expected {StreamClassIdentity}<partial, final> as ADT_TAGGED_HEAP_HANDLE; received {wire.HandleType} / {wire.Ty?.TyCase}.");
        }

        RequireStreamTypeMetadata(
            expectedPartialType,
            wire.Ty.ClassTy.TypeArgs[0],
            "stream partial");
        RequireStreamTypeMetadata(
            expectedFinalType,
            wire.Ty.ClassTy.TypeArgs[1],
            "stream final");
        return new BamlStreamNativeHandle(
            ownership.Claim(wire),
            wire.Ty.ClassTy.Name);
    }

    internal static BamlStreamPull<BamlGeneratedValue> DecodeStreamPull(
        ReadOnlySpan<byte> bytes,
        ReadOnlySpan<byte> expectedPartialType,
        string expectedPartialOption,
        string bamlFunction,
        NativeApi api)
    {
        RequireExpectedStreamType(expectedPartialType, "partial");
        ArgumentException.ThrowIfNullOrWhiteSpace(expectedPartialOption);
        ArgumentException.ThrowIfNullOrWhiteSpace(bamlFunction);
        ArgumentNullException.ThrowIfNull(api);
        BamlOutboundResult envelope = ParseCallResult(bytes, "BAML stream pull result");
        using OutboundOwnershipScope ownership = OutboundOwnershipScope.Create(envelope, api);
        var budget = new BamlDecodeBudget();
        switch (envelope.ResultCase)
        {
            case BamlOutboundResult.ResultOneofCase.Error:
                throw DecodeError(envelope.Error, bamlFunction, ownership, api, budget);
            case BamlOutboundResult.ResultOneofCase.Panic:
                throw DecodePanic(envelope.Panic, bamlFunction, ownership, api, budget);
            case BamlOutboundResult.ResultOneofCase.Ok:
                break;
            default:
                throw new BamlProtocolException(
                    "The native bridge returned an empty BAML stream pull result.",
                    "BamlOutboundResult.result was absent for ai.stream.Stream.next.");
        }

        BamlOutboundValue value = envelope.Ok;
        if (value.ValueCase != BamlOutboundValue.ValueOneofCase.UnionVariantValue)
        {
            throw new BamlProtocolException(
                "The native bridge returned the wrong BAML stream pull result.",
                $"Expected the typed partial-or-finished union, received {value.ValueCase}.");
        }

        BamlValueUnionVariant union = value.UnionVariantValue;
        if (!string.IsNullOrEmpty(union.Name)
            || union.IsOptional
            || union.IsSinglePattern
            || union.SelfType?.TyCase != BamlTy.TyOneofCase.Union
            || union.SelfType.Union.Options.Count != 2)
        {
            throw new BamlProtocolException(
                "The native bridge returned an invalid BAML stream pull descriptor.",
                "Stream.next must return the exact unnamed two-arm partial-or-Done union.");
        }

        int partialArmCount = 0;
        int finishedArmCount = 0;
        foreach (BamlTy option in union.SelfType.Union.Options)
        {
            if (expectedPartialType.SequenceEqual(option.ToByteArray()))
            {
                partialArmCount++;
            }
            if (IsFinishedType(option))
            {
                finishedArmCount++;
            }
        }
        if (partialArmCount != 1 || finishedArmCount != 1)
        {
            throw new BamlProtocolException(
                "The native bridge returned contradictory stream pull type metadata.",
                $"Expected one {Convert.ToHexString(expectedPartialType)} arm and one non-generic {StreamDoneIdentity} arm; received {partialArmCount} and {finishedArmCount} matches.");
        }
        if (union.Value is null)
        {
            throw new BamlProtocolException(
                "The native bridge returned a BAML stream pull without a payload.",
                "The selected stream union arm omitted its value.");
        }

        if (StringComparer.Ordinal.Equals(union.ValueOptionName, expectedPartialOption))
        {
            return BamlStreamPull<BamlGeneratedValue>.FromPartial(
                Decode(
                    union.Value,
                    $"$stream<{expectedPartialOption}>",
                    ownership,
                    api,
                    budget,
                    depth: 1));
        }

        if (!StringComparer.Ordinal.Equals(union.ValueOptionName, StreamDoneIdentity))
        {
            throw new BamlProtocolException(
                "The native bridge selected an unknown BAML stream pull arm.",
                $"Expected {expectedPartialOption} or {StreamDoneIdentity}, received {union.ValueOptionName}.");
        }

        if (union.Value.ValueCase != BamlOutboundValue.ValueOneofCase.ClassValue
            || !StringComparer.Ordinal.Equals(
                union.Value.ClassValue.Name,
                StreamDoneIdentity)
            || union.Value.ClassValue.Fields.Count != 0
            || union.Value.ClassValue.TypeArgs.Count != 0)
        {
            throw new BamlProtocolException(
                "The native bridge returned an invalid BAML stream-finished payload.",
                $"The {StreamDoneIdentity} arm must contain its empty nominal class value.");
        }

        return BamlStreamPull<BamlGeneratedValue>.Finished;
    }

    internal static void ReleaseOwnedCallResult(ReadOnlySpan<byte> bytes, NativeApi api)
    {
        ArgumentNullException.ThrowIfNull(api);
        BamlOutboundResult envelope = ParseCallResult(bytes, "late BAML result");

        using OutboundOwnershipScope ownership = OutboundOwnershipScope.Create(envelope, api);
    }

    private static BamlOutboundResult ParseCallResult(
        ReadOnlySpan<byte> bytes,
        string description)
    {
        try
        {
            return BamlOutboundResult.Parser.ParseFrom(bytes);
        }
        catch (InvalidProtocolBufferException error)
        {
            throw new BamlProtocolException(
                $"The native bridge returned a malformed {description}.",
                error.Message);
        }
    }

    private static void RequireExpectedStreamType(
        ReadOnlySpan<byte> metadata,
        string description)
    {
        if (metadata.IsEmpty)
        {
            throw new ArgumentException(
                $"The generated stream {description} type metadata is empty.",
                nameof(metadata));
        }
    }

    private static void RequireStreamTypeMetadata(
        ReadOnlySpan<byte> expected,
        BamlTy actual,
        string description)
    {
        if (!expected.SequenceEqual(actual.ToByteArray()))
        {
            throw new BamlProtocolException(
                $"The native bridge returned contradictory {description} type metadata.",
                $"Expected {Convert.ToHexString(expected)}, received {Convert.ToHexString(actual.ToByteArray())}.");
        }
    }

    private static bool IsFinishedType(BamlTy actual) =>
        actual.TyCase == BamlTy.TyOneofCase.ClassTy
        && StringComparer.Ordinal.Equals(actual.ClassTy.Name, StreamDoneIdentity)
        && actual.ClassTy.TypeArgs.Count == 0;

    private static BamlExecutionException DecodeError(
        BamlOutboundError error,
        string? bamlFunction,
        OutboundOwnershipScope ownership,
        NativeApi? api,
        BamlDecodeBudget budget)
    {
        if (error.Value is null)
        {
            throw new BamlProtocolException(
                "The native bridge returned an invalid BAML error.",
                "BamlOutboundError.value was absent.");
        }

        var thrownValue = new BamlValue(
            Decode(error.Value, "$error", ownership, api, budget, depth: 0));
        var trace = new BamlTrace(error.Trace);
        return StringComparer.Ordinal.Equals(
            thrownValue.NominalTypeName,
            "baml.errors.TypeMismatch")
            ? new BamlTypeMismatchException(
                "The BAML call returned a value that did not satisfy its expected BAML type.",
                thrownValue,
                bamlFunction,
                trace)
            : new BamlErrorException(
                "The BAML call returned a typed error.",
                thrownValue,
                bamlFunction,
                trace);
    }

    private static Exception DecodePanic(
        BamlOutboundPanic panic,
        string? bamlFunction,
        OutboundOwnershipScope ownership,
        NativeApi? api,
        BamlDecodeBudget budget)
    {
        if (panic.IsExitPanic)
        {
            BamlProcessExit.Exit(panic.ExitCode);
        }

        if (panic.Value is null)
        {
            return new BamlProtocolException(
                "The native bridge returned an invalid BAML panic.",
                "BamlOutboundPanic.value was absent.");
        }

        var value = new BamlValue(
            Decode(panic.Value, "$panic", ownership, api, budget, depth: 0));
        var trace = new BamlTrace(panic.Trace);
        if (StringComparer.Ordinal.Equals(
            value.NominalTypeName,
            "baml.panics.Cancelled"))
        {
            return new BamlOperationCanceledException(
                "The BAML call was canceled by the engine.",
                BamlCancellationOrigin.Engine,
                BamlCancellationTokens.CreateEngineToken(),
                bamlFunction,
                trace);
        }

        return new BamlPanicException(
            "The BAML call panicked.",
            bamlFunction,
            new BamlPanicInfo(value, isExitPanic: false, exitCode: null),
            trace);
    }

    internal static InboundValue Encode(BamlGeneratedValue value) =>
        Encode(value, api: null, ownership: null, functionCallId: 0);

    private static InboundValue Encode(
        BamlGeneratedValue value,
        NativeApi? api,
        EncodedCallArguments? ownership,
        ulong functionCallId)
    {
        ArgumentNullException.ThrowIfNull(value);
        return value.Kind switch
        {
            PrimitiveCarrierKind.Null => new InboundValue(),
            PrimitiveCarrierKind.Bool => new InboundValue { BoolValue = value.ReadBool() },
            PrimitiveCarrierKind.Int => new InboundValue
            {
                IntValue = BamlInteger.Require(value.ReadInt(), "generated argument"),
            },
            PrimitiveCarrierKind.Float => EncodeFloat(value.ReadFloat()),
            PrimitiveCarrierKind.String => new InboundValue { StringValue = value.ReadString() },
            PrimitiveCarrierKind.Bytes => new InboundValue
            {
                Uint8ArrayValue = ByteString.CopyFrom(value.ReadBytes()),
            },
            PrimitiveCarrierKind.BigInt => new InboundValue
            {
                BigintValue = FormatBigInt(value.ReadBigInt()),
            },
            PrimitiveCarrierKind.List => EncodeList(value, api, ownership, functionCallId),
            PrimitiveCarrierKind.Map => EncodeMap(value, api, ownership, functionCallId),
            PrimitiveCarrierKind.Class => EncodeClass(value, api, ownership, functionCallId),
            PrimitiveCarrierKind.Enum => EncodeEnum(value),
            PrimitiveCarrierKind.Union => EncodeUnion(value, api, ownership, functionCallId),
            PrimitiveCarrierKind.Media => MediaProtocol.Encode(
                RequireNativeApi(api, value.Kind),
                RequireOwnership(ownership, value.Kind),
                value.ReadMedia()),
            PrimitiveCarrierKind.Prompt => new InboundValue
            {
                PromptAstValue = value.ReadPromptAst(),
            },
            PrimitiveCarrierKind.Type => new InboundValue
            {
                TyDefValue = value.ReadType().WireCopy(),
            },
            PrimitiveCarrierKind.Handle => EncodeHandle(
                value.ReadHandle(),
                RequireOwnership(ownership, value.Kind)),
            PrimitiveCarrierKind.HostCallable => HostCallableProtocol.Encode(
                value.ReadHostCallable(),
                RequireNativeApi(api, value.Kind),
                RequireOwnership(ownership, value.Kind),
                functionCallId),
            _ => throw Unsupported(value.Kind),
        };
    }

    internal static BamlGeneratedValue Decode(BamlOutboundValue value)
    {
        ArgumentNullException.ThrowIfNull(value);
        var envelope = new BamlOutboundResult { Ok = value };
        using OutboundOwnershipScope ownership = OutboundOwnershipScope.Create(envelope, api: null);
        return Decode(
            value,
            "$result",
            ownership,
            api: null,
            new BamlDecodeBudget(),
            depth: 0);
    }

    private static BamlGeneratedValue Decode(
        BamlOutboundValue value,
        string path,
        OutboundOwnershipScope ownership,
        NativeApi? api,
        BamlDecodeBudget budget,
        int depth)
    {
        ArgumentNullException.ThrowIfNull(value);
        budget.Visit(path, depth);
        return value.ValueCase switch
        {
            BamlOutboundValue.ValueOneofCase.None =>
                BamlGeneratedValue.CreateNull(path),
            BamlOutboundValue.ValueOneofCase.NullValue =>
                BamlGeneratedValue.CreateNull(path),
            BamlOutboundValue.ValueOneofCase.BoolValue =>
                BamlGeneratedValue.CreateBool(value.BoolValue, path),
            BamlOutboundValue.ValueOneofCase.IntValue =>
                BamlGeneratedValue.CreateInt(
                    BamlInteger.Require(value.IntValue, path),
                    path),
            BamlOutboundValue.ValueOneofCase.FloatValue =>
                FiniteFloat(value.FloatValue, path),
            BamlOutboundValue.ValueOneofCase.StringValue =>
                BamlGeneratedValue.CreateString(value.StringValue, path),
            BamlOutboundValue.ValueOneofCase.Uint8ArrayValue =>
                DecodeBytes(value.Uint8ArrayValue, path),
            BamlOutboundValue.ValueOneofCase.BigintValue =>
                BamlGeneratedValue.CreateBigInt(ParseBigInt(value.BigintValue, path), path),
            BamlOutboundValue.ValueOneofCase.LiteralValue =>
                DecodeLiteral(value.LiteralValue, path),
            BamlOutboundValue.ValueOneofCase.ListValue =>
                DecodeList(value.ListValue, path, ownership, api, budget, depth),
            BamlOutboundValue.ValueOneofCase.MapValue =>
                DecodeMap(value.MapValue, path, ownership, api, budget, depth),
            BamlOutboundValue.ValueOneofCase.ClassValue =>
                DecodeClass(value.ClassValue, path, ownership, api, budget, depth),
            BamlOutboundValue.ValueOneofCase.EnumValue =>
                DecodeEnum(value.EnumValue, path),
            BamlOutboundValue.ValueOneofCase.UnionVariantValue =>
                DecodeUnion(value.UnionVariantValue, path, ownership, api, budget, depth),
            BamlOutboundValue.ValueOneofCase.MediaValue =>
                MediaProtocol.DecodeInline(value.MediaValue, path),
            BamlOutboundValue.ValueOneofCase.PromptAstValue =>
                BamlGeneratedValue.CreatePromptAst(value.PromptAstValue, path),
            BamlOutboundValue.ValueOneofCase.TyValue =>
                BamlGeneratedValue.CreateType(
                    new global::Baml.BamlType(value.TyValue),
                    path),
            BamlOutboundValue.ValueOneofCase.TyDefValue =>
                BamlGeneratedValue.CreateType(
                    new global::Baml.BamlType(value.TyDefValue),
                    path),
            BamlOutboundValue.ValueOneofCase.HandleValue =>
                DecodeHandle(value.HandleValue, path, ownership, api, budget, depth),
            _ => throw new BamlProtocolException(
                "The native bridge returned an unsupported value in the managed runtime slice.",
                $"Unsupported BamlOutboundValue case {value.ValueCase} at {path}."),
        };
    }

    private static InboundValue EncodeList(
        BamlGeneratedValue value,
        NativeApi? api,
        EncodedCallArguments? ownership,
        ulong functionCallId)
    {
        var list = new InboundListValue();
        foreach (BamlGeneratedValue item in value.ReadList())
        {
            list.Values.Add(Encode(item, api, ownership, functionCallId));
        }

        var inbound = new InboundValue { ListValue = list };
        if (value.ItemTypeMetadata is { Length: > 0 } itemTypeMetadata)
        {
            inbound.ValueType = new BamlTy
            {
                List = new BamlTyList
                {
                    Item = ParseTypeMetadata(itemTypeMetadata, "list item"),
                },
            };
        }

        return inbound;
    }

    private static InboundValue EncodeMap(
        BamlGeneratedValue value,
        NativeApi? api,
        EncodedCallArguments? ownership,
        ulong functionCallId)
    {
        var map = new InboundMapValue();
        foreach ((string key, BamlGeneratedValue item) in value.ReadMapEntries())
        {
            map.Entries.Add(new InboundMapEntry
            {
                StringKey = key,
                Value = Encode(item, api, ownership, functionCallId),
            });
        }

        var inbound = new InboundValue { MapValue = map };
        if (value.KeyTypeMetadata is { Length: > 0 } keyTypeMetadata
            && value.ValueTypeMetadata is { Length: > 0 } valueTypeMetadata)
        {
            inbound.ValueType = new BamlTy
            {
                Map = new BamlTyMap
                {
                    Key = ParseTypeMetadata(keyTypeMetadata, "map key"),
                    Value = ParseTypeMetadata(valueTypeMetadata, "map value"),
                },
            };
        }

        return inbound;
    }

    private static InboundValue EncodeClass(
        BamlGeneratedValue value,
        NativeApi? api,
        EncodedCallArguments? ownership,
        ulong functionCallId)
    {
        var classType = new BamlTyClass { Name = value.ReadClassIdentityForEncode() };
        foreach (byte[] metadata in value.ReadClassTypeArgumentsForEncode())
        {
            classType.TypeArgs.Add(ParseTypeMetadata(metadata, "class type argument"));
        }
        var @class = new InboundClassValue();
        foreach ((string name, BamlGeneratedValue field) in value.ReadClassFields())
        {
            @class.Fields.Add(new InboundMapEntry
            {
                StringKey = name,
                Value = Encode(field, api, ownership, functionCallId),
            });
        }

        return new InboundValue
        {
            ValueType = new BamlTy { ClassTy = classType },
            ClassValue = @class,
        };
    }

    private static InboundValue EncodeEnum(BamlGeneratedValue value) =>
        new()
        {
            EnumValue = new InboundEnumValue
            {
                Name = value.ReadEnumIdentityForEncode(),
                Value = value.ReadEnumWireValueForEncode(),
            },
        };

    private static InboundValue EncodeUnion(
        BamlGeneratedValue value,
        NativeApi? api,
        EncodedCallArguments? ownership,
        ulong functionCallId)
    {
        InboundValue encoded =
            Encode(value.ReadUnionPayload(), api, ownership, functionCallId);
        if (encoded.ValueType is null
            && value.UnionSelectedTypeMetadata is { Length: > 0 } selectedTypeMetadata)
        {
            BamlTy selectedType =
                ParseTypeMetadata(selectedTypeMetadata, "selected union option");
            if (selectedType.TyCase is BamlTy.TyOneofCase.Union
                or BamlTy.TyOneofCase.Optional)
            {
                throw new BamlProtocolException(
                    "A generated union codec selected a non-concrete inbound type.",
                    "InboundValue.value_type must identify the selected value node, not a union or optional shell.");
            }

            encoded.ValueType = selectedType;
        }

        return encoded;
    }

    private static InboundValue EncodeHandle(
        global::Baml.BamlHandle value,
        EncodedCallArguments ownership)
    {
        BamlSafeHandle transferred = value.CloneOwnedHandle();
        ownership.AddTransfer(transferred);
        return new InboundValue
        {
            Handle = new global::BamlBridge.Cffi.V1.BamlHandle
            {
                Key = transferred.Key,
                HandleType = value.HandleType,
            },
        };
    }

    private static NativeApi RequireNativeApi(NativeApi? api, PrimitiveCarrierKind kind) =>
        api ?? throw new BamlProtocolException(
            "The managed bridge cannot encode an owned value without a native call context.",
            $"Generated carrier kind {kind} requires native ownership.");

    private static EncodedCallArguments RequireOwnership(
        EncodedCallArguments? ownership,
        PrimitiveCarrierKind kind) =>
        ownership ?? throw new BamlProtocolException(
            "The managed bridge cannot encode an owned value without a native call context.",
            $"Generated carrier kind {kind} requires an ownership transaction.");

    private static BamlGeneratedValue DecodeBytes(ByteString value, string path)
    {
        BamlDecodeBudget.RequireBytes(value.Length, path);
        return BamlGeneratedValue.CreateBytes(value.Span, path);
    }

    private static BamlGeneratedValue DecodeList(
        BamlValueList list,
        string path,
        OutboundOwnershipScope ownership,
        NativeApi? api,
        BamlDecodeBudget budget,
        int depth)
    {
        ArgumentNullException.ThrowIfNull(list);
        BamlDecodeBudget.RequireCollection(list.Items.Count, path);
        var items = new List<BamlGeneratedValue>(list.Items.Count);
        for (int index = 0; index < list.Items.Count; index++)
        {
            items.Add(Decode(
                list.Items[index],
                $"{path}[{index}]",
                ownership,
                api,
                budget,
                depth + 1));
        }

        return BamlGeneratedValue.CreateList(
            items.AsReadOnly(),
            Metadata(list.ItemType, path, "list item"),
            path);
    }

    private static BamlGeneratedValue DecodeMap(
        BamlValueMap map,
        string path,
        OutboundOwnershipScope ownership,
        NativeApi? api,
        BamlDecodeBudget budget,
        int depth)
    {
        ArgumentNullException.ThrowIfNull(map);
        BamlDecodeBudget.RequireCollection(map.Entries.Count, path);
        var entries = new List<KeyValuePair<string, BamlGeneratedValue>>(map.Entries.Count);
        foreach (BamlOutboundMapEntry entry in map.Entries)
        {
            string entryPath = MapPath(path, entry.Key);
            entries.Add(new(
                entry.Key,
                Decode(entry.Value, entryPath, ownership, api, budget, depth + 1)));
        }

        return BamlGeneratedValue.CreateMap(
            entries.AsReadOnly(),
            Metadata(map.KeyType, path, "map key"),
            Metadata(map.ValueType, path, "map value"),
            path);
    }

    private static BamlGeneratedValue DecodeClass(
        BamlValueClass @class,
        string path,
        OutboundOwnershipScope ownership,
        NativeApi? api,
        BamlDecodeBudget budget,
        int depth)
    {
        ArgumentNullException.ThrowIfNull(@class);
        if (string.IsNullOrEmpty(@class.Name))
        {
            throw new BamlProtocolException(
                "The native bridge returned a class without an identity.",
                $"Class identity was empty at {path}.");
        }

        if (MediaProtocol.TryContract(@class.Name, out MediaContract mediaContract))
        {
            return DecodeMediaClass(@class, mediaContract, path, ownership, api);
        }

        BamlDecodeBudget.RequireCollection(@class.Fields.Count, path);
        var fields = new List<KeyValuePair<string, BamlGeneratedValue>>(@class.Fields.Count);
        foreach (BamlOutboundMapEntry field in @class.Fields)
        {
            fields.Add(new(
                field.Key,
                Decode(
                    field.Value,
                    MapPath(path, field.Key),
                    ownership,
                    api,
                    budget,
                    depth + 1)));
        }

        IReadOnlyList<byte[]> typeArguments = @class.TypeArgs
            .Select(argument => Metadata(argument, path, "class type argument"))
            .ToList()
            .AsReadOnly();
        return BamlGeneratedValue.CreateClass(
            @class.Name,
            fields.AsReadOnly(),
            typeArguments,
            path);
    }

    private static BamlGeneratedValue DecodeMediaClass(
        BamlValueClass @class,
        MediaContract expected,
        string path,
        OutboundOwnershipScope ownership,
        NativeApi? api)
    {
        if (@class.TypeArgs.Count != 0
            || @class.Fields.Count != 1
            || !StringComparer.Ordinal.Equals(@class.Fields[0].Key, "_data")
            || @class.Fields[0].Value is null)
        {
            throw new BamlProtocolException(
                "The native bridge returned malformed BAML media.",
                $"Media class {@class.Name} at {path} must have no type arguments and exactly one _data field.");
        }

        BamlOutboundValue data = @class.Fields[0].Value;
        if (data.ValueCase == BamlOutboundValue.ValueOneofCase.MediaValue)
        {
            BamlGeneratedValue decoded = MediaProtocol.DecodeInline(data.MediaValue, path);
            if (!MediaProtocol.TryContract(
                    decoded.ReadMedia(),
                    out MediaContract actual,
                    out _)
                || actual.MediaType != expected.MediaType)
            {
                throw new BamlProtocolException(
                    "The native bridge returned contradictory BAML media metadata.",
                    $"Media class {@class.Name} at {path} contained inline {actual.MediaType}.");
            }

            return decoded;
        }

        if (data.ValueCase != BamlOutboundValue.ValueOneofCase.HandleValue)
        {
            throw new BamlProtocolException(
                "The native bridge returned malformed BAML media.",
                $"Media class {@class.Name} at {path} contained {data.ValueCase} instead of a media handle.");
        }

        BamlOutboundHandle wire = data.HandleValue;
        if (wire.HandleType != expected.HandleType)
        {
            throw new BamlProtocolException(
                "The native bridge returned contradictory BAML media metadata.",
                $"Media class {@class.Name} at {path} contained handle type {wire.HandleType}; expected {expected.HandleType}.");
        }

        NativeApi nativeApi = api ?? throw new BamlProtocolException(
            "The native bridge returned owned BAML media outside a native call context.",
            $"Media class {@class.Name} at {path} requires native accessors.");
        return MediaProtocol.DecodeHandle(
            nativeApi,
            ownership.Borrow(wire),
            wire.HandleType,
            expected,
            path);
    }

    private static BamlGeneratedValue DecodeHandle(
        BamlOutboundHandle wire,
        string path,
        OutboundOwnershipScope ownership,
        NativeApi? api,
        BamlDecodeBudget budget,
        int depth)
    {
        ArgumentNullException.ThrowIfNull(wire);
        if (MediaProtocol.TryContract(wire.HandleType, out MediaContract mediaContract))
        {
            NativeApi nativeApi = api ?? throw new BamlProtocolException(
                "The native bridge returned owned BAML media outside a native call context.",
                $"Media handle {wire.Key} at {path} requires native accessors.");
            return MediaProtocol.DecodeHandle(
                nativeApi,
                ownership.Borrow(wire),
                wire.HandleType,
                mediaContract,
                path);
        }

        string fqn = wire.HandleType switch
        {
            BamlHandleType.FunctionRef => "baml.internal.Function",
            BamlHandleType.UntaggedRustData => "baml.internal.RustData",
            BamlHandleType.UntaggedBexHeap => "baml.internal.HeapValue",
            BamlHandleType.AdtPromptAst => "baml.llm.PromptAst",
            BamlHandleType.AdtFunctionSpec => "ai.FunctionSpec",
            BamlHandleType.AdtTaggedHeapHandle => TaggedHandleIdentity(wire, path),
            _ => throw new BamlProtocolException(
                "The native bridge returned an unsupported BAML handle.",
                $"Handle type {wire.HandleType} at {path} has no public managed projection."),
        };
        byte[]? metadata = wire.Ty is null || wire.Ty.TyCase == BamlTy.TyOneofCase.None
            ? null
            : wire.Ty.ToByteArray();
        global::Baml.BamlTypeDescriptor descriptor =
            global::Baml.BamlTypeDescriptor.CreateHandle(fqn);
        if (wire.HandleType is BamlHandleType.AdtTaggedHeapHandle
            or BamlHandleType.AdtFunctionSpec)
        {
            global::Baml.BamlTypeDescriptor encodedType =
                global::Baml.BamlTypeDescriptor.FromMetadata(metadata);
            if (encodedType.Kind != global::Baml.BamlTypeDescriptorKind.Class
                || !StringComparer.Ordinal.Equals(encodedType.Fqn, fqn))
            {
                throw new BamlProtocolException(
                    "The native bridge returned contradictory opaque-handle metadata.",
                    $"Opaque handle {wire.Key} at {path} identified {fqn}, but its descriptor was {encodedType}.");
            }

            descriptor = global::Baml.BamlTypeDescriptor.CreateHandle(
                fqn,
                encodedType.Arguments);
        }
        var handle = new global::Baml.BamlHandle(
            ownership.Claim(wire),
            descriptor,
            wire.HandleType,
            metadata);
        return BamlGeneratedValue.CreateHandle(handle, sourcePath: path);
    }

    private static string TaggedHandleIdentity(BamlOutboundHandle wire, string path)
    {
        if (wire.Ty is null)
        {
            throw new BamlProtocolException(
                "The native bridge returned a tagged handle without type metadata.",
                $"Tagged handle {wire.Key} at {path} omitted BamlTy.");
        }

        string identity = wire.Ty.TyCase switch
        {
            BamlTy.TyOneofCase.ClassTy => wire.Ty.ClassTy.Name,
            BamlTy.TyOneofCase.Interface => wire.Ty.Interface.Name,
            _ => string.Empty,
        };
        if (string.IsNullOrEmpty(identity))
        {
            throw new BamlProtocolException(
                "The native bridge returned invalid tagged-handle type metadata.",
                $"Tagged handle {wire.Key} at {path} used BamlTy case {wire.Ty.TyCase} without a nominal identity.");
        }

        return identity;
    }

    private static BamlGeneratedValue DecodeEnum(BamlValueEnum @enum, string path)
    {
        ArgumentNullException.ThrowIfNull(@enum);
        if (string.IsNullOrEmpty(@enum.Name))
        {
            throw new BamlProtocolException(
                "The native bridge returned an enum without an identity.",
                $"Enum identity was empty at {path}.");
        }

        return BamlGeneratedValue.CreateEnum(
            @enum.Name,
            @enum.Value,
            @enum.IsDynamic,
            path);
    }

    private static BamlGeneratedValue DecodeUnion(
        BamlValueUnionVariant union,
        string path,
        OutboundOwnershipScope ownership,
        NativeApi? api,
        BamlDecodeBudget budget,
        int depth)
    {
        ArgumentNullException.ThrowIfNull(union);
        if (union.Value is null)
        {
            throw new BamlProtocolException(
                "The native bridge returned a union without a payload.",
                $"Union payload was absent at {path}.");
        }

        byte[] selfType = Metadata(union.SelfType, path, "union self");
        byte[]? selectedType = SelectedUnionType(union, path);
        if (selectedType is null && string.IsNullOrEmpty(union.ValueOptionName))
        {
            throw new BamlProtocolException(
                "The native bridge returned a union without an active option identity.",
                $"Union value_option_name and selected_option_index were both absent at {path}.");
        }

        string selectedDisplayName = string.IsNullOrEmpty(union.ValueOptionName)
            ? union.HasSelectedOptionIndex
                ? $"option {union.SelectedOptionIndex}"
                : "selected option"
            : union.ValueOptionName;
        BamlGeneratedValue payload = Decode(
            union.Value,
            $"{path}<{selectedDisplayName}>",
            ownership,
            api,
            budget,
            depth + 1);
        return BamlGeneratedValue.CreateUnion(
            selfType,
            selectedType,
            union.ValueOptionName,
            payload,
            path);
    }

    private static byte[]? SelectedUnionType(BamlValueUnionVariant union, string path)
    {
        if (!union.HasSelectedOptionIndex)
        {
            return null;
        }

        if (union.SelfType?.TyCase != BamlTy.TyOneofCase.Union)
        {
            throw new BamlProtocolException(
                "The native bridge returned an indexed union without a union self type.",
                $"Union self_type at {path} was {union.SelfType?.TyCase.ToString() ?? "absent"}.");
        }

        uint selectedIndex = union.SelectedOptionIndex;
        if (selectedIndex >= union.SelfType.Union.Options.Count)
        {
            throw new BamlProtocolException(
                "The native bridge returned an out-of-range union option index.",
                $"Union selected_option_index {selectedIndex} at {path} is outside self_type's {union.SelfType.Union.Options.Count} option(s).");
        }

        BamlTy selectedType = union.SelfType.Union.Options[(int)selectedIndex].Clone();
        while (selectedType.TyCase == BamlTy.TyOneofCase.Optional
            && selectedType.Optional?.Inner is not null)
        {
            selectedType = selectedType.Optional.Inner.Clone();
        }

        return selectedType.ToByteArray();
    }

    private static BamlGeneratedValue DecodeLiteral(BamlLiteralValue literal, string path)
    {
        BamlGeneratedValue decoded = literal.LiteralCase switch
        {
            BamlLiteralValue.LiteralOneofCase.StringValue =>
                BamlGeneratedValue.CreateString(literal.StringValue, path),
            BamlLiteralValue.LiteralOneofCase.IntValue =>
                BamlGeneratedValue.CreateInt(BamlInteger.Require(literal.IntValue, path), path),
            BamlLiteralValue.LiteralOneofCase.BoolValue =>
                BamlGeneratedValue.CreateBool(literal.BoolValue, path),
            BamlLiteralValue.LiteralOneofCase.FloatValue =>
                ParseLiteralFloat(literal.FloatValue, path),
            BamlLiteralValue.LiteralOneofCase.BigintValue =>
                BamlGeneratedValue.CreateBigInt(
                    ParseBigInt(literal.BigintValue, path),
                    path),
            _ => throw new BamlProtocolException(
                "The native bridge returned an unsupported literal in the managed runtime slice.",
                $"Unsupported BamlLiteralValue case {literal.LiteralCase} at {path}."),
        };
        var typeLiteral = new BamlTyLiteral();
        switch (literal.LiteralCase)
        {
            case BamlLiteralValue.LiteralOneofCase.StringValue:
                typeLiteral.StringValue = literal.StringValue;
                break;
            case BamlLiteralValue.LiteralOneofCase.IntValue:
                typeLiteral.IntValue = literal.IntValue;
                break;
            case BamlLiteralValue.LiteralOneofCase.BoolValue:
                typeLiteral.BoolValue = literal.BoolValue;
                break;
            case BamlLiteralValue.LiteralOneofCase.FloatValue:
                typeLiteral.FloatValue = literal.FloatValue;
                break;
            case BamlLiteralValue.LiteralOneofCase.BigintValue:
                typeLiteral.BigintValue = literal.BigintValue;
                break;
        }

        return decoded.WithOccurrenceType(new BamlTy { Literal = typeLiteral }.ToByteArray());
    }

    private static BamlGeneratedValue ParseLiteralFloat(string source, string path)
    {
        if (!double.TryParse(
                source,
                NumberStyles.Float,
                CultureInfo.InvariantCulture,
                out double value))
        {
            throw new BamlProtocolException(
                "The native bridge returned an invalid float literal.",
                $"Could not parse BAML float literal {source} at {path}.");
        }

        return FiniteFloat(value, path);
    }

    private static BigInteger ParseBigInt(string source, string path)
    {
        if (string.IsNullOrEmpty(source))
        {
            throw InvalidBigInt(source, path);
        }

        bool negative = source[0] == '-';
        ReadOnlySpan<char> digits = negative ? source.AsSpan(1) : source.AsSpan();
        if (digits.IsEmpty
            || digits.IndexOfAnyExceptInRange('0', '9') >= 0
                && digits.IndexOfAnyExcept("0123456789abcdefABCDEF") >= 0)
        {
            throw InvalidBigInt(source, path);
        }

        BigInteger magnitude;
        try
        {
            magnitude = BigInteger.Parse(
                string.Concat("0", digits),
                NumberStyles.AllowHexSpecifier,
                CultureInfo.InvariantCulture);
        }
        catch (FormatException)
        {
            throw InvalidBigInt(source, path);
        }

        return negative ? -magnitude : magnitude;
    }

    private static string FormatBigInt(BigInteger value) =>
        value.Sign < 0
            ? "-" + BigInteger.Abs(value).ToString("x", CultureInfo.InvariantCulture)
            : value.ToString("x", CultureInfo.InvariantCulture);

    private static BamlProtocolException InvalidBigInt(string source, string path) =>
        new(
            "The native bridge returned an invalid BAML bigint.",
            $"Could not parse hexadecimal bigint {source} at {path}.");

    private static InboundValue EncodeFloat(double value)
    {
        if (!double.IsFinite(value))
        {
            throw new BamlProtocolException(
                "A generated codec produced a non-finite BAML float.",
                $"Generated argument contained {value}.");
        }

        return new InboundValue { FloatValue = value };
    }

    private static BamlGeneratedValue FiniteFloat(double value, string path)
    {
        if (!double.IsFinite(value))
        {
            throw new BamlProtocolException(
                "The native bridge returned a non-finite BAML float.",
                $"BAML float at {path} was {value}.");
        }

        return BamlGeneratedValue.CreateFloat(value, path);
    }

    private static byte[] Metadata(BamlTy? type, string path, string description)
    {
        if (type is null || type.TyCase == BamlTy.TyOneofCase.None)
        {
            throw new BamlProtocolException(
                $"The native bridge omitted {description} type metadata.",
                $"Missing {description} type at {path}.");
        }

        return type.ToByteArray();
    }

    private static BamlTy ParseTypeMetadata(byte[]? metadata, string description)
    {
        if (metadata is null || metadata.Length == 0)
        {
            throw new BamlProtocolException(
                $"A generated codec omitted {description} type metadata.",
                $"Missing inbound {description} type metadata.");
        }

        try
        {
            BamlTy type = BamlTy.Parser.ParseFrom(metadata);
            if (type.TyCase == BamlTy.TyOneofCase.None)
            {
                throw new BamlProtocolException(
                    $"A generated codec produced empty {description} type metadata.",
                    $"Inbound {description} type metadata has no type case.");
            }

            return type;
        }
        catch (InvalidProtocolBufferException error)
        {
            throw new BamlProtocolException(
                $"A generated codec produced malformed {description} type metadata.",
                error.Message);
        }
    }

    private static string MapPath(string path, string key) =>
        $"{path}[{CSharpQuoted(key)}]";

    private static string CSharpQuoted(string value) =>
        "\"" + value.Replace("\\", "\\\\", StringComparison.Ordinal)
            .Replace("\"", "\\\"", StringComparison.Ordinal) + "\"";

    private static BamlProtocolException Unsupported(PrimitiveCarrierKind kind) =>
        new(
            "A generated codec produced an unsupported value in the managed runtime slice.",
            $"Unsupported generated carrier {kind}.");
}
