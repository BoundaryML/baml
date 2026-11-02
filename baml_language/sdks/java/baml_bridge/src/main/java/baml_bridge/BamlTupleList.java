package baml_bridge;

import java.util.AbstractList;
import java.util.List;
import java.util.RandomAccess;

/**
 * A BAML tuple that reached Java with no generated record for its identity:
 * the list representations of the tuple fallback chain, {@code (T1 | T2 | ...)[]}
 * or the plain {@code List<Object>} of its slots. It is an ordinary immutable
 * list, exactly the slots in order (equal to any list of the same elements),
 * that also keeps the tuple type the engine sent it with ({@link #tupleType}).
 *
 * <p>Sent back to BAML (a callback returning what it received, a value passed
 * on to another call), it crosses as the tuple it is: a list stating that
 * carried tuple type, each slot under its own slot type, rather than a list a
 * tuple slot would have to supply the shape for. A carried type the wire
 * cannot state (one naming a declaration the runtime created) leaves it an
 * ordinary list.
 */
public final class BamlTupleList extends AbstractList<Object> implements RandomAccess {
    private final BamlType tupleType;
    private final Object[] elements;
    private final Object[] slotValues;

    private BamlTupleList(BamlType tupleType, List<Object> elements, List<Object> slotValues) {
        this.tupleType = tupleType;
        this.elements = elements.toArray();
        this.slotValues = slotValues.toArray();
    }

    /**
     * Internal: the list for a tuple of {@code tupleType} whose slots decoded
     * to {@code slotValues}, presented as {@code elements} (the same values,
     * each re-wrapped as its arm of the slot join where the list is
     * {@code (T1 | T2 | ...)[]}). Both hold exactly one entry per slot.
     */
    public static BamlTupleList of(BamlType tupleType, List<Object> elements, List<Object> slotValues) {
        if (tupleType.kind() != BamlType.Kind.TUPLE
                || elements.size() != tupleType.tupleItems().size()
                || slotValues.size() != elements.size()) {
            throw new IllegalArgumentException(
                    "a " + tupleType + " list needs exactly one entry per slot, not " + elements.size());
        }
        return new BamlTupleList(tupleType, elements, slotValues);
    }

    /** The tuple type the engine sent this tuple with: its carried identity. */
    public BamlType tupleType() {
        return tupleType;
    }

    /** The slot values in position order, unwrapped from any join arm: what restating sends. */
    Object[] slotValues() {
        return slotValues.clone();
    }

    @Override
    public Object get(int index) {
        return elements[index];
    }

    @Override
    public int size() {
        return elements.length;
    }
}
