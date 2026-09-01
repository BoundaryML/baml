using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Globalization;
using System.Numerics;
using System.Security.Cryptography;
using System.Text;
using System.Reflection;
using Baml;
using Baml.Cffi;
using Baml.Generated.V1;
using Baml.Proto;
using BamlBridge.Cffi.V1;
using Google.Protobuf;

internal static unsafe class Program
{
    private static int releasedBuffers;
    private static int clonedHandles;
    private static int releasedHandles;
    private static int bridgeRegistrations;
    private static int fakeCallCount;
    private static int fakeCancelCount;
    private static long nextFakeCallId;
    private static string? lastFakeFunction;
    private static byte[]? lastFakeArguments;
    private static byte[]? fakeResult;
    private static uint lastFakeCallbackId;
    private static long nextFakeHandleKey = 5000;
    private static string fakeMediaUrl = string.Empty;
    private static string fakeMediaBase64 = string.Empty;
    private static string fakeMediaFile = string.Empty;
    private static string fakeMediaMimeType = string.Empty;

    public static int Main(string[] args)
    {
        if (args.Length != 0)
        {
            return RunNativeChild(args);
        }

        VerifyOptional();
        VerifyNullable();
        VerifyUnions();
        VerifyIntegerBoundary();
        VerifyProtocolSurfaceIsPrivate();
        VerifyGeneratedContract();
        VerifyPrimitiveProtocol();
        VerifyCompositeProtocol();
        VerifyFailureContract();
        VerifyManagedValueModels();
        VerifyNativeAbiLayoutAndValidation();
        VerifyOwnedBuffers();
        VerifyCallPipeline();
        VerifyCallbackIdentifiersAndCopy();
        VerifySafeHandleOwnership();
        VerifyStreamProtocolOwnership();
        VerifyDeferredStreamArgumentOwnership();
        VerifyMediaAndHandleProtocol();
        Console.WriteLine("managed_foundation=ok");
        return 0;
    }

    private static int RunNativeChild(string[] args)
    {
        if (args.Length == 2 && args[0] == "native-success")
        {
            NativeApi api = NativeApi.Instance;
            Require(api.ProductVersion == args[1], "native product version changed");
            Require(api.NewFunctionCall() == 1, "native call identifier changed");
            Console.WriteLine("native_loader=ok");
            return 0;
        }

        if (args.Length == 2 && args[0] == "native-failure")
        {
            try
            {
                string unexpectedlyLoaded = NativeApi.Instance.ProductVersion;
                throw new InvalidOperationException(
                    $"expected native loading to fail with {args[1]}, but loaded {unexpectedlyLoaded}");
            }
            catch (BamlInitializationException error)
            {
                Require(
                    error.ToString().Contains(args[1], StringComparison.Ordinal),
                    $"native failure did not contain {args[1]}: {error}");
                Console.WriteLine("native_failure=fail_closed");
                return 0;
            }
        }

        if (args.Length == 2 && args[0] == "register-success")
        {
            (BamlGeneratedRegistry registry, _, _, _) = CreateGeneratedRegistry();
            byte[] bytecode = [1, 2, 3, 4];
            string fingerprint = Fingerprint(bytecode);
            BamlGeneratedProgram first = BamlGeneratedContract.RegisterProgram(
                BamlGeneratedContract.Version,
                bytecode,
                fingerprint,
                RuntimeIdentity.PackageVersion,
                RuntimeIdentity.RequiredBridgeVersion,
                registry);
            BamlGeneratedProgram second = BamlGeneratedContract.RegisterProgram(
                BamlGeneratedContract.Version,
                bytecode,
                fingerprint,
                RuntimeIdentity.PackageVersion,
                RuntimeIdentity.RequiredBridgeVersion,
                registry);
            Require(
                ReferenceEquals(first.NativeState, second.NativeState),
                "same-fingerprint registration repeated native initialization");

            byte[] other = [5, 6, 7, 8];
            Expect<BamlProgramConflictException>(() =>
                _ = BamlGeneratedContract.RegisterProgram(
                    BamlGeneratedContract.Version,
                    other,
                    Fingerprint(other),
                    RuntimeIdentity.PackageVersion,
                    RuntimeIdentity.RequiredBridgeVersion,
                    registry));
            Console.WriteLine("program_registration=ok");
            return 0;
        }

        if (args.Length == 2 && args[0] == "hard-exit")
        {
            long exitCode = long.Parse(args[1], System.Globalization.CultureInfo.InvariantCulture);
            Console.WriteLine("hard_exit_before");
            Console.Out.Flush();
            try
            {
                var envelope = new BamlOutboundResult
                {
                    Panic = new BamlOutboundPanic
                    {
                        IsExitPanic = true,
                        ExitCode = exitCode,
                    },
                };
                _ = PrimitiveProtocol.DecodeCallResult(envelope.ToByteArray(), "fixture.exit");
            }
            finally
            {
                Console.WriteLine("hard_exit_unreachable_finally");
            }

            return 99;
        }

        throw new ArgumentException(
            "usage: Baml.Bridge.Tests [native-success <version>|native-failure <marker>|hard-exit <code>]");
    }

    private static void VerifyOptional()
    {
        BamlOptional<long> unset = default;
        BamlOptional<long> zero = 0;
        BamlOptional<long> anotherZero = BamlOptional<long>.FromValue(0);
        BamlOptional<string?> explicitNull = BamlOptional<string?>.FromValue(null);
        Require(!unset.IsSet && !new BamlOptional<long>().IsSet, "optional default changed");
        Require(zero.IsSet && zero.Value == 0, "optional zero was omitted");
        Require(explicitNull.IsSet && explicitNull.Value is null, "optional null was omitted");
        Require(unset == BamlOptional<long>.Unset, "optional unset equality changed");
        Require(unset != zero, "optional set-default collapsed to unset");
        Require(zero == anotherZero, "equal optional values changed");
        Require(zero.GetHashCode() == anotherZero.GetHashCode(), "equal optional hashes changed");
        Require(unset.ToString() == "<unset>", "optional unset display changed");
        Require(explicitNull.ToString() == "<null>", "optional explicit-null display changed");
        Require(!unset.TryGetValue(out long unsetValue) && unsetValue == 0, "optional unset TryGetValue changed");
        Require(zero.TryGetValue(out long setValue) && setValue == 0, "optional set TryGetValue changed");

        BamlOptional<BamlNullable<string>> omittedNullable = default;
        BamlOptional<BamlNullable<string>> explicitBamlNull =
            BamlNullable<string>.Null;
        BamlOptional<BamlNullable<string>> present =
            BamlNullable<string>.FromValue("value");
        Require(!omittedNullable.IsSet, "composed optional nullable omission changed");
        Require(
            explicitBamlNull.IsSet && explicitBamlNull.Value.IsNull,
            "composed optional nullable null changed");
        Require(
            present.IsSet && present.Value.Value == "value",
            "composed optional nullable value changed");
        Expect<InvalidOperationException>(() => _ = unset.Value);
    }

    private static void VerifyNullable()
    {
        BamlNullable<long> nullLong = default;
        BamlNullable<long> zero = BamlNullable.FromValue(0L);
        BamlNullable<long> anotherZero = BamlNullable<long>.FromValue(0);
        BamlNullable<string> nullString = BamlNullable<string>.FromValue(null!);
        Require(nullLong.IsNull && BamlNullable.Null<long>().IsNull, "nullable default changed");
        Require(!zero.IsNull && zero.Value == 0, "nullable zero became null");
        Require(nullString.IsNull, "CLR null did not map to BAML null");
        Require(zero.Match(() => -1, value => value) == 0, "nullable match changed");
        Require(nullLong != zero, "nullable value collapsed to null");
        Require(zero == anotherZero, "equal nullable values changed");
        Require(zero.GetHashCode() == anotherZero.GetHashCode(), "equal nullable hashes changed");
        Require(nullLong.ToString() == "<null>", "nullable null display changed");
        Require(zero.ToString() == "0", "nullable value display changed");
        Require(!nullLong.TryGetValue(out long nullValue) && nullValue == 0, "nullable null TryGetValue changed");
        Require(zero.TryGetValue(out long presentValue) && presentValue == 0, "nullable value TryGetValue changed");
        Expect<ArgumentNullException>(() => _ = zero.Match(null!, value => value));
        Expect<ArgumentNullException>(() => _ = zero.Match(() => -1, null!));
        Expect<InvalidOperationException>(() => _ = nullLong.Value);
    }

    private static void VerifyUnions()
    {
        for (int arity = 2; arity <= 32; arity++)
        {
            Type? definition = typeof(BamlUnion<,>).Assembly.GetType(
                $"Baml.BamlUnion`{arity}",
                throwOnError: false,
                ignoreCase: false);
            Require(definition is not null, $"BamlUnion arity {arity} is missing");
        }

        BamlUnion<string, long> text = "hello";
        BamlUnion<string, long> number = 42L;
        Require(text.IsT0 && text.AsT0 == "hello", "union T0 construction changed");
        Require(number.IsT1 && number.AsT1 == 42, "union T1 construction changed");
        Require(
            text.Match(value => value.Length, value => checked((int)value)) == 5,
            "union match changed");
        Require(text != number, "cross-case union equality changed");
        Expect<InvalidOperationException>(() => _ = text.AsT1);

        BamlUnion<string, string> duplicate0 =
            BamlUnion<string, string>.FromT0("same");
        BamlUnion<string, string> duplicate1 =
            BamlUnion<string, string>.FromT1("same");
        Require(
            duplicate0 != duplicate1
            && duplicate0.IsT0
            && duplicate1.IsT1,
            "duplicate-projection union cases collapsed");

        int switchedCase = -1;
        BamlUnion<string, long, bool> three =
            BamlUnion<string, long, bool>.FromT2(true);
        three.Switch(
            _ => switchedCase = 0,
            _ => switchedCase = 1,
            value => switchedCase = value ? 2 : -2);
        Require(switchedCase == 2, "union switch changed");

        BamlUnion<long, long, long, long, long, long, long, long,
            long, long, long, long, long, long, long, long,
            long, long, long, long, long, long, long, long,
            long, long, long, long, long, long, long, long> thirtyTwo =
            BamlUnion<long, long, long, long, long, long, long, long,
                long, long, long, long, long, long, long, long,
                long, long, long, long, long, long, long, long,
                long, long, long, long, long, long, long, long>.FromT31(32);
        Require(thirtyTwo.IsT31 && thirtyTwo.AsT31 == 32, "union arity 32 changed");
        Require(
            thirtyTwo.Match(
                _ => 0, _ => 0, _ => 0, _ => 0,
                _ => 0, _ => 0, _ => 0, _ => 0,
                _ => 0, _ => 0, _ => 0, _ => 0,
                _ => 0, _ => 0, _ => 0, _ => 0,
                _ => 0, _ => 0, _ => 0, _ => 0,
                _ => 0, _ => 0, _ => 0, _ => 0,
                _ => 0, _ => 0, _ => 0, _ => 0,
                _ => 0, _ => 0, _ => 0, value => value) == 32,
            "union arity 32 match changed");

        BamlUnion<string, long> invalid = default;
        Require(invalid == default, "invalid union equality changed");
        Require(invalid.GetHashCode() == 0, "invalid union hash changed");
        Expect<InvalidOperationException>(() => _ = invalid.AsT0);
        Expect<InvalidOperationException>(() =>
            _ = invalid.Match(_ => 0, _ => 1));

        BamlOptional<BamlUnion<string, long>> omitted = default;
        BamlOptional<BamlUnion<string, long>> supplied = text;
        BamlUnion<string, long>? nullable = null;
        Require(!omitted.IsSet, "optional union omission changed");
        Require(supplied.IsSet && supplied.Value == text, "optional union value changed");
        Require(nullable is null, "nullable union null changed");
    }

    private static void VerifyIntegerBoundary()
    {
        Require(BamlInteger.Require(BamlInteger.Minimum, "$.min") == BamlInteger.Minimum, "min rejected");
        Require(BamlInteger.Require(BamlInteger.Maximum, "$.max") == BamlInteger.Maximum, "max rejected");
        foreach (long value in new[]
                 {
                     BamlInteger.Minimum - 1,
                     BamlInteger.Maximum + 1,
                     long.MinValue,
                     long.MaxValue,
                 })
        {
            Expect<BamlProtocolException>(() => _ = BamlInteger.Require(value, "$.value"));
        }
    }

    private static void VerifyProtocolSurfaceIsPrivate()
    {
        string[] leaked = typeof(BamlOptional<>).Assembly
            .GetExportedTypes()
            .Where(type => type.Namespace?.StartsWith("BamlBridge.Cffi.V1", StringComparison.Ordinal) == true)
            .Select(type => type.FullName!)
            .ToArray();
        Require(leaked.Length == 0, $"private protocol types leaked: {string.Join(", ", leaked)}");

        foreach (Type type in typeof(BamlOptional<>).Assembly.GetExportedTypes())
        {
            foreach (MemberInfo member in type.GetMembers(
                         BindingFlags.Public
                         | BindingFlags.Instance
                         | BindingFlags.Static
                         | BindingFlags.DeclaredOnly))
            {
                IEnumerable<Type> signature = member switch
                {
                    MethodInfo method =>
                        method.GetParameters().Select(parameter => parameter.ParameterType)
                            .Append(method.ReturnType),
                    ConstructorInfo constructor =>
                        constructor.GetParameters().Select(parameter => parameter.ParameterType),
                    PropertyInfo property => [property.PropertyType],
                    FieldInfo field => [field.FieldType],
                    EventInfo eventInfo when eventInfo.EventHandlerType is not null =>
                        [eventInfo.EventHandlerType],
                    _ => [],
                };
                Require(
                    signature.All(parameter => !ContainsPrivateProtocolType(parameter)),
                    $"public member {type.FullName}.{member.Name} exposes private protocol metadata");
            }
        }
    }

    private static bool ContainsPrivateProtocolType(Type type)
    {
        Type element = type;
        while (element.HasElementType)
        {
            element = element.GetElementType()!;
        }

        if (element.Namespace?.StartsWith("BamlBridge.Cffi.V1", StringComparison.Ordinal) == true
            || element.Namespace?.StartsWith("Google.Protobuf", StringComparison.Ordinal) == true)
        {
            return true;
        }

        return element.IsGenericType
            && element.GetGenericArguments().Any(ContainsPrivateProtocolType);
    }

    private static void VerifyGeneratedContract()
    {
        Expect<BamlVersionMismatchException>(() =>
            _ = BamlGeneratedContract.CreateRegistryBuilder(BamlGeneratedContract.Version + 1));

        (BamlGeneratedRegistry registry, BamlGeneratedFunction<string> function, BamlGeneratedArgument<string, string> required, BamlGeneratedArgument<string, string?> optional) =
            CreateGeneratedRegistry();

        BamlGeneratedArgumentsBuilder<string> missing = registry.CreateArgumentsBuilder(function);
        Expect<InvalidOperationException>(() => _ = missing.Build());
        Expect<InvalidOperationException>(() => missing.Omit(required));

        BamlGeneratedArgumentsBuilder<string> arguments = registry.CreateArgumentsBuilder(function);
        arguments.Add(required, "hello");
        arguments.Omit(optional);
        BamlGeneratedArguments<string> frozen = arguments.Build();
        Require(frozen.Supplied().Count() == 1, "optional omission changed");
        Expect<InvalidOperationException>(() => arguments.Add(required, "again"));

        BamlGeneratedArgumentsBuilder<string> explicitNull = registry.CreateArgumentsBuilder(function);
        explicitNull.Add(required, "hello");
        explicitNull.Add(optional, null);
        Require(explicitNull.Build().Supplied().Count() == 2, "explicit null became omission");

        BamlGeneratedArgumentsBuilder<string> contradictory = registry.CreateArgumentsBuilder(function);
        contradictory.Add(required, "hello");
        contradictory.Omit(optional);
        Expect<InvalidOperationException>(() => contradictory.Add(optional, "value"));

        BamlGeneratedRegistryBuilder foreignBuilder =
            BamlGeneratedContract.CreateRegistryBuilder(BamlGeneratedContract.Version);
        BamlGeneratedType<string> foreignString = foreignBuilder.DeclareType<string>("foreign.string");
        foreignBuilder.RegisterCodec(foreignString, new StringCodec());
        Expect<InvalidOperationException>(() =>
            _ = foreignBuilder.DeclareFunction("foreign.echo", "call", default(BamlGeneratedType<string>)));

        BamlGeneratedRegistryBuilder missingCodec =
            BamlGeneratedContract.CreateRegistryBuilder(BamlGeneratedContract.Version);
        _ = missingCodec.DeclareType<long>("missing.int");
        Expect<InvalidOperationException>(() => _ = missingCodec.Build());

        byte[] bytecode = [1, 2, 3];
        Expect<BamlVersionMismatchException>(() =>
            _ = BamlGeneratedContract.RegisterProgram(
                BamlGeneratedContract.Version + 1,
                ReadOnlyMemory<byte>.Empty,
                string.Empty,
                "wrong",
                "wrong",
                null!));
        Expect<BamlVersionMismatchException>(() =>
            _ = BamlGeneratedContract.RegisterProgram(
                BamlGeneratedContract.Version,
                bytecode,
                Fingerprint(bytecode),
                "wrong",
                RuntimeIdentity.RequiredBridgeVersion,
                registry));
        Expect<BamlProgramIntegrityException>(() =>
            _ = BamlGeneratedContract.RegisterProgram(
                BamlGeneratedContract.Version,
                ReadOnlyMemory<byte>.Empty,
                Fingerprint(bytecode),
                RuntimeIdentity.PackageVersion,
                RuntimeIdentity.RequiredBridgeVersion,
                registry));
        Expect<BamlProgramIntegrityException>(() =>
            _ = BamlGeneratedContract.RegisterProgram(
                BamlGeneratedContract.Version,
                bytecode,
                Fingerprint(bytecode).ToUpperInvariant(),
                RuntimeIdentity.PackageVersion,
                RuntimeIdentity.RequiredBridgeVersion,
                registry));
        Expect<BamlProgramIntegrityException>(() =>
            _ = BamlGeneratedContract.RegisterProgram(
                BamlGeneratedContract.Version,
                bytecode,
                new string('0', 64),
                RuntimeIdentity.PackageVersion,
                RuntimeIdentity.RequiredBridgeVersion,
                registry));

        Type[] generatedSurface = typeof(BamlGeneratedContract).Assembly
            .GetExportedTypes()
            .Where(type => type.Namespace == "Baml.Generated.V1")
            .ToArray();
        Require(generatedSurface.Length != 0, "generated contract surface is missing");
        Require(
            generatedSurface.All(type =>
                type.GetCustomAttributes(typeof(System.ComponentModel.EditorBrowsableAttribute), inherit: false)
                    .Cast<System.ComponentModel.EditorBrowsableAttribute>()
                    .Any(attribute => attribute.State == System.ComponentModel.EditorBrowsableState.Never)),
            "generated contract surface is not uniformly editor-hidden");

        VerifyGeneratedProvenanceFailures();
        VerifyGeneratedGenericProvenanceFailures();
    }

    private static void VerifyGeneratedProvenanceFailures()
    {
        BamlGeneratedRegistryBuilder first =
            BamlGeneratedContract.CreateRegistryBuilder(BamlGeneratedContract.Version);
        BamlGeneratedType<string> firstText = first.DeclareType<string>("same.id");
        first.RegisterCodec(firstText, new StringCodec());
        Expect<InvalidOperationException>(() => _ = first.DeclareType<string>("same.id"));
        Expect<InvalidOperationException>(() => first.RegisterCodec(firstText, new StringCodec()));

        BamlGeneratedRegistryBuilder second =
            BamlGeneratedContract.CreateRegistryBuilder(BamlGeneratedContract.Version);
        BamlGeneratedType<string> secondText = second.DeclareType<string>("same.id");
        second.RegisterCodec(secondText, new StringCodec());
        Expect<InvalidOperationException>(() => first.RegisterCodec(secondText, new StringCodec()));
        Expect<InvalidOperationException>(() =>
            _ = first.DeclareFunction("cross.type", "call", secondText));

        BamlGeneratedFunction<string> firstFunction =
            first.DeclareFunction("same.function", "call", firstText);
        Expect<InvalidOperationException>(() =>
            _ = first.DeclareFunction("same.function", "call", firstText));
        BamlGeneratedArgument<string, string> firstArgument =
            first.DeclareArgument(firstFunction, "value", firstText);
        Expect<InvalidOperationException>(() =>
            _ = first.DeclareArgument(firstFunction, "value", firstText));
        Expect<InvalidOperationException>(() =>
            _ = first.DeclareArgument(default(BamlGeneratedFunction<string>), "default", firstText));

        BamlGeneratedFunction<string> otherFunction =
            first.DeclareFunction("other.function", "call", firstText);
        BamlGeneratedArgument<string, string> otherArgument =
            first.DeclareArgument(otherFunction, "other", firstText);

        BamlGeneratedRegistry firstRegistry = first.Build();
        BamlGeneratedRegistry secondRegistry = second.Build();
        Expect<InvalidOperationException>(() => _ = first.Build());
        Expect<InvalidOperationException>(() => _ = first.DeclareType<string>("after.build"));
        Expect<InvalidOperationException>(() => first.RegisterCodec(firstText, new StringCodec()));
        Expect<InvalidOperationException>(() =>
            _ = first.DeclareFunction("after.build", "call", firstText));
        Expect<InvalidOperationException>(() =>
            _ = first.DeclareArgument(firstFunction, "after", firstText));

        Expect<InvalidOperationException>(() =>
            _ = firstRegistry.CreateArgumentsBuilder(default(BamlGeneratedFunction<string>)));
        Expect<InvalidOperationException>(() =>
            _ = secondRegistry.CreateArgumentsBuilder(firstFunction));
        Expect<InvalidOperationException>(() => _ = firstRegistry.Encode(secondText, "value"));
        Expect<InvalidOperationException>(() =>
            _ = firstRegistry.Decode(secondText, BamlGeneratedValue.CreateString("value")));

        BamlGeneratedArgumentsBuilder<string> arguments =
            firstRegistry.CreateArgumentsBuilder(firstFunction);
        Expect<InvalidOperationException>(() =>
            arguments.Add(default(BamlGeneratedArgument<string, string>), "value"));
        Expect<InvalidOperationException>(() =>
            arguments.Omit(default(BamlGeneratedArgument<string, string>)));
        Expect<InvalidOperationException>(() => arguments.Add(otherArgument, "value"));
        arguments.Add(firstArgument, "value");
        Expect<InvalidOperationException>(() => arguments.Add(firstArgument, "again"));

        BamlGeneratedRegistryBuilder omissionBuilder =
            BamlGeneratedContract.CreateRegistryBuilder(BamlGeneratedContract.Version);
        BamlGeneratedType<string> omissionText =
            omissionBuilder.DeclareType<string>("omission.string");
        omissionBuilder.RegisterCodec(omissionText, new StringCodec());
        BamlGeneratedFunction<string> omissionFunction =
            omissionBuilder.DeclareFunction("omission.function", "call", omissionText);
        BamlGeneratedArgument<string, string> omitted =
            omissionBuilder.DeclareArgument(
                omissionFunction,
                "optional",
                omissionText,
                optional: true);
        BamlGeneratedRegistry omissionRegistry = omissionBuilder.Build();
        BamlGeneratedArgumentsBuilder<string> omissions =
            omissionRegistry.CreateArgumentsBuilder(omissionFunction);
        omissions.Omit(omitted);
        Expect<InvalidOperationException>(() => omissions.Omit(omitted));
        _ = omissions.Build();
        Expect<InvalidOperationException>(() => omissions.Add(omitted, "late"));

        BamlGeneratedRegistryBuilder receiverBuilder =
            BamlGeneratedContract.CreateRegistryBuilder(BamlGeneratedContract.Version);
        BamlGeneratedType<string> receiverText =
            receiverBuilder.DeclareType<string>("receiver.string");
        receiverBuilder.RegisterCodec(receiverText, new StringCodec());
        BamlGeneratedFunction<string> receiverFunction =
            receiverBuilder.DeclareFunction("receiver.function", "call", receiverText);
        _ = receiverBuilder.DeclareArgument(
            receiverFunction,
            "self",
            receiverText,
            isSelf: true);
        Expect<InvalidOperationException>(() =>
            _ = receiverBuilder.DeclareArgument(
                receiverFunction,
                "second_self",
                receiverText,
                isSelf: true));

        BamlGeneratedFunction<string> optionalReceiverFunction =
            receiverBuilder.DeclareFunction("optional.receiver", "call", receiverText);
        Expect<InvalidOperationException>(() =>
            _ = receiverBuilder.DeclareArgument(
                optionalReceiverFunction,
                "self",
                receiverText,
                optional: true,
                isSelf: true));
    }

    private static void VerifyGeneratedGenericProvenanceFailures()
    {
        byte[] stringMetadata =
            PrimitiveType(BamlTyPrimitiveKind.BamlTyPrimitiveString).ToByteArray();
        BamlGeneratedRegistryBuilder builder =
            BamlGeneratedContract.CreateRegistryBuilder(BamlGeneratedContract.Version);
        BamlGeneratedType<string> text =
            builder.DeclareType<string>("generic.string", stringMetadata);
        builder.RegisterCodec(text, new StringCodec());
        builder.RegisterGenericBinding(text);
        Expect<InvalidOperationException>(() => builder.RegisterGenericBinding(text));

        BamlGeneratedGenericFunction identity =
            builder.DeclareGenericFunction("generic.identity", "call");
        Expect<InvalidOperationException>(() =>
            _ = builder.DeclareGenericFunction("generic.identity", "call"));
        BamlGeneratedTypeParameter parameter =
            builder.DeclareTypeParameter(identity, "T");
        Expect<InvalidOperationException>(() =>
            _ = builder.DeclareTypeParameter(identity, "T"));
        BamlGeneratedGenericArgument value =
            builder.DeclareGenericArgument(identity, "value");
        Expect<InvalidOperationException>(() =>
            _ = builder.DeclareGenericArgument(identity, "value"));
        BamlGeneratedGenericArgument optional =
            builder.DeclareGenericArgument(identity, "optional", optional: true);

        BamlGeneratedGenericFunction other =
            builder.DeclareGenericFunction("generic.other", "call");
        BamlGeneratedTypeParameter otherParameter =
            builder.DeclareTypeParameter(other, "U");
        BamlGeneratedGenericArgument otherValue =
            builder.DeclareGenericArgument(other, "other");

        BamlGeneratedGenericFunction receiver =
            builder.DeclareGenericFunction("generic.receiver", "call");
        _ = builder.DeclareGenericArgument(receiver, "self", isSelf: true);
        Expect<InvalidOperationException>(() =>
            _ = builder.DeclareGenericArgument(receiver, "second_self", isSelf: true));
        BamlGeneratedGenericFunction optionalReceiver =
            builder.DeclareGenericFunction("generic.optional_receiver", "call");
        Expect<InvalidOperationException>(() =>
            _ = builder.DeclareGenericArgument(
                optionalReceiver,
                "self",
                optional: true,
                isSelf: true));

        BamlGeneratedRegistry registry = builder.Build();
        Expect<InvalidOperationException>(() =>
            _ = builder.DeclareTypeParameter(identity, "after"));
        Expect<InvalidOperationException>(() =>
            _ = builder.DeclareGenericArgument(identity, "after"));

        BamlGeneratedType<string> resolved = registry.ResolveType<string>("generic.T");
        BamlGeneratedTypeBinding binding = registry.BindType(parameter, resolved);
        BamlGeneratedTypeBinding otherBinding = registry.BindType(otherParameter, resolved);
        Expect<InvalidOperationException>(() =>
            _ = registry.BindType(default(BamlGeneratedTypeParameter), resolved));
        Expect<InvalidOperationException>(() =>
            _ = registry.BindFunction(identity, resolved));
        Expect<InvalidOperationException>(() =>
            _ = registry.BindFunction(identity, resolved, otherBinding));

        BamlGeneratedBoundFunction<string> bound =
            registry.BindFunction(identity, resolved, binding);
        BamlGeneratedGenericArgumentsBuilder<string> missing =
            registry.CreateArgumentsBuilder(bound);
        Expect<InvalidOperationException>(() => _ = missing.Build());
        Expect<InvalidOperationException>(() => missing.Omit(value));

        BamlGeneratedGenericArgumentsBuilder<string> arguments =
            registry.CreateArgumentsBuilder(bound);
        Expect<InvalidOperationException>(() =>
            arguments.Add(default(BamlGeneratedGenericArgument), resolved, "default"));
        Expect<InvalidOperationException>(() =>
            arguments.Add(otherValue, resolved, "other"));
        arguments.Add(value, resolved, "typed");
        arguments.Omit(optional);
        BamlGeneratedGenericArguments<string> frozen = arguments.Build();
        Require(frozen.Supplied().Count() == 1, "generic optional omission changed");
        CallFunctionArgs encoded = CallFunctionArgs.Parser.ParseFrom(
            PrimitiveProtocol.EncodeCallArguments(frozen, 77));
        Require(
            encoded.CallId == 77
            && encoded.Kwargs.Count == 1
            && encoded.Kwargs[0].StringKey == "value"
            && encoded.Kwargs[0].Value.StringValue == "typed"
            && encoded.TypeArgs.Count == 1
            && encoded.TypeArgs[0].TypeVar == "T"
            && encoded.TypeArgs[0].TypeValue.Primitive.Kind
                == BamlTyPrimitiveKind.BamlTyPrimitiveString,
            "generic call metadata encoding changed");
        Expect<InvalidOperationException>(() =>
            arguments.Add(value, resolved, "late"));

        BamlGeneratedType<IReadOnlyList<string>> list =
            registry.ResolveType<IReadOnlyList<string>>("generic.list");
        InboundValue encodedList = PrimitiveProtocol.Encode(
            registry.Encode(list, Array.AsReadOnly(["one", "two"])));
        Require(
            encodedList.ListValue.Values.Count == 2
            && encodedList.ListValue.Values[0].StringValue == "one"
            && encodedList.ListValue.Values[1].StringValue == "two",
            "generic list encode changed");
        var outboundList = new BamlValueList
        {
            ItemType = BamlTy.Parser.ParseFrom(stringMetadata),
        };
        outboundList.Items.Add(new BamlOutboundValue { StringValue = "one" });
        outboundList.Items.Add(new BamlOutboundValue { StringValue = "two" });
        IReadOnlyList<string> decodedList = registry.Decode(
            list,
            PrimitiveProtocol.Decode(new BamlOutboundValue { ListValue = outboundList }));
        Require(decodedList.SequenceEqual(["one", "two"]), "generic list codec changed");
        BamlGeneratedType<BamlNullable<string>> nullable =
            registry.ResolveType<BamlNullable<string>>("generic.nullable");
        Require(
            registry.Decode(nullable, registry.Encode(nullable, BamlNullable.Null<string>()))
                .IsNull,
            "generic nullable codec changed");

        BamlTypeMappingException narrow = Expect<BamlTypeMappingException>(() =>
            _ = registry.ResolveType<int>("generic.narrow"));
        Require(
            narrow.ClrType == typeof(int)
            && narrow.Position == "generic.narrow"
            && narrow.Path == "generic.narrow"
            && narrow.CanonicalReplacement == "long",
            "generic type mapping diagnostic changed");
        Expect<BamlTypeMappingException>(() =>
            _ = registry.ResolveType<List<string>>("generic.mutable_list"));

        BamlGeneratedRegistryBuilder foreignBuilder =
            BamlGeneratedContract.CreateRegistryBuilder(BamlGeneratedContract.Version);
        BamlGeneratedType<string> foreignText =
            foreignBuilder.DeclareType<string>("foreign.generic.string", stringMetadata);
        foreignBuilder.RegisterCodec(foreignText, new StringCodec());
        foreignBuilder.RegisterGenericBinding(foreignText);
        BamlGeneratedRegistry foreignRegistry = foreignBuilder.Build();
        BamlGeneratedType<string> foreignResolved =
            foreignRegistry.ResolveType<string>("foreign.generic.T");
        Expect<InvalidOperationException>(() =>
            _ = registry.BindType(parameter, foreignResolved));
        Expect<InvalidOperationException>(() =>
            _ = foreignRegistry.CreateArgumentsBuilder(bound));
    }

    private static void VerifyPrimitiveProtocol()
    {
        BamlGeneratedValue[] values =
        [
            BamlGeneratedValue.CreateNull(),
            BamlGeneratedValue.CreateBool(true),
            BamlGeneratedValue.CreateInt(BamlInteger.Minimum),
            BamlGeneratedValue.CreateInt(BamlInteger.Maximum),
            BamlGeneratedValue.CreateFloat(1.25),
            BamlGeneratedValue.CreateString("héllo\0雪"),
            BamlGeneratedValue.CreateBytes([0, 1, 254, 255]),
        ];
        foreach (BamlGeneratedValue value in values)
        {
            InboundValue inbound = PrimitiveProtocol.Encode(value);
            Require(inbound is IMessage, "private protocol positive control failed");
        }
        Expect<BamlProtocolException>(() =>
            _ = PrimitiveProtocol.Encode(BamlGeneratedValue.CreateFloat(double.NaN)));

        BigInteger huge = BigInteger.Parse(
            "1234567890123456789012345678901234567890",
            System.Globalization.CultureInfo.InvariantCulture);
        InboundValue encodedHuge = PrimitiveProtocol.Encode(
            BamlGeneratedValue.CreateBigInt(huge));
        Require(
            encodedHuge.BigintValue
                == huge.ToString("x", System.Globalization.CultureInfo.InvariantCulture),
            "positive bigint wire encoding changed");
        InboundValue encodedNegativeHuge = PrimitiveProtocol.Encode(
            BamlGeneratedValue.CreateBigInt(-huge));
        Require(
            encodedNegativeHuge.BigintValue
                == "-" + huge.ToString("x", System.Globalization.CultureInfo.InvariantCulture),
            "negative bigint wire encoding changed");
        Require(
            PrimitiveProtocol.Decode(
                    new BamlOutboundValue
                    {
                        BigintValue = encodedNegativeHuge.BigintValue,
                    })
                .ReadBigInt() == -huge,
            "bigint wire decoding changed");
        Expect<BamlProtocolException>(() =>
            _ = PrimitiveProtocol.Decode(
                new BamlOutboundValue { BigintValue = "xyz" }));

        BamlGeneratedRegistryBuilder stringBuilder =
            BamlGeneratedContract.CreateRegistryBuilder(BamlGeneratedContract.Version);
        BamlGeneratedType<string> stringType =
            stringBuilder.DeclareType<string>("unicode.string");
        stringBuilder.RegisterCodec(stringType, new StringCodec());
        BamlGeneratedRegistry stringRegistry = stringBuilder.Build();
        Expect<BamlProtocolException>(() =>
            _ = stringRegistry.Encode(stringType, "\ud800"));

        Require(
            PrimitiveProtocol.Decode(new BamlOutboundValue { IntValue = BamlInteger.Minimum }).ReadInt()
                == BamlInteger.Minimum,
            "minimum outbound BAML integer changed");
        Require(
            PrimitiveProtocol.Decode(new BamlOutboundValue { IntValue = BamlInteger.Maximum }).ReadInt()
                == BamlInteger.Maximum,
            "maximum outbound BAML integer changed");
        Expect<BamlProtocolException>(() =>
            _ = PrimitiveProtocol.Decode(
                new BamlOutboundValue { IntValue = BamlInteger.Minimum - 1 }));
        Expect<BamlProtocolException>(() =>
            _ = PrimitiveProtocol.Decode(
                new BamlOutboundValue { IntValue = BamlInteger.Maximum + 1 }));
        Expect<BamlProtocolException>(() =>
            _ = PrimitiveProtocol.Decode(
                new BamlOutboundValue { FloatValue = double.PositiveInfinity }));

        var literal = new BamlOutboundValue
        {
            LiteralValue = new BamlLiteralValue { StringValue = "literal" },
        };
        Require(
            PrimitiveProtocol.Decode(literal).ReadString() == "literal",
            "primitive literal decode changed");

        const string contradictoryUnion =
            "6a2622143a120a0642040a026c660a0842060a0463726c662a062263726c662232061a0463726c66";
        BamlOutboundValue envelope =
            BamlOutboundValue.Parser.ParseFrom(Convert.FromHexString(contradictoryUnion));
        Require(
            envelope.ValueCase == BamlOutboundValue.ValueOneofCase.UnionVariantValue,
            "golden contradictory-union envelope changed");
        Require(
            PrimitiveProtocol.Decode(envelope).Kind == PrimitiveCarrierKind.Union,
            "union carrier decode changed");
    }

    private static void VerifyCompositeProtocol()
    {
        (BamlGeneratedRegistry registry, _, _, _) = CreateGeneratedRegistry();
        var context = new BamlGeneratedCodecContext(registry);
        BamlTy stringType = PrimitiveType(BamlTyPrimitiveKind.BamlTyPrimitiveString);
        BamlTy intType = PrimitiveType(BamlTyPrimitiveKind.BamlTyPrimitiveInt);
        var unknownType = new BamlTy { Unknown = new BamlTyUnknown() };

        var outboundList = new BamlValueList { ItemType = stringType.Clone() };
        outboundList.Items.Add(new BamlOutboundValue { StringValue = "first" });
        outboundList.Items.Add(new BamlOutboundValue { StringValue = "second" });
        BamlGeneratedValue decodedList = PrimitiveProtocol.Decode(
            new BamlOutboundValue { ListValue = outboundList });
        outboundList.Items[0].StringValue = "mutated";
        IReadOnlyList<BamlGeneratedValue> list = context.ReadList(
            decodedList,
            stringType.ToByteArray());
        Require(
            list.Count == 2 && context.ReadString(list[0]) == "first",
            "list decode did not take an owned snapshot");
        Expect<NotSupportedException>(() =>
            ((IList<BamlGeneratedValue>)list)[0] = context.String("replacement"));
        Expect<BamlProtocolException>(() =>
            _ = context.ReadList(decodedList, intType.ToByteArray()));

        var canaryFallbackList = new BamlValueList { ItemType = unknownType.Clone() };
        canaryFallbackList.Items.Add(new BamlOutboundValue { StringValue = "legacy" });
        IReadOnlyList<BamlGeneratedValue> fallbackList = context.ReadList(
            PrimitiveProtocol.Decode(
                new BamlOutboundValue { ListValue = canaryFallbackList }),
            stringType.ToByteArray());
        Require(
            fallbackList.Count == 1 && context.ReadString(fallbackList[0]) == "legacy",
            "Canary unknown list fallback no longer decodes through a generated type");

        var inputItems = new List<BamlGeneratedValue> { context.String("owned") };
        BamlGeneratedValue inputList = context.List(inputItems);
        inputItems[0] = context.String("mutated");
        InboundValue encodedList = PrimitiveProtocol.Encode(inputList);
        Require(
            encodedList.ListValue.Values.Count == 1
            && encodedList.ListValue.Values[0].StringValue == "owned",
            "list encode did not take an owned snapshot");
        InboundValue encodedTypedList = PrimitiveProtocol.Encode(
            context.List(Array.Empty<BamlGeneratedValue>(), stringType.ToByteArray()));
        Require(
            encodedTypedList.ValueType?.TyCase == BamlTy.TyOneofCase.List
            && encodedTypedList.ValueType.List.Item.Equals(stringType)
            && encodedTypedList.ListValue.Values.Count == 0,
            "typed list encode did not carry its exact node-local value_type");

        var outboundMap = new BamlValueMap
        {
            KeyType = stringType.Clone(),
            ValueType = intType.Clone(),
        };
        outboundMap.Entries.Add(new BamlOutboundMapEntry
        {
            Key = "answer",
            Value = new BamlOutboundValue { IntValue = 42 },
        });
        BamlGeneratedValue decodedMap = PrimitiveProtocol.Decode(
            new BamlOutboundValue { MapValue = outboundMap });
        outboundMap.Entries[0].Value.IntValue = 0;
        IReadOnlyDictionary<string, BamlGeneratedValue> map = context.ReadMap(
            decodedMap,
            stringType.ToByteArray(),
            intType.ToByteArray());
        Require(
            map.Count == 1 && context.ReadInt(map["answer"]) == 42,
            "map decode did not take an owned snapshot");
        InboundValue encodedTypedMap = PrimitiveProtocol.Encode(
            context.Map(
                Array.Empty<KeyValuePair<string, BamlGeneratedValue>>(),
                stringType.ToByteArray(),
                intType.ToByteArray()));
        Require(
            encodedTypedMap.ValueType?.TyCase == BamlTy.TyOneofCase.Map
            && encodedTypedMap.ValueType.Map.Key.Equals(stringType)
            && encodedTypedMap.ValueType.Map.Value.Equals(intType)
            && encodedTypedMap.MapValue.Entries.Count == 0,
            "typed map encode did not carry its exact node-local value_type");
        Expect<BamlProtocolException>(() =>
            _ = context.ReadMap(
                decodedMap,
                stringType.ToByteArray(),
                stringType.ToByteArray()));
        Expect<NotSupportedException>(() =>
            ((IDictionary<string, BamlGeneratedValue>)map).Add(
                "other",
                context.Int(1)));

        var canaryFallbackMap = new BamlValueMap
        {
            KeyType = stringType.Clone(),
            ValueType = unknownType.Clone(),
        };
        canaryFallbackMap.Entries.Add(new BamlOutboundMapEntry
        {
            Key = "legacy",
            Value = new BamlOutboundValue { IntValue = 7 },
        });
        IReadOnlyDictionary<string, BamlGeneratedValue> fallbackMap = context.ReadMap(
            PrimitiveProtocol.Decode(
                new BamlOutboundValue { MapValue = canaryFallbackMap }),
            stringType.ToByteArray(),
            intType.ToByteArray());
        Require(
            fallbackMap.Count == 1 && context.ReadInt(fallbackMap["legacy"]) == 7,
            "Canary unknown map fallback no longer decodes through a generated type");

        var duplicateMap = new BamlValueMap
        {
            KeyType = stringType.Clone(),
            ValueType = intType.Clone(),
        };
        duplicateMap.Entries.Add(new BamlOutboundMapEntry
        {
            Key = "same",
            Value = new BamlOutboundValue { IntValue = 1 },
        });
        duplicateMap.Entries.Add(new BamlOutboundMapEntry
        {
            Key = "same",
            Value = new BamlOutboundValue { IntValue = 2 },
        });
        BamlGeneratedValue decodedDuplicateMap = PrimitiveProtocol.Decode(
            new BamlOutboundValue { MapValue = duplicateMap });
        Expect<BamlProtocolException>(() =>
            _ = context.ReadMap(
                decodedDuplicateMap,
                stringType.ToByteArray(),
                intType.ToByteArray()));

        const string classIdentity = "nominal.Profile";
        var outboundClass = new BamlValueClass { Name = classIdentity };
        outboundClass.Fields.Add(new BamlOutboundMapEntry
        {
            Key = "name",
            Value = new BamlOutboundValue { StringValue = "Ada" },
        });
        BamlGeneratedValue decodedClass = PrimitiveProtocol.Decode(
            new BamlOutboundValue { ClassValue = outboundClass });
        IReadOnlyDictionary<string, BamlGeneratedValue> fields =
            context.ReadClass(decodedClass, classIdentity);
        Require(
            fields.Count == 1 && context.ReadString(fields["name"]) == "Ada",
            "class field decode changed");
        Expect<BamlProtocolException>(() =>
            _ = context.ReadClass(decodedClass, "nominal.Other"));

        InboundValue encodedClass = PrimitiveProtocol.Encode(
            context.Class(
                classIdentity,
                [new("name", context.String("Grace"))]));
        Require(
            encodedClass.ValueType?.TyCase == BamlTy.TyOneofCase.ClassTy
            && encodedClass.ValueType.ClassTy.Name == classIdentity
            && encodedClass.ClassValue.Fields.Count == 1
            && encodedClass.ClassValue.Fields[0].StringKey == "name",
            "class encode changed");

        const string enumIdentity = "nominal.Status";
        BamlGeneratedValue decodedEnum = PrimitiveProtocol.Decode(
            new BamlOutboundValue
            {
                EnumValue = new BamlValueEnum
                {
                    Name = enumIdentity,
                    Value = "http-error",
                },
            });
        Require(
            context.ReadEnum(decodedEnum, enumIdentity) == "http-error",
            "enum decode changed");
        Expect<BamlProtocolException>(() =>
            _ = context.ReadEnum(decodedEnum, "nominal.OtherStatus"));
        BamlGeneratedValue dynamicEnum = PrimitiveProtocol.Decode(
            new BamlOutboundValue
            {
                EnumValue = new BamlValueEnum
                {
                    Name = enumIdentity,
                    Value = "future",
                    IsDynamic = true,
                },
            });
        Expect<BamlProtocolException>(() =>
            _ = context.ReadEnum(dynamicEnum, enumIdentity));
        InboundValue encodedEnum = PrimitiveProtocol.Encode(
            context.Enum(enumIdentity, "ok"));
        Require(
            encodedEnum.EnumValue.Name == enumIdentity
            && encodedEnum.EnumValue.Value == "ok",
            "enum encode changed");

        BamlTy unionType = new() { Union = new BamlTyUnion() };
        unionType.Union.Options.Add(stringType.Clone());
        unionType.Union.Options.Add(intType.Clone());
        string[] options = ["string", "int"];
        var outboundUnion = new BamlOutboundValue
        {
            UnionVariantValue = new BamlValueUnionVariant
            {
                SelfType = unionType.Clone(),
                ValueOptionName = "int",
                Value = new BamlOutboundValue { IntValue = 7 },
            },
        };
        BamlGeneratedValue decodedUnion = PrimitiveProtocol.Decode(outboundUnion);
        BamlGeneratedUnionValue selected = context.ReadUnion(
            decodedUnion,
            unionType.ToByteArray(),
            options);
        Require(
            selected.CaseIndex == 1 && context.ReadInt(selected.Value) == 7,
            "union metadata decode changed");

        BamlOutboundValue genericUnion = outboundUnion.Clone();
        genericUnion.UnionVariantValue.ValueOptionName = "T";
        BamlGeneratedUnionValue genericSelected = context.ReadUnion(
            PrimitiveProtocol.Decode(genericUnion),
            unionType.ToByteArray(),
            options,
            [stringType.ToByteArray(), intType.ToByteArray()]);
        Require(
            genericSelected.CaseIndex == 1
            && context.ReadInt(genericSelected.Value) == 7,
            "generic Canary union option did not resolve from its concrete payload");

        InboundValue encodedUnion = PrimitiveProtocol.Encode(
            context.Union(
                unionType.ToByteArray(),
                intType.ToByteArray(),
                "int",
                context.Int(7)));
        Require(
            encodedUnion.ValueCase == InboundValue.ValueOneofCase.IntValue
            && encodedUnion.IntValue == 7
            && encodedUnion.ValueType?.Equals(intType) == true,
            "inbound union did not project its payload with the selected exact type");
        Expect<BamlProtocolException>(() =>
            _ = PrimitiveProtocol.Encode(
                context.Union(
                    unionType.ToByteArray(),
                    unionType.ToByteArray(),
                    "invalid",
                    context.Int(7))));
        BamlTy optionalIntType = new()
        {
            Optional = new BamlTyOptional { Inner = intType.Clone() },
        };
        Expect<BamlProtocolException>(() =>
            _ = PrimitiveProtocol.Encode(
                context.Union(
                    unionType.ToByteArray(),
                    optionalIntType.ToByteArray(),
                    "invalid optional",
                    context.Int(7))));

        BamlOutboundValue indexedUnion = outboundUnion.Clone();
        indexedUnion.UnionVariantValue.SelectedOptionIndex = 1;
        indexedUnion.UnionVariantValue.ValueOptionName = "misleading display name";
        BamlGeneratedUnionValue indexedSelected = context.ReadUnion(
            PrimitiveProtocol.Decode(indexedUnion),
            unionType.ToByteArray(),
            options,
            [stringType.ToByteArray(), intType.ToByteArray()]);
        Require(
            indexedSelected.CaseIndex == 1
            && context.ReadInt(indexedSelected.Value) == 7,
            "outbound selected_option_index did not override the display-only option name");

        BamlTy stringListType = new()
        {
            List = new BamlTyList { Item = stringType.Clone() },
        };
        BamlTy intListType = new()
        {
            List = new BamlTyList { Item = intType.Clone() },
        };
        BamlTy nullableListUnion = new() { Union = new BamlTyUnion() };
        nullableListUnion.Union.Options.Add(
            PrimitiveType(BamlTyPrimitiveKind.BamlTyPrimitiveNull));
        nullableListUnion.Union.Options.Add(stringListType.Clone());
        nullableListUnion.Union.Options.Add(intListType.Clone());
        var indexedEmptyList = new BamlOutboundValue
        {
            UnionVariantValue = new BamlValueUnionVariant
            {
                SelfType = nullableListUnion,
                ValueOptionName = "string[]",
                SelectedOptionIndex = 2,
                Value = new BamlOutboundValue
                {
                    ListValue = new BamlValueList { ItemType = intType.Clone() },
                },
            },
        };
        BamlGeneratedUnionValue compactSelected = context.ReadUnion(
            PrimitiveProtocol.Decode(indexedEmptyList),
            indexedEmptyList.UnionVariantValue.SelfType.ToByteArray(),
            ["string[]", "int[]"],
            [stringListType.ToByteArray(), intListType.ToByteArray()]);
        Require(
            compactSelected.CaseIndex == 1
            && context.ReadList(compactSelected.Value, intType.ToByteArray()).Count == 0,
            "raw union option index was applied directly instead of resolving through self_type");

        BamlOutboundValue outOfRangeUnion = indexedUnion.Clone();
        outOfRangeUnion.UnionVariantValue.SelectedOptionIndex = 2;
        Expect<BamlProtocolException>(() => _ = PrimitiveProtocol.Decode(outOfRangeUnion));

        BamlTy reorderedUnionType = new() { Union = new BamlTyUnion() };
        reorderedUnionType.Union.Options.Add(intType.Clone());
        reorderedUnionType.Union.Options.Add(stringType.Clone());
        BamlGeneratedUnionValue reordered = context.ReadUnion(
            decodedUnion,
            reorderedUnionType.ToByteArray(),
            options);
        Require(
            reordered.CaseIndex == 1 && context.ReadInt(reordered.Value) == 7,
            "Canary union normalization changed the selected generated case");

        BamlTy boolType = new()
        {
            Primitive = new BamlTyPrimitive
            {
                Kind = BamlTyPrimitiveKind.BamlTyPrimitiveBool,
            },
        };
        BamlTy wrongUnionType = new() { Union = new BamlTyUnion() };
        wrongUnionType.Union.Options.Add(boolType);
        wrongUnionType.Union.Options.Add(stringType.Clone());
        Expect<BamlProtocolException>(() =>
            _ = context.ReadUnion(
                decodedUnion,
                wrongUnionType.ToByteArray(),
                options));

        BamlOutboundValue unknownOption = outboundUnion.Clone();
        unknownOption.UnionVariantValue.ValueOptionName = "bool";
        BamlGeneratedValue decodedUnknown = PrimitiveProtocol.Decode(unknownOption);
        Expect<BamlProtocolException>(() =>
            _ = context.ReadUnion(
                decodedUnknown,
                unionType.ToByteArray(),
                options));

        BamlOutboundValue contradictoryPayload = outboundUnion.Clone();
        contradictoryPayload.UnionVariantValue.Value =
            new BamlOutboundValue { StringValue = "not-an-int" };
        BamlGeneratedUnionValue contradictory = context.ReadUnion(
            PrimitiveProtocol.Decode(contradictoryPayload),
            unionType.ToByteArray(),
            options);
        BamlProtocolException pathError = Expect<BamlProtocolException>(() =>
            _ = context.ReadInt(contradictory.Value));
        Require(
            pathError.SensitiveDiagnostic.Contains("$result<int>", StringComparison.Ordinal),
            "nested union decode error lost its value path");

        BamlTy literalType = new()
        {
            Literal = new BamlTyLiteral { StringValue = "fixed" },
        };
        BamlTy literalUnionType = new() { Union = new BamlTyUnion() };
        literalUnionType.Union.Options.Add(literalType.Clone());
        literalUnionType.Union.Options.Add(stringType.Clone());
        var literalUnion = new BamlValue(
            PrimitiveProtocol.Decode(
                new BamlOutboundValue
                {
                    UnionVariantValue = new BamlValueUnionVariant
                    {
                        SelfType = literalUnionType,
                        ValueOptionName = "\"fixed\"",
                        Value = new BamlOutboundValue { StringValue = "fixed" },
                    },
                }));
        Require(
            literalUnion.TryGetUnion(out int literalCase, out BamlValue? literalPayload)
                && literalCase == 0
                && literalPayload.Type.Literal == "fixed"
                && !literalPayload.TryGet(out string? projectedLiteral)
                && projectedLiteral is null,
            "literal union occurrence identity was erased");
        Expect<BamlTypeMappingException>(() => _ = literalPayload!.As<string>());
    }

    private static BamlTy PrimitiveType(BamlTyPrimitiveKind kind) =>
        new()
        {
            Primitive = new BamlTyPrimitive { Kind = kind },
        };

    private static (
        BamlGeneratedRegistry Registry,
        BamlGeneratedFunction<string> Function,
        BamlGeneratedArgument<string, string> Required,
        BamlGeneratedArgument<string, string?> Optional) CreateGeneratedRegistry()
    {
        BamlGeneratedRegistryBuilder builder =
            BamlGeneratedContract.CreateRegistryBuilder(BamlGeneratedContract.Version);
        BamlGeneratedType<string> text = builder.DeclareType<string>("baml.string");
        BamlGeneratedType<string?> nullableText = builder.DeclareType<string?>("baml.string?");
        builder.RegisterCodec(text, new StringCodec());
        builder.RegisterCodec(nullableText, new NullableStringCodec());
        BamlGeneratedFunction<string> function =
            builder.DeclareFunction("test.echo", "call", text);
        BamlGeneratedArgument<string, string> required =
            builder.DeclareArgument(function, "value", text);
        BamlGeneratedArgument<string, string?> optional =
            builder.DeclareArgument(function, "optional", nullableText, optional: true);
        return (builder.Build(), function, required, optional);
    }

    private static string Fingerprint(ReadOnlySpan<byte> bytes) =>
        Convert.ToHexString(SHA256.HashData(bytes)).ToLowerInvariant();

    private static void VerifyNativeAbiLayoutAndValidation()
    {
        Require(sizeof(BamlBuffer) == 16, "BamlBuffer layout changed");
        Require(sizeof(BamlBridgeInfoV1) == 64, "BamlBridgeInfoV1 layout changed");
        Require(sizeof(BamlApiV1) == 200, "BamlApiV1 layout changed");
        (string Field, int Offset)[] layout =
        [
            (nameof(BamlApiV1.AbiVersion), 0),
            (nameof(BamlApiV1.StructSize), 8),
            (nameof(BamlApiV1.Version), 16),
            (nameof(BamlApiV1.InitializeRuntimeFromBytecode), 24),
            (nameof(BamlApiV1.FreeBuffer), 32),
            (nameof(BamlApiV1.RegisterCallback), 40),
            (nameof(BamlApiV1.CallFunction), 48),
            (nameof(BamlApiV1.NewFunctionCall), 56),
            (nameof(BamlApiV1.CancelFunctionCall), 64),
            (nameof(BamlApiV1.RegisterHostDispatchCallback), 72),
            (nameof(BamlApiV1.RegisterHostReleaseCallback), 80),
            (nameof(BamlApiV1.CompleteHostCall), 88),
            (nameof(BamlApiV1.HandleClone), 96),
            (nameof(BamlApiV1.HandleRelease), 104),
            (nameof(BamlApiV1.MediaFromUrl), 112),
            (nameof(BamlApiV1.MediaFromFile), 120),
            (nameof(BamlApiV1.MediaFromBase64), 128),
            (nameof(BamlApiV1.MediaUrl), 136),
            (nameof(BamlApiV1.MediaFile), 144),
            (nameof(BamlApiV1.MediaBase64), 152),
            (nameof(BamlApiV1.MediaMimeType), 160),
            (nameof(BamlApiV1.RegisterBridge), 168),
            (nameof(BamlApiV1.RegisterUnhandledSpawnErrorCallback), 176),
            (nameof(BamlApiV1.ShutdownRuntime), 184),
            (nameof(BamlApiV1.InitializeRuntimeFromBytecodeWithMetadata), 192),
        ];
        foreach ((string field, int offset) in layout)
        {
            Require(
                Marshal.OffsetOf<BamlApiV1>(field) == (nint)offset,
                $"BamlApiV1.{field} offset changed");
        }

        BamlApiV1 table = CreateValidTable();
        NativeApi.ValidateTable(&table);
        Require(
            BamlApiV1Layout.RequiredPrefixSize == 200,
            "BamlApiV1 required prefix changed");
        table = CreateValidTable();
        table.StructSize += 64;
        NativeApi.ValidateTable(&table);

        ExpectInvalidTable(default, passNull: true);
        table = CreateValidTable();
        table.AbiVersion = 999;
        ExpectInvalidTable(table);
        table = CreateValidTable();
        table.StructSize = 192;
        ExpectInvalidTable(table);
        table = CreateValidTable();
        table.RegisterBridge = null;
        ExpectInvalidTable(table);
        for (int field = 0; field < 23; field++)
        {
            table = CreateValidTable();
            ClearRequiredFunction(ref table, field);
            ExpectInvalidTable(table);
        }

        releasedBuffers = 0;
        bridgeRegistrations = 0;
        table = CreateValidTable();
        NativeApi.RegisterBridge(&table);
        Require(bridgeRegistrations == 1, "bridge registration count changed");
        Require(releasedBuffers == 1, "bridge registration buffer was not released once");
    }

    private static void VerifyOwnedBuffers()
    {
        BamlApiV1 table = CreateValidTable();
        releasedBuffers = 0;
        string version = NativeBuffer.ReadUtf8AndFree(&table, Allocate("0.15.0"u8));
        Require(version == "0.15.0" && releasedBuffers == 1, "owned version buffer was not released once");

        BamlBuffer empty = default;
        Require(NativeBuffer.CopyAndFree(&table, empty).Length == 0, "empty buffer changed");
        Require(releasedBuffers == 2, "zero-length buffer was not released once");

        BamlBuffer invalid = Allocate([0xff]);
        ExpectInvalidBuffer(&table, invalid, decodeUtf8: true);
        Require(releasedBuffers == 3, "invalid UTF-8 buffer was not released once");

        BamlBuffer nullNonempty = new() { Pointer = null, Length = 1 };
        ExpectInvalidBuffer(&table, nullNonempty, decodeUtf8: false);
        Require(releasedBuffers == 4, "invalid null buffer was not released once");
    }

    private static void VerifyFailureContract()
    {
        Require(typeof(BamlException).IsAbstract, "BamlException must remain abstract");
        Require(typeof(BamlExecutionException).IsAbstract, "BamlExecutionException must remain abstract");
        Require(typeof(BamlInitializationException).IsAbstract, "BamlInitializationException must remain abstract");
        Require(typeof(BamlInteropException).IsAbstract, "BamlInteropException must remain abstract");
        Require(!typeof(BamlErrorException).IsSealed, "BamlErrorException must remain extensible for type mismatch");
        Require(typeof(BamlTypeMismatchException).IsSealed, "type mismatch exception must remain sealed");
        Require(typeof(BamlPanicException).IsSealed, "panic exception must remain sealed");
        Require(typeof(BamlHostCallbackException).IsSealed, "host callback exception must remain sealed");
        Require(typeof(BamlOperationCanceledException).IsSealed, "operation cancellation must remain sealed");
        Require(
            typeof(BamlErrorException).BaseType == typeof(BamlExecutionException)
                && typeof(BamlTypeMismatchException).BaseType == typeof(BamlErrorException),
            "execution exception hierarchy changed");
        Require(
            !typeof(BamlException).IsAssignableFrom(typeof(BamlOperationCanceledException))
                && typeof(OperationCanceledException).IsAssignableFrom(typeof(BamlOperationCanceledException)),
            "operation cancellation moved into the failure hierarchy");
        Require(
            (int)BamlCancellationOrigin.Caller == 0
                && (int)BamlCancellationOrigin.Engine == 1
                && (int)BamlCancellationOrigin.StreamDisposed == 2,
            "cancellation origin values changed");
        Require(
            typeof(BamlException).Assembly.GetType("Baml.BamlTraceFrame") is null,
            "rendered traces grew fabricated frame structure");
        Require(
            typeof(BamlException).Assembly
                .GetExportedTypes()
                .Where(type => type.Name.StartsWith("Baml", StringComparison.Ordinal))
                .Where(type => typeof(Exception).IsAssignableFrom(type))
                .SelectMany(type => type.GetConstructors())
                .Count() == 0,
            "bridge exceptions exposed public constructors");

        const string secret = "Bearer secret-prompt signed-url";
        var typedErrorEnvelope = new BamlOutboundResult
        {
            Error = new BamlOutboundError
            {
                Value = NominalValue("fixture.errors.Failure", secret),
            },
        };
        typedErrorEnvelope.Error.Trace.Add("outer frame");
        typedErrorEnvelope.Error.Trace.Add("inner frame");
        BamlErrorException typedError = Expect<BamlErrorException>(() =>
            _ = PrimitiveProtocol.DecodeCallResult(
                typedErrorEnvelope.ToByteArray(),
                "fixture.call"));
        Require(typedError.BamlFunction == "fixture.call", "error lost the managed function identity");
        Require(typedError.ErrorName == "fixture.errors.Failure", "error lost nominal identity");
        Require(
            typedError.ThrownValue.Kind == BamlValueKind.Class
                && typedError.ThrownValue.Type.Kind == BamlTypeDescriptorKind.Class
                && typedError.ThrownValue.Type.Fqn == "fixture.errors.Failure",
            "error lost its public value descriptor");
        Require(
            typedError.Trace.Lines.SequenceEqual(["outer frame", "inner frame"]),
            "error trace order changed");
        Require(
            !typedError.Message.Contains(secret, StringComparison.Ordinal)
                && !typedError.ToString().Contains(secret, StringComparison.Ordinal),
            "error formatting leaked its thrown value");
        BamlErrorException equalTypedError = Expect<BamlErrorException>(() =>
            _ = PrimitiveProtocol.DecodeCallResult(
                typedErrorEnvelope.ToByteArray(),
                "fixture.call"));
        Require(
            typedError.ThrownValue.Equals(equalTypedError.ThrownValue)
                && typedError.ThrownValue.GetHashCode()
                    == equalTypedError.ThrownValue.GetHashCode()
                && !typedError.ThrownValue.ToString().Contains(secret, StringComparison.Ordinal),
            "decoded BamlValue structural identity or redaction changed");

        var mismatchEnvelope = new BamlOutboundResult
        {
            Error = new BamlOutboundError
            {
                Value = NominalValue("baml.errors.TypeMismatch", secret),
            },
        };
        BamlTypeMismatchException mismatch = Expect<BamlTypeMismatchException>(() =>
            _ = PrimitiveProtocol.DecodeCallResult(mismatchEnvelope.ToByteArray(), "fixture.call"));
        Require(
            mismatch.ErrorName == "baml.errors.TypeMismatch"
                && !typeof(BamlTypeMismatchException)
                    .GetProperties(BindingFlags.Public | BindingFlags.Instance | BindingFlags.DeclaredOnly)
                    .Any(),
            "type mismatch invented fields absent from the wire");

        var panicEnvelope = new BamlOutboundResult
        {
            Panic = new BamlOutboundPanic
            {
                Value = NominalValue("baml.panics.UserPanic", secret),
                IsExitPanic = false,
                ExitCode = 91,
            },
        };
        panicEnvelope.Panic.Trace.Add("panic frame");
        BamlPanicException panic = Expect<BamlPanicException>(() =>
            _ = PrimitiveProtocol.DecodeCallResult(panicEnvelope.ToByteArray(), "fixture.call"));
        Require(
            panic.BamlFunction == "fixture.call"
                && !panic.Panic.IsExitPanic
                && panic.Panic.ExitCode is null
                && panic.Trace.Lines.SequenceEqual(["panic frame"]),
            "catchable panic metadata changed");
        Require(
            !panic.Message.Contains(secret, StringComparison.Ordinal)
                && !panic.ToString().Contains(secret, StringComparison.Ordinal),
            "panic formatting leaked its value");

        var canceledEnvelope = new BamlOutboundResult
        {
            Panic = new BamlOutboundPanic
            {
                Value = NominalValue("baml.panics.Cancelled", "engine stopped"),
            },
        };
        BamlOperationCanceledException engineCanceled = Expect<BamlOperationCanceledException>(() =>
            _ = PrimitiveProtocol.DecodeCallResult(canceledEnvelope.ToByteArray(), "fixture.call"));
        Require(
            engineCanceled.Origin == BamlCancellationOrigin.Engine
                && engineCanceled.BamlFunction == "fixture.call"
                && engineCanceled.CancellationToken.IsCancellationRequested,
            "engine cancellation metadata changed");

        var missingErrorValue = new BamlOutboundResult { Error = new BamlOutboundError() };
        Expect<BamlProtocolException>(() =>
            _ = PrimitiveProtocol.DecodeCallResult(missingErrorValue.ToByteArray(), "fixture.call"));

        var traceSource = new List<string> { "one", "two" };
        var trace = new BamlTrace(traceSource);
        traceSource[0] = "mutated";
        var equalTrace = new BamlTrace(["one", "two"]);
        Require(
            trace.Equals(equalTrace)
                && trace.GetHashCode() == equalTrace.GetHashCode()
                && trace.Lines[0] == "one",
            "trace ownership or structural equality changed");
    }

    private static void VerifyManagedValueModels()
    {
        BamlStreamState<string> pending = default;
        BamlStreamState<string> incomplete = BamlStreamState<string>.Incomplete("partial");
        BamlStreamState<string> complete = BamlStreamState<string>.Complete("final");
        Require(
            pending.State == BamlStreamStateKind.Pending
                && pending.Value is null
                && !pending.IsComplete
                && incomplete.State == BamlStreamStateKind.Incomplete
                && !incomplete.IsComplete
                && complete.State == BamlStreamStateKind.Complete
                && complete.IsComplete
                && complete == BamlStreamState<string>.Complete("final"),
            "stream-state value semantics changed");

        byte[] nullableStringMetadata = new BamlTy
        {
            Optional = new BamlTyOptional
            {
                Inner = PrimitiveType(BamlTyPrimitiveKind.BamlTyPrimitiveString),
            },
        }.ToByteArray();
        BamlGeneratedRegistryBuilder streamStateBuilder =
            BamlGeneratedContract.CreateRegistryBuilder(BamlGeneratedContract.Version);
        BamlGeneratedType<string?> nullableStringType =
            streamStateBuilder.DeclareType<string?>("string?", nullableStringMetadata);
        streamStateBuilder.RegisterCodec(nullableStringType, new NullableStringCodec());
        var streamStateContext = new BamlGeneratedCodecContext(streamStateBuilder.Build());
        BamlGeneratedValue pendingWire = streamStateContext.StreamState(
            nullableStringType,
            default(BamlStreamState<string?>));
        BamlGeneratedValue incompleteWire = streamStateContext.StreamState(
            nullableStringType,
            BamlStreamState<string?>.Incomplete("partial"));
        Require(
            streamStateContext.ReadStreamState(nullableStringType, pendingWire).State
                == BamlStreamStateKind.Pending
                && streamStateContext.ReadStreamState(nullableStringType, incompleteWire)
                    == BamlStreamState<string?>.Incomplete("partial"),
            "stream-state generated codec did not preserve pending/incomplete values");
        IReadOnlyDictionary<string, BamlGeneratedValue> streamStateFields =
            streamStateContext.ReadClass(incompleteWire, "StreamState");
        Require(
            streamStateFields.Count == 2
                && streamStateContext.ReadString(streamStateFields["state"]) == "Incomplete",
            "stream-state generated codec changed its exact carrier");
        BamlGeneratedValue invalidStreamState = streamStateContext.Class(
            "StreamState",
            new KeyValuePair<string, BamlGeneratedValue>[]
            {
                new("value", streamStateContext.String("value")),
                new("state", streamStateContext.String("Unknown")),
            });
        Expect<BamlProtocolException>(() =>
            _ = streamStateContext.ReadStreamState(nullableStringType, invalidStreamState));

        byte[] source = [0, 1, 254, 255];
        BamlImage image = BamlImage.FromBytes(source, "image/png");
        source[0] = 99;
        Require(
            image.TryGetBytes(out ReadOnlyMemory<byte> imageBytes, out string? imageType)
                && imageBytes.Span.SequenceEqual(new byte[] { 0, 1, 254, 255 })
                && imageType == "image/png"
                && !image.IsUrl,
            "image bytes were not snapshotted");
        BamlImage equalImage = BamlImage.FromBase64("AAH+/w==", "image/png");
        Require(
            image.Equals(equalImage) && image.GetHashCode() == equalImage.GetHashCode(),
            "media structural equality changed");

        BamlAudio audio = BamlAudio.FromUrl(
            "https://example.com/audio.wav?signature=sensitive",
            "audio/wav");
        Require(
            audio.IsUrl
                && audio.TryGetUrl(out string? audioUrl)
                && audioUrl.Contains("signature=sensitive", StringComparison.Ordinal)
                && !audio.ToString().Contains("signature=sensitive", StringComparison.Ordinal),
            "media URL access or redaction changed");
        BamlVideo video = BamlVideo.FromBytes(new byte[] { 7, 8 }, "video/mp4");
        BamlPdf pdf = BamlPdf.FromBytes(new byte[] { 9, 10 });
        Require(
            video.TryGetBytes(out ReadOnlyMemory<byte> videoBytes, out string? videoType)
                && videoBytes.Span.SequenceEqual(new byte[] { 7, 8 })
                && videoType == "video/mp4"
                && pdf.TryGetBytes(out ReadOnlyMemory<byte> pdfBytes, out string? pdfType)
                && pdfBytes.Span.SequenceEqual(new byte[] { 9, 10 })
                && pdfType == "application/pdf",
            "video/PDF owned media values changed");
        Expect<FormatException>(() => _ = BamlImage.FromBase64("not base64", "image/png"));

        string mediaPath = Path.Combine(
            Path.GetTempPath(),
            $"baml-media-{Guid.NewGuid():N}.bin");
        try
        {
            File.WriteAllBytes(mediaPath, [3, 4, 5]);
            BamlAudio fileAudio = BamlAudio.FromFileAsync(mediaPath, "audio/test")
                .GetAwaiter()
                .GetResult();
            File.Delete(mediaPath);
            Require(
                fileAudio.TryGetBytes(out ReadOnlyMemory<byte> fileBytes, out string? fileType)
                    && fileBytes.Span.SequenceEqual(new byte[] { 3, 4, 5 })
                    && fileType == "audio/test",
                "file media was not eagerly owned");
        }
        finally
        {
            File.Delete(mediaPath);
        }

        byte[] requestBody = [10, 11, 12];
        var request = new BamlHttpRequest(
            "request-id",
            "POST",
            "https://example.com/private?token=sensitive",
            [new("X-Trace", "one"), new("X-Trace", "two")],
            "application/octet-stream",
            requestBody);
        requestBody[0] = 99;
        using HttpRequestMessage first = request.ToHttpRequestMessage();
        using HttpRequestMessage second = request.ToHttpRequestMessage();
        byte[] firstBody = first.Content!.ReadAsByteArrayAsync().GetAwaiter().GetResult();
        Require(
            request.Id == "request-id"
                && request.Method == "POST"
                && request.Headers.Count == 2
                && request.Body.Span.SequenceEqual(new byte[] { 10, 11, 12 })
                && firstBody.SequenceEqual(new byte[] { 10, 11, 12 })
                && !ReferenceEquals(first, second)
                && !request.ToString().Contains("token=sensitive", StringComparison.Ordinal),
            "HTTP request snapshot, adapter, or redaction changed");

        var retry = new BamlRetryPolicy(3, 10, 100, 2.0);
        var client = new BamlClient(
            "fallback",
            BamlClientType.Fallback,
            [BamlClient.FromShorthand("openai/gpt")],
            retry,
            counter: 2);
        Require(
            client.Name == "fallback"
                && client.ClientType == BamlClientType.Fallback
                && client.SubClients.Count == 1
                && client.RetryPolicy!.Equals(new BamlRetryPolicy(3, 10, 100, 2.0))
                && client.Counter == 2,
            "client/retry immutable projection changed");
        Expect<ArgumentOutOfRangeException>(() =>
            _ = new BamlRetryPolicy(-1, null, null, null));

        BamlValue explicitNull = BamlValue.Null;
        BamlValue boolValue = BamlValue.Bool(true);
        BamlValue intValue = BamlValue.Int(BamlInteger.Maximum);
        BamlValue floatValue = BamlValue.Float(1.5);
        BamlValue bigIntValue = BamlValue.BigInt(
            BigInteger.Parse("123456789012345678901234567890", CultureInfo.InvariantCulture));
        BamlValue stringValue = BamlValue.String("value");
        byte[] dynamicBytesSource = [1, 2, 3];
        BamlValue bytesValue = BamlValue.Bytes(dynamicBytesSource);
        dynamicBytesSource[0] = 99;
        BamlValue listValue = BamlValue.List([intValue, BamlValue.String("mixed")]);
        BamlValue mapA = BamlValue.Map(
            [new("z", BamlValue.Int(2)), new("a", BamlValue.Int(1))]);
        BamlValue mapB = BamlValue.Map(
            [new("a", BamlValue.Int(1)), new("z", BamlValue.Int(2))]);
        Require(
            explicitNull.Kind == BamlValueKind.Null
                && explicitNull.Type.Kind == BamlTypeDescriptorKind.Null
                && boolValue.As<bool>()
                && intValue.As<long>() == BamlInteger.Maximum
                && intValue.As<long?>() == BamlInteger.Maximum
                && floatValue.As<double>() == 1.5
                && bigIntValue.As<BigInteger>()
                    == BigInteger.Parse(
                        "123456789012345678901234567890",
                        CultureInfo.InvariantCulture)
                && stringValue.As<string>() == "value"
                && bytesValue.As<ReadOnlyMemory<byte>>().Span.SequenceEqual(new byte[] { 1, 2, 3 })
                && listValue.Type.Kind == BamlTypeDescriptorKind.List
                && listValue.Type.Arguments[0].Kind == BamlTypeDescriptorKind.Unknown
                && mapA.Type.Kind == BamlTypeDescriptorKind.Map
                && mapA.Type.Arguments[0].Kind == BamlTypeDescriptorKind.String
                && mapA.Type.Arguments[1].Kind == BamlTypeDescriptorKind.Unknown
                && mapA.Equals(mapB)
                && mapA.GetHashCode() == mapB.GetHashCode(),
            "dynamic primitive, ownership, or descriptor semantics changed");
        long? nullableLong = null;
        Require(
            explicitNull.TryGet(out BamlValue? decodedNull)
                && ReferenceEquals(decodedNull, BamlValue.Null)
                && explicitNull.TryGet(out long? decodedNullableLong)
                && decodedNullableLong is null
                && ReferenceEquals(BamlValue.From(nullableLong), BamlValue.Null)
                && !explicitNull.TryGet(out string? rejectedString)
                && rejectedString is null,
            "dynamic null escaped its explicit nullable contract");
        Expect<BamlTypeMappingException>(() => _ = BamlValue.From(42));
        Expect<BamlTypeMappingException>(() => _ = BamlValue.From<object>(42L));
        Expect<BamlTypeMappingException>(() => _ = BamlValue.From(new byte[] { 1, 2 }));
        Expect<BamlTypeMappingException>(() => _ = BamlValue.From<string?>(null));
        Expect<BamlTypeMappingException>(() => _ = explicitNull.As<string>());
        Expect<BamlTypeMismatchException>(() => _ = stringValue.As<long>());
        Expect<BamlTypeMappingException>(() => _ = BamlValue.Int(BamlInteger.Maximum + 1));
        Expect<BamlTypeMappingException>(() => _ = BamlValue.Float(double.PositiveInfinity));
        Expect<BamlTypeMappingException>(() => _ = BamlValue.List([null!]));
        Expect<BamlTypeMappingException>(() => _ = BamlValue.Map(
            [new("same", BamlValue.Int(1)), new("same", BamlValue.Int(2))]));

        BamlGeneratedRegistryBuilder dynamicBuilder =
            BamlGeneratedContract.CreateRegistryBuilder(BamlGeneratedContract.Version);
        var dynamicMetadata = new BamlTy
        {
            ClassTy = new BamlTyClass { Name = "test.DynamicRecord" },
        };
        var nullableDynamicMetadata = new BamlTy
        {
            Optional = new BamlTyOptional { Inner = dynamicMetadata.Clone() },
        };
        BamlGeneratedType<DynamicRecord?> nullableDynamicType =
            dynamicBuilder.DeclareType<DynamicRecord?>(
                "test.DynamicRecord?",
                nullableDynamicMetadata.ToByteArray());
        BamlGeneratedType<DynamicRecord> dynamicType =
            dynamicBuilder.DeclareType<DynamicRecord>(
                "test.DynamicRecord",
                dynamicMetadata.ToByteArray());
        dynamicBuilder.RegisterCodec(nullableDynamicType, new NullableDynamicRecordCodec());
        dynamicBuilder.RegisterCodec(dynamicType, new DynamicRecordCodec());
        BamlGeneratedRegistry dynamicRegistry = dynamicBuilder.Build();
        var dynamicRecord = new DynamicRecord("registered");
        BamlValue dynamicRecordValue = BamlValue.From(dynamicRecord);
        BamlGeneratedValue nullableDynamicGenerated = dynamicRegistry.Encode(
            nullableDynamicType,
            dynamicRecord);
        var nullableDynamicRecordValue = new BamlValue(
            nullableDynamicGenerated.WithDeclaredType(nullableDynamicMetadata.ToByteArray()));
        Require(
            dynamicRecordValue.Kind == BamlValueKind.Class
                && dynamicRecordValue.Type.Fqn == "test.DynamicRecord"
                && !dynamicRecordValue.Type.IsNullable
                && dynamicRecordValue.TryGet(out DynamicRecord? restoredRecord)
                && restoredRecord == dynamicRecord
                && nullableDynamicRecordValue.Type.IsNullable
                && nullableDynamicRecordValue.TryGet(
                    out DynamicRecord? restoredNullableRecord)
                && restoredNullableRecord == dynamicRecord,
            "nullable-reference generated codec aliases did not preserve exact descriptors");
        BamlGeneratedRegistryBuilder contradictoryBuilder =
            BamlGeneratedContract.CreateRegistryBuilder(BamlGeneratedContract.Version);
        var contradictoryMetadata = new BamlTy
        {
            ClassTy = new BamlTyClass { Name = "test.OtherDynamicRecord" },
        };
        BamlGeneratedType<DynamicRecord> contradictoryType =
            contradictoryBuilder.DeclareType<DynamicRecord>(
                "test.OtherDynamicRecord",
                contradictoryMetadata.ToByteArray());
        contradictoryBuilder.RegisterCodec(contradictoryType, new DynamicRecordCodec());
        InvalidOperationException contradictory = Expect<InvalidOperationException>(() =>
            _ = contradictoryBuilder.Build());
        Require(
            contradictory.Message.Contains(
                "contradictory context-free BAML descriptors",
                StringComparison.Ordinal)
            && contradictory.Message.Contains("test.DynamicRecord", StringComparison.Ordinal)
            && contradictory.Message.Contains("test.OtherDynamicRecord", StringComparison.Ordinal),
            "genuinely contradictory dynamic codec mapping did not fail closed");
        Expect<BamlTypeMismatchException>(() => _ = stringValue.As<DynamicRecord>());
        Require(
            !stringValue.TryGetEnumVariant(out string? wrongEnum)
                && wrongEnum is null
                && !stringValue.TryGetClassFields(out var wrongFields)
                && wrongFields is null
                && !stringValue.TryGetUnion(out int wrongCase, out BamlValue? wrongUnion)
                && wrongCase == 0
                && wrongUnion is null,
            "dynamic wrong-kind inspection changed");

        foreach (Type type in new[]
        {
            typeof(BamlImage),
            typeof(BamlAudio),
            typeof(BamlVideo),
            typeof(BamlPdf),
            typeof(BamlHttpRequest),
            typeof(BamlRetryPolicy),
            typeof(BamlClient),
            typeof(Baml.BamlHandle),
            typeof(BamlValue),
            typeof(BamlTypeDescriptor),
        })
        {
            Require(type.GetConstructors().Length == 0, $"{type.Name} exposed a public constructor");
            Require(type.IsSealed, $"{type.Name} must remain sealed");
        }
    }

    private static BamlOutboundValue NominalValue(string identity, string detail)
    {
        var value = new BamlValueClass { Name = identity };
        value.Fields.Add(new BamlOutboundMapEntry
        {
            Key = "detail",
            Value = new BamlOutboundValue { StringValue = detail },
        });
        return new BamlOutboundValue { ClassValue = value };
    }

    private static void VerifyCallbackIdentifiersAndCopy()
    {
        var allocator = new CallbackIdAllocator(uint.MaxValue - 1);
        Require(allocator.Next() == uint.MaxValue, "final callback identifier changed");
        Expect<BamlProtocolException>(() => _ = allocator.Next());
        Expect<BamlProtocolException>(() => _ = allocator.Next());

        var concurrent = new CallbackIdAllocator();
        uint[] identifiers = new uint[16_384];
        Parallel.For(0, identifiers.Length, index => identifiers[index] = concurrent.Next());
        Require(
            identifiers.All(identifier => identifier != 0)
                && identifiers.Distinct().Count() == identifiers.Length,
            "callback identifier allocation repeated or returned zero");
        Require(
            NativeApi.RequireFunctionCallIdentifier(ulong.MaxValue) == ulong.MaxValue,
            "native function-call identifier was narrowed");
        Expect<BamlProtocolException>(() => _ = NativeApi.RequireFunctionCallIdentifier(0));

        long lateBefore = NativeCallbacks.LateOrDuplicateResults;
        (uint id, Task<byte[]> task) = NativeCallbacks.AddPending();
        byte[] borrowed = [0, 1, 254, 255];
        fixed (byte* pointer = borrowed)
        {
            NativeCallbacks.ResultPointer(id, pointer, (nuint)borrowed.Length);
            borrowed[0] = 99;
            NativeCallbacks.ResultPointer(id, pointer, (nuint)borrowed.Length);
        }

        Require(task.GetAwaiter().GetResult().SequenceEqual(new byte[] { 0, 1, 254, 255 }), "callback bytes were not copied");
        Require(
            NativeCallbacks.LateOrDuplicateResults == lateBefore + 1,
            "duplicate callback was not contained");

        byte[] unknownResult = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue { StringValue = "unknown" },
        }.ToByteArray();
        fixed (byte* unknownPointer = unknownResult)
        {
            NativeCallbacks.ResultPointer(
                uint.MaxValue,
                unknownPointer,
                (nuint)unknownResult.Length);
        }
        Require(
            NativeCallbacks.LateOrDuplicateResults == lateBefore + 2,
            "unknown callback was not cleanup-only");

        VerifyCallbackTerminalRaces();

        (uint invalidId, Task<byte[]> invalidTask) = NativeCallbacks.AddPending();
        NativeCallbacks.ResultPointer(invalidId, null, 1);
        ExpectTaskFault<BamlProtocolException>(invalidTask);
        Expect<BamlProtocolException>(NativeCallbacks.ThrowIfCallbackFailed);
    }

    private static void VerifyCallbackTerminalRaces()
    {
        const int Iterations = 128;
        long lateBefore = NativeCallbacks.LateOrDuplicateResults;
        int cancellationWinners = 0;
        for (int iteration = 0; iteration < Iterations; iteration++)
        {
            using var source = new CancellationTokenSource();
            source.Cancel();
            (uint id, Task<byte[]> completion) = NativeCallbacks.AddPending();
            using var barrier = new Barrier(2);
            Task cancel = Task.Run(() =>
            {
                barrier.SignalAndWait();
                _ = NativeCallbacks.TryCancel(id, source.Token);
            });
            Task result = Task.Run(() =>
            {
                barrier.SignalAndWait();
                byte[] bytes = [checked((byte)iteration)];
                fixed (byte* pointer = bytes)
                {
                    NativeCallbacks.ResultPointer(id, pointer, 1);
                }
            });
            Task.WaitAll(cancel, result);
            try
            {
                byte[] bytes = completion.GetAwaiter().GetResult();
                Require(
                    bytes.Length == 1 && bytes[0] == checked((byte)iteration),
                    "terminal race changed the winning result bytes");
            }
            catch (OperationCanceledException error)
            {
                cancellationWinners++;
                Require(
                    error.CancellationToken == source.Token
                        && completion.Status == TaskStatus.Canceled,
                    "terminal race lost the winning cancellation token");
            }
        }

        Require(NativeCallbacks.PendingCount == 0, "terminal races leaked pending callbacks");
        Require(
            NativeCallbacks.LateOrDuplicateResults == lateBefore + cancellationWinners,
            "terminal races did not classify every late result exactly once");
    }

    private static void VerifyCallPipeline()
    {
        ResetFakeCalls();
        BamlApiV1 table = CreateValidTable();
        var api = new NativeApi(&table, "test");
        (BamlGeneratedRegistry registry, BamlGeneratedFunction<string> function, BamlGeneratedArgument<string, string> required, BamlGeneratedArgument<string, string?> optional) =
            CreateGeneratedRegistry();
        var program = new BamlGeneratedProgram(
            registry,
            new Baml.Runtime.ProgramNativeState(api, "test"));

        BamlGeneratedArgumentsBuilder<string> builder = registry.CreateArgumentsBuilder(function);
        builder.Add(required, "héllo\0雪");
        builder.Add(optional, null);
        BamlGeneratedArguments<string> arguments = builder.Build();
        fakeResult = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue { StringValue = "native-result" },
        }.ToByteArray();

        Require(program.Call(function, arguments) == "native-result", "sync call result changed");
        VerifyLastFakeCall("test.echo", "héllo\0雪", includesExplicitNull: true);
        Require(
            program.CallAsync(function, arguments).GetAwaiter().GetResult() == "native-result",
            "async call result changed");
        VerifyLastFakeCall("test.echo", "héllo\0雪", includesExplicitNull: true);
        Require(fakeCallCount == 2, "sync and async calls did not share native dispatch");

        using (var preCanceled = new CancellationTokenSource())
        {
            preCanceled.Cancel();
            Task<string> canceled = program.CallAsync(function, arguments, preCanceled.Token);
            BamlOperationCanceledException error = ExpectBamlCanceled(
                canceled,
                preCanceled.Token);
            Require(
                error.Origin == BamlCancellationOrigin.Caller
                    && error.BamlFunction == "test.echo"
                    && error.Trace is null,
                "pre-dispatch cancellation metadata changed");
            Require(fakeCallCount == 2, "pre-canceled call reached native dispatch");
        }

        fakeResult = null;
        using (var canceledDuringEncoding = new CancellationTokenSource())
        {
            Task<byte[]> canceled = api.InvokeFunctionAsync(
                "test.encode-cancel",
                callId =>
                {
                    canceledDuringEncoding.Cancel();
                    return new CallFunctionArgs { CallId = callId }.ToByteArray();
                },
                canceledDuringEncoding.Token);
            ExpectCanceled(canceled, canceledDuringEncoding.Token);
            Require(fakeCallCount == 2, "cancellation before dispatch still invoked native code");
            Require(fakeCancelCount == 0, "undispatched call invoked native cancellation");
        }

        using (var cancelAfterDispatch = new CancellationTokenSource())
        {
            Task<byte[]> canceled = api.InvokeFunctionAsync(
                "test.cancel",
                callId => new CallFunctionArgs { CallId = callId }.ToByteArray(),
                cancelAfterDispatch.Token);
            uint callbackId = lastFakeCallbackId;
            cancelAfterDispatch.Cancel();
            ExpectCanceled(canceled, cancelAfterDispatch.Token);
            Require(fakeCallCount == 3, "cancelable call was not dispatched");
            Require(fakeCancelCount == 1, "dispatched cancellation did not reach native code once");

            long lateBefore = NativeCallbacks.LateOrDuplicateResults;
            byte[] lateResult = new BamlOutboundResult
            {
                Ok = new BamlOutboundValue { StringValue = "late" },
            }.ToByteArray();
            fixed (byte* lateResultPointer = lateResult)
            {
                NativeCallbacks.ResultPointer(
                    callbackId,
                    lateResultPointer,
                    (nuint)lateResult.Length);
            }
            Require(
                NativeCallbacks.LateOrDuplicateResults == lateBefore + 1,
                "late canceled callback was not cleanup-only");
            NativeCallbacks.ThrowIfCallbackFailed();
        }

        fakeResult = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue { StringValue = "winner" },
        }.ToByteArray();
        using (var cancelAfterResult = new CancellationTokenSource())
        {
            Task<byte[]> completed = api.InvokeFunctionAsync(
                "test.result-wins",
                callId => new CallFunctionArgs { CallId = callId }.ToByteArray(),
                cancelAfterResult.Token);
            Require(completed.GetAwaiter().GetResult().SequenceEqual(fakeResult), "result bytes changed");
            cancelAfterResult.Cancel();
            Require(fakeCancelCount == 1, "cancellation ran after the result had won");
        }

        fakeResult = null;
        using (var callerWinsAfterDispatch = new CancellationTokenSource())
        {
            Task<string> canceled = program.CallAsync(
                function,
                arguments,
                callerWinsAfterDispatch.Token);
            callerWinsAfterDispatch.Cancel();
            BamlOperationCanceledException error = ExpectBamlCanceled(
                canceled,
                callerWinsAfterDispatch.Token);
            Require(
                error.Origin == BamlCancellationOrigin.Caller
                    && error.BamlFunction == "test.echo",
                "post-dispatch cancellation metadata changed");
            Require(fakeCancelCount == 2, "public caller cancellation did not cancel native once");
        }

        fakeResult = new BamlOutboundResult
        {
            Panic = new BamlOutboundPanic
            {
                Value = NominalValue("baml.panics.Cancelled", "engine stopped"),
            },
        }.ToByteArray();
        using (var uncanceledCaller = new CancellationTokenSource())
        {
            Task<string> canceled = program.CallAsync(
                function,
                arguments,
                uncanceledCaller.Token);
            BamlOperationCanceledException error = ExpectBamlCanceled(
                canceled,
                expectedToken: null);
            Require(
                error.Origin == BamlCancellationOrigin.Engine
                    && error.BamlFunction == "test.echo"
                    && error.CancellationToken != uncanceledCaller.Token,
                "engine cancellation was falsely attributed to the caller");
        }
    }

    private static void VerifyLastFakeCall(
        string expectedFunction,
        string expectedRequired,
        bool includesExplicitNull)
    {
        Require(lastFakeFunction == expectedFunction, "native function identity changed");
        CallFunctionArgs call = CallFunctionArgs.Parser.ParseFrom(
            lastFakeArguments ?? throw new InvalidOperationException("native arguments were not captured"));
        Require(call.CallId != 0, "native call identifier was omitted");
        Require(call.Kwargs.Count == 2, "generated argument cardinality changed");
        Require(
            call.Kwargs[0].StringKey == "value"
                && call.Kwargs[0].Value.StringValue == expectedRequired,
            "required generated argument bytes changed");
        Require(
            call.Kwargs[1].StringKey == "optional"
                && call.Kwargs[1].Value.ValueCase == InboundValue.ValueOneofCase.None
                && includesExplicitNull,
            "explicit null generated argument bytes changed");
    }

    private static void ResetFakeCalls()
    {
        fakeCallCount = 0;
        fakeCancelCount = 0;
        nextFakeCallId = 1000;
        lastFakeFunction = null;
        lastFakeArguments = null;
        fakeResult = null;
        lastFakeCallbackId = 0;
    }

    private static void VerifySafeHandleOwnership()
    {
        clonedHandles = 0;
        releasedHandles = 0;
        var original = new BamlSafeHandle(7, &CloneHandle, &ReleaseHandle);
        BamlSafeHandle clone = original.CloneOwned();
        Require(original.Key == 7 && clone.Key == 8, "handle clone identity changed");
        Require(clonedHandles == 1, "handle clone count changed");
        using (var lease = new BamlSafeHandleLease(original))
        {
            Require(lease.Key == 7, "safe-handle lease changed key");
        }

        original.Dispose();
        original.Dispose();
        clone.Dispose();
        Require(releasedHandles == 2, "owned handles were not released exactly once");
        Expect<ObjectDisposedException>(() => _ = original.CloneOwned());

        var fullWidth = new BamlSafeHandle(ulong.MaxValue, &CloneHandle, &ReleaseHandle);
        Require(!fullWidth.IsInvalid && fullWidth.Key == ulong.MaxValue, "full-width handle was narrowed");
        fullWidth.Dispose();
        Require(releasedHandles == 3, "full-width handle was not released");
    }

    private static void VerifyStreamProtocolOwnership()
    {
        BamlApiV1 table = CreateValidTable();
        var api = new NativeApi(&table, "test");
        BamlTy partialType = PrimitiveType(BamlTyPrimitiveKind.BamlTyPrimitiveString);
        BamlTy finalType = PrimitiveType(BamlTyPrimitiveKind.BamlTyPrimitiveString);
        byte[] partialMetadata = partialType.ToByteArray();
        byte[] finalMetadata = finalType.ToByteArray();

        clonedHandles = 0;
        releasedHandles = 0;
        using (var original = new BamlSafeHandle(40, &CloneHandle, &ReleaseHandle))
        {
            using EncodedCallArguments encoded =
                PrimitiveProtocol.EncodeStreamHandleArguments(original, 91);
            CallFunctionArgs call = CallFunctionArgs.Parser.ParseFrom(encoded.Bytes);
            Require(
                clonedHandles == 1
                    && releasedHandles == 0
                    && call.CallId == 91
                    && call.Kwargs.Count == 1
                    && call.Kwargs[0].StringKey == "self"
                    && call.Kwargs[0].Value.Handle.Key == 41
                    && call.Kwargs[0].Value.Handle.HandleType
                        == BamlHandleType.AdtTaggedHeapHandle,
                "stream self encoding did not clone one owned tagged handle");
        }
        Require(
            releasedHandles == 2,
            "rolled-back stream self clone and source were not released exactly once");

        var streamClass = new BamlTyClass { Name = "ai.stream.Stream" };
        streamClass.TypeArgs.Add(partialType);
        streamClass.TypeArgs.Add(finalType);
        var streamEnvelope = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue
            {
                HandleValue = new BamlOutboundHandle
                {
                    Key = 50,
                    HandleType = BamlHandleType.AdtTaggedHeapHandle,
                    Ty = new BamlTy { ClassTy = streamClass },
                },
            },
        };
        int releasedBeforeHandle = releasedHandles;
        using (BamlStreamNativeHandle stream = PrimitiveProtocol.DecodeStreamHandle(
            streamEnvelope.ToByteArray(),
            partialMetadata,
            finalMetadata,
            "test.echo$stream",
            api))
        {
            Require(
                stream.Handle.Key == 50
                    && stream.ClassIdentity == "ai.stream.Stream",
                "stream factory handle descriptor changed");
            Require(
                releasedHandles == releasedBeforeHandle,
                "claimed stream handle was released before driver ownership ended");
        }
        Require(
            releasedHandles == releasedBeforeHandle + 1,
            "claimed stream handle was not released exactly once");

        streamClass.Name = "test.NotStream";
        Expect<BamlProtocolException>(() =>
            _ = PrimitiveProtocol.DecodeStreamHandle(
                streamEnvelope.ToByteArray(),
                partialMetadata,
                finalMetadata,
                "test.echo$stream",
                api));
        streamClass.Name = "ai.stream.Stream";

        BamlOutboundResult partialEnvelope = StreamPullResult(
            partialType,
            "string",
            new BamlOutboundValue { StringValue = "partial" });
        BamlStreamPull<BamlGeneratedValue> partial = PrimitiveProtocol.DecodeStreamPull(
            partialEnvelope.ToByteArray(),
            partialMetadata,
            "string",
            "ai.stream.Stream.next",
            api);
        Require(
            partial.HasPartial && partial.Partial.ReadString() == "partial",
            "exact native stream partial did not decode");

        BamlOutboundResult finishedEnvelope = StreamPullResult(
            partialType,
            "ai.stream.Done",
            new BamlOutboundValue
            {
                ClassValue = new BamlValueClass { Name = "ai.stream.Done" },
            });
        Require(
            !PrimitiveProtocol.DecodeStreamPull(
                finishedEnvelope.ToByteArray(),
                partialMetadata,
                "string",
                "ai.stream.Stream.next",
                api).HasPartial,
            "exact native stream finished arm did not decode");

        Google.Protobuf.Collections.RepeatedField<BamlTy> options =
            finishedEnvelope.Ok.UnionVariantValue.SelfType.Union.Options;
        BamlTy firstOption = options[0];
        options[0] = options[1];
        options[1] = firstOption;
        Require(
            !PrimitiveProtocol.DecodeStreamPull(
                finishedEnvelope.ToByteArray(),
                partialMetadata,
                "string",
                "ai.stream.Stream.next",
                api).HasPartial,
            "canonical union ordering changed stream pull arm recognition");
        options[1] = options[0].Clone();
        Expect<BamlProtocolException>(() =>
            _ = PrimitiveProtocol.DecodeStreamPull(
                finishedEnvelope.ToByteArray(),
                partialMetadata,
                "string",
                "ai.stream.Stream.next",
                api));
    }

    private static BamlOutboundResult StreamPullResult(
        BamlTy partialType,
        string selectedOption,
        BamlOutboundValue selectedValue)
    {
        var union = new BamlTyUnion();
        union.Options.Add(partialType.Clone());
        union.Options.Add(new BamlTy
        {
            ClassTy = new BamlTyClass { Name = "ai.stream.Done" },
        });
        return new BamlOutboundResult
        {
            Ok = new BamlOutboundValue
            {
                UnionVariantValue = new BamlValueUnionVariant
                {
                    SelfType = new BamlTy { Union = union },
                    ValueOptionName = selectedOption,
                    Value = selectedValue,
                },
            },
        };
    }

    private static void VerifyDeferredStreamArgumentOwnership()
    {
        byte[] textMetadata =
            PrimitiveType(BamlTyPrimitiveKind.BamlTyPrimitiveString).ToByteArray();
        byte[] handleMetadata = new BamlTy
        {
            ClassTy = new BamlTyClass { Name = "test.Handle" },
        }.ToByteArray();
        BamlGeneratedRegistryBuilder builder =
            BamlGeneratedContract.CreateRegistryBuilder(BamlGeneratedContract.Version);
        BamlGeneratedType<string> text =
            builder.DeclareType<string>("string", textMetadata);
        BamlGeneratedType<Baml.BamlHandle> handleType =
            builder.DeclareType<Baml.BamlHandle>("test.Handle", handleMetadata);
        builder.RegisterCodec(text, new StringCodec());
        builder.RegisterCodec(handleType, new HandleCodec());
        BamlGeneratedFunction<string> function =
            builder.DeclareFunction("test.echo$stream", "stream", text);
        BamlGeneratedArgument<string, Baml.BamlHandle> handleArgument =
            builder.DeclareArgument(function, "resource", handleType);
        BamlGeneratedRegistry registry = builder.Build();

        clonedHandles = 0;
        releasedHandles = 0;
        using var original = new Baml.BamlHandle(
            new BamlSafeHandle(60, &CloneHandle, &ReleaseHandle),
            BamlTypeDescriptor.CreateHandle("test.Handle"),
            wireTypeMetadata: handleMetadata);
        BamlGeneratedArgumentsBuilder<string> arguments =
            registry.CreateArgumentsBuilder(function);
        arguments.Add(handleArgument, original);
        int programFactoryCalls = 0;
        var deferredProgram = new Lazy<BamlGeneratedProgram>(() =>
        {
            Interlocked.Increment(ref programFactoryCalls);
            throw new InvalidOperationException("the native program must remain cold");
        });

        BamlStream<string, string> stream = BamlGeneratedContract.CreateStream(
            deferredProgram,
            function,
            text,
            arguments.Build(),
            "string");
        Require(
            clonedHandles == 1
                && releasedHandles == 0
                && programFactoryCalls == 0
                && !original.IsClosed,
            "stream creation did not clone arguments before a cold native factory");
        stream.DisposeAsync().AsTask().GetAwaiter().GetResult();
        Require(
            clonedHandles == 1
                && releasedHandles == 1
                && programFactoryCalls == 0
                && !original.IsClosed,
            "disposing an unstarted stream did not release only its deferred argument clone");
        original.Dispose();
        Require(releasedHandles == 2, "original stream argument handle ownership changed");
    }

    private static void VerifyMediaAndHandleProtocol()
    {
        BamlApiV1 table = CreateValidTable();
        var api = new NativeApi(&table, "test");
        byte[] expectedBytes = [0, 1, 254, 255];
        BamlGeneratedValue inline = PrimitiveProtocol.Decode(new BamlOutboundValue
        {
            MediaValue = new BamlValueMedia
            {
                Media = MediaTypeEnum.Image,
                MimeType = "image/png",
                Base64 = Convert.ToBase64String(expectedBytes),
            },
        });
        BamlImage inlineImage = new BamlValue(inline).As<BamlImage>();
        Require(
            inlineImage.TryGetBytes(out ReadOnlyMemory<byte> inlineBytes, out string? inlineMime)
                && inlineBytes.Span.SequenceEqual(expectedBytes)
                && inlineMime == "image/png"
                && BamlValue.From(inlineImage).Kind == BamlValueKind.Media,
            "inline/dynamic media restoration changed");

        clonedHandles = 0;
        releasedHandles = 0;
        BamlImage input = BamlImage.FromBytes(expectedBytes, "image/png");
        ulong rolledBackKey;
        using (var ownership = new EncodedCallArguments([]))
        {
            InboundValue encoded = MediaProtocol.Encode(api, ownership, input);
            rolledBackKey = encoded.ClassValue.Fields[0].Value.Handle.Key;
            Require(
                encoded.ValueType?.TyCase == BamlTy.TyOneofCase.Media
                    && encoded.ValueType.Media.Kind == BamlTyMediaKind.Image
                    && encoded.ClassValue.Fields.Count == 1
                    && encoded.ClassValue.Fields[0].StringKey == "_data"
                    && encoded.ClassValue.Fields[0].Value.Handle.HandleType
                        == BamlHandleType.AdtMediaImage
                    && clonedHandles == 1
                    && releasedHandles == 1,
                "media encode did not create and clone one ephemeral owner");
        }
        Require(
            rolledBackKey != 0 && releasedHandles == 2,
            "unpublished media transfer did not roll back exactly once");

        ulong committedKey;
        using (var ownership = new EncodedCallArguments([]))
        {
            InboundValue encoded = MediaProtocol.Encode(api, ownership, input);
            committedKey = encoded.ClassValue.Fields[0].Value.Handle.Key;
            ownership.Commit();
        }
        Require(releasedHandles == 3, "committed media transfer was released by managed code");
        _ = table.HandleRelease(committedKey);
        Require(releasedHandles == 4, "native-owned media transfer was not independently releasable");

        fakeMediaUrl = "https://example.com/media.png?secret=value";
        fakeMediaBase64 = string.Empty;
        fakeMediaFile = string.Empty;
        fakeMediaMimeType = "image/png";
        BamlOutboundResult mediaEnvelope = MediaHandleResult(
            "baml.media.Image",
            key: 700,
            BamlHandleType.AdtMediaImage);
        int releasedBeforeDecode = releasedHandles;
        BamlImage restored = new BamlValue(
            PrimitiveProtocol.DecodeCallResult(mediaEnvelope.ToByteArray(), "media", api))
            .As<BamlImage>();
        Require(
            restored.TryGetUrl(out string? restoredUrl)
                && restoredUrl == fakeMediaUrl
                && !restored.ToString().Contains("secret=value", StringComparison.Ordinal)
                && releasedHandles == releasedBeforeDecode + 1,
            "owned media handle was not restored/redacted/released exactly once");

        int releasedBeforeFailure = releasedHandles;
        Expect<BamlProtocolException>(() =>
            _ = PrimitiveProtocol.DecodeCallResult(
                MediaHandleResult(
                    "baml.media.Audio",
                    key: 701,
                    BamlHandleType.AdtMediaImage).ToByteArray(),
                "media",
                api));
        Require(
            releasedHandles == releasedBeforeFailure + 1,
            "media decode failure did not release its outbound handle");

        var resourceWire = new BamlOutboundHandle
        {
            Key = 800,
            HandleType = BamlHandleType.AdtTaggedHeapHandle,
            Ty = new BamlTy
            {
                ClassTy = new BamlTyClass { Name = "baml.http.Response" },
            },
        };
        var resourceEnvelope = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue { HandleValue = resourceWire },
        };
        BamlValue dynamicHandle = new(
            PrimitiveProtocol.DecodeCallResult(resourceEnvelope.ToByteArray(), "handle", api));
        using Baml.BamlHandle handle = dynamicHandle.As<Baml.BamlHandle>();
        Require(
            dynamicHandle.Kind == BamlValueKind.Handle
                && dynamicHandle.Type.Kind == BamlTypeDescriptorKind.Handle
                && dynamicHandle.Type.Fqn == "baml.http.Response"
                && BamlValue.From(handle).Equals(dynamicHandle),
            "dynamic handle identity or descriptor changed");
        using Baml.BamlHandle clone = handle.Clone();
        Require(
            !BamlValue.From(clone).Equals(dynamicHandle),
            "cloned dynamic handles did not retain wrapper identity semantics");

        BamlGeneratedRegistryBuilder handleRegistryBuilder =
            BamlGeneratedContract.CreateRegistryBuilder(BamlGeneratedContract.Version);
        var handleContext = new BamlGeneratedCodecContext(handleRegistryBuilder.Build());
        byte[] responseMetadata = resourceWire.Ty.ToByteArray();
        BamlGeneratedValue exactHandle = handleContext.Handle(
            handle,
            "baml.http.Response",
            responseMetadata);
        Require(
            ReferenceEquals(
                handleContext.ReadHandle(
                    exactHandle,
                    "baml.http.Response",
                    responseMetadata),
                handle),
            "exact opaque-resource codec changed wrapper ownership");
        byte[] globMetadata = new BamlTy
        {
            ClassTy = new BamlTyClass { Name = "baml.glob.Glob" },
        }.ToByteArray();
        Expect<BamlTypeMappingException>(() =>
            _ = handleContext.Handle(handle, "baml.glob.Glob", globMetadata));
        Expect<BamlProtocolException>(() =>
            _ = handleContext.ReadHandle(exactHandle, "baml.glob.Glob", globMetadata));

        var rowsClass = new BamlTyClass { Name = "baml.csv.Rows" };
        rowsClass.TypeArgs.Add(PrimitiveType(BamlTyPrimitiveKind.BamlTyPrimitiveString));
        var rowsEnvelope = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue
            {
                HandleValue = new BamlOutboundHandle
                {
                    Key = 801,
                    HandleType = BamlHandleType.AdtTaggedHeapHandle,
                    Ty = new BamlTy { ClassTy = rowsClass },
                },
            },
        };
        using Baml.BamlHandle rows = new BamlValue(
            PrimitiveProtocol.DecodeCallResult(rowsEnvelope.ToByteArray(), "rows", api))
            .As<Baml.BamlHandle>();
        Require(
            rows.Type.Fqn == "baml.csv.Rows"
                && rows.Type.Arguments.Count == 1
                && rows.Type.Arguments[0].Kind == BamlTypeDescriptorKind.String,
            "generic opaque-resource descriptor arguments were dropped");
    }

    private static BamlOutboundResult MediaHandleResult(
        string classIdentity,
        ulong key,
        BamlHandleType handleType)
    {
        var mediaClass = new BamlValueClass { Name = classIdentity };
        mediaClass.Fields.Add(new BamlOutboundMapEntry
        {
            Key = "_data",
            Value = new BamlOutboundValue
            {
                HandleValue = new BamlOutboundHandle
                {
                    Key = key,
                    HandleType = handleType,
                },
            },
        });
        return new BamlOutboundResult
        {
            Ok = new BamlOutboundValue { ClassValue = mediaClass },
        };
    }

    private static BamlApiV1 CreateValidTable() => new()
    {
        AbiVersion = 2,
        StructSize = (nuint)sizeof(BamlApiV1),
        Version = &Version,
        InitializeRuntimeFromBytecode = &Initialize,
        FreeBuffer = &FreeBuffer,
        RegisterCallback = &RegisterResult,
        CallFunction = &Call,
        NewFunctionCall = &NewCall,
        CancelFunctionCall = &Cancel,
        RegisterHostDispatchCallback = &RegisterHostDispatch,
        RegisterHostReleaseCallback = &RegisterHostRelease,
        CompleteHostCall = &CompleteHostCall,
        HandleClone = &CloneHandle,
        HandleRelease = &ReleaseHandle,
        MediaFromUrl = &MediaFromUrl,
        MediaFromFile = &MediaFromFile,
        MediaFromBase64 = &MediaFromBase64,
        MediaUrl = &MediaUrl,
        MediaFile = &MediaFile,
        MediaBase64 = &MediaBase64,
        MediaMimeType = &MediaMimeType,
        RegisterBridge = &RegisterBridge,
        RegisterUnhandledSpawnErrorCallback = &RegisterUnhandledSpawnError,
        ShutdownRuntime = &Shutdown,
        InitializeRuntimeFromBytecodeWithMetadata = &InitializeWithMetadata,
    };

    private static void ExpectInvalidTable(BamlApiV1 table, bool passNull = false)
    {
        try
        {
            NativeApi.ValidateTable(passNull ? null : &table);
        }
        catch (BamlNativeLibraryLoadException)
        {
            return;
        }

        throw new InvalidOperationException("expected invalid native API table rejection");
    }

    private static void ClearRequiredFunction(ref BamlApiV1 table, int field)
    {
        switch (field)
        {
            case 0: table.Version = null; break;
            case 1: table.InitializeRuntimeFromBytecode = null; break;
            case 2: table.FreeBuffer = null; break;
            case 3: table.RegisterCallback = null; break;
            case 4: table.CallFunction = null; break;
            case 5: table.NewFunctionCall = null; break;
            case 6: table.CancelFunctionCall = null; break;
            case 7: table.RegisterHostDispatchCallback = null; break;
            case 8: table.RegisterHostReleaseCallback = null; break;
            case 9: table.CompleteHostCall = null; break;
            case 10: table.HandleClone = null; break;
            case 11: table.HandleRelease = null; break;
            case 12: table.MediaFromUrl = null; break;
            case 13: table.MediaFromFile = null; break;
            case 14: table.MediaFromBase64 = null; break;
            case 15: table.MediaUrl = null; break;
            case 16: table.MediaFile = null; break;
            case 17: table.MediaBase64 = null; break;
            case 18: table.MediaMimeType = null; break;
            case 19: table.RegisterBridge = null; break;
            case 20: table.RegisterUnhandledSpawnErrorCallback = null; break;
            case 21: table.ShutdownRuntime = null; break;
            case 22: table.InitializeRuntimeFromBytecodeWithMetadata = null; break;
            default: throw new ArgumentOutOfRangeException(nameof(field));
        }
    }

    private static void ExpectInvalidBuffer(BamlApiV1* table, BamlBuffer buffer, bool decodeUtf8)
    {
        try
        {
            if (decodeUtf8)
            {
                _ = NativeBuffer.ReadUtf8AndFree(table, buffer);
            }
            else
            {
                _ = NativeBuffer.CopyAndFree(table, buffer);
            }
        }
        catch (BamlProtocolException)
        {
            return;
        }

        throw new InvalidOperationException("expected invalid native buffer rejection");
    }

    private static BamlBuffer Allocate(ReadOnlySpan<byte> bytes)
    {
        byte* pointer = (byte*)NativeMemory.Alloc((nuint)bytes.Length);
        bytes.CopyTo(new Span<byte>(pointer, bytes.Length));
        return new BamlBuffer { Pointer = pointer, Length = (nuint)bytes.Length };
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static BamlBuffer Version() => Allocate("0.15.0"u8);

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static BamlBuffer Initialize(byte* bytes, nuint length) => default;

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static BamlBuffer InitializeWithMetadata(byte* bytes, nuint length, byte* embeddedBamlToml) => default;

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void FreeBuffer(BamlBuffer buffer)
    {
        Interlocked.Increment(ref releasedBuffers);
        if (buffer.Pointer is not null)
        {
            NativeMemory.Free(buffer.Pointer);
        }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void RegisterResult(delegate* unmanaged[Cdecl]<uint, byte*, nuint, void> callback)
    {
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void Call(byte* args, nuint length, uint callbackId)
    {
        if (length > int.MaxValue || (length != 0 && args is null))
        {
            throw new InvalidDataException("fake call received an invalid argument buffer");
        }

        lastFakeArguments = length == 0
            ? []
            : new ReadOnlySpan<byte>(args, checked((int)length)).ToArray();
        CallFunctionArgs call = CallFunctionArgs.Parser.ParseFrom(lastFakeArguments);
        lastFakeFunction = call.CallTargetCase == CallFunctionArgs.CallTargetOneofCase.FunctionName
            ? call.FunctionName
            : null;
        lastFakeCallbackId = callbackId;
        Interlocked.Increment(ref fakeCallCount);
        byte[]? result = fakeResult;
        if (result is not null)
        {
            fixed (byte* resultPointer = result)
            {
                NativeCallbacks.ResultPointer(callbackId, resultPointer, (nuint)result.Length);
            }
        }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static ulong NewCall() => checked((ulong)Interlocked.Increment(ref nextFakeCallId));

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static int Cancel(ulong id)
    {
        Interlocked.Increment(ref fakeCancelCount);
        return id == 0 ? 1 : 0;
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void RegisterHostDispatch(delegate* unmanaged[Cdecl]<ulong, uint, byte*, nuint, void> callback)
    {
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void RegisterHostRelease(delegate* unmanaged[Cdecl]<ulong, void> callback)
    {
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void CompleteHostCall(uint callId, int isError, byte* content, nuint length)
    {
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static BamlCffiStatus CloneHandle(ulong key, ulong* output)
    {
        Interlocked.Increment(ref clonedHandles);
        *output = key + 1;
        return BamlCffiStatus.Ok;
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static BamlCffiStatus ReleaseHandle(ulong key)
    {
        Interlocked.Increment(ref releasedHandles);
        return key == 0 ? BamlCffiStatus.InvalidHandle : BamlCffiStatus.Ok;
    }

    private static BamlCffiStatus CreateFakeMedia(
        int kind,
        byte* value,
        byte* mime,
        ulong* key,
        int* handleType,
        bool isUrl)
    {
        if (value is null || key is null || handleType is null)
        {
            return BamlCffiStatus.UnexpectedNullPointer;
        }

        *handleType = kind switch
        {
            1 => (int)BamlHandleType.AdtMediaImage,
            2 => (int)BamlHandleType.AdtMediaAudio,
            3 => (int)BamlHandleType.AdtMediaPdf,
            4 => (int)BamlHandleType.AdtMediaVideo,
            _ => 0,
        };
        if (*handleType == 0)
        {
            return BamlCffiStatus.UnsupportedHandleType;
        }

        string representation = Marshal.PtrToStringUTF8((nint)value)!;
        fakeMediaUrl = isUrl ? representation : string.Empty;
        fakeMediaBase64 = isUrl ? string.Empty : representation;
        fakeMediaFile = string.Empty;
        fakeMediaMimeType = mime is null ? string.Empty : Marshal.PtrToStringUTF8((nint)mime)!;
        *key = unchecked((ulong)Interlocked.Increment(ref nextFakeHandleKey));
        return BamlCffiStatus.Ok;
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static BamlCffiStatus MediaFromUrl(
        int kind,
        byte* value,
        byte* mime,
        ulong* key,
        int* handleType) =>
        CreateFakeMedia(kind, value, mime, key, handleType, isUrl: true);

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static BamlCffiStatus MediaFromFile(
        int kind,
        byte* value,
        byte* mime,
        ulong* key,
        int* handleType) =>
        BamlCffiStatus.UnsupportedHandleType;

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static BamlCffiStatus MediaFromBase64(
        int kind,
        byte* value,
        byte* mime,
        ulong* key,
        int* handleType) =>
        CreateFakeMedia(kind, value, mime, key, handleType, isUrl: false);

    private static BamlCffiStatus FakeMediaField(ulong key, BamlBuffer* output, string value)
    {
        if (key == 0 || output is null)
        {
            return BamlCffiStatus.InvalidHandle;
        }

        *output = Allocate(Encoding.UTF8.GetBytes(value));
        return BamlCffiStatus.Ok;
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static BamlCffiStatus MediaUrl(ulong key, int handleType, BamlBuffer* output) =>
        FakeMediaField(key, output, fakeMediaUrl);

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static BamlCffiStatus MediaFile(ulong key, int handleType, BamlBuffer* output) =>
        FakeMediaField(key, output, fakeMediaFile);

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static BamlCffiStatus MediaBase64(ulong key, int handleType, BamlBuffer* output) =>
        FakeMediaField(key, output, fakeMediaBase64);

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static BamlCffiStatus MediaMimeType(ulong key, int handleType, BamlBuffer* output) =>
        FakeMediaField(key, output, fakeMediaMimeType);

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static BamlBuffer RegisterBridge(BamlBridgeInfoV1* info)
    {
        if (info is null
            || info->StructSize != (nuint)sizeof(BamlBridgeInfoV1)
            || info->Language != 5
            || info->SdkVersionLength > int.MaxValue
            || (info->SdkVersionLength != 0 && info->SdkVersion is null))
        {
            return Allocate("invalid bridge registration"u8);
        }

        string version = Encoding.UTF8.GetString(
            new ReadOnlySpan<byte>(
                info->SdkVersion,
                checked((int)info->SdkVersionLength)));
        if (version != RuntimeIdentity.PackageVersion)
        {
            return Allocate("invalid bridge version"u8);
        }

        Interlocked.Increment(ref bridgeRegistrations);
        return default;
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void RegisterUnhandledSpawnError(
        delegate* unmanaged[Cdecl]<sbyte*, nuint, int, void> callback)
    {
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static BamlBuffer Shutdown() => default;

    private static TException Expect<TException>(Action action)
        where TException : Exception
    {
        try
        {
            action();
        }
        catch (TException error)
        {
            return error;
        }

        throw new InvalidOperationException($"expected {typeof(TException).Name}");
    }

    private static void ExpectTaskFault<TException>(Task task)
        where TException : Exception
    {
        try
        {
            task.GetAwaiter().GetResult();
        }
        catch (TException)
        {
            return;
        }

        throw new InvalidOperationException($"expected faulted task with {typeof(TException).Name}");
    }

    private static void ExpectCanceled(Task task, CancellationToken expectedToken)
    {
        try
        {
            task.GetAwaiter().GetResult();
        }
        catch (OperationCanceledException error)
        {
            Require(error.CancellationToken == expectedToken, "canceled task lost its caller token");
            Require(task.Status == TaskStatus.Canceled, "canceled task did not have Canceled status");
            return;
        }

        throw new InvalidOperationException("expected canceled task");
    }

    private static BamlOperationCanceledException ExpectBamlCanceled(
        Task task,
        CancellationToken? expectedToken)
    {
        try
        {
            task.GetAwaiter().GetResult();
        }
        catch (BamlOperationCanceledException error)
        {
            if (expectedToken.HasValue)
            {
                Require(
                    error.CancellationToken == expectedToken.Value,
                    "BAML cancellation lost its winning token");
            }

            Require(task.Status == TaskStatus.Canceled, "BAML cancellation task was not Canceled");
            return error;
        }

        throw new InvalidOperationException("expected BamlOperationCanceledException");
    }

    private static void Require(bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidOperationException(message);
        }
    }

    private sealed class StringCodec : IBamlGeneratedCodec<string>
    {
        public BamlGeneratedValue Encode(BamlGeneratedCodecContext context, string value) =>
            context.String(value);

        public string Decode(BamlGeneratedCodecContext context, BamlGeneratedValue value) =>
            context.ReadString(value);
    }

    private sealed class NullableStringCodec : IBamlGeneratedCodec<string?>
    {
        public BamlGeneratedValue Encode(BamlGeneratedCodecContext context, string? value) =>
            value is null ? context.Null() : context.String(value);

        public string? Decode(BamlGeneratedCodecContext context, BamlGeneratedValue value) =>
            value.IsNull ? null : context.ReadString(value);
    }

    private sealed class HandleCodec : IBamlGeneratedCodec<Baml.BamlHandle>
    {
        public BamlGeneratedValue Encode(
            BamlGeneratedCodecContext context,
            Baml.BamlHandle value) => context.Handle(value);

        public Baml.BamlHandle Decode(
            BamlGeneratedCodecContext context,
            BamlGeneratedValue value) => context.ReadHandle(value);
    }

    private sealed record DynamicRecord(string Value);

    private sealed class DynamicRecordCodec : IBamlGeneratedCodec<DynamicRecord>
    {
        public BamlGeneratedValue Encode(
            BamlGeneratedCodecContext context,
            DynamicRecord value) =>
            context.Class(
                "test.DynamicRecord",
                [new KeyValuePair<string, BamlGeneratedValue>("value", context.String(value.Value))]);

        public DynamicRecord Decode(
            BamlGeneratedCodecContext context,
            BamlGeneratedValue value)
        {
            IReadOnlyDictionary<string, BamlGeneratedValue> fields =
                context.ReadClass(value, "test.DynamicRecord");
            return new DynamicRecord(context.ReadString(fields["value"]));
        }
    }

    private sealed class NullableDynamicRecordCodec : IBamlGeneratedCodec<DynamicRecord?>
    {
        public BamlGeneratedValue Encode(
            BamlGeneratedCodecContext context,
            DynamicRecord? value) =>
            value is null
                ? context.Null()
                : context.Class(
                    "test.DynamicRecord",
                    [
                        new KeyValuePair<string, BamlGeneratedValue>(
                            "value",
                            context.String(value.Value)),
                    ]);

        public DynamicRecord? Decode(
            BamlGeneratedCodecContext context,
            BamlGeneratedValue value)
        {
            if (value.IsNull)
            {
                return null;
            }

            IReadOnlyDictionary<string, BamlGeneratedValue> fields =
                context.ReadClass(value, "test.DynamicRecord");
            return new DynamicRecord(context.ReadString(fields["value"]));
        }
    }
}
