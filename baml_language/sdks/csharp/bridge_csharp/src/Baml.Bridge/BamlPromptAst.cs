using Baml.Bridge;

namespace Baml;

public sealed record BamlPromptMessage
{
    public BamlPromptMessage(string role, string content)
    {
        Role = role ?? throw new ArgumentNullException(nameof(role));
        Content = content ?? throw new ArgumentNullException(nameof(content));
    }

    public string Role { get; }

    public string Content { get; }
}

public sealed class BamlPromptAst : IDisposable
{
    private const int PromptAstHandleType = 11;
    private NativeHandle? _handle;

    internal BamlPromptAst(NativeHandle handle)
    {
        if (handle.HandleType != PromptAstHandleType)
        {
            handle.Dispose();
            throw new BamlBridgeException(
                $"The native runtime returned handle type {handle.HandleType}, but a BAML prompt AST requires {PromptAstHandleType}.");
        }

        _handle = handle;
    }

    public BamlPromptAst Clone() => new(GetHandle().Clone("clone BamlPromptAst"));

    public string Text() => TextAsync(CancellationToken.None).GetAwaiter().GetResult();

    public Task<string> TextAsync(CancellationToken cancellationToken = default) =>
        CallDispatcher.CallAsync<string>(
            "baml.llm.PromptAst.text",
            [("self", this)],
            Array.Empty<(string Name, Type Type)>(),
            cancellationToken);

    public IReadOnlyList<BamlPromptMessage> Messages() =>
        MessagesAsync(CancellationToken.None).GetAwaiter().GetResult();

    public async Task<IReadOnlyList<BamlPromptMessage>> MessagesAsync(
        CancellationToken cancellationToken = default) =>
        await CallDispatcher.CallAsync<List<BamlPromptMessage>>(
                "baml.llm.PromptAst.messages",
                [("self", this)],
                Array.Empty<(string Name, Type Type)>(),
                cancellationToken)
            .ConfigureAwait(false);

    public override string ToString() => Text();

    public void Dispose()
    {
        Interlocked.Exchange(ref _handle, null)?.Dispose();
        GC.SuppressFinalize(this);
    }

    internal (ulong Key, int HandleType) CloneForWire()
    {
        var clone = GetHandle().Clone("clone BamlPromptAst for BAML argument");
        var key = clone.Key;
        var handleType = clone.HandleType;
        clone.SetHandleAsInvalid();
        clone.Dispose();
        return (key, handleType);
    }

    private NativeHandle GetHandle() => Volatile.Read(ref _handle)
        ?? throw new ObjectDisposedException(GetType().FullName);
}
