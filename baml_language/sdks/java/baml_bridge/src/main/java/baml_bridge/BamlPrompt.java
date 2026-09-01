package baml_bridge;

import java.util.Arrays;
import java.util.List;
import java.util.concurrent.CompletableFuture;

/**
 * Portable host representation of {@code ai.Prompt}.
 *
 * <p>The payload is the canonical prompt tree copied from the protobuf boundary,
 * not an engine handle. Every method passes a fresh copy back through the
 * ordinary BAML call path, so one prompt can be reused for repeated calls or by
 * another runtime.
 */
public final class BamlPrompt {
    private static final String[] SELF_NAMES = {"self"};
    private static final BamlType MESSAGE_LIST =
            BamlType.list(BamlType.classByFqn("ai.PromptMessage"));

    private final byte[] wire;

    private BamlPrompt(byte[] wire) {
        if (wire == null || wire.length == 0) {
            throw new IllegalArgumentException("prompt payload is empty");
        }
        this.wire = Arrays.copyOf(wire, wire.length);
    }

    /** Internal wire-decoder entry point. */
    public static BamlPrompt fromWire(byte[] wire) {
        return new BamlPrompt(wire);
    }

    /** Internal wire-encoder entry point. */
    public byte[] bamlWireCopy() {
        return Arrays.copyOf(wire, wire.length);
    }

    public String text() {
        return (String) BamlFfi.callSync("ai.Prompt.text", SELF_NAMES, new Object[] {this}, BamlType.STRING);
    }

    public String text(BamlCallContext ctx) {
        return (String)
                BamlFfi.callSync(
                        "ai.Prompt.text", SELF_NAMES, new Object[] {this}, BamlType.STRING, ctx);
    }

    @SuppressWarnings("unchecked")
    public CompletableFuture<String> text_async() {
        return (CompletableFuture<String>) (CompletableFuture<?>)
                BamlFfi.callAsync(
                        "ai.Prompt.text", SELF_NAMES, new Object[] {this}, BamlType.STRING);
    }

    @SuppressWarnings("unchecked")
    public CompletableFuture<String> text_async(BamlCallContext ctx) {
        return (CompletableFuture<String>) (CompletableFuture<?>)
                BamlFfi.callAsync(
                        "ai.Prompt.text", SELF_NAMES, new Object[] {this}, BamlType.STRING, ctx);
    }

    /**
     * Return generated {@code ai.PromptMessage} values when that class is
     * registered by the SDK, or the bridge's structural fallback otherwise.
     */
    @SuppressWarnings("unchecked")
    public <T> List<T> messages() {
        return (List<T>)
                BamlFfi.callSync(
                        "ai.Prompt.messages", SELF_NAMES, new Object[] {this}, MESSAGE_LIST);
    }

    @SuppressWarnings("unchecked")
    public <T> List<T> messages(BamlCallContext ctx) {
        return (List<T>)
                BamlFfi.callSync(
                        "ai.Prompt.messages", SELF_NAMES, new Object[] {this}, MESSAGE_LIST, ctx);
    }

    @SuppressWarnings("unchecked")
    public <T> CompletableFuture<List<T>> messages_async() {
        return (CompletableFuture<List<T>>) (CompletableFuture<?>)
                BamlFfi.callAsync(
                        "ai.Prompt.messages", SELF_NAMES, new Object[] {this}, MESSAGE_LIST);
    }

    @SuppressWarnings("unchecked")
    public <T> CompletableFuture<List<T>> messages_async(BamlCallContext ctx) {
        return (CompletableFuture<List<T>>) (CompletableFuture<?>)
                BamlFfi.callAsync(
                        "ai.Prompt.messages", SELF_NAMES, new Object[] {this}, MESSAGE_LIST, ctx);
    }
}
