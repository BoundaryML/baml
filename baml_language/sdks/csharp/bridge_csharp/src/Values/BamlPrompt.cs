using System.Collections.ObjectModel;

using Baml.Generated.V1;
using BamlBridge.Cffi.V1;

namespace Baml;

/// <summary>
/// A portable, provider-neutral rendered prompt. The prompt tree is copied at
/// the bridge boundary and can be passed through multiple calls without
/// consuming an engine handle.
/// </summary>
public sealed class BamlPrompt
{
    private readonly BamlValuePromptAst wire;
    private readonly BamlGeneratedRegistry registry;

    internal BamlPrompt(
        BamlValuePromptAst wire,
        BamlGeneratedRegistry registry)
    {
        ArgumentNullException.ThrowIfNull(wire);
        ArgumentNullException.ThrowIfNull(registry);
        if (wire.ValueCase == BamlValuePromptAst.ValueOneofCase.None)
        {
            throw new BamlProtocolException(
                "The native bridge returned an empty BAML prompt.",
                "BamlValuePromptAst had no prompt tree variant.");
        }

        this.wire = wire.Clone();
        this.registry = registry;
    }

    /// <summary>Render this prompt as readable text.</summary>
    public string Text(CancellationToken cancellationToken = default) =>
        TextAsync(cancellationToken).GetAwaiter().GetResult();

    /// <summary>Render this prompt as readable text asynchronously.</summary>
    public async Task<string> TextAsync(
        CancellationToken cancellationToken = default)
    {
        BamlGeneratedValue result = await InvokeAsync(
                "ai.Prompt.text",
                cancellationToken)
            .ConfigureAwait(false);
        return result.ReadString();
    }

    /// <summary>Return the ordered structural messages in this prompt.</summary>
    public IReadOnlyList<BamlPromptMessage> Messages(
        CancellationToken cancellationToken = default) =>
        MessagesAsync(cancellationToken).GetAwaiter().GetResult();

    /// <summary>Return the ordered structural messages asynchronously.</summary>
    public async Task<IReadOnlyList<BamlPromptMessage>> MessagesAsync(
        CancellationToken cancellationToken = default)
    {
        BamlGeneratedValue result = await InvokeAsync(
                "ai.Prompt.messages",
                cancellationToken)
            .ConfigureAwait(false);
        return Array.AsReadOnly(
            result.ReadList()
                .Select(BamlPromptMessage.FromGenerated)
                .ToArray());
    }

    internal BamlValuePromptAst WireCopy() => wire.Clone();

    private Task<BamlGeneratedValue> InvokeAsync(
        string functionIdentity,
        CancellationToken cancellationToken)
    {
        IReadOnlyList<KeyValuePair<string, BamlGeneratedValue>> arguments =
        [
            new("self", BamlGeneratedValue.CreatePromptAst(wire)),
        ];
        return registry.RequireProgram().CallRuntimeMethodAsync(
            functionIdentity,
            arguments,
            cancellationToken);
    }
}

/// <summary>A structural message returned by <see cref="BamlPrompt.Messages"/>.</summary>
public sealed class BamlPromptMessage
{
    internal BamlPromptMessage(
        string role,
        string content,
        IReadOnlyList<BamlValue> parts,
        IReadOnlyDictionary<string, BamlValue> metadata)
    {
        Role = role;
        Content = content;
        Parts = parts;
        Metadata = metadata;
    }

    public string Role { get; }

    public string Content { get; }

    public IReadOnlyList<BamlValue> Parts { get; }

    public IReadOnlyDictionary<string, BamlValue> Metadata { get; }

    internal static BamlPromptMessage FromGenerated(BamlGeneratedValue value)
    {
        if (!StringComparer.Ordinal.Equals(
                value.ReadClassIdentity(),
                "ai.PromptMessage"))
        {
            throw new BamlProtocolException(
                "The native bridge returned a non-message prompt item.",
                $"ai.Prompt.messages returned {value.ReadClassIdentity()}.");
        }

        IReadOnlyDictionary<string, BamlGeneratedValue> fields =
            new ReadOnlyDictionary<string, BamlGeneratedValue>(
                value.ReadClassFields().ToDictionary(
                    pair => pair.Key,
                    pair => pair.Value,
                    StringComparer.Ordinal));
        BamlGeneratedValue role = Required(fields, "role");
        BamlGeneratedValue content = Required(fields, "content");
        BamlGeneratedValue parts = Required(fields, "parts");
        BamlGeneratedValue metadata = Required(fields, "metadata");
        return new BamlPromptMessage(
            role.ReadString(),
            content.ReadString(),
            Array.AsReadOnly(
                parts.ReadList().Select(item => new BamlValue(item)).ToArray()),
            new ReadOnlyDictionary<string, BamlValue>(
                metadata.ReadMapEntries().ToDictionary(
                    pair => pair.Key,
                    pair => new BamlValue(pair.Value),
                    StringComparer.Ordinal)));
    }

    private static BamlGeneratedValue Required(
        IReadOnlyDictionary<string, BamlGeneratedValue> fields,
        string name) =>
        fields.TryGetValue(name, out BamlGeneratedValue? value)
            ? value
            : throw new BamlProtocolException(
                "The native bridge returned a malformed prompt message.",
                $"ai.PromptMessage omitted field {name}.");
}
