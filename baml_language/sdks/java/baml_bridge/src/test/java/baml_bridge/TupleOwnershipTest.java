package baml_bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_bridge.internal.ProtoReader;
import baml_bridge.internal.WireWriter;
import baml_sdk.baml.media.Image;

import java.util.ArrayList;
import java.util.List;

import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

/**
 * Offline ownership checks for tuple decoding: the engine references a
 * tuple's slots carry are owned exactly once, whichever host representation
 * the tuple takes, and a tuple that fails part-way releases what its earlier
 * slots already took instead of leaving them to the {@code Cleaner}.
 */
class TupleOwnershipTest {
    private static final int OV_INT = 4;
    private static final int OV_LIST = 11;
    private static final int OV_HANDLE = 16;
    private static final int OV_TUPLE = 23;
    private static final BamlType IMAGE = BamlType.media(1);

    /** A generated tuple record for {@code (image, string)}. */
    public record Tuple2_ImageString(Image item0, String item1) {}

    private final List<Long> released = new ArrayList<>();

    @BeforeEach
    void observeReleases() {
        TypeRegistry.registerTuple(
                Tuple2_ImageString.class.getName(), BamlType.tuple(IMAGE, BamlType.STRING));
        BamlHandle.releaseObserver = key -> {
            synchronized (released) {
                released.add(key);
            }
        };
    }

    @AfterEach
    void stopObserving() {
        BamlHandle.releaseObserver = null;
    }

    /** This test's releases: the Cleaner may release other tests' keys meanwhile. */
    private List<Long> released() {
        synchronized (released) {
            return released.stream().filter(key -> key >= 71 && key <= 83).toList();
        }
    }

    private static byte[] ovInt(long v) {
        WireWriter w = new WireWriter();
        w.writeInt64(OV_INT, v);
        return w.toByteArray();
    }

    private static byte[] ovImage(long key) {
        WireWriter handle = new WireWriter();
        handle.writeInt64(1, key); // key = 1
        handle.writeInt64(2, BamlHandle.ADT_MEDIA_IMAGE); // handle_type = 2
        WireWriter ov = new WireWriter();
        ov.writeMessage(OV_HANDLE, handle.toByteArray());
        return ov.toByteArray();
    }

    private static byte[] ovList(byte[]... items) {
        WireWriter list = new WireWriter();
        for (byte[] item : items) {
            list.writeMessage(2, item);
        }
        WireWriter ov = new WireWriter();
        ov.writeMessage(OV_LIST, list.toByteArray());
        return ov.toByteArray();
    }

    private static byte[] ovCarriedTuple(BamlType tupleType, byte[]... items) {
        WireWriter tuple = new WireWriter();
        for (byte[] item : items) {
            tuple.writeMessage(1, item);
        }
        tuple.writeMessage(2, tupleType.toWireTy());
        WireWriter ov = new WireWriter();
        ov.writeMessage(OV_TUPLE, tuple.toByteArray());
        return ov.toByteArray();
    }

    @Test
    void a_later_slot_failure_releases_handles_from_earlier_slots() {
        // Slot 1 is a tuple of the wrong arity: decoding it fails the tuple.
        BamlType imagePair = BamlType.tuple(IMAGE, BamlType.tuple(BamlType.INT, BamlType.INT));
        byte[] shortSlot = ovCarriedTuple(
                imagePair, ovImage(71), ovCarriedTuple(BamlType.tuple(BamlType.INT), ovInt(5)));
        assertThrows(BamlError.class, () -> ProtoReader.decodeWithDesc(shortSlot, imagePair, false));
        // Released at once and exactly once, with no fallback decoding slot 0 again.
        assertEquals(List.of(71L), released());

        // The references a container in an earlier slot holds go too.
        BamlType imagesPair = BamlType.tuple(BamlType.list(IMAGE), BamlType.tuple(BamlType.INT, BamlType.INT));
        byte[] nested = ovCarriedTuple(
                imagesPair,
                ovList(ovImage(72), ovImage(73)),
                ovCarriedTuple(BamlType.tuple(BamlType.INT), ovInt(5)));
        assertThrows(BamlError.class, () -> ProtoReader.decodeWithDesc(nested, imagesPair, false));
        assertEquals(List.of(71L, 72L, 73L), released());

        // Slot 1 decodes, but the registered record refuses it (an int where
        // it holds a string): the slots are released, not traded for a list.
        BamlType imageString = BamlType.tuple(IMAGE, BamlType.STRING);
        byte[] refused = ovCarriedTuple(imageString, ovImage(74), ovInt(5));
        assertThrows(RuntimeException.class, () -> ProtoReader.decodeWithDesc(refused, imageString, false));
        assertEquals(List.of(71L, 72L, 73L, 74L), released());
    }

    @Test
    void the_list_fallback_holds_the_slots_decoded_once() {
        // `(image, image)` has no record: it falls back to a list of its slots.
        BamlType imagePair = BamlType.tuple(IMAGE, IMAGE);
        Object got = ProtoReader.decodeWithDesc(ovCarriedTuple(imagePair, ovImage(81), ovImage(82)), imagePair, false);
        List<?> list = assertInstanceOf(BamlTupleList.class, got);
        assertEquals(2, list.size());
        Image first = assertInstanceOf(Image.class, list.get(0));
        Image second = assertInstanceOf(Image.class, list.get(1));
        assertEquals(81L, first.bamlHandle().key());
        assertEquals(82L, second.bamlHandle().key());
        // Reading the list again yields the same owners, not fresh decodes.
        assertSame(first, list.get(0));
        assertTrue(released().isEmpty(), "decoding released " + released());
        first.bamlHandle().close();
        second.bamlHandle().close();
        first.bamlHandle().close();
        assertEquals(List.of(81L, 82L), released());

        // A registered tuple decodes to its record and takes each handle once.
        BamlType imageString = BamlType.tuple(IMAGE, BamlType.STRING);
        WireWriter s = new WireWriter();
        s.writeString(3, "a"); // string_value = 3
        Object record = ProtoReader.decodeWithDesc(ovCarriedTuple(imageString, ovImage(83), s.toByteArray()), imageString, false);
        Tuple2_ImageString tuple = assertInstanceOf(Tuple2_ImageString.class, record);
        assertEquals(83L, tuple.item0().bamlHandle().key());
        assertEquals(List.of(81L, 82L), released());
    }
}
