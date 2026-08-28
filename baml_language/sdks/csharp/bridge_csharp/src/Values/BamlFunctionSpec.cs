using Baml.Generated.V1;
using System.Collections.ObjectModel;

namespace Baml;

/// <summary>
/// An opaque, bound <c>ai.FunctionSpec&lt;TFinal&gt;</c> capability.
/// </summary>
public sealed class BamlFunctionSpec<TFinal> : IDisposable
{
    private readonly BamlGeneratedRegistry registry;
    private readonly TypeDeclaration<TFinal> finalType;
    private BamlHandle? handle;

    internal BamlFunctionSpec(
        BamlHandle handle,
        BamlGeneratedRegistry registry,
        TypeDeclaration<TFinal> finalType)
    {
        this.handle = handle ?? throw new ArgumentNullException(nameof(handle));
        this.registry = registry ?? throw new ArgumentNullException(nameof(registry));
        this.finalType = finalType ?? throw new ArgumentNullException(nameof(finalType));
    }

    /// <summary>Execute this bound spec using its default client.</summary>
    public TFinal Call(CancellationToken cancellationToken = default) =>
        CallAsync(cancellationToken).GetAwaiter().GetResult();

    /// <summary>Execute this bound spec asynchronously using its default client.</summary>
    public async Task<TFinal> CallAsync(
        CancellationToken cancellationToken = default)
    {
        BamlGeneratedValue result = await InvokeAsync(
                "ai.FunctionSpec.call",
                additionalArguments: null,
                cancellationToken)
            .ConfigureAwait(false);
        return registry.Decode(finalType, result);
    }

    /// <summary>Parse an existing model reply using this spec's output type.</summary>
    public TFinal Parse(
        string json,
        CancellationToken cancellationToken = default) =>
        ParseAsync(json, cancellationToken).GetAwaiter().GetResult();

    /// <summary>Parse an existing model reply asynchronously.</summary>
    public async Task<TFinal> ParseAsync(
        string json,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(json);
        BamlGeneratedValue result = await InvokeAsync(
                "ai.FunctionSpec.parse",
                [new("json", BamlGeneratedValue.CreateString(json))],
                cancellationToken)
            .ConfigureAwait(false);
        return registry.Decode(finalType, result);
    }

    /// <summary>Render the portable provider-neutral prompt for this spec.</summary>
    public BamlPrompt Prompt(CancellationToken cancellationToken = default) =>
        PromptAsync(cancellationToken).GetAwaiter().GetResult();

    /// <summary>Render the portable prompt asynchronously.</summary>
    public async Task<BamlPrompt> PromptAsync(
        CancellationToken cancellationToken = default)
    {
        BamlGeneratedValue result = await InvokeAsync(
                "ai.FunctionSpec.prompt",
                additionalArguments: null,
                cancellationToken)
            .ConfigureAwait(false);
        return new BamlPrompt(result.ReadPromptAst(), registry);
    }

    /// <summary>Build the provider HTTP request without invoking the model.</summary>
    public BamlValue BuildRequest(CancellationToken cancellationToken = default) =>
        BuildRequestAsync(cancellationToken).GetAwaiter().GetResult();

    /// <summary>Build the provider HTTP request asynchronously.</summary>
    public async Task<BamlValue> BuildRequestAsync(
        CancellationToken cancellationToken = default) =>
        new(await InvokeAsync(
                "ai.FunctionSpec.build_request",
                additionalArguments: null,
                cancellationToken)
            .ConfigureAwait(false));

    public string Name(CancellationToken cancellationToken = default) =>
        NameAsync(cancellationToken).GetAwaiter().GetResult();

    public async Task<string> NameAsync(
        CancellationToken cancellationToken = default) =>
        (await InvokeAsync(
                "ai.FunctionSpec.name",
                additionalArguments: null,
                cancellationToken)
            .ConfigureAwait(false)).ReadString();

    public IReadOnlyDictionary<string, BamlValue> Arguments(
        CancellationToken cancellationToken = default) =>
        ArgumentsAsync(cancellationToken).GetAwaiter().GetResult();

    public async Task<IReadOnlyDictionary<string, BamlValue>> ArgumentsAsync(
        CancellationToken cancellationToken = default)
    {
        BamlGeneratedValue result = await InvokeAsync(
                "ai.FunctionSpec.arguments",
                additionalArguments: null,
                cancellationToken)
            .ConfigureAwait(false);
        return new ReadOnlyDictionary<string, BamlValue>(
            result.ReadMapEntries().ToDictionary(
                pair => pair.Key,
                pair => new BamlValue(pair.Value),
                StringComparer.Ordinal));
    }

    public BamlType OutputType(CancellationToken cancellationToken = default) =>
        OutputTypeAsync(cancellationToken).GetAwaiter().GetResult();

    public async Task<BamlType> OutputTypeAsync(
        CancellationToken cancellationToken = default) =>
        (await InvokeAsync(
                "ai.FunctionSpec.output_type",
                additionalArguments: null,
                cancellationToken)
            .ConfigureAwait(false)).ReadType();

    /// <summary>
    /// Return the spec's toolbox as a type-erased BAML value. Tool schemas and
    /// callbacks remain represented by their canonical BAML values.
    /// </summary>
    public BamlValue Tools(CancellationToken cancellationToken = default) =>
        ToolsAsync(cancellationToken).GetAwaiter().GetResult();

    public async Task<BamlValue> ToolsAsync(
        CancellationToken cancellationToken = default) =>
        new(await InvokeAsync(
                "ai.FunctionSpec.tools",
                additionalArguments: null,
                cancellationToken)
            .ConfigureAwait(false));

    public string ClientId(CancellationToken cancellationToken = default) =>
        ClientIdAsync(cancellationToken).GetAwaiter().GetResult();

    public async Task<string> ClientIdAsync(
        CancellationToken cancellationToken = default) =>
        (await InvokeAsync(
                "ai.FunctionSpec.client_id",
                additionalArguments: null,
                cancellationToken)
            .ConfigureAwait(false)).ReadString();

    public void Dispose()
    {
        BamlHandle? current = Interlocked.Exchange(ref handle, null);
        current?.Dispose();
    }

    public override string ToString() => "<BamlFunctionSpec>";

    internal BamlHandle Handle =>
        Volatile.Read(ref handle)
        ?? throw new ObjectDisposedException(nameof(BamlFunctionSpec<TFinal>));

    internal void RequireRegistry(BamlGeneratedRegistry expected)
    {
        _ = Handle;
        if (!ReferenceEquals(registry, expected))
        {
            throw new InvalidOperationException(
                "The BAML FunctionSpec belongs to another generated program.");
        }
    }

    private Task<BamlGeneratedValue> InvokeAsync(
        string functionIdentity,
        IReadOnlyList<KeyValuePair<string, BamlGeneratedValue>>? additionalArguments,
        CancellationToken cancellationToken)
    {
        var arguments = new List<KeyValuePair<string, BamlGeneratedValue>>(
            1 + (additionalArguments?.Count ?? 0))
        {
            new("self", BamlGeneratedValue.CreateHandle(Handle)),
        };
        if (additionalArguments is not null)
        {
            arguments.AddRange(additionalArguments);
        }

        return registry.RequireProgram().CallRuntimeMethodAsync(
            functionIdentity,
            arguments.AsReadOnly(),
            cancellationToken);
    }
}
