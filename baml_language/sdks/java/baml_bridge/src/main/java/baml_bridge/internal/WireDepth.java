package baml_bridge.internal;

/**
 * How deep an encoded value nests, measured as the engine's decoder measures
 * it: the number of protobuf messages nested below the value, counting its type
 * statement and every payload (the port of {@code bridge_ctypes::wire_depth}).
 *
 * <p>prost refuses a message nested more than {@link #WIRE_NESTING_LIMIT}
 * messages deep, before it reads any key the message carries. A value that
 * deep can never cross, and the handle clones it holds would never be
 * released, so the encoder measures what it built before it ships it.
 *
 * <p>The measure reads the schema of {@code baml_inbound.proto},
 * {@code baml_type.proto} and the media and prompt messages of
 * {@code baml_outbound.proto}: a message is as deep as its deepest
 * sub-message plus one, and a scalar field costs nothing.
 */
public final class WireDepth {
    /**
     * How many nested messages the engine decodes before it refuses one as too
     * deep: prost's recursion limit ({@code WIRE_NESTING_LIMIT}).
     */
    public static final int WIRE_NESTING_LIMIT = 100;

    /**
     * The messages enclosing a call argument's value:
     * {@code CallFunctionArgs} → {@code InboundMapEntry} → the value.
     */
    public static final int ARGUMENT_DEPTH = 2;

    /** The message enclosing a callback's result or thrown value: the {@code HostCallCompletion}. */
    public static final int RESULT_DEPTH = 1;

    private WireDepth() {}

    /** Where an encoded value sits, which decides how many messages enclose it. */
    public enum Root {
        /** A call argument. */
        ARGUMENT(ARGUMENT_DEPTH),
        /** A host callback's result or thrown value. */
        RESULT(RESULT_DEPTH);

        private final int enclosing;

        Root(int enclosing) {
            this.enclosing = enclosing;
        }

        /** Whether an {@code InboundValue} nesting {@code depth} messages below itself fits under this root. */
        public boolean admits(int depth) {
            return enclosing + depth <= WIRE_NESTING_LIMIT;
        }
    }

    /** How many messages the encoded {@code InboundValue} nests below itself. */
    public static int inboundValueDepth(byte[] inboundValue) {
        return depth(Message.INBOUND_VALUE, new WireReader(inboundValue));
    }

    /** How many messages the encoded {@code BamlTy} nests below itself. */
    public static int bamlTyDepth(byte[] bamlTy) {
        return depth(Message.BAML_TY, new WireReader(bamlTy));
    }

    private static int depth(Message message, WireReader r) {
        int deepest = 0;
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            Message child = wire == WireWriter.WIRE_LEN ? message.child(field) : null;
            if (child == null) {
                r.skipField(wire);
            } else {
                deepest = Math.max(deepest, 1 + depth(child, r.readMessage()));
            }
        }
        return deepest;
    }

    /** The message types a value nests, each naming the message every one of its fields holds. */
    private enum Message {
        INBOUND_VALUE,
        INBOUND_LIST,
        INBOUND_MAP,
        INBOUND_ENTRY,
        INBOUND_CLASS,
        LEAF,
        BAML_TY,
        TY_WITH_ARGS,
        TY_ITEMS,
        TY_INTERFACE,
        TY_ASSOCIATED_BINDING,
        TY_FUNCTION,
        TY_FUNCTION_PARAM,
        TY_PAIR,
        MEDIA,
        PROMPT,
        PROMPT_MESSAGE,
        PROMPT_MULTIPLE,
        PROMPT_SIMPLE,
        PROMPT_SIMPLE_MULTIPLE;

        /** The message field {@code field} holds, or {@code null} for a scalar field. */
        Message child(int field) {
            return switch (this) {
                // value_type = 1; list = 6, map = 7, class = 8, enum = 9,
                // handle = 10, ty_value = 13, media = 15, prompt = 16.
                case INBOUND_VALUE -> switch (field) {
                    case 1, 13 -> BAML_TY;
                    case 6 -> INBOUND_LIST;
                    case 7 -> INBOUND_MAP;
                    case 8 -> INBOUND_CLASS;
                    case 9, 10 -> LEAF;
                    case 15 -> MEDIA;
                    case 16 -> PROMPT;
                    default -> null;
                };
                case INBOUND_LIST -> field == 1 ? INBOUND_VALUE : null;
                case INBOUND_MAP -> field == 1 ? INBOUND_ENTRY : null;
                // enum_key = 5, value = 6.
                case INBOUND_ENTRY -> switch (field) {
                    case 5 -> LEAF;
                    case 6 -> INBOUND_VALUE;
                    default -> null;
                };
                case INBOUND_CLASS -> field == 2 ? INBOUND_ENTRY : null;
                case LEAF -> null;
                // class_ty = 2, type_alias = 9: heads with type_args; list = 4,
                // map = 5, optional = 6, union = 7, interface = 12,
                // function = 14, future = 15, projection = 23; every other
                // arm is one message with nothing below it.
                case BAML_TY -> switch (field) {
                    case 2, 9 -> TY_WITH_ARGS;
                    case 4, 6, 7 -> TY_ITEMS;
                    case 5, 15, 23 -> TY_PAIR;
                    case 12 -> TY_INTERFACE;
                    case 14 -> TY_FUNCTION;
                    case 1, 3, 8, 10, 11, 13, 16, 17, 18, 19, 22, 24 -> LEAF;
                    default -> null;
                };
                case TY_WITH_ARGS -> field == 2 ? BAML_TY : null;
                // BamlTyList item, BamlTyOptional inner, BamlTyUnion options.
                case TY_ITEMS -> field == 1 ? BAML_TY : null;
                // BamlTyMap key = 1, value = 2; BamlTyFuture value = 1,
                // error = 2; BamlTyAssociatedTypeProjection base = 1,
                // interface = 2.
                case TY_PAIR -> field == 1 || field == 2 ? BAML_TY : null;
                case TY_INTERFACE -> switch (field) {
                    case 2 -> BAML_TY;
                    case 3 -> TY_ASSOCIATED_BINDING;
                    default -> null;
                };
                case TY_ASSOCIATED_BINDING, TY_FUNCTION_PARAM -> field == 2 ? BAML_TY : null;
                case TY_FUNCTION -> switch (field) {
                    case 3 -> TY_FUNCTION_PARAM;
                    case 4, 5 -> BAML_TY;
                    default -> null;
                };
                // file_content = 6.
                case MEDIA -> field == 6 ? LEAF : null;
                // simple = 1, message = 2, multiple = 3.
                case PROMPT -> switch (field) {
                    case 1 -> PROMPT_SIMPLE;
                    case 2 -> PROMPT_MESSAGE;
                    case 3 -> PROMPT_MULTIPLE;
                    default -> null;
                };
                case PROMPT_MESSAGE -> field == 2 ? PROMPT_SIMPLE : null;
                case PROMPT_MULTIPLE -> field == 1 ? PROMPT : null;
                // media = 2, multiple = 3.
                case PROMPT_SIMPLE -> switch (field) {
                    case 2 -> MEDIA;
                    case 3 -> PROMPT_SIMPLE_MULTIPLE;
                    default -> null;
                };
                case PROMPT_SIMPLE_MULTIPLE -> field == 1 ? PROMPT_SIMPLE : null;
            };
        }
    }
}
