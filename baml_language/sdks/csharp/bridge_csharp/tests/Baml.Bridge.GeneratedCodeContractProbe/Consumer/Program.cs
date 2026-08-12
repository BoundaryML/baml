using System.Numerics;
using Baml.Generated.V1;

namespace Acme.Generated;

public enum Priority
{
    Low,
    High,
}

public sealed partial class Person
{
    public required string DisplayName { get; init; }

    public required Priority Priority { get; init; }

    public required byte[] Avatar { get; init; }
}

public sealed class Contact
{
    private Contact(int activeCase, object value)
    {
        ActiveCase = activeCase;
        Value = value;
    }

    public int ActiveCase { get; }

    public object Value { get; }

    public static Contact Email(string value) => new(0, value);

    public static Contact Owner(Person value) => new(1, value);
}

public sealed class Envelope<T>
{
    public required T Owner { get; init; }

    public required IReadOnlyList<T> Items { get; init; }

    public required IReadOnlyDictionary<string, T> Index { get; init; }

    public required T? OptionalOwner { get; init; }

    public required bool Enabled { get; init; }

    public required long ExactCount { get; init; }

    public required double Score { get; init; }

    public required BigInteger HugeCount { get; init; }

    public required Contact Contact { get; init; }

    public required BamlGeneratedValue Dynamic { get; init; }

    public required BamlGeneratedMedia Media { get; init; }

    public required BamlGeneratedHandle Handle { get; init; }
}

internal sealed class StringCodec : IBamlGeneratedCodec<string>
{
    public BamlGeneratedValue Encode(
        BamlGeneratedCodecContext context,
        string value) =>
        context.String(value);

    public string Decode(
        BamlGeneratedCodecContext context,
        BamlGeneratedValue value) =>
        context.ReadString(value);
}

internal sealed class NullableStringCodec : IBamlGeneratedCodec<string?>
{
    public BamlGeneratedValue Encode(
        BamlGeneratedCodecContext context,
        string? value) =>
        value is null
            ? context.Null()
            : context.String(value);

    public string? Decode(
        BamlGeneratedCodecContext context,
        BamlGeneratedValue value) =>
        value.IsNull
            ? null
            : context.ReadString(value);
}

internal sealed class PriorityCodec(
    BamlGeneratedType<Priority> priorityType)
    : IBamlGeneratedCodec<Priority>
{
    public BamlGeneratedValue Encode(
        BamlGeneratedCodecContext context,
        Priority value) =>
        context.Enum(
            priorityType,
            value switch
            {
                Priority.Low => "low-priority",
                Priority.High => "HIGH_PRIORITY",
                _ => throw new InvalidOperationException(
                    "Unknown generated priority."),
            });

    public Priority Decode(
        BamlGeneratedCodecContext context,
        BamlGeneratedValue value) =>
        context.ReadEnum(priorityType, value) switch
        {
            "low-priority" => Priority.Low,
            "HIGH_PRIORITY" => Priority.High,
            _ => throw new InvalidOperationException(
                "Unknown generated priority wire identity."),
        };
}

internal sealed class PersonCodec(
    BamlGeneratedType<Person> personType,
    BamlGeneratedType<Priority> priorityType)
    : IBamlGeneratedCodec<Person>
{
    public BamlGeneratedValue Encode(
        BamlGeneratedCodecContext context,
        Person value)
    {
        ArgumentNullException.ThrowIfNull(value);
        return context.Object(
            personType,
            [
                new(
                    "display_name",
                    context.String(value.DisplayName)),
                new(
                    "priority-wire",
                    context.Encode(priorityType, value.Priority)),
                new(
                    "avatar_bytes",
                    context.Bytes(value.Avatar)),
            ]);
    }

    public Person Decode(
        BamlGeneratedCodecContext context,
        BamlGeneratedValue value)
    {
        BamlGeneratedObject fields =
            context.ReadObject(personType, value);
        return new Person
        {
            DisplayName =
                context.ReadString(fields.Required("display_name")),
            Priority =
                context.Decode(
                    priorityType,
                    fields.Required("priority-wire")),
            Avatar =
                context.ReadBytes(fields.Required("avatar_bytes")),
        };
    }
}

internal sealed class PersonListCodec(
    BamlGeneratedType<Person> personType)
    : IBamlGeneratedCodec<IReadOnlyList<Person>>
{
    public BamlGeneratedValue Encode(
        BamlGeneratedCodecContext context,
        IReadOnlyList<Person> value) =>
        context.List(
            value
                .Select(item => context.Encode(personType, item))
                .ToArray());

    public IReadOnlyList<Person> Decode(
        BamlGeneratedCodecContext context,
        BamlGeneratedValue value) =>
        context
            .ReadList(value)
            .Select(item => context.Decode(personType, item))
            .ToArray();
}

internal sealed class PersonMapCodec(
    BamlGeneratedType<Person> personType)
    : IBamlGeneratedCodec<IReadOnlyDictionary<string, Person>>
{
    public BamlGeneratedValue Encode(
        BamlGeneratedCodecContext context,
        IReadOnlyDictionary<string, Person> value) =>
        context.Map(
            value
                .Select(pair => new KeyValuePair<string, BamlGeneratedValue>(
                    pair.Key,
                    context.Encode(personType, pair.Value)))
                .ToArray());

    public IReadOnlyDictionary<string, Person> Decode(
        BamlGeneratedCodecContext context,
        BamlGeneratedValue value)
    {
        BamlGeneratedMap map = context.ReadMap(value);
        return new Dictionary<string, Person>(
            StringComparer.Ordinal)
        {
            ["primary"] =
                context.Decode(
                    personType,
                    map.Required("primary")),
        };
    }
}

internal sealed class ContactCodec(
    BamlGeneratedType<Contact> contactType,
    BamlGeneratedType<Person> personType)
    : IBamlGeneratedCodec<Contact>
{
    public BamlGeneratedValue Encode(
        BamlGeneratedCodecContext context,
        Contact value) =>
        value.ActiveCase switch
        {
            0 => context.Union(
                contactType,
                0,
                context.String((string)value.Value)),
            1 => context.Union(
                contactType,
                1,
                context.Encode(personType, (Person)value.Value)),
            _ => throw new InvalidOperationException(
                "Unknown generated contact case."),
        };

    public Contact Decode(
        BamlGeneratedCodecContext context,
        BamlGeneratedValue value)
    {
        BamlGeneratedUnion union =
            context.ReadUnion(contactType, value);
        return union.ActiveCase switch
        {
            0 => Contact.Email(context.ReadString(union.Value)),
            1 => Contact.Owner(
                context.Decode(personType, union.Value)),
            _ => throw new InvalidOperationException(
                "Unknown generated contact wire case."),
        };
    }
}

internal sealed class DynamicCodec :
    IBamlGeneratedCodec<BamlGeneratedValue>
{
    public BamlGeneratedValue Encode(
        BamlGeneratedCodecContext context,
        BamlGeneratedValue value) =>
        context.Dynamic(value);

    public BamlGeneratedValue Decode(
        BamlGeneratedCodecContext context,
        BamlGeneratedValue value) =>
        context.ReadDynamic(value);
}

internal sealed class MediaCodec :
    IBamlGeneratedCodec<BamlGeneratedMedia>
{
    public BamlGeneratedValue Encode(
        BamlGeneratedCodecContext context,
        BamlGeneratedMedia value) =>
        context.Media(value);

    public BamlGeneratedMedia Decode(
        BamlGeneratedCodecContext context,
        BamlGeneratedValue value) =>
        context.ReadMedia(value);
}

internal sealed class HandleCodec :
    IBamlGeneratedCodec<BamlGeneratedHandle>
{
    public BamlGeneratedValue Encode(
        BamlGeneratedCodecContext context,
        BamlGeneratedHandle value) =>
        context.Handle(value);

    public BamlGeneratedHandle Decode(
        BamlGeneratedCodecContext context,
        BamlGeneratedValue value) =>
        context.ReadHandle(value);
}

internal sealed class RequestCodec(
    BamlGeneratedType<BamlGeneratedRequest> requestType)
    : IBamlGeneratedCodec<BamlGeneratedRequest>
{
    public BamlGeneratedValue Encode(
        BamlGeneratedCodecContext context,
        BamlGeneratedRequest value) =>
        context.Object(
            requestType,
            [
                new("method", context.String(value.Method)),
                new("path", context.String(value.Path)),
                new("has_body", context.Boolean(value.HasBody)),
            ]);

    public BamlGeneratedRequest Decode(
        BamlGeneratedCodecContext context,
        BamlGeneratedValue value)
    {
        BamlGeneratedObject fields =
            context.ReadObject(requestType, value);
        return new BamlGeneratedRequest(
            context.ReadString(fields.Required("method")),
            context.ReadString(fields.Required("path")),
            context.ReadBoolean(fields.Required("has_body")));
    }
}

internal sealed class EnvelopeCodec(
    BamlGeneratedType<Envelope<Person>> envelopeType,
    BamlGeneratedType<Person> personType,
    BamlGeneratedType<IReadOnlyList<Person>> personListType,
    BamlGeneratedType<IReadOnlyDictionary<string, Person>> personMapType,
    BamlGeneratedType<Contact> contactType,
    BamlGeneratedType<BamlGeneratedValue> dynamicType,
    BamlGeneratedType<BamlGeneratedMedia> mediaType,
    BamlGeneratedType<BamlGeneratedHandle> handleType)
    : IBamlGeneratedCodec<Envelope<Person>>
{
    public BamlGeneratedValue Encode(
        BamlGeneratedCodecContext context,
        Envelope<Person> value) =>
        context.Object(
            envelopeType,
            [
                new("owner", context.Encode(personType, value.Owner)),
                new("items", context.Encode(personListType, value.Items)),
                new("index", context.Encode(personMapType, value.Index)),
                new(
                    "optional_owner",
                    value.OptionalOwner is null
                        ? context.Null()
                        : context.Encode(personType, value.OptionalOwner)),
                new("enabled", context.Boolean(value.Enabled)),
                new("exact_count", context.Integer(value.ExactCount)),
                new("score", context.Float(value.Score)),
                new("huge_count", context.BigInteger(value.HugeCount)),
                new("contact", context.Encode(contactType, value.Contact)),
                new("dynamic", context.Encode(dynamicType, value.Dynamic)),
                new("media", context.Encode(mediaType, value.Media)),
                new("handle", context.Encode(handleType, value.Handle)),
            ]);

    public Envelope<Person> Decode(
        BamlGeneratedCodecContext context,
        BamlGeneratedValue value)
    {
        BamlGeneratedObject fields =
            context.ReadObject(envelopeType, value);
        BamlGeneratedValue optional =
            fields.Required("optional_owner");
        return new Envelope<Person>
        {
            Owner =
                context.Decode(personType, fields.Required("owner")),
            Items =
                context.Decode(personListType, fields.Required("items")),
            Index =
                context.Decode(personMapType, fields.Required("index")),
            OptionalOwner =
                optional.IsNull
                    ? null
                    : context.Decode(personType, optional),
            Enabled =
                context.ReadBoolean(fields.Required("enabled")),
            ExactCount =
                context.ReadInteger(fields.Required("exact_count")),
            Score =
                context.ReadFloat(fields.Required("score")),
            HugeCount =
                context.ReadBigInteger(fields.Required("huge_count")),
            Contact =
                context.Decode(contactType, fields.Required("contact")),
            Dynamic =
                context.Decode(dynamicType, fields.Required("dynamic")),
            Media =
                context.Decode(mediaType, fields.Required("media")),
            Handle =
                context.Decode(handleType, fields.Required("handle")),
        };
    }
}

internal sealed record GeneratedArtifacts(
    BamlGeneratedRegistry Registry,
    BamlGeneratedType<string> StringType,
    BamlGeneratedType<string?> NullableStringType,
    BamlGeneratedType<Person> PersonType,
    BamlGeneratedType<Envelope<Person>> EnvelopeType,
    BamlGeneratedFunction<Person> EchoPerson,
    BamlGeneratedArgument<Person, Person> EchoPersonArgument,
    BamlGeneratedFunction<Person> EchoPersonAlternate,
    BamlGeneratedArgument<Person, Person> EchoPersonAlternateArgument,
    BamlGeneratedFunction<string> OptionalState,
    BamlGeneratedArgument<string, string?> OptionalStateArgument,
    BamlGeneratedFunction<string> PersonLabel,
    BamlGeneratedArgument<string, Person> PersonLabelSelf,
    BamlGeneratedFunction<BamlGeneratedRequest> BuildRequest,
    BamlGeneratedArgument<BamlGeneratedRequest, Person> BuildRequestArgument,
    BamlGeneratedGenericFunction GenericDefault,
    BamlGeneratedResultTypeParameter GenericResult,
    BamlGeneratedStreamFunction<string, Person> StreamPerson,
    BamlGeneratedStreamArgument<string, Person, Person> StreamPersonArgument);

internal static class GeneratedRegistration
{
    internal const int GeneratedContractVersion = 1;
    internal const string GeneratedRuntimePackageVersion = "0.0.0-a3";
    internal const string RequiredBridgeVersion = "bridge-v1";

    internal static GeneratedArtifacts Create()
    {
        BamlGeneratedRegistryBuilder builder =
            BamlGeneratedContract.CreateRegistryBuilder(
                GeneratedContractVersion,
                GeneratedRuntimePackageVersion,
                RequiredBridgeVersion);

        BamlGeneratedType<string> stringType =
            builder.DeclareType<string>("builtin.string");
        BamlGeneratedType<string?> nullableStringType =
            builder.DeclareType<string?>("builtin.string?");
        BamlGeneratedType<Priority> priorityType =
            builder.DeclareType<Priority>("user.acme.Priority");
        BamlGeneratedType<Person> personType =
            builder.DeclareType<Person>("user.acme.Person");
        BamlGeneratedType<IReadOnlyList<Person>> personListType =
            builder.DeclareType<IReadOnlyList<Person>>(
                "list<user.acme.Person>");
        BamlGeneratedType<IReadOnlyDictionary<string, Person>> personMapType =
            builder.DeclareType<IReadOnlyDictionary<string, Person>>(
                "map<string,user.acme.Person>");
        BamlGeneratedType<Contact> contactType =
            builder.DeclareType<Contact>(
                "union<string,user.acme.Person>");
        BamlGeneratedType<BamlGeneratedValue> dynamicType =
            builder.DeclareType<BamlGeneratedValue>("dynamic");
        BamlGeneratedType<BamlGeneratedMedia> mediaType =
            builder.DeclareType<BamlGeneratedMedia>("media");
        BamlGeneratedType<BamlGeneratedHandle> handleType =
            builder.DeclareType<BamlGeneratedHandle>("handle");
        BamlGeneratedType<Envelope<Person>> envelopeType =
            builder.DeclareType<Envelope<Person>>(
                "user.acme.Envelope<user.acme.Person>");
        BamlGeneratedType<BamlGeneratedRequest> requestType =
            builder.DeclareType<BamlGeneratedRequest>(
                "runtime.build_request");

        builder.RegisterCodec(stringType, new StringCodec());
        builder.RegisterCodec(
            nullableStringType,
            new NullableStringCodec());
        builder.RegisterCodec(
            priorityType,
            new PriorityCodec(priorityType));
        builder.RegisterCodec(
            personType,
            new PersonCodec(personType, priorityType));
        builder.RegisterCodec(
            personListType,
            new PersonListCodec(personType));
        builder.RegisterCodec(
            personMapType,
            new PersonMapCodec(personType));
        builder.RegisterCodec(
            contactType,
            new ContactCodec(contactType, personType));
        builder.RegisterCodec(dynamicType, new DynamicCodec());
        builder.RegisterCodec(mediaType, new MediaCodec());
        builder.RegisterCodec(handleType, new HandleCodec());
        builder.RegisterCodec(
            envelopeType,
            new EnvelopeCodec(
                envelopeType,
                personType,
                personListType,
                personMapType,
                contactType,
                dynamicType,
                mediaType,
                handleType));
        builder.RegisterCodec(
            requestType,
            new RequestCodec(requestType));

        BamlGeneratedFunction<Person> echoPerson =
            builder.DeclareFunction(
                "probe.echo_person",
                "call",
                personType);
        BamlGeneratedArgument<Person, Person> echoPersonArgument =
            builder.DeclareArgument(
                echoPerson,
                "person",
                personType);
        BamlGeneratedFunction<Person> echoPersonAlternate =
            builder.DeclareFunction(
                "probe.echo_person",
                "alternate",
                personType);
        BamlGeneratedArgument<Person, Person>
            echoPersonAlternateArgument =
                builder.DeclareArgument(
                    echoPersonAlternate,
                    "person",
                    personType);

        BamlGeneratedFunction<string> optionalState =
            builder.DeclareFunction(
                "probe.optional_state",
                "call",
                stringType);
        BamlGeneratedArgument<string, string?> optionalStateArgument =
            builder.DeclareArgument(
                optionalState,
                "value",
                nullableStringType,
                optional: true);

        BamlGeneratedFunction<string> personLabel =
            builder.DeclareFunction(
                "probe.person_label",
                "method",
                stringType);
        BamlGeneratedArgument<string, Person> personLabelSelf =
            builder.DeclareArgument(
                personLabel,
                "self",
                personType,
                isSelf: true);

        BamlGeneratedFunction<BamlGeneratedRequest> buildRequest =
            builder.DeclareFunction(
                "probe.echo_person",
                "build_request",
                requestType);
        BamlGeneratedArgument<BamlGeneratedRequest, Person>
            buildRequestArgument =
                builder.DeclareArgument(
                    buildRequest,
                    "person",
                    personType);

        BamlGeneratedGenericFunction genericDefault =
            builder.DeclareResultGenericFunction(
                "probe.generic_default",
                "call",
                "TResult",
                out BamlGeneratedResultTypeParameter genericResult);

        BamlGeneratedStreamFunction<string, Person> streamPerson =
            builder.DeclareStreamFunction(
                "probe.stream_person",
                "stream",
                stringType,
                personType);
        BamlGeneratedStreamArgument<string, Person, Person>
            streamPersonArgument =
                builder.DeclareStreamArgument(
                    streamPerson,
                    "person",
                    personType);

        return new GeneratedArtifacts(
            builder.Build(),
            stringType,
            nullableStringType,
            personType,
            envelopeType,
            echoPerson,
            echoPersonArgument,
            echoPersonAlternate,
            echoPersonAlternateArgument,
            optionalState,
            optionalStateArgument,
            personLabel,
            personLabelSelf,
            buildRequest,
            buildRequestArgument,
            genericDefault,
            genericResult,
            streamPerson,
            streamPersonArgument);
    }
}

internal sealed class GeneratedClient(
    BamlGeneratedProgram program,
    GeneratedArtifacts generated)
{
    internal Person EchoPerson(Person person)
    {
        BamlGeneratedArgumentsBuilder<Person> builder =
            generated.Registry.CreateArgumentsBuilder(
                generated.EchoPerson);
        builder.Set(generated.EchoPersonArgument, person);
        return program.Call(
            generated.EchoPerson,
            builder.Build());
    }

    internal async Task<Person> EchoPersonAsync(
        Person person,
        CancellationToken cancellationToken = default)
    {
        BamlGeneratedArgumentsBuilder<Person> builder =
            generated.Registry.CreateArgumentsBuilder(
                generated.EchoPerson);
        builder.Set(generated.EchoPersonArgument, person);
        return await program.CallAsync(
            generated.EchoPerson,
            builder.Build(),
            cancellationToken);
    }

    internal string OptionalOmitted()
    {
        BamlGeneratedArgumentsBuilder<string> builder =
            generated.Registry.CreateArgumentsBuilder(
                generated.OptionalState);
        builder.Omit(generated.OptionalStateArgument);
        return program.Call(
            generated.OptionalState,
            builder.Build());
    }

    internal string OptionalExplicitNull()
    {
        BamlGeneratedArgumentsBuilder<string> builder =
            generated.Registry.CreateArgumentsBuilder(
                generated.OptionalState);
        builder.Set(generated.OptionalStateArgument, null);
        return program.Call(
            generated.OptionalState,
            builder.Build());
    }

    internal string PersonLabel(Person self)
    {
        BamlGeneratedArgumentsBuilder<string> builder =
            generated.Registry.CreateArgumentsBuilder(
                generated.PersonLabel);
        builder.Set(generated.PersonLabelSelf, self);
        return program.Call(
            generated.PersonLabel,
            builder.Build());
    }

    internal BamlGeneratedRequest BuildEchoPersonRequest(Person person)
    {
        BamlGeneratedArgumentsBuilder<BamlGeneratedRequest> builder =
            generated.Registry.CreateArgumentsBuilder(
                generated.BuildRequest);
        builder.Set(generated.BuildRequestArgument, person);
        return program.Call(
            generated.BuildRequest,
            builder.Build());
    }

    internal string GenericStringDefault()
    {
        BamlGeneratedTypeBinding<string> binding =
            generated.Registry.BindResultType(
                generated.GenericResult,
                generated.StringType);
        BamlGeneratedFunction<string> function =
            generated.Registry.BindResult(
                generated.GenericDefault,
                binding);
        BamlGeneratedArgumentsBuilder<string> builder =
            generated.Registry.CreateArgumentsBuilder(function);
        return program.Call(function, builder.Build());
    }

    internal BamlGeneratedStream<string, Person> StreamPerson(
        Person person)
    {
        BamlGeneratedStreamArgumentsBuilder<string, Person> builder =
            generated.Registry.CreateArgumentsBuilder(
                generated.StreamPerson);
        builder.Set(generated.StreamPersonArgument, person);
        return program.Stream(
            generated.StreamPerson,
            builder.Build());
    }
}

internal static class ContractChecks
{
    internal static void CarrierRoundTrip(
        GeneratedArtifacts generated,
        Person owner)
    {
        byte[] sourceBytes = [0x10, 0x20, 0x30];
        var media = new BamlGeneratedMedia(
            "image",
            "image/png",
            sourceBytes);
        var handle = new BamlGeneratedHandle(
            "tool",
            "handle-1",
            new Dictionary<string, string>(StringComparer.Ordinal)
            {
                ["scope"] = "test",
            });
        var envelope = new Envelope<Person>
        {
            Owner = owner,
            Items = [owner],
            Index = new Dictionary<string, Person>(
                StringComparer.Ordinal)
            {
                ["primary"] = owner,
            },
            OptionalOwner = null,
            Enabled = true,
            ExactCount = BamlGeneratedContract.MaximumInteger,
            Score = 0.125,
            HugeCount =
                BigInteger.Parse(
                    "123456789012345678901234567890",
                    System.Globalization.CultureInfo.InvariantCulture),
            Contact = Contact.Owner(owner),
            Dynamic =
                generated.Registry.Encode(
                    generated.StringType,
                    "dynamic-value"),
            Media = media,
            Handle = handle,
        };

        BamlGeneratedValue encoded =
            generated.Registry.Encode(
                generated.EnvelopeType,
                envelope);
        sourceBytes[0] = 0xff;
        owner.Avatar[0] = 0xee;
        Envelope<Person> decoded =
            generated.Registry.Decode(
                generated.EnvelopeType,
                encoded);
        byte[] firstMediaCopy = decoded.Media.Data;
        firstMediaCopy[0] = 0xdd;

        if (decoded.Owner.Avatar[0] != 0x01
            || decoded.Media.Data[0] != 0x10
            || decoded.Items.Count != 1
            || decoded.Index["primary"].Priority != Priority.High
            || decoded.OptionalOwner is not null
            || !decoded.Enabled
            || decoded.ExactCount
                != BamlGeneratedContract.MaximumInteger
            || decoded.Score != 0.125
            || decoded.HugeCount != envelope.HugeCount
            || decoded.Contact.ActiveCase != 1
            || decoded.Handle.Metadata["scope"] != "test")
        {
            throw new InvalidOperationException(
                "The representative generated V1 carrier graph did not round trip.");
        }

        Expect<OverflowException>(
            () => _ = new EnvelopeCodecProbe().EncodeTooLarge(),
            "Out-of-range generated integer was accepted.");
        Expect<ArgumentOutOfRangeException>(
            () => _ = new EnvelopeCodecProbe().EncodeNonFinite(),
            "Non-finite generated float was accepted.");
    }

    internal static void NegativeTokenChecks(
        GeneratedArtifacts generated)
    {
        BamlGeneratedRegistryBuilder first =
            NewBuilder();
        BamlGeneratedRegistryBuilder second =
            NewBuilder();
        BamlGeneratedType<string> firstString =
            first.DeclareType<string>("same.identity");
        BamlGeneratedType<string> secondString =
            second.DeclareType<string>("same.identity");
        Expect<InvalidOperationException>(
            () => first.RegisterCodec(
                secondString,
                new StringCodec()),
            "A same-ID/same-identity token from another builder was accepted.");
        first.RegisterCodec(firstString, new StringCodec());
        second.RegisterCodec(secondString, new StringCodec());

        BamlGeneratedFunction<string> firstFunction =
            first.DeclareFunction(
                "same.function",
                "call",
                firstString);
        _ = second.DeclareFunction(
            "same.function",
            "call",
            secondString);
        Expect<InvalidOperationException>(
            () => first.DeclareFunction(
                "same.function",
                "call",
                firstString),
            "A duplicate function+variant identity was accepted.");
        Expect<InvalidOperationException>(
            () => first.DeclareType<string>("same.identity"),
            "A duplicate BAML type identity was accepted.");

        BamlGeneratedRegistry firstRegistry = first.Build();
        BamlGeneratedRegistry secondRegistry = second.Build();
        Expect<InvalidOperationException>(
            () => first.DeclareFunction(
                "frozen.function",
                "call",
                firstString),
            "A frozen registry builder was mutable.");
        Expect<InvalidOperationException>(
            () => firstRegistry.Encode(
                secondString,
                "foreign"),
            "A cross-registry type token was accepted.");
        Expect<InvalidOperationException>(
            () => firstRegistry.Encode(
                default(BamlGeneratedType<string>),
                "default"),
            "A default type token was accepted.");
        Expect<InvalidOperationException>(
            () => firstRegistry.CreateArgumentsBuilder(
                default(BamlGeneratedFunction<string>)),
            "A default function token was accepted.");
        Expect<InvalidOperationException>(
            () => generated.Registry.CreateArgumentsBuilder(
                default(BamlGeneratedStreamFunction<string, Person>)),
            "A default stream function token was accepted.");

        BamlGeneratedArgumentsBuilder<string> frozenArguments =
            firstRegistry.CreateArgumentsBuilder(firstFunction);
        Expect<InvalidOperationException>(
            () => frozenArguments.Set(
                default(BamlGeneratedArgument<string, string>),
                "default"),
            "A default argument token was accepted.");
        _ = frozenArguments.Build();
        Expect<InvalidOperationException>(
            () => _ = frozenArguments.Build(),
            "A frozen arguments builder was reusable.");

        BamlGeneratedArgumentsBuilder<Person> echoArguments =
            generated.Registry.CreateArgumentsBuilder(
                generated.EchoPerson);
        Expect<InvalidOperationException>(
            () => echoArguments.Set(
                generated.EchoPersonAlternateArgument,
                new Person
                {
                    DisplayName = "wrong-function",
                    Priority = Priority.Low,
                    Avatar = [0x01],
                }),
            "An argument token from another function was accepted.");

        BamlGeneratedRegistryBuilder genericBuilder =
            NewBuilder();
        BamlGeneratedType<string> genericString =
            genericBuilder.DeclareType<string>("generic.string");
        genericBuilder.RegisterCodec(
            genericString,
            new StringCodec());
        BamlGeneratedGenericFunction genericOne =
            genericBuilder.DeclareResultGenericFunction(
                "generic.one",
                "call",
                "T",
                out BamlGeneratedResultTypeParameter parameterOne);
        BamlGeneratedGenericFunction genericTwo =
            genericBuilder.DeclareResultGenericFunction(
                "generic.two",
                "call",
                "T",
                out _);
        BamlGeneratedRegistry genericRegistry =
            genericBuilder.Build();
        Expect<InvalidOperationException>(
            () => genericRegistry.BindResultType(
                default(BamlGeneratedResultTypeParameter),
                genericString),
            "A default result type parameter token was accepted.");
        BamlGeneratedTypeBinding<string> binding =
            genericRegistry.BindResultType(
                parameterOne,
                genericString);
        Expect<InvalidOperationException>(
            () => genericRegistry.BindResult(
                genericOne,
                default(BamlGeneratedTypeBinding<string>)),
            "A default result type binding token was accepted.");
        Expect<InvalidOperationException>(
            () => genericRegistry.BindResult(
                genericTwo,
                binding),
            "A contradictory result-only generic type binding was accepted.");
        _ = genericRegistry.BindResult(genericOne, binding);

        _ = secondRegistry;
    }

    internal static void VersionChecks(
        GeneratedArtifacts generated)
    {
        Expect<NotSupportedException>(
            () => _ = BamlGeneratedContract.CreateRegistryBuilder(
                BamlGeneratedContract.ContractVersion + 1,
                BamlGeneratedContract.RuntimePackageVersion,
                BamlGeneratedContract.BridgeVersion),
            "An incompatible generated contract version was accepted.");
        Expect<NotSupportedException>(
            () => _ = BamlGeneratedContract.CreateRegistryBuilder(
                BamlGeneratedContract.ContractVersion,
                "0.0.0-wrong",
                BamlGeneratedContract.BridgeVersion),
            "An incompatible generated runtime version was accepted.");
        Expect<NotSupportedException>(
            () => _ = BamlGeneratedContract.RegisterProgram(
                BamlGeneratedContract.ContractVersion,
                BamlGeneratedContract.RuntimePackageVersion,
                "bridge-wrong",
                ReadOnlyMemory<byte>.Empty,
                string.Empty,
                null!),
            "An incompatible required bridge version was not checked first.");
        Expect<InvalidOperationException>(
            () => _ = BamlGeneratedContract.RegisterProgram(
                BamlGeneratedContract.ContractVersion,
                BamlGeneratedContract.RuntimePackageVersion,
                BamlGeneratedContract.BridgeVersion,
                new byte[] { 0x01 },
                new string('0', 64),
                generated.Registry),
            "A contradictory generated bytecode fingerprint was accepted.");
    }

    private static BamlGeneratedRegistryBuilder NewBuilder() =>
        BamlGeneratedContract.CreateRegistryBuilder(
            GeneratedRegistration.GeneratedContractVersion,
            GeneratedRegistration.GeneratedRuntimePackageVersion,
            GeneratedRegistration.RequiredBridgeVersion);

    internal static void Expect<TException>(
        Action action,
        string failure)
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

        throw new InvalidOperationException(failure);
    }

    private sealed class EnvelopeCodecProbe
    {
        internal BamlGeneratedValue EncodeTooLarge()
        {
            GeneratedArtifacts generated =
                GeneratedRegistration.Create();
            return generated.Registry.Encode(
                generated.EnvelopeType,
                CreateEnvelope(
                    BamlGeneratedContract.MaximumInteger + 1,
                    1.0));
        }

        internal BamlGeneratedValue EncodeNonFinite()
        {
            GeneratedArtifacts generated =
                GeneratedRegistration.Create();
            return generated.Registry.Encode(
                generated.EnvelopeType,
                CreateEnvelope(1, double.NaN));
        }

        private static Envelope<Person> CreateEnvelope(
            long exactCount,
            double score)
        {
            var person = new Person
            {
                DisplayName = "probe",
                Priority = Priority.Low,
                Avatar = [0x01],
            };
            return new Envelope<Person>
            {
                Owner = person,
                Items = [person],
                Index = new Dictionary<string, Person>(
                    StringComparer.Ordinal)
                {
                    ["primary"] = person,
                },
                OptionalOwner = null,
                Enabled = true,
                ExactCount = exactCount,
                Score = score,
                HugeCount = BigInteger.One,
                Contact = Contact.Email("probe@example.com"),
                Dynamic = CreateDynamic(),
                Media = new BamlGeneratedMedia(
                    "image",
                    "image/png",
                    new byte[] { 0x01 }),
                Handle = new BamlGeneratedHandle(
                    "tool",
                    "probe",
                    new Dictionary<string, string>()),
            };
        }

        private static BamlGeneratedValue CreateDynamic()
        {
            GeneratedArtifacts generated =
                GeneratedRegistration.Create();
            return generated.Registry.Encode(
                generated.StringType,
                "dynamic");
        }
    }
}

internal static class Program
{
    public static async Task<int> Main()
    {
        GeneratedArtifacts generated =
            GeneratedRegistration.Create();
        byte[] bytecode = [0x01, 0x02, 0x03];
        BamlGeneratedProgram program =
            BamlGeneratedContract.RegisterProgram(
                GeneratedRegistration.GeneratedContractVersion,
                GeneratedRegistration.GeneratedRuntimePackageVersion,
                GeneratedRegistration.RequiredBridgeVersion,
                bytecode,
                "039058c6f2c0cb492c533b0a4d14ef77cc0f78abccced5287d84a1a2011cfb81",
                generated.Registry);
        var client = new GeneratedClient(program, generated);
        var original = new Person
        {
            DisplayName = "Ada",
            Priority = Priority.High,
            Avatar = [0x01, 0x02, 0x03],
        };

        Person sync = client.EchoPerson(original);
        Person asyncResult = await client.EchoPersonAsync(original);
        if (sync.DisplayName != "Ada"
            || asyncResult.Priority != Priority.High)
        {
            throw new InvalidOperationException(
                "Fixed sync/async generated calls failed.");
        }

        if (client.OptionalOmitted() != "omitted"
            || client.OptionalExplicitNull() != "explicit-null")
        {
            throw new InvalidOperationException(
                "Optional omission was not distinct from explicit null.");
        }

        if (client.PersonLabel(original) != "self-ok")
        {
            throw new InvalidOperationException(
                "The generated receiver/self token failed.");
        }

        BamlGeneratedRequest request =
            client.BuildEchoPersonRequest(original);
        if (request.Method != "POST"
            || request.Path != "/v1/call/probe.echo_person"
            || !request.HasBody)
        {
            throw new InvalidOperationException(
                "The generated build-request companion failed.");
        }

        if (client.GenericStringDefault() != "generic-string")
        {
            throw new InvalidOperationException(
                "The result-only generic binding failed.");
        }

        BamlGeneratedStream<string, Person> stream =
            client.StreamPerson(original);
        Person final = await stream.GetFinalAsync();
        if (!stream.Partials.SequenceEqual(
                ["A", "Ad", "Ada"],
                StringComparer.Ordinal)
            || final.DisplayName != "Ada")
        {
            throw new InvalidOperationException(
                "The typed stream partial/final tokens failed.");
        }

        ContractChecks.CarrierRoundTrip(generated, original);
        ContractChecks.NegativeTokenChecks(generated);
        ContractChecks.VersionChecks(generated);

        using var cancellation = new CancellationTokenSource();
        cancellation.Cancel();
        Task<Person> canceled =
            client.EchoPersonAsync(
                original,
                cancellation.Token);
        try
        {
            _ = await canceled;
            throw new InvalidOperationException(
                "A pre-canceled generated async call completed.");
        }
        catch (OperationCanceledException exception)
            when (exception.CancellationToken
                == cancellation.Token)
        {
        }

        if (canceled.Status != TaskStatus.Canceled)
        {
            throw new InvalidOperationException(
                $"Generated async cancellation produced {canceled.Status}, not Canceled.");
        }

        Console.WriteLine(
            "cross_assembly_generated_codec=full_v1_representative_ok");
        Console.WriteLine(
            "cross_assembly_generated_dispatch=sync_async_generic_optional_self_request_stream_ok");
        Console.WriteLine(
            "generated_token_negatives=cross_builder_duplicate_default_frozen_contradictory_ok");
        Console.WriteLine(
            "generated_async_cancellation=status_canceled_exact_token_ok");
        Console.WriteLine(
            $"generated_program_fingerprint={program.Fingerprint}");
        Console.WriteLine(
            $"generated_contract_version={BamlGeneratedContract.ContractVersion}");
        Console.WriteLine(
            $"generated_runtime_version={program.RuntimePackageVersion}");
        Console.WriteLine(
            $"generated_bridge_version={program.BridgeVersion}");
        return 0;
    }
}
