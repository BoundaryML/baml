using System.Runtime.ExceptionServices;

using Baml.Cffi;
using Baml.Generated.V1;
using BamlBridge.Cffi.V1;

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

        // A host callable has no type of its own: the function type its slot
        // declares is stated beside its handle, and the engine binds it to
        // that type.
        BamlTy functionType = FunctionType(callable.Descriptor);
        HostValueRegistration registration =
            HostValueRegistry.Shared.RegisterCallable(callable, functionCallId);
        ownership.AddTransfer(registration);
        return new InboundValue
        {
            ValueType = functionType,
            Handle = new global::BamlBridge.Cffi.V1.BamlHandle
            {
                Key = registration.Key,
                HandleType = BamlHandleType.HostValueCallable,
            },
        };
    }

    /// <summary>
    /// The declared function type of a generated host callable: each parameter's
    /// declared type (an optional one by its wire name), the return type (the
    /// unit type <c>null</c> for a void result) and the throws type.
    /// </summary>
    private static BamlTy FunctionType(BamlGeneratedHostCallableDescriptor descriptor)
    {
        var function = new BamlTyFunction
        {
            Ret = descriptor.Result.Type is { } result
                ? PrimitiveProtocol.ParseTypeMetadata(result.Metadata, "host callable result")
                : new BamlTy { Primitive = new BamlTyPrimitive { Kind = BamlTyPrimitiveKind.BamlTyPrimitiveNull } },
            Throws = PrimitiveProtocol.ParseTypeMetadata(
                descriptor.Throws.Metadata,
                "host callable throws type"),
        };
        foreach (BamlGeneratedHostParameter parameter in descriptor.Parameters)
        {
            var wire = new BamlTyFunctionParam
            {
                Ty = PrimitiveProtocol.ParseTypeMetadata(
                    parameter.Type.Metadata,
                    "host callable parameter"),
                Mode = parameter.Optional
                    ? BamlTyFunctionParamMode.Optional
                    : BamlTyFunctionParamMode.Required,
            };
            if (parameter.Optional)
            {
                wire.Name = parameter.WireIdentity;
            }

            function.Params.Add(wire);
        }

        return new BamlTy { Function = function };
    }

    /// <summary>
    /// Bind a host invocation's arguments to the callable's parameters. Each
    /// decoded argument claims the engine references it keeps from
    /// <paramref name="arguments"/>; the caller disposes it afterwards to
    /// release the rest.
    /// </summary>
    internal static IReadOnlyList<object?> BindArguments(
        BamlGeneratedHostCallableDescriptor descriptor,
        HostCallArguments arguments)
    {
        ArgumentNullException.ThrowIfNull(descriptor);
        ArgumentNullException.ThrowIfNull(arguments);
        BamlToHostCall call = arguments.Call;
        // One budget for the whole call, as for one result envelope.
        var budget = new BamlDecodeBudget();
        BamlGeneratedValue Decode(BamlOutboundValue value, BamlGeneratedHostParameter parameter) =>
            PrimitiveProtocol.DecodeHostArgument(
                value,
                $"$args.{parameter.WireIdentity}",
                arguments.Ownership,
                arguments.Api,
                budget);

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

                BamlGeneratedHostParameter required = parameters[requiredIndex];
                bound[requiredIndex] = required.Decode(Decode(supplied.Value, required));
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
            bound[index] = parameter.Decode(Decode(supplied.Value, parameter));
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

    /// <summary>
    /// The host's own exception as the engine builds <c>baml.errors.HostCallable</c>
    /// from it. <paramref name="key"/> is the registry's key for the exception,
    /// the engine-built class's <c>_handle</c>.
    /// </summary>
    internal static HostException Raised(Exception exception, ulong key)
    {
        ArgumentNullException.ThrowIfNull(exception);
        if (key == 0)
        {
            throw new ArgumentOutOfRangeException(nameof(key));
        }

        var raised = new HostException
        {
            Message = exception.Message,
            ClassName = exception.GetType().FullName ?? exception.GetType().Name,
            Language = "C#",
            Handle = key,
        };
        if (exception.StackTrace is { } traceback)
        {
            raised.Traceback = traceback;
        }

        return raised;
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
