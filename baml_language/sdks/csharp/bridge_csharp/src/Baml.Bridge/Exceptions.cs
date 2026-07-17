namespace Baml;

public class BamlException : Exception
{
    public BamlException(string message, object? value, IReadOnlyList<string>? bamlTrace, Exception? innerException = null)
        : base(message, innerException)
    {
        Value = value;
        BamlTrace = bamlTrace ?? Array.Empty<string>();
    }

    public object? Value { get; }

    public IReadOnlyList<string> BamlTrace { get; }

    public string? ClassName { get; internal init; }
}

public sealed class BamlError : BamlException
{
    public BamlError(object? value, IReadOnlyList<string>? bamlTrace)
        : base("A BAML function threw an error.", value, bamlTrace)
    {
    }
}

public sealed class BamlPanic : BamlException
{
    public BamlPanic(object? value, IReadOnlyList<string>? bamlTrace)
        : base("A BAML function panicked.", value, bamlTrace)
    {
    }
}

public sealed class BamlCancelledException : OperationCanceledException
{
    public BamlCancelledException(object? value, IReadOnlyList<string>? bamlTrace)
        : base("A BAML function was canceled by the runtime.")
    {
        Value = value;
        BamlTrace = bamlTrace ?? Array.Empty<string>();
    }

    public object? Value { get; }

    public IReadOnlyList<string> BamlTrace { get; }

    public string? ClassName { get; internal init; }
}

public sealed class BamlTypeMismatchException : ArgumentException
{
    public BamlTypeMismatchException(object? value, IReadOnlyList<string>? bamlTrace)
        : base(GetMessage(value))
    {
        Value = value;
        BamlTrace = bamlTrace ?? Array.Empty<string>();
    }

    public object? Value { get; }

    public IReadOnlyList<string> BamlTrace { get; }

    public string? ClassName { get; internal init; }

    private static string GetMessage(object? value)
    {
        if (value is IReadOnlyDictionary<string, object?> fields
            && fields.TryGetValue("message", out var message)
            && message is not null)
        {
            return Convert.ToString(message, System.Globalization.CultureInfo.InvariantCulture)
                ?? "A BAML call received a value with the wrong type.";
        }

        return "A BAML call received a value with the wrong type.";
    }
}

public sealed class BamlProgramConflictException : InvalidOperationException
{
    internal BamlProgramConflictException(string activeFingerprint, string requestedFingerprint)
        : base($"This process already initialized BAML program {activeFingerprint}; it cannot initialize distinct program {requestedFingerprint}.")
    {
        ActiveFingerprint = activeFingerprint;
        RequestedFingerprint = requestedFingerprint;
    }

    public string ActiveFingerprint { get; }

    public string RequestedFingerprint { get; }
}

public sealed class BamlSdkVersionMismatchException : InvalidOperationException
{
    internal BamlSdkVersionMismatchException(string generatedVersion, string runtimeVersion)
        : base(
            $"Generated BAML C# SDK version {generatedVersion} cannot run with baml-bridge {runtimeVersion}. "
            + "Regenerate the C# SDK with the matching BAML CLI or align the baml-bridge package version.")
    {
        GeneratedVersion = generatedVersion;
        RuntimeVersion = runtimeVersion;
    }

    public string GeneratedVersion { get; }

    public string RuntimeVersion { get; }
}

public sealed class BamlBridgeException : BamlException
{
    internal BamlBridgeException(string message, Exception? innerException = null)
        : base(message, null, null, innerException)
    {
    }
}
