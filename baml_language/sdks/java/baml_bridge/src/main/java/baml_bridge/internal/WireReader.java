package baml_bridge.internal;

import java.nio.charset.StandardCharsets;
import java.util.Arrays;

/**
 * Minimal protobuf wire-format reader (proto3, hand-rolled). The mirror of
 * {@link WireWriter}: varint (wire type 0), fixed64/double (wire type 1), and
 * length-delimited strings/bytes/embedded messages (wire type 2). Unknown
 * fields are skipped so newer engine fields do not break decoding.
 *
 * <p>Field numbers for the BAML messages are documented in {@link ProtoReader}
 * against {@code baml_outbound.proto}.
 */
public final class WireReader {
    private final byte[] buf;
    private int pos;
    private final int end;

    public WireReader(byte[] buf) {
        this(buf, 0, buf.length);
    }

    private WireReader(byte[] buf, int offset, int length) {
        this.buf = buf;
        this.pos = offset;
        this.end = offset + length;
    }

    public boolean hasRemaining() {
        return pos < end;
    }

    /** Read a field tag. Use {@link #fieldOf}/{@link #wireOf} to split it. */
    public int readTag() {
        return (int) readVarint();
    }

    public static int fieldOf(int tag) {
        return tag >>> 3;
    }

    public static int wireOf(int tag) {
        return tag & 0x7;
    }

    /** Read a base-128 varint as a signed 64-bit value. */
    public long readVarint() {
        long result = 0;
        int shift = 0;
        while (true) {
            if (pos >= end) {
                throw new IllegalStateException("truncated varint");
            }
            int b = buf[pos++] & 0xFF;
            result |= (long) (b & 0x7F) << shift;
            if ((b & 0x80) == 0) {
                return result;
            }
            shift += 7;
            if (shift >= 64) {
                throw new IllegalStateException("malformed varint (exceeds 64 bits)");
            }
        }
    }

    /** Read a fixed64 as a little-endian IEEE-754 double. */
    public double readDouble() {
        if (pos + 8 > end) {
            throw new IllegalStateException("truncated fixed64");
        }
        long bits = 0;
        for (int i = 0; i < 8; i++) {
            bits |= (long) (buf[pos++] & 0xFF) << (8 * i);
        }
        return Double.longBitsToDouble(bits);
    }

    /** Read a length-delimited payload as a fresh byte[]. */
    public byte[] readBytes() {
        int len = (int) readVarint();
        if (len < 0 || pos + len > end) {
            throw new IllegalStateException("length-delimited field overruns buffer");
        }
        byte[] out = Arrays.copyOfRange(buf, pos, pos + len);
        pos += len;
        return out;
    }

    /** Read a length-delimited UTF-8 string. */
    public String readString() {
        return new String(readBytes(), StandardCharsets.UTF_8);
    }

    /** Read a length-delimited sub-message as a bounded {@link WireReader}. */
    public WireReader readMessage() {
        int len = (int) readVarint();
        if (len < 0 || pos + len > end) {
            throw new IllegalStateException("embedded message overruns buffer");
        }
        WireReader sub = new WireReader(buf, pos, len);
        pos += len;
        return sub;
    }

    /** Skip a field of the given wire type (unknown / ignored fields). */
    public void skipField(int wireType) {
        switch (wireType) {
            case WireWriter.WIRE_VARINT:
                readVarint();
                break;
            case WireWriter.WIRE_FIXED64:
                advance(8);
                break;
            case WireWriter.WIRE_LEN:
                advance((int) readVarint());
                break;
            case 5: // fixed32
                advance(4);
                break;
            default:
                throw new IllegalStateException("unsupported wire type " + wireType);
        }
    }

    private void advance(int n) {
        if (n < 0 || pos + n > end) {
            throw new IllegalStateException("skip overruns buffer");
        }
        pos += n;
    }
}
