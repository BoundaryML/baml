using System.Text;

namespace Baml.Cffi;

internal static unsafe class NativeBuffer
{
    private static readonly UTF8Encoding StrictUtf8 = new(
        encoderShouldEmitUTF8Identifier: false,
        throwOnInvalidBytes: true);

    internal static byte[] CopyAndFree(BamlApiV1* api, BamlBuffer buffer)
    {
        try
        {
            if (buffer.Length > int.MaxValue)
            {
                throw new BamlProtocolException(
                    "The native bridge returned an oversized buffer.",
                    $"Native buffer length {buffer.Length} exceeds Int32.MaxValue.");
            }

            if (buffer.Length != 0 && buffer.Pointer is null)
            {
                throw new BamlProtocolException(
                    "The native bridge returned an invalid buffer.",
                    "A nonempty native buffer had a null pointer.");
            }

            return buffer.Length == 0
                ? []
                : new ReadOnlySpan<byte>(buffer.Pointer, checked((int)buffer.Length)).ToArray();
        }
        finally
        {
            api->FreeBuffer(buffer);
        }
    }

    internal static string ReadUtf8AndFree(BamlApiV1* api, BamlBuffer buffer)
    {
        byte[] bytes = CopyAndFree(api, buffer);
        try
        {
            return StrictUtf8.GetString(bytes);
        }
        catch (DecoderFallbackException error)
        {
            throw new BamlProtocolException(
                "The native bridge returned invalid UTF-8.",
                error.Message);
        }
    }
}
