using System.Reflection;

using Baml.Generated.V1;

internal static class Program
{
    public static int Main()
    {
        BamlGeneratedRegistryBuilder builder =
            BamlGeneratedContract.CreateRegistryBuilder(BamlGeneratedContract.Version);
        BamlGeneratedType<string> text = builder.DeclareType<string>("baml.string");
        builder.RegisterCodec(text, new StringCodec());
        BamlGeneratedFunction<string> echo =
            builder.DeclareFunction("consumer.echo", "call", text);
        BamlGeneratedArgument<string, string> value =
            builder.DeclareArgument(echo, "wire_value", text);
        BamlGeneratedRegistry registry = builder.Build();
        BamlGeneratedArgumentsBuilder<string> arguments =
            registry.CreateArgumentsBuilder(echo);
        arguments.Add(value, "hello");
        _ = arguments.Build();
        string runtimeVersion = typeof(Baml.BamlValue).Assembly
            .GetCustomAttribute<AssemblyInformationalVersionAttribute>()
            ?.InformationalVersion
            ?? throw new InvalidOperationException(
                "Baml.Bridge is missing its informational package version.");

        try
        {
            _ = BamlGeneratedContract.RegisterProgram(
                BamlGeneratedContract.Version,
                new byte[] { 1, 2, 3 },
                new string('0', 64),
                runtimeVersion,
                runtimeVersion,
                registry);
        }
        catch (Baml.BamlProgramIntegrityException)
        {
            Console.WriteLine("unrelated_generated_consumer=ok");
            return 0;
        }

        throw new InvalidOperationException(
            "generated program fingerprint misuse was not rejected");
    }

    private sealed class StringCodec : IBamlGeneratedCodec<string>
    {
        public BamlGeneratedValue Encode(BamlGeneratedCodecContext context, string value) =>
            context.String(value);

        public string Decode(BamlGeneratedCodecContext context, BamlGeneratedValue value) =>
            context.ReadString(value);
    }
}
