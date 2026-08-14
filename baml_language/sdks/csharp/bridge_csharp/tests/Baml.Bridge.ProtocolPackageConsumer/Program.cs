using System.Reflection;
using Baml.ProtocolProbe;

internal static class Program
{
    public static int Main()
    {
        const string expectedDescriptor =
            "baml_bridge.cffi.v1.BamlOutboundValue";
        if (!StringComparer.Ordinal.Equals(
                ProtocolPackageSurface.OutboundDescriptorName,
                expectedDescriptor))
        {
            throw new InvalidOperationException(
                "The package did not expose the expected private descriptor through its handwritten facade.");
        }

        PublicApiAuditResult audit =
            PublicApiSignatureGraphAudit.Run(typeof(ProtocolPackageSurface).Assembly);

        Console.WriteLine("exact_package_consumer=ok");
        Console.WriteLine("transport_generation=absent");
        Console.WriteLine("public_protobuf_surface=absent");
        Console.WriteLine(
            $"public_api_declarations_inspected={audit.DeclarationCount}");
        Console.WriteLine(
            $"public_api_signature_types_inspected={audit.SignatureTypeCount}");
        Console.WriteLine("private_transport_positive_control=present");
        return 0;
    }
}

internal readonly record struct PublicApiAuditResult(
    int DeclarationCount,
    int SignatureTypeCount);

internal sealed class PublicApiSignatureGraphAudit
{
    private const BindingFlags PublicDeclaredMembers =
        BindingFlags.Public
        | BindingFlags.Instance
        | BindingFlags.Static
        | BindingFlags.DeclaredOnly;

    private readonly Assembly _assembly;
    private readonly HashSet<Type> _inspectedDeclarations = [];
    private readonly HashSet<Type> _inspectedSignatureTypes = [];

    private PublicApiSignatureGraphAudit(Assembly assembly)
    {
        _assembly = assembly;
    }

    public static PublicApiAuditResult Run(Assembly assembly)
    {
        ArgumentNullException.ThrowIfNull(assembly);
        return new PublicApiSignatureGraphAudit(assembly).Run();
    }

    private PublicApiAuditResult Run()
    {
        Type[] exportedTypes = _assembly.GetExportedTypes();
        if (exportedTypes.Length == 0)
        {
            throw new InvalidOperationException(
                "The package assembly has no exported types, so its public API was not auditable.");
        }

        foreach (Type exportedType in exportedTypes)
        {
            InspectPublicDeclaration(
                exportedType,
                $"exported type {DisplayName(exportedType)}");
        }

        Type[] missedExportedTypes = exportedTypes
            .Where(type => !_inspectedDeclarations.Contains(type))
            .ToArray();
        if (missedExportedTypes.Length != 0)
        {
            throw new InvalidOperationException(
                "The public API audit did not inspect every exported type: "
                + string.Join(", ", missedExportedTypes.Select(DisplayName)));
        }

        AssertPrivateTransportPositiveControl();

        return new PublicApiAuditResult(
            _inspectedDeclarations.Count,
            _inspectedSignatureTypes.Count);
    }

    private void InspectPublicDeclaration(Type type, string position)
    {
        if (!_inspectedDeclarations.Add(type))
        {
            return;
        }

        InspectSignatureType(type, position);
        InspectGenericParameters(
            type.IsGenericTypeDefinition ? type.GetGenericArguments() : [],
            $"{position} generic parameter");

        foreach (Type nestedType in type.GetNestedTypes(BindingFlags.Public))
        {
            InspectPublicDeclaration(
                nestedType,
                $"{position} nested public type {DisplayName(nestedType)}");
        }

        foreach (ConstructorInfo constructor in type.GetConstructors(
                     PublicDeclaredMembers))
        {
            string constructorPosition =
                $"{position} constructor {DisplayName(constructor)}";
            InspectParameters(
                constructor.GetParameters(),
                constructorPosition);
        }

        foreach (MethodInfo method in type.GetMethods(PublicDeclaredMembers))
        {
            string methodPosition =
                $"{position} method {DisplayName(method)}";
            InspectParameter(
                method.ReturnParameter,
                $"{methodPosition} return");
            InspectParameters(method.GetParameters(), methodPosition);
            InspectGenericParameters(
                method.IsGenericMethodDefinition
                    ? method.GetGenericArguments()
                    : [],
                $"{methodPosition} generic parameter");
        }

        foreach (FieldInfo field in type.GetFields(PublicDeclaredMembers))
        {
            string fieldPosition =
                $"{position} field {field.Name}";
            InspectSignatureType(field.FieldType, fieldPosition);
            InspectCustomModifiers(field, fieldPosition);
        }

        foreach (PropertyInfo property in type.GetProperties(
                     PublicDeclaredMembers))
        {
            if (property.GetMethod?.IsPublic != true
                && property.SetMethod?.IsPublic != true)
            {
                continue;
            }

            string propertyPosition =
                $"{position} property {property.Name}";
            InspectSignatureType(property.PropertyType, propertyPosition);
            InspectCustomModifiers(property, propertyPosition);
            InspectParameters(
                property.GetIndexParameters(),
                $"{propertyPosition} indexer");
        }

        foreach (EventInfo eventInfo in type.GetEvents(PublicDeclaredMembers))
        {
            if (eventInfo.AddMethod?.IsPublic != true
                && eventInfo.RemoveMethod?.IsPublic != true
                && eventInfo.RaiseMethod?.IsPublic != true)
            {
                continue;
            }

            Type eventHandlerType = eventInfo.EventHandlerType
                ?? throw new InvalidOperationException(
                    $"{position} event {eventInfo.Name} has no event-handler type.");
            InspectSignatureType(
                eventHandlerType,
                $"{position} event {eventInfo.Name}");
        }
    }

    private void InspectParameters(
        IEnumerable<ParameterInfo> parameters,
        string position)
    {
        foreach (ParameterInfo parameter in parameters)
        {
            string parameterName = parameter.Name
                ?? $"position {parameter.Position}";
            InspectParameter(
                parameter,
                $"{position} parameter {parameterName}");
        }
    }

    private void InspectParameter(ParameterInfo parameter, string position)
    {
        InspectSignatureType(parameter.ParameterType, position);
        foreach (Type modifier in parameter.GetRequiredCustomModifiers())
        {
            InspectSignatureType(
                modifier,
                $"{position} required custom modifier");
        }

        foreach (Type modifier in parameter.GetOptionalCustomModifiers())
        {
            InspectSignatureType(
                modifier,
                $"{position} optional custom modifier");
        }
    }

    private void InspectCustomModifiers(FieldInfo field, string position)
    {
        foreach (Type modifier in field.GetRequiredCustomModifiers())
        {
            InspectSignatureType(
                modifier,
                $"{position} required custom modifier");
        }

        foreach (Type modifier in field.GetOptionalCustomModifiers())
        {
            InspectSignatureType(
                modifier,
                $"{position} optional custom modifier");
        }
    }

    private void InspectCustomModifiers(PropertyInfo property, string position)
    {
        foreach (Type modifier in property.GetRequiredCustomModifiers())
        {
            InspectSignatureType(
                modifier,
                $"{position} required custom modifier");
        }

        foreach (Type modifier in property.GetOptionalCustomModifiers())
        {
            InspectSignatureType(
                modifier,
                $"{position} optional custom modifier");
        }
    }

    private void InspectGenericParameters(
        IEnumerable<Type> genericParameters,
        string position)
    {
        foreach (Type genericParameter in genericParameters)
        {
            if (!genericParameter.IsGenericParameter)
            {
                throw new InvalidOperationException(
                    $"{position} unexpectedly contained non-parameter type "
                    + $"{DisplayName(genericParameter)}.");
            }

            InspectSignatureType(
                genericParameter,
                $"{position} {genericParameter.Name}");
        }
    }

    private void InspectSignatureType(Type type, string position)
    {
        RejectForbiddenTransportType(type, position);
        if (!_inspectedSignatureTypes.Add(type))
        {
            return;
        }

        if (type.HasElementType)
        {
            Type elementType = type.GetElementType()
                ?? throw new InvalidOperationException(
                    $"{position} has an element-type shape but no element type.");
            InspectSignatureType(elementType, $"{position} element");
        }

        if (type.IsFunctionPointer)
        {
            InspectSignatureType(
                type.GetFunctionPointerReturnType(),
                $"{position} function-pointer return");
            Type[] parameterTypes =
                type.GetFunctionPointerParameterTypes();
            for (int index = 0; index < parameterTypes.Length; index++)
            {
                InspectSignatureType(
                    parameterTypes[index],
                    $"{position} function-pointer parameter {index}");
            }

            foreach (Type convention in
                     type.GetFunctionPointerCallingConventions())
            {
                InspectSignatureType(
                    convention,
                    $"{position} function-pointer calling convention");
            }
        }

        if (type.IsGenericParameter)
        {
            foreach (Type constraint in
                     type.GetGenericParameterConstraints())
            {
                InspectSignatureType(
                    constraint,
                    $"{position} constraint");
            }
        }
        else if (type.IsGenericType)
        {
            Type genericDefinition = type.GetGenericTypeDefinition();
            if (!ReferenceEquals(genericDefinition, type))
            {
                InspectSignatureType(
                    genericDefinition,
                    $"{position} generic definition");
            }

            Type[] genericArguments = type.GetGenericArguments();
            for (int index = 0; index < genericArguments.Length; index++)
            {
                InspectSignatureType(
                    genericArguments[index],
                    $"{position} generic argument {index}");
            }
        }

        if (type.BaseType is Type baseType)
        {
            InspectSignatureType(baseType, $"{position} base type");
        }

        foreach (Type implementedInterface in type.GetInterfaces())
        {
            InspectSignatureType(
                implementedInterface,
                $"{position} implemented interface");
        }
    }

    private void AssertPrivateTransportPositiveControl()
    {
        Type[] assemblyTypes = _assembly.GetTypes();
        Type[] internalTransportTypes = assemblyTypes
            .Where(type =>
                IsGeneratedTransportNamespace(type.Namespace)
                && !type.IsVisible)
            .ToArray();

        if (internalTransportTypes.Length == 0)
        {
            throw new InvalidOperationException(
                "The package contains no private generated transport type; "
                + "the public-surface rejection audit has no positive control.");
        }

        bool hasGoogleProtobufImplementationEdge =
            internalTransportTypes.Any(type =>
                type.GetInterfaces().Any(interfaceType =>
                    IsGoogleProtobufNamespace(interfaceType.Namespace)));
        if (!hasGoogleProtobufImplementationEdge)
        {
            throw new InvalidOperationException(
                "The private transport positive control does not implement a "
                + "Google.Protobuf interface.");
        }
    }

    private static void RejectForbiddenTransportType(
        Type type,
        string position)
    {
        if (IsGeneratedTransportNamespace(type.Namespace)
            || IsGoogleProtobufNamespace(type.Namespace))
        {
            throw new InvalidOperationException(
                $"{position} leaked private transport type {DisplayName(type)}.");
        }
    }

    private static bool IsGeneratedTransportNamespace(string? typeNamespace)
    {
        return IsNamespaceOrChild(typeNamespace, "BamlBridge.Cffi");
    }

    private static bool IsGoogleProtobufNamespace(string? typeNamespace)
    {
        return IsNamespaceOrChild(typeNamespace, "Google.Protobuf");
    }

    private static bool IsNamespaceOrChild(
        string? candidate,
        string forbiddenRoot)
    {
        return candidate is not null
            && (StringComparer.Ordinal.Equals(candidate, forbiddenRoot)
                || candidate.StartsWith(
                    forbiddenRoot + ".",
                    StringComparison.Ordinal));
    }

    private static string DisplayName(Type type)
    {
        return type.FullName ?? type.ToString();
    }

    private static string DisplayName(MethodBase method)
    {
        return $"{method.DeclaringType?.FullName}.{method.Name}";
    }
}
