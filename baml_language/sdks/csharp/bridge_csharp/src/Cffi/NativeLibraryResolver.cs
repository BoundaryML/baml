using System.Reflection;
using System.Runtime.ExceptionServices;
using System.Runtime.InteropServices;

namespace Baml.Cffi;

internal static class NativeLibraryResolver
{
    private const string OverrideVariable = "BAML_BRIDGE_CSHARP_NATIVE_LIBRARY";

    private static readonly Lazy<string?> ConfiguredOverride = new(
        () => Environment.GetEnvironmentVariable(OverrideVariable),
        LazyThreadSafetyMode.ExecutionAndPublication);

    private static ExceptionDispatchInfo? registrationFailure;
    private static int usedOverride;

    internal static bool UsedOverride => Volatile.Read(ref usedOverride) != 0;

    static NativeLibraryResolver()
    {
        try
        {
            NativeLibrary.SetDllImportResolver(
                typeof(NativeLibraryResolver).Assembly,
                Resolve);
        }
        catch (InvalidOperationException error)
        {
            registrationFailure = ExceptionDispatchInfo.Capture(
                new BamlNativeLibraryLoadException(
                    "The Baml.Bridge assembly could not install its native-library resolver.",
                    error));
        }
    }

    internal static void EnsureRegistered() => registrationFailure?.Throw();

    private static nint Resolve(
        string libraryName,
        Assembly assembly,
        DllImportSearchPath? searchPath)
    {
        if (!StringComparer.Ordinal.Equals(libraryName, NativeMethods.LibraryName))
        {
            return 0;
        }

        string? configured = ConfiguredOverride.Value;
        if (string.IsNullOrEmpty(configured))
        {
            return 0;
        }

        if (!Path.IsPathFullyQualified(configured))
        {
            throw new BamlNativeLibraryLoadException(
                $"{OverrideVariable} must be an absolute native-library file path; received {configured}.");
        }

        if (!File.Exists(configured))
        {
            throw new BamlNativeLibraryLoadException(
                $"{OverrideVariable} file does not exist: {configured}.");
        }

        try
        {
            nint handle = NativeLibrary.Load(configured, assembly, searchPath);
            Volatile.Write(ref usedOverride, 1);
            return handle;
        }
        catch (Exception error)
            when (error is DllNotFoundException or BadImageFormatException or FileLoadException)
        {
            throw new BamlNativeLibraryLoadException(
                $"Failed to load the exact {OverrideVariable} file {configured}; packaged fallback is disabled.",
                error);
        }
    }
}
