using System.Runtime.ExceptionServices;

using Baml.Cffi;
using Baml.Generated.V1;
using BamlBridge.Cffi.V1;
using Google.Protobuf;

namespace Baml.Proto;

internal static class HostCallableProtocol
{
    private const string HostCallableIdentity = "baml.errors.HostCallable";

    internal static InboundValue Encode(
        BamlGeneratedHostCallable callable,
        NativeApi api,
        EncodedCallArguments ownership,
        ulong functionCallId)
    {
        ArgumentNullException.ThrowIfNull(callable);
        ArgumentNullException.ThrowIfNull(api);
        ArgumentNullException.ThrowIfNull(ownership);
        functionCallId = NativeApi.RequireFunctionCallIdentifier(functionCallId);

        HostValueRegistration registration =
            HostValueRegistry.Shared.RegisterCallable(callable, functionCallId);
        ownership.AddTransfer(registration);
        return new InboundValue
        {
            Handle = new global::BamlBridge.Cffi.V1.BamlHandle
            {
                Key = registration.Key,
                HandleType = BamlHandleType.HostValueCallable,
            },
        };
    }

    internal static IReadOnlyList<object?> BindArguments(
        BamlGeneratedHostCallableDescriptor descriptor,
        ReadOnlySpan<byte> bytes)
    {
        ArgumentNullException.ThrowIfNull(descriptor);
        BamlToHostCall call;
        try
        {
            call = BamlToHostCall.Parser.ParseFrom(bytes);
        }
        catch (InvalidProtocolBufferException error)
        {
            throw new BamlProtocolException(
                "The native bridge supplied malformed host callback arguments.",
                error.Message);
        }

        IReadOnlyList<BamlGeneratedHostParameter> parameters = descriptor.Parameters;
        int requiredCount = parameters.TakeWhile(parameter => !parameter.Optional).Count();
        var optionalByName = parameters
            .Skip(requiredCount)
            .ToDictionary(parameter => parameter.WireIdentity, StringComparer.Ordinal);
        var bound = new object?[parameters.Count];
        var suppliedOptionals = new HashSet<string>(StringComparer.Ordinal);
        int requiredIndex = 0;
        bool sawOptional = false;
        foreach (BamlToHostArg supplied in call.Args)
        {
            if (supplied is null || supplied.Value is null)
            {
                throw InvalidArguments("A callback argument was missing its BAML value.");
            }

            if (!supplied.IsOptionalArg)
            {
                if (sawOptional)
                {
                    throw InvalidArguments(
                        "A required callback argument appeared after a supplied optional argument.");
                }

                if (supplied.ArgName.Length != 0)
                {
                    throw InvalidArguments(
                        "A required callback argument carried an optional wire name.");
                }

                if (requiredIndex >= requiredCount)
                {
                    throw InvalidArguments("The callback received too many required arguments.");
                }

                BamlGeneratedValue decoded = PrimitiveProtocol.Decode(supplied.Value);
                bound[requiredIndex] = parameters[requiredIndex].Decode(decoded);
                requiredIndex++;
                continue;
            }

            sawOptional = true;
            if (string.IsNullOrEmpty(supplied.ArgName)
                || !optionalByName.TryGetValue(
                    supplied.ArgName,
                    out BamlGeneratedHostParameter? parameter))
            {
                throw InvalidArguments(
                    $"The callback received unknown optional argument {supplied.ArgName}.");
            }

            if (!suppliedOptionals.Add(supplied.ArgName))
            {
                throw InvalidArguments(
                    $"The callback received optional argument {supplied.ArgName} more than once.");
            }

            int index = IndexOf(parameters, parameter);
            bound[index] = parameter.Decode(PrimitiveProtocol.Decode(supplied.Value));
        }

        if (requiredIndex != requiredCount)
        {
            throw InvalidArguments(
                $"The callback expected {requiredCount} required argument(s), received {requiredIndex}.");
        }

        for (int index = requiredCount; index < parameters.Count; index++)
        {
            BamlGeneratedHostParameter parameter = parameters[index];
            if (!suppliedOptionals.Contains(parameter.WireIdentity))
            {
                bound[index] = parameter.CreateUnset();
            }
        }

        return Array.AsReadOnly(bound);
    }

    internal static byte[] EncodeException(Exception exception, ulong key)
    {
        ArgumentNullException.ThrowIfNull(exception);
        if (key == 0)
        {
            throw new ArgumentOutOfRangeException(nameof(key));
        }

        var value = new InboundValue
        {
            ValueType = new BamlTy
            {
                ClassTy = new BamlTyClass { Name = HostCallableIdentity },
            },
            ClassValue = new InboundClassValue(),
        };
        value.ClassValue.Fields.Add(Field("message", String(exception.Message)));
        value.ClassValue.Fields.Add(Field(
            "class_name",
            String(exception.GetType().FullName ?? exception.GetType().Name)));
        value.ClassValue.Fields.Add(Field("language", String("C#")));
        value.ClassValue.Fields.Add(Field(
            "traceback",
            exception.StackTrace is null ? new InboundValue() : String(exception.StackTrace)));
        value.ClassValue.Fields.Add(Field(
            "_handle",
            new InboundValue
            {
                Handle = new global::BamlBridge.Cffi.V1.BamlHandle
                {
                    Key = key,
                    HandleType = BamlHandleType.HostValueOpaque,
                },
            }));
        byte[] bytes = value.ToByteArray();
        if (bytes.Length == 0)
        {
            throw new BamlProtocolException(
                "The managed bridge could not encode a host callback exception.",
                "The required HostCallable payload encoded to zero bytes.");
        }

        return bytes;
    }

    internal static void ThrowIfHostCallbackError(BamlOutboundResult envelope)
    {
        ArgumentNullException.ThrowIfNull(envelope);
        if (envelope.ResultCase != BamlOutboundResult.ResultOneofCase.Error
            || envelope.Error?.Value?.ValueCase
                != BamlOutboundValue.ValueOneofCase.ClassValue
            || !StringComparer.Ordinal.Equals(
                envelope.Error.Value.ClassValue.Name,
                HostCallableIdentity))
        {
            return;
        }

        BamlValueClass value = envelope.Error.Value.ClassValue;
        BamlOutboundValue? handleValue = FindField(value, "_handle");
        if (handleValue?.ValueCase == BamlOutboundValue.ValueOneofCase.HandleValue
            && handleValue.HandleValue.HandleType == BamlHandleType.HostValueOpaque
            && handleValue.HandleValue.Key != 0
            && HostValueRegistry.Shared.TryRestoreException(
                handleValue.HandleValue.Key,
                out ExceptionDispatchInfo? exception))
        {
            exception!.Throw();
        }

        string message = ReadStringField(value, "message")
            ?? "A host callback threw an exception whose managed identity is no longer available.";
        string? className = ReadStringField(value, "class_name");
        string? language = ReadStringField(value, "language");
        string detail = string.Join(
            " ",
            new[] { language, className }.Where(part => !string.IsNullOrWhiteSpace(part)));
        throw new BamlHostCallbackException(
            detail.Length == 0 ? message : $"{detail}: {message}");
    }

    private static int IndexOf(
        IReadOnlyList<BamlGeneratedHostParameter> parameters,
        BamlGeneratedHostParameter target)
    {
        for (int index = 0; index < parameters.Count; index++)
        {
            if (ReferenceEquals(parameters[index], target))
            {
                return index;
            }
        }

        throw new InvalidOperationException(
            "The generated host-callable parameter lookup is contradictory.");
    }

    private static BamlOutboundValue? FindField(BamlValueClass value, string name) =>
        value.Fields.FirstOrDefault(field =>
            StringComparer.Ordinal.Equals(field.Key, name))?.Value;

    private static string? ReadStringField(BamlValueClass value, string name)
    {
        BamlOutboundValue? field = FindField(value, name);
        return field?.ValueCase == BamlOutboundValue.ValueOneofCase.StringValue
            ? field.StringValue
            : null;
    }

    private static InboundMapEntry Field(string name, InboundValue value) =>
        new() { StringKey = name, Value = value };

    private static InboundValue String(string value) => new() { StringValue = value };

    private static BamlProtocolException InvalidArguments(string diagnostic) =>
        new("The native bridge supplied invalid host callback arguments.", diagnostic);
}
