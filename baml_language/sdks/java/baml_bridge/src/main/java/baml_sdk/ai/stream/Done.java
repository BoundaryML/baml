package baml_sdk.ai.stream;

/**
 * Runtime-owned stream exhaustion sentinel ({@code ai.stream.Done}).
 *
 * <p>A sentinel is distinct from {@code null}, which can be a legitimate
 * partial value. The bridge registers this class under {@link #FQN} so an
 * outbound {@code class_value(ai.stream.Done, {})} decodes to this type.
 */
public final class Done {
    /** Canonical BAML stdlib FQN. */
    public static final String FQN = "ai.stream.Done";

    public Done() {}
}
