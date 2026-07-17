using System.Collections.Concurrent;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.ExceptionServices;
using System.Runtime.InteropServices;
using BamlBridge.Cffi.V1;
using Google.Protobuf;
using WireHandle = BamlBridge.Cffi.V1.BamlHandle;

namespace Baml.Bridge;

internal static class HostValueRegistry
{
    private static readonly ConcurrentDictionary<ulong, HostValueEntry> Entries = new();
    private static long _nextKey;

    internal static InboundValue EncodeCallable(Delegate callback, ProtoCodec.EncodeContext context)
    {
        ArgumentNullException.ThrowIfNull(callback);
        var key = Add(new CallableEntry(callback, ExecutionContext.Capture()));
        context.TrackHostValue(key);
        return HandleValue(key, BamlHandleType.HostValueCallable);
    }

    internal static Exception? FindException(BamlOutboundResult result)
    {
        if (result.ResultCase != BamlOutboundResult.ResultOneofCase.Error)
        {
            return null;
        }

        var value = UnwrapUnion(result.Error.Value);
        if (value?.ValueCase != BamlOutboundValue.ValueOneofCase.ClassValue
            || !string.Equals(value.ClassValue.Name, "baml.errors.HostCallable", StringComparison.Ordinal))
        {
            return null;
        }

        var handle = value.ClassValue.Fields
            .FirstOrDefault(static field => string.Equals(field.Key, "_handle", StringComparison.Ordinal))
            ?.Value;
        if (handle?.ValueCase != BamlOutboundValue.ValueOneofCase.HandleValue
            || handle.HandleValue.HandleType != BamlHandleType.HostValueOpaque)
        {
            return null;
        }

        return Entries.TryGetValue(handle.HandleValue.Key, out var entry)
            && entry is OpaqueExceptionEntry exception
                ? exception.Exception
                : null;
    }

    internal static void RollBack(ulong key) => Entries.TryRemove(key, out _);

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    internal static unsafe void Dispatch(ulong hostValueKey, uint callId, byte* args, nuint length)
    {
        try
        {
            if (args == null && length != 0)
            {
                CompleteWithBridgeError(callId, "Native host-call dispatch supplied a null argument pointer with nonzero length.");
                return;
            }

            if (length > int.MaxValue)
            {
                CompleteWithBridgeError(callId, $"Native host-call payload length {length} exceeds the managed limit.");
                return;
            }

            var payload = length == 0
                ? Array.Empty<byte>()
                : new ReadOnlySpan<byte>(args, checked((int)length)).ToArray();
            if (!Entries.TryGetValue(hostValueKey, out var entry) || entry is not CallableEntry callable)
            {
                CompleteWithBridgeError(callId, $"No C# host callable is registered for key {hostValueKey}.");
                return;
            }

            ThreadPool.UnsafeQueueUserWorkItem(
                static state => _ = DispatchAsync(state.Callable, state.CallId, state.Payload),
                new DispatchState(callable, callId, payload),
                preferLocal: false);
        }
        catch (Exception error)
        {
            CompleteWithBridgeError(callId, $"C# host-call dispatch failed: {error.Message}");
        }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    internal static void Release(ulong hostValueKey)
    {
        try
        {
            Entries.TryRemove(hostValueKey, out _);
        }
        catch
        {
            // Exceptions must never cross the unmanaged callback boundary.
        }
    }

    private static async Task DispatchAsync(CallableEntry callable, uint callId, byte[] payload)
    {
        try
        {
            var call = BamlToHostCall.Parser.ParseFrom(payload);
            var result = await callable.InvokeAsync(call).ConfigureAwait(false);
            using var context = new ProtoCodec.EncodeContext();
            var encoded = ProtoCodec.Encode(result, context).ToByteArray();
            NativeApi.CompleteHostCall(callId, isError: false, encoded);
            context.TransferHostValues();
        }
        catch (Exception error)
        {
            try
            {
                var original = UnwrapInvocationException(error);
                using var context = new ProtoCodec.EncodeContext();
                var encoded = EncodeThrownException(original, context).ToByteArray();
                NativeApi.CompleteHostCall(callId, isError: true, encoded);
                context.TransferHostValues();
            }
            catch (Exception completionError)
            {
                CompleteWithBridgeError(
                    callId,
                    $"C# host callable failed and its exception could not be encoded: {completionError.Message}");
            }
        }
    }

    private static InboundValue EncodeThrownException(Exception error, ProtoCodec.EncodeContext context) =>
        error is BamlException { Value: not null } bamlException
            ? ProtoCodec.Encode(bamlException.Value, context)
            : EncodeHostException(error, context);

    private static InboundValue EncodeHostException(Exception error, ProtoCodec.EncodeContext context)
    {
        var key = Add(new OpaqueExceptionEntry(error));
        context.TrackHostValue(key);
        var fields = new List<InboundMapEntry>
        {
            StringField("message", error.Message),
            StringField("class_name", error.GetType().FullName ?? error.GetType().Name),
            StringField("language", "csharp"),
        };
        if (!string.IsNullOrWhiteSpace(error.StackTrace))
        {
            fields.Add(StringField("traceback", error.StackTrace));
        }

        fields.Add(new InboundMapEntry
        {
            StringKey = "_handle",
            Value = HandleValue(key, BamlHandleType.HostValueOpaque),
        });
        return new InboundValue
        {
            ClassValue = new InboundClassValue
            {
                ClassTy = new BamlTyClass { Name = "baml.errors.HostCallable" },
                Fields = { fields },
            },
        };
    }

    private static InboundMapEntry StringField(string name, string value) => new()
    {
        StringKey = name,
        Value = new InboundValue { StringValue = value },
    };

    private static InboundValue HandleValue(ulong key, BamlHandleType type) => new()
    {
        Handle = new WireHandle
        {
            Key = key,
            HandleType = type,
        },
    };

    private static ulong Add(HostValueEntry entry)
    {
        while (true)
        {
            var key = unchecked((ulong)Interlocked.Increment(ref _nextKey));
            if (key != 0 && Entries.TryAdd(key, entry))
            {
                return key;
            }
        }
    }

    private static BamlOutboundValue? UnwrapUnion(BamlOutboundValue? value)
    {
        while (value?.ValueCase == BamlOutboundValue.ValueOneofCase.UnionVariantValue)
        {
            value = value.UnionVariantValue.Value;
        }

        return value;
    }

    private static Exception UnwrapInvocationException(Exception error) =>
        error is TargetInvocationException { InnerException: { } inner } ? inner : error;

    private static void CompleteWithBridgeError(uint callId, string message)
    {
        try
        {
            using var context = new ProtoCodec.EncodeContext();
            var encoded = EncodeHostException(new BamlBridgeException(message), context).ToByteArray();
            NativeApi.CompleteHostCall(callId, isError: true, encoded);
            context.TransferHostValues();
        }
        catch
        {
            try
            {
                NativeApi.CompleteHostCall(callId, isError: false, ReadOnlySpan<byte>.Empty);
            }
            catch
            {
                // There is no remaining recovery path, but the unmanaged callback still must not unwind.
            }
        }
    }

    private abstract record HostValueEntry;

    private sealed record OpaqueExceptionEntry(Exception Exception) : HostValueEntry;

    private sealed record CallableEntry(Delegate Callback, ExecutionContext? Context) : HostValueEntry
    {
        internal async Task<object?> InvokeAsync(BamlToHostCall call)
        {
            Task<object?>? invocation = null;
            if (Context is null)
            {
                invocation = InvokeCoreAsync(call);
            }
            else
            {
                ExecutionContext.Run(
                    Context.CreateCopy(),
                    _ => invocation = InvokeCoreAsync(call),
                    null);
            }

            return await invocation!.ConfigureAwait(false);
        }

        private async Task<object?> InvokeCoreAsync(BamlToHostCall call)
        {
            var methodParameters = Callback.GetType().GetMethod(nameof(Action.Invoke))?.GetParameters()
                ?? throw new BamlBridgeException(
                    $"C# host callable {Callback.GetType().FullName} has no public Invoke contract.");
            var arguments = new object?[methodParameters.Length];
            var assigned = new bool[methodParameters.Length];
            var positionalIndex = 0;
            foreach (var argument in call.Args)
            {
                int targetIndex;
                if (argument.IsOptionalArg)
                {
                    targetIndex = Array.FindIndex(
                        methodParameters,
                        parameter => string.Equals(
                            parameter.GetCustomAttribute<BamlWireNameAttribute>()?.Name ?? parameter.Name,
                            argument.ArgName,
                            StringComparison.Ordinal));
                    if (targetIndex < 0)
                    {
                        throw new BamlBridgeException(
                            $"C# host callable {Callback.Method.Name} has no parameter named {argument.ArgName}.");
                    }
                }
                else
                {
                    while (positionalIndex < assigned.Length && assigned[positionalIndex])
                    {
                        positionalIndex++;
                    }

                    targetIndex = positionalIndex++;
                    if (targetIndex >= methodParameters.Length)
                    {
                        throw new BamlBridgeException(
                            $"C# host callable {Callback.Method.Name} received too many positional arguments.");
                    }
                }

                if (assigned[targetIndex])
                {
                    throw new BamlBridgeException(
                        $"C# host callable {Callback.Method.Name} received parameter {methodParameters[targetIndex].Name} twice.");
                }

                arguments[targetIndex] = ProtoCodec.DecodeOutbound(argument.Value, methodParameters[targetIndex].ParameterType);
                assigned[targetIndex] = true;
            }

            for (var index = 0; index < methodParameters.Length; index++)
            {
                if (assigned[index])
                {
                    continue;
                }

                if (!IsBamlOptional(methodParameters[index].ParameterType))
                {
                    throw new BamlBridgeException(
                        $"C# host callable {Callback.Method.Name} did not receive required parameter {methodParameters[index].Name}.");
                }

                arguments[index] = Activator.CreateInstance(methodParameters[index].ParameterType);
            }

            object? result;
            try
            {
                result = Callback.DynamicInvoke(arguments);
            }
            catch (TargetInvocationException error) when (error.InnerException is not null)
            {
                ExceptionDispatchInfo.Capture(error.InnerException).Throw();
                throw;
            }

            return await AwaitResult(result).ConfigureAwait(false);
        }

        private static bool IsBamlOptional(Type type) =>
            type.IsGenericType && type.GetGenericTypeDefinition() == typeof(BamlOptional<>);

        private static async Task<object?> AwaitResult(object? result)
        {
            if (result is null)
            {
                return null;
            }

            if (result is Task task)
            {
                await task.ConfigureAwait(false);
                return task.GetType().IsGenericType
                    ? task.GetType().GetProperty("Result", BindingFlags.Public | BindingFlags.Instance)!.GetValue(task)
                    : null;
            }

            if (result is ValueTask valueTask)
            {
                await valueTask.ConfigureAwait(false);
                return null;
            }

            var resultType = result.GetType();
            if (resultType.IsGenericType && resultType.GetGenericTypeDefinition() == typeof(ValueTask<>))
            {
                var valueTaskResult = (Task)resultType.GetMethod(nameof(ValueTask<int>.AsTask), Type.EmptyTypes)!.Invoke(result, null)!;
                await valueTaskResult.ConfigureAwait(false);
                return valueTaskResult.GetType().GetProperty("Result", BindingFlags.Public | BindingFlags.Instance)!.GetValue(valueTaskResult);
            }

            return result;
        }
    }

    private sealed record DispatchState(CallableEntry Callable, uint CallId, byte[] Payload);
}
