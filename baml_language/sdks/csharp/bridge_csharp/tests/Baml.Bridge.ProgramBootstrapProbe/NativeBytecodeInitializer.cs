using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;

internal sealed unsafe partial class NativeBytecodeInitializer : IDisposable
{
    private const string NativeLibraryName = "bridge_cffi";
    private const uint ApiVersion = 2;

    private readonly BamlApiPrefix* api;
    private bool disposed;
    private int calls;

    internal NativeBytecodeInitializer(string? nativeLibraryPath)
    {
        if (nativeLibraryPath is not null)
        {
            string path = Path.GetFullPath(nativeLibraryPath);
            if (!Path.IsPathFullyQualified(path)
                || !File.Exists(path))
            {
                throw new FileNotFoundException(
                    "native library does not exist at an absolute path",
                    path);
            }

            NativeLibrary.SetDllImportResolver(
                typeof(NativeBytecodeInitializer).Assembly,
                (libraryName, assembly, searchPath) =>
                {
                    if (!StringComparer.Ordinal.Equals(
                            libraryName,
                            NativeLibraryName))
                    {
                        return IntPtr.Zero;
                    }

                    return NativeLibrary.Load(
                        path,
                        assembly,
                        searchPath);
                });
        }

        api = NativeMethods.GetApiV1();
        Require(api is not null, "baml_get_api_v1 returned null");
        Require(
            api->AbiVersion == ApiVersion,
            $"unexpected ABI version {api->AbiVersion}");
        Require(
            api->StructSize >= (nuint)sizeof(BamlApiPrefix),
            $"truncated API table {api->StructSize}");
        Require(api->Version is not null, "version is null");
        Require(
            api->InitializeRuntimeFromBytecode is not null,
            "initialize_runtime_from_bytecode is null");
        Require(api->FreeBuffer is not null, "free_buffer is null");
        ProductVersion = Consume(api->Version());
    }

    internal string ProductVersion { get; }

    internal int Calls => Volatile.Read(ref calls);

    public void Dispose()
    {
        disposed = true;
    }

    internal string Initialize(byte[] bytes)
    {
        ObjectDisposedException.ThrowIf(disposed, this);
        Interlocked.Increment(ref calls);
        fixed (byte* pointer = bytes)
        {
            return Consume(
                api->InitializeRuntimeFromBytecode(
                    pointer,
                    (nuint)bytes.Length));
        }
    }

    private string Consume(BamlBuffer buffer)
    {
        try
        {
            if (buffer.Length == 0)
            {
                return String.Empty;
            }

            Require(
                buffer.Pointer is not null,
                "non-empty native buffer has null pointer");
            Require(
                buffer.Length <= Int32.MaxValue,
                $"native buffer is too large: {buffer.Length}");
            return new UTF8Encoding(
                encoderShouldEmitUTF8Identifier: false,
                throwOnInvalidBytes: true).GetString(
                    new ReadOnlySpan<byte>(
                        buffer.Pointer,
                        checked((int)buffer.Length)));
        }
        finally
        {
            api->FreeBuffer(buffer);
        }
    }

    private static void Require(bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidOperationException(message);
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    private readonly struct BamlBuffer
    {
        public readonly byte* Pointer;
        public readonly nuint Length;
    }

    [StructLayout(LayoutKind.Sequential)]
    private readonly struct BamlApiPrefix
    {
        public readonly uint AbiVersion;
        public readonly nuint StructSize;
        public readonly delegate* unmanaged[Cdecl]<BamlBuffer> Version;
        public readonly delegate* unmanaged[Cdecl]<
            byte*,
            nuint,
            BamlBuffer> InitializeRuntimeFromBytecode;
        public readonly delegate* unmanaged[Cdecl]<BamlBuffer, void> FreeBuffer;
    }

    private static partial class NativeMethods
    {
        [LibraryImport(
            NativeLibraryName,
            EntryPoint = "baml_get_api_v1")]
        [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
        internal static partial BamlApiPrefix* GetApiV1();
    }
}
