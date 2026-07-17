using BamlBridge.Cffi.V1;
using Google.Protobuf;
using System.Runtime.ExceptionServices;

namespace Baml.Bridge;

internal static class CallDispatcher
{
    internal static async Task<T> CallAsync<T>(
        string functionName,
        IReadOnlyList<(string Name, object? Value)> arguments,
        IReadOnlyList<(string Name, Type Type)> typeArguments,
        CancellationToken cancellationToken)
    {
        var response = await CallRawAsync(
                functionName,
                arguments,
                typeArguments,
                cancellationToken)
            .ConfigureAwait(false);
        return ProtoCodec.DecodeResult<T>(response);
    }

    internal static async Task<BamlUnion<TPartial, BamlStreamFinished>> CallStreamNextAsync<TPartial>(
        IBamlStreamValue stream,
        CancellationToken cancellationToken)
    {
        var response = await CallRawAsync(
                "baml.llm.Stream.next",
                [("self", stream)],
                Array.Empty<(string Name, Type Type)>(),
                cancellationToken)
            .ConfigureAwait(false);
        return ProtoCodec.DecodeStreamNext<TPartial>(response);
    }

    private static async Task<ReadOnlyMemory<byte>> CallRawAsync(
        string functionName,
        IReadOnlyList<(string Name, object? Value)> arguments,
        IReadOnlyList<(string Name, Type Type)> typeArguments,
        CancellationToken cancellationToken)
    {
        var nativeCallId = NativeApi.NewFunctionCall();
        if (nativeCallId == 0)
        {
            throw new BamlBridgeException("The native runtime returned an invalid function-call ID.");
        }

        using var encoding = new ProtoCodec.EncodeContext();
        var request = new CallFunctionArgs { CallId = nativeCallId };
        var typeArgumentNames = new HashSet<string>(StringComparer.Ordinal);
        foreach (var (name, type) in typeArguments)
        {
            ArgumentException.ThrowIfNullOrWhiteSpace(name);
            ArgumentNullException.ThrowIfNull(type);
            if (name.Contains('\0', StringComparison.Ordinal) || !typeArgumentNames.Add(name))
            {
                throw new BamlBridgeException($"Invalid or duplicate BAML type-variable binding {name}.");
            }

            request.TypeArgs.Add(new BamlTyArg
            {
                TypeVar = name,
                TypeValue = ProtoTypeCodec.Encode(type),
            });
        }

        foreach (var (name, value) in arguments)
        {
            ArgumentException.ThrowIfNullOrWhiteSpace(name);
            if (name.Contains('\0', StringComparison.Ordinal))
            {
                throw new ArgumentException("A BAML argument name cannot contain a NUL character.", nameof(arguments));
            }
            request.Kwargs.Add(new InboundMapEntry
            {
                StringKey = name,
                Value = ProtoCodec.Encode(value, encoding),
            });
        }

        var pendingResponse = NativeApi.CallFunctionAsync(
            functionName,
            request.ToByteArray(),
            nativeCallId,
            cancellationToken);
        encoding.TransferHostValues();
        var response = await pendingResponse.ConfigureAwait(false);
        if (response.HostException is { } hostException)
        {
            ExceptionDispatchInfo.Capture(hostException).Throw();
        }

        return response.Payload;
    }
}
