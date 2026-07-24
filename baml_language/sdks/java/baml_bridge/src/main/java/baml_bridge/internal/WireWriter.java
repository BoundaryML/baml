package baml_bridge.internal;

import java.io.ByteArrayOutputStream;
import java.nio.charset.StandardCharsets;

/**
 * Minimal protobuf wire-format writer (proto3, hand-rolled — no
 * protobuf-java / protoc dependency). Only the pieces the BAML primitives
 * slice needs: varint (wire type 0), fixed64/double (wire type 1), and
 * length-delimited strings/bytes/embedded messages (wire type 2).
 *
 * <p>Wire format reference: https://protobuf.dev/programming-guides/encoding/.
 * Field numbers for the BAML messages are documented in {@link ProtoWriter}
 * against {@code baml_inbound.proto}.
 */
public final class WireWriter {
    /** Protobuf wire types used by this codec. */
    public static final int WIRE_VARINT = 0;
    public static final int WIRE_FIXED64 = 1;
    public static final int WIRE_LEN = 2;

    private final ByteArrayOutputStream out = new ByteArrayOutputStream();

    /** Encode a tag = (fieldNumber &lt;&lt; 3) | wireType as a varint. */
    public void writeTag(int fieldNumber, int wireType) {
        writeVarint(((long) fieldNumber << 3) | wireType);
    }

    /**
     * Write a base-128 varint. {@code value} is treated as unsigned 64-bit, so
     * a negative {@code int64} sign-extends to the full 10 bytes — matching
     * proto3 {@code int64} encoding.
     */
    public void writeVarint(long value) {
        while (true) {
            if ((value & ~0x7FL) == 0) {
                out.write((int) (value & 0x7F));
                return;
            }
            out.write((int) ((value & 0x7F) | 0x80));
            value >>>= 7;
        }
    }

    /** {@code int64} / {@code uint64} field (wire type 0). */
    public void writeInt64(int fieldNumber, long value) {
        writeTag(fieldNumber, WIRE_VARINT);
        writeVarint(value);
    }

    /** {@code bool} field (wire type 0). */
    public void writeBool(int fieldNumber, boolean value) {
        writeTag(fieldNumber, WIRE_VARINT);
        writeVarint(value ? 1L : 0L);
    }

    /** {@code double} field (wire type 1, little-endian IEEE-754). */
    public void writeDouble(int fieldNumber, double value) {
        writeTag(fieldNumber, WIRE_FIXED64);
        long bits = Double.doubleToRawLongBits(value);
        for (int i = 0; i < 8; i++) {
            out.write((int) (bits & 0xFF));
            bits >>>= 8;
        }
    }

    /** UTF-8 {@code string} field (wire type 2). */
    public void writeString(int fieldNumber, String value) {
        writeLengthDelimited(fieldNumber, value.getBytes(StandardCharsets.UTF_8));
    }

    /** {@code bytes} field (wire type 2). */
    public void writeBytes(int fieldNumber, byte[] value) {
        writeLengthDelimited(fieldNumber, value);
    }

    /**
     * Embedded-message field (wire type 2). Presence is explicit: emitting the
     * tag + length (even for a zero-length payload) marks the oneof arm set,
     * which is how an empty list/map round-trips as empty rather than null.
     */
    public void writeMessage(int fieldNumber, byte[] messageBytes) {
        writeLengthDelimited(fieldNumber, messageBytes);
    }

    /** Append an already-encoded message body (used to add sparse metadata). */
    public void writeRawBytes(byte[] bytes) {
        out.write(bytes, 0, bytes.length);
    }

    private void writeLengthDelimited(int fieldNumber, byte[] payload) {
        writeTag(fieldNumber, WIRE_LEN);
        writeVarint(payload.length);
        out.write(payload, 0, payload.length);
    }

    /** The encoded bytes accumulated so far. */
    public byte[] toByteArray() {
        return out.toByteArray();
    }
}
