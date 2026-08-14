using BamlBridge.Cffi.V1;

internal static class Program
{
    private const long BamlIntMin = -4_611_686_018_427_387_904L;
    private const long BamlIntMax = 4_611_686_018_427_387_903L;
    private const string ExactCrLfEnvelopeHex =
        "6a2622143a120a0642040a026c660a0842060a0463726c662a062263726c662232061a0463726c66";

    public static async Task<int> Main(string[] args)
    {
        if (args.Length > 1)
        {
            Console.Error.WriteLine(
                "usage: Baml.Bridge.ProtocolProbe [rust-produced-host-call-vectors]");
            return 2;
        }

        byte[] bytes = Convert.FromHexString(ExactCrLfEnvelopeHex);
        BamlOutboundValue good = BamlOutboundValue.Parser.ParseFrom(bytes);
        string value = DecodeLineEnding(good, good.UnionVariantValue.SelfType);
        Require(
            StringComparer.Ordinal.Equals(value, "crlf"),
            $"expected crlf, received {value}");

        BamlOutboundValue contradictory = good.Clone();
        contradictory.UnionVariantValue.ValueOptionName = "\"lf\"";
        try
        {
            _ = DecodeLineEnding(
                contradictory,
                good.UnionVariantValue.SelfType);
            throw new InvalidOperationException(
                "Contradictory selected-arm metadata was accepted.");
        }
        catch (InvalidDataException error)
        {
            Require(
                error.Message.Contains("contradicts", StringComparison.Ordinal),
                $"unexpected contradiction diagnostic: {error.Message}");
        }

        Console.WriteLine("exact_literal_union_decode=ok");
        Console.WriteLine($"wire_bytes={bytes.Length}");
        Console.WriteLine("contradictory_metadata=rejected");
        await VerifyOptionalCallbackSlots();
        Console.WriteLine("optional_callback_slots=5/5");
        VerifyMalformedOptionalCallbackSlots();
        Console.WriteLine("malformed_optional_callback_slots=6/6");
        await VerifyHostCallbackIntegerBoundaries();
        Console.WriteLine("callback_baml_int_valid=4/4");
        Console.WriteLine("callback_baml_int_rejected=12/12");
        Console.WriteLine("callback_baml_int_fail_closed=ok");
        if (args.Length == 1)
        {
            await VerifyRustProducedOptionalCallbackSlots(args[0]);
            Console.WriteLine("rust_produced_optional_callback_slots=5/5");
        }
        VerifyBamlIntegerDomain();
        Console.WriteLine("baml_int_vectors=encode_decode_checked");
        return 0;
    }

    private static void VerifyBamlIntegerDomain()
    {
        long[] valid =
        [
            BamlIntMin,
            BamlIntMin + 1,
            -1,
            0,
            1,
            BamlIntMax - 1,
            BamlIntMax,
        ];
        foreach (long value in valid)
        {
            InboundValue inbound = EncodeBamlInt(value, "$.argument");
            Require(inbound.IntValue == value, $"inbound int changed {value}");
            long decoded = DecodeBamlInt(
                new BamlOutboundValue { IntValue = value },
                "$.result");
            Require(decoded == value, $"outbound int changed {value}");
        }

        long[] invalid =
        [
            long.MinValue,
            BamlIntMin - 1,
            BamlIntMax + 1,
            long.MaxValue,
        ];
        foreach (long value in invalid)
        {
            Expect<ArgumentOutOfRangeException>(
                () => _ = EncodeBamlInt(value, "$.argument"));
            Expect<InvalidDataException>(
                () => _ = DecodeBamlInt(
                    new BamlOutboundValue { IntValue = value },
                    "$.result"));
        }

        Require(
            DecodeBamlIntLiteral(
                new BamlOutboundValue { IntValue = BamlIntMax },
                BamlIntMax,
                "$.literal")
                == BamlIntMax,
            "max literal must decode");
        Expect<InvalidDataException>(
            () => _ = DecodeBamlIntLiteral(
                new BamlOutboundValue { IntValue = 1 },
                2,
                "$.literal"));

        InboundListValue encodedList =
            EncodeBamlIntList([BamlIntMin, 0, BamlIntMax], "$.items");
        Require(encodedList.Values.Count == 3, "encoded int list length changed");
        long[] decodedList = DecodeBamlIntList(
            new BamlOutboundValue
            {
                ListValue = new BamlValueList
                {
                    Items =
                    {
                        new BamlOutboundValue { IntValue = BamlIntMin },
                        new BamlOutboundValue { IntValue = 0 },
                        new BamlOutboundValue { IntValue = BamlIntMax },
                    },
                },
            },
            "$.items");
        Require(
            decodedList.AsSpan().SequenceEqual(
                [BamlIntMin, 0, BamlIntMax]),
            "decoded int list changed values");

        Expect<ArgumentOutOfRangeException>(
            () => _ = EncodeBamlIntList([0, BamlIntMax + 1], "$.items"));
        Expect<InvalidDataException>(
            () => _ = DecodeBamlIntList(
                new BamlOutboundValue
                {
                    ListValue = new BamlValueList
                    {
                        Items =
                        {
                            new BamlOutboundValue { IntValue = 0 },
                            new BamlOutboundValue { IntValue = BamlIntMin - 1 },
                        },
                    },
                },
                "$.items"));
    }

    private static InboundValue EncodeBamlInt(long value, string path)
    {
        if (value is < BamlIntMin or > BamlIntMax)
        {
            throw new ArgumentOutOfRangeException(
                nameof(value),
                value,
                $"{path} is outside the BAML int domain [{BamlIntMin}, {BamlIntMax}].");
        }

        return new InboundValue { IntValue = value };
    }

    private static long DecodeBamlInt(BamlOutboundValue value, string path)
    {
        ArgumentNullException.ThrowIfNull(value);
        if (value.ValueCase != BamlOutboundValue.ValueOneofCase.IntValue)
        {
            throw new InvalidDataException(
                $"{path} expected a BAML int, received {value.ValueCase}.");
        }

        long decoded = value.IntValue;
        if (decoded is < BamlIntMin or > BamlIntMax)
        {
            throw new InvalidDataException(
                $"{path} contains out-of-domain BAML int carrier value {decoded}.");
        }

        return decoded;
    }

    private static long DecodeBamlIntLiteral(
        BamlOutboundValue value,
        long expected,
        string path)
    {
        long decoded = DecodeBamlInt(value, path);
        if (decoded != expected)
        {
            throw new InvalidDataException(
                $"{path} expected integer literal {expected}, received {decoded}.");
        }

        return decoded;
    }

    private static InboundListValue EncodeBamlIntList(
        IReadOnlyList<long> values,
        string path)
    {
        var encoded = new InboundListValue();
        for (int index = 0; index < values.Count; index++)
        {
            encoded.Values.Add(EncodeBamlInt(values[index], $"{path}[{index}]"));
        }

        return encoded;
    }

    private static long[] DecodeBamlIntList(
        BamlOutboundValue value,
        string path)
    {
        ArgumentNullException.ThrowIfNull(value);
        if (value.ValueCase != BamlOutboundValue.ValueOneofCase.ListValue)
        {
            throw new InvalidDataException(
                $"{path} expected a BAML list, received {value.ValueCase}.");
        }

        var decoded = new long[value.ListValue.Items.Count];
        for (int index = 0; index < decoded.Length; index++)
        {
            decoded[index] = DecodeBamlInt(
                value.ListValue.Items[index],
                $"{path}[{index}]");
        }

        return decoded;
    }

    private static async Task VerifyOptionalCallbackSlots()
    {
        var cases = new[]
        {
            (
                Call(RequiredInt(7)),
                "7:unset:unset"),
            (
                Call(RequiredInt(7), OptionalString("first", "alpha")),
                "7:alpha:unset"),
            (
                Call(RequiredInt(7), OptionalInt("later", 9)),
                "7:unset:9"),
            (
                Call(RequiredInt(7), OptionalNull("first")),
                "7:null:unset"),
            (
                Call(
                    RequiredInt(7),
                    OptionalString("first", "alpha"),
                    OptionalInt("later", 9)),
                "7:alpha:9"),
        };

        foreach ((BamlToHostCall call, string expected) in cases)
        {
            string actual = await DispatchOptionalCallback(
                call,
                OptionalCallback);
            Require(
                StringComparer.Ordinal.Equals(actual, expected),
                $"optional callback binding mismatch: expected {expected}, received {actual}");
        }
    }

    private static async Task VerifyRustProducedOptionalCallbackSlots(
        string vectorPath)
    {
        if (!Path.IsPathFullyQualified(vectorPath)
            || !File.Exists(vectorPath))
        {
            throw new FileNotFoundException(
                "Rust-produced host-call vector file must exist at an absolute path.",
                vectorPath);
        }

        string[] lines = await File.ReadAllLinesAsync(vectorPath);
        Require(lines.Length == 5, $"expected 5 Rust vectors, received {lines.Length}");
        foreach (string line in lines)
        {
            string[] fields = line.Split('\t', count: 2);
            Require(fields.Length == 2, "Rust vector line omitted expected value or payload.");
            BamlToHostCall call = BamlToHostCall.Parser.ParseFrom(
                Convert.FromBase64String(fields[1]));
            string actual = await DispatchOptionalCallback(
                call,
                OptionalCallback);
            Require(
                StringComparer.Ordinal.Equals(actual, fields[0]),
                $"Rust-produced optional callback mismatch: expected {fields[0]}, received {actual}");
        }
    }

    private static void VerifyMalformedOptionalCallbackSlots()
    {
        BamlToHostCall[] malformed =
        [
            Call(),
            Call(OptionalInt("required", 7)),
            Call(RequiredInt(7), OptionalString("unknown", "value")),
            Call(
                RequiredInt(7),
                OptionalString("first", "a"),
                OptionalString("first", "b")),
            Call(RequiredInt(7), OptionalInt("first", 1)),
            Call(RequiredInt(7), OptionalNull("later")),
        ];
        foreach (BamlToHostCall call in malformed)
        {
            Expect<InvalidDataException>(
                () => _ = DispatchOptionalCallback(
                    call,
                    OptionalCallback));
        }
    }

    private static async Task VerifyHostCallbackIntegerBoundaries()
    {
        var valid = new[]
        {
            (
                Call(RequiredInt(BamlIntMin)),
                $"{BamlIntMin}:unset:unset"),
            (
                Call(RequiredInt(BamlIntMax)),
                $"{BamlIntMax}:unset:unset"),
            (
                Call(RequiredInt(0), OptionalInt("later", BamlIntMin)),
                $"0:unset:{BamlIntMin}"),
            (
                Call(RequiredInt(0), OptionalInt("later", BamlIntMax)),
                $"0:unset:{BamlIntMax}"),
        };
        int validCount = 0;
        foreach ((BamlToHostCall call, string expected) in valid)
        {
            string actual = await DispatchOptionalCallback(
                call,
                OptionalCallback);
            Require(
                StringComparer.Ordinal.Equals(actual, expected),
                $"callback BAML int boundary mismatch: expected {expected}, received {actual}");
            validCount++;
        }

        long[] invalid =
        [
            BamlIntMin - 1,
            BamlIntMax + 1,
            long.MinValue,
            long.MaxValue,
        ];
        int callbackInvocations = 0;
        int rejectedCount = 0;
        Task<string> CountingCallback(
            long required,
            BamlOptional<string?> first,
            BamlOptional<long> later,
            CancellationToken cancellationToken)
        {
            callbackInvocations++;
            return OptionalCallback(required, first, later, cancellationToken);
        }

        foreach (long value in invalid)
        {
            ExpectInvalidData(
                () => _ = DispatchOptionalCallback(
                    Call(RequiredInt(value)),
                    CountingCallback),
                "$.callback.required");
            rejectedCount++;
            ExpectInvalidData(
                () => _ = DispatchOptionalCallback(
                    Call(RequiredInt(0), OptionalInt("later", value)),
                    CountingCallback),
                "$.callback.later");
            rejectedCount++;
        }

        ExpectInvalidData(
            () => _ = DispatchOptionalCallback(
                Call(new BamlToHostArg()),
                CountingCallback),
            "$.callback.required");
        rejectedCount++;
        ExpectInvalidData(
            () => _ = DispatchOptionalCallback(
                Call(RequiredString("not-an-int")),
                CountingCallback),
            "$.callback.required");
        rejectedCount++;
        ExpectInvalidData(
            () => _ = DispatchOptionalCallback(
                Call(RequiredInt(0), OptionalNull("later")),
                CountingCallback),
            "$.callback.later");
        rejectedCount++;
        ExpectInvalidData(
            () => _ = DispatchOptionalCallback(
                Call(RequiredInt(0), OptionalString("later", "not-an-int")),
                CountingCallback),
            "$.callback.later");
        rejectedCount++;
        Require(validCount == 4, $"expected 4 valid callback integers, ran {validCount}");
        Require(rejectedCount == 12, $"expected 12 rejected callback integers, ran {rejectedCount}");
        Require(
            callbackInvocations == 0,
            $"invalid callback integers invoked the host callback {callbackInvocations} time(s)");
    }

    private static Task<string> OptionalCallback(
        long required,
        BamlOptional<string?> first,
        BamlOptional<long> later,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        string firstState = !first.IsSet
            ? "unset"
            : first.Value is null
                ? "null"
                : first.Value;
        string laterState = !later.IsSet
            ? "unset"
            : later.Value.ToString(
                System.Globalization.CultureInfo.InvariantCulture);
        return Task.FromResult($"{required}:{firstState}:{laterState}");
    }

    private static Task<string> DispatchOptionalCallback(
        BamlToHostCall call,
        Func<
            long,
            BamlOptional<string?>,
            BamlOptional<long>,
            CancellationToken,
            Task<string>> callback)
    {
        ArgumentNullException.ThrowIfNull(call);
        ArgumentNullException.ThrowIfNull(callback);
        if (call.Args.Count == 0)
        {
            throw new InvalidDataException("Missing required callback argument required.");
        }

        BamlToHostArg requiredArg = call.Args[0];
        if (requiredArg.IsOptionalArg
            || requiredArg.ArgName.Length != 0)
        {
            throw new InvalidDataException(
                "The first callback argument must be the required positional int.");
        }
        long required = DecodeHostCallbackInt(
            requiredArg,
            "$.callback.required");

        BamlOptional<string?> first = default;
        BamlOptional<long> later = default;
        foreach (BamlToHostArg supplied in call.Args.Skip(1))
        {
            if (!supplied.IsOptionalArg || supplied.Value is null)
            {
                throw new InvalidDataException(
                    "Trailing callback arguments must be supplied optionals.");
            }

            switch (supplied.ArgName)
            {
                case "first" when !first.IsSet:
                    first = BamlOptional<string?>.FromValue(
                        supplied.Value.ValueCase switch
                        {
                            BamlOutboundValue.ValueOneofCase.None => null,
                            BamlOutboundValue.ValueOneofCase.StringValue =>
                                supplied.Value.StringValue,
                            _ => throw new InvalidDataException(
                                "Optional callback argument first must be string or null."),
                        });
                    break;
                case "later" when !later.IsSet:
                    later = BamlOptional<long>.FromValue(
                        DecodeHostCallbackInt(
                            supplied,
                            "$.callback.later"));
                    break;
                case "first":
                case "later":
                    throw new InvalidDataException(
                        $"Duplicate supplied optional callback argument {supplied.ArgName}.");
                default:
                    throw new InvalidDataException(
                        $"Unknown supplied optional callback argument {supplied.ArgName}.");
            }
        }

        return callback(
            required,
            first,
            later,
            CancellationToken.None);
    }

    private static long DecodeHostCallbackInt(
        BamlToHostArg argument,
        string path)
    {
        ArgumentNullException.ThrowIfNull(argument);
        if (argument.Value is null)
        {
            throw new InvalidDataException(
                $"{path} is missing its BAML int value.");
        }

        return DecodeBamlInt(argument.Value, path);
    }

    private static BamlToHostCall Call(params BamlToHostArg[] args)
    {
        var call = new BamlToHostCall();
        call.Args.Add(args);
        return call;
    }

    private static BamlToHostArg RequiredInt(long value) =>
        new()
        {
            Value = new BamlOutboundValue { IntValue = value },
        };

    private static BamlToHostArg RequiredString(string value) =>
        new()
        {
            Value = new BamlOutboundValue { StringValue = value },
        };

    private static BamlToHostArg OptionalString(string name, string value) =>
        new()
        {
            ArgName = name,
            IsOptionalArg = true,
            Value = new BamlOutboundValue { StringValue = value },
        };

    private static BamlToHostArg OptionalInt(string name, long value) =>
        new()
        {
            ArgName = name,
            IsOptionalArg = true,
            Value = new BamlOutboundValue { IntValue = value },
        };

    private static BamlToHostArg OptionalNull(string name) =>
        new()
        {
            ArgName = name,
            IsOptionalArg = true,
            Value = new BamlOutboundValue(),
        };

    private static string DecodeLineEnding(
        BamlOutboundValue outbound,
        BamlTy expectedUnionType)
    {
        ArgumentNullException.ThrowIfNull(outbound);
        ArgumentNullException.ThrowIfNull(expectedUnionType);
        if (outbound.ValueCase
            != BamlOutboundValue.ValueOneofCase.UnionVariantValue)
        {
            throw new InvalidDataException(
                $"Expected a union envelope, received {outbound.ValueCase}.");
        }

        BamlValueUnionVariant variant = outbound.UnionVariantValue;
        if (variant.SelfType is null || !variant.SelfType.Equals(expectedUnionType))
        {
            throw new InvalidDataException(
                "The union self_type does not match the generated descriptor.");
        }

        if (variant.Value is null
            || variant.Value.ValueCase
                != BamlOutboundValue.ValueOneofCase.StringValue)
        {
            throw new InvalidDataException(
                "The selected line-ending literal does not contain a string payload.");
        }

        string expectedLiteral = variant.ValueOptionName switch
        {
            "\"lf\"" => "lf",
            "\"crlf\"" => "crlf",
            _ => throw new InvalidDataException(
                $"Selected arm {variant.ValueOptionName} is not a member of the generated union descriptor."),
        };

        if (!StringComparer.Ordinal.Equals(
                variant.Value.StringValue,
                expectedLiteral))
        {
            throw new InvalidDataException(
                $"Selected arm {variant.ValueOptionName} contradicts payload {variant.Value.StringValue}.");
        }

        return variant.Value.StringValue;
    }

    private static void Require(bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidOperationException(message);
        }
    }

    private static void Expect<TException>(Action action)
        where TException : Exception
    {
        try
        {
            action();
            throw new InvalidOperationException(
                $"Expected {typeof(TException).Name} was not thrown.");
        }
        catch (TException)
        {
        }
    }

    private static void ExpectInvalidData(
        Action action,
        string expectedPath)
    {
        try
        {
            action();
            throw new InvalidOperationException(
                "Expected InvalidDataException was not thrown.");
        }
        catch (InvalidDataException error)
        {
            Require(
                error.Message.Contains(expectedPath, StringComparison.Ordinal),
                $"invalid-data diagnostic omitted {expectedPath}: {error.Message}");
        }
    }

    private readonly struct BamlOptional<T>
    {
        private readonly T _value;

        private BamlOptional(T value)
        {
            IsSet = true;
            _value = value;
        }

        public bool IsSet { get; }

        public T Value =>
            IsSet
                ? _value
                : throw new InvalidOperationException("The optional value is unset.");

        public static BamlOptional<T> FromValue(T value) => new(value);
    }
}
