namespace Baml;

public sealed class BamlProgram
{
    private readonly byte[] _fingerprint;

    internal BamlProgram(byte[] fingerprint, string fingerprintText)
    {
        _fingerprint = fingerprint;
        FingerprintText = fingerprintText;
    }

    internal ReadOnlySpan<byte> Fingerprint => _fingerprint;

    public string FingerprintText { get; }

    public T Call<T>(string functionName, params (string Name, object? Value)[] arguments) =>
        Call<T>(functionName, arguments, Array.Empty<(string Name, Type Type)>());

    [System.ComponentModel.EditorBrowsable(System.ComponentModel.EditorBrowsableState.Never)]
    public T Call<T>(
        string functionName,
        (string Name, object? Value)[] arguments,
        (string Name, Type Type)[] typeArguments) =>
        CallAsync<T>(functionName, arguments, typeArguments, CancellationToken.None).GetAwaiter().GetResult();

    public Task<T> CallAsync<T>(
        string functionName,
        (string Name, object? Value)[] arguments,
        CancellationToken cancellationToken = default) =>
        CallAsync<T>(
            functionName,
            arguments,
            Array.Empty<(string Name, Type Type)>(),
            cancellationToken);

    [System.ComponentModel.EditorBrowsable(System.ComponentModel.EditorBrowsableState.Never)]
    public Task<T> CallAsync<T>(
        string functionName,
        (string Name, object? Value)[] arguments,
        (string Name, Type Type)[] typeArguments,
        CancellationToken cancellationToken = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(functionName);
        if (functionName.Contains('\0', StringComparison.Ordinal))
        {
            throw new ArgumentException("A BAML function name cannot contain a NUL character.", nameof(functionName));
        }
        ArgumentNullException.ThrowIfNull(arguments);
        ArgumentNullException.ThrowIfNull(typeArguments);
        return Bridge.CallDispatcher.CallAsync<T>(functionName, arguments, typeArguments, cancellationToken);
    }
}
