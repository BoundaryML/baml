namespace Baml;

public abstract class BamlException : Exception
{
    internal BamlException(string message)
        : base(message)
    {
    }

    internal BamlException(string message, Exception innerException)
        : base(message, innerException)
    {
    }
}

public abstract class BamlInitializationException : BamlException
{
    internal BamlInitializationException(string message)
        : base(message)
    {
    }

    internal BamlInitializationException(string message, Exception innerException)
        : base(message, innerException)
    {
    }
}

public sealed class BamlProgramConflictException : BamlInitializationException
{
    internal BamlProgramConflictException(string message)
        : base(message)
    {
    }
}

public sealed class BamlVersionMismatchException : BamlInitializationException
{
    internal BamlVersionMismatchException(string message)
        : base(message)
    {
    }
}

public sealed class BamlProgramIntegrityException : BamlInitializationException
{
    internal BamlProgramIntegrityException(string message)
        : base(message)
    {
    }
}

public sealed class BamlNativeLibraryLoadException : BamlInitializationException
{
    internal BamlNativeLibraryLoadException(string message)
        : base(message)
    {
    }

    internal BamlNativeLibraryLoadException(string message, Exception innerException)
        : base(message, innerException)
    {
    }
}

public abstract class BamlInteropException : BamlException
{
    internal BamlInteropException(string message)
        : base(message)
    {
    }

    internal BamlInteropException(string message, Exception innerException)
        : base(message, innerException)
    {
    }
}

public sealed class BamlProtocolException : BamlInteropException
{
    internal BamlProtocolException(string safeMessage, string sensitiveDiagnostic)
        : base(safeMessage)
    {
        SensitiveDiagnostic = sensitiveDiagnostic;
    }

    internal string SensitiveDiagnostic { get; }
}

public sealed class BamlTypeMappingException : BamlException
{
    internal BamlTypeMappingException(
        Type clrType,
        string position,
        string path,
        string diagnostic,
        string? canonicalReplacement = null)
        : base(diagnostic)
    {
        ArgumentNullException.ThrowIfNull(clrType);
        ArgumentException.ThrowIfNullOrWhiteSpace(position);
        ArgumentException.ThrowIfNullOrWhiteSpace(path);
        ClrType = clrType;
        Position = position;
        Path = path;
        CanonicalReplacement = canonicalReplacement;
    }

    public Type ClrType { get; }

    public string Position { get; }

    public string Path { get; }

    public string? CanonicalReplacement { get; }
}
