package baml_bridge;

import java.util.concurrent.CompletableFuture;

/**
 * Opaque, engine-owned {@code ai.FunctionSpec<TOut>} capability.
 *
 * <p>The generated {@code Fn_spec(...)} factory obtains this value through the
 * Spec operation on the authored function. All other behavior is expressed as
 * ordinary methods on the capability; no synthetic {@code $parse},
 * {@code $render_prompt}, or {@code $build_request} function is involved.
 */
public final class BamlFunctionSpec<TOut> {
    private static final String[] SELF_NAMES = {"self"};

    private final BamlHandle handle;

    private BamlFunctionSpec(BamlHandle handle) {
        this.handle = handle;
    }

    /** Internal wire-decoder entry point. */
    public static BamlFunctionSpec<?> fromHandle(BamlHandle handle) {
        return new BamlFunctionSpec<>(handle);
    }

    /** Internal wire-encoder entry point. */
    public BamlHandle bamlHandle() {
        return handle;
    }

    @SuppressWarnings("unchecked")
    public TOut call() {
        return (TOut) BamlFfi.callSync("ai.FunctionSpec.call", SELF_NAMES, new Object[] {this}, null);
    }

    @SuppressWarnings("unchecked")
    public TOut call(Object client, Object on_event) {
        return (TOut)
                BamlFfi.callSync(
                        "ai.FunctionSpec.call",
                        new String[] {"self", "client", "on_event"},
                        new Object[] {this, client, on_event},
                        null);
    }

    @SuppressWarnings("unchecked")
    public CompletableFuture<TOut> call_async() {
        return (CompletableFuture<TOut>) (CompletableFuture<?>)
                BamlFfi.callAsync("ai.FunctionSpec.call", SELF_NAMES, new Object[] {this}, null);
    }

    @SuppressWarnings("unchecked")
    public CompletableFuture<TOut> call_async(Object client, Object on_event) {
        return (CompletableFuture<TOut>) (CompletableFuture<?>)
                BamlFfi.callAsync(
                        "ai.FunctionSpec.call",
                        new String[] {"self", "client", "on_event"},
                        new Object[] {this, client, on_event},
                        null);
    }

    @SuppressWarnings("unchecked")
    public TOut parse(String json) {
        return (TOut)
                BamlFfi.callSync(
                        "ai.FunctionSpec.parse",
                        new String[] {"self", "json"},
                        new Object[] {this, json},
                        null);
    }

    @SuppressWarnings("unchecked")
    public CompletableFuture<TOut> parse_async(String json) {
        return (CompletableFuture<TOut>) (CompletableFuture<?>)
                BamlFfi.callAsync(
                        "ai.FunctionSpec.parse",
                        new String[] {"self", "json"},
                        new Object[] {this, json},
                        null);
    }

    public BamlPrompt prompt() {
        return (BamlPrompt)
                BamlFfi.callSync(
                        "ai.FunctionSpec.prompt", SELF_NAMES, new Object[] {this}, null);
    }

    @SuppressWarnings("unchecked")
    public CompletableFuture<BamlPrompt> prompt_async() {
        return (CompletableFuture<BamlPrompt>) (CompletableFuture<?>)
                BamlFfi.callAsync(
                        "ai.FunctionSpec.prompt", SELF_NAMES, new Object[] {this}, null);
    }

    public Object build_request() {
        return BamlFfi.callSync("ai.FunctionSpec.build_request", SELF_NAMES, new Object[] {this}, null);
    }

    public Object build_request(Object client) {
        return BamlFfi.callSync(
                "ai.FunctionSpec.build_request",
                new String[] {"self", "client"},
                new Object[] {this, client},
                null);
    }

    public CompletableFuture<Object> build_request_async() {
        return BamlFfi.callAsync(
                "ai.FunctionSpec.build_request", SELF_NAMES, new Object[] {this}, null);
    }

    public CompletableFuture<Object> build_request_async(Object client) {
        return BamlFfi.callAsync(
                "ai.FunctionSpec.build_request",
                new String[] {"self", "client"},
                new Object[] {this, client},
                null);
    }

    public String name() {
        return (String) BamlFfi.callSync("ai.FunctionSpec.name", SELF_NAMES, new Object[] {this}, null);
    }

    @SuppressWarnings("unchecked")
    public CompletableFuture<String> name_async() {
        return (CompletableFuture<String>) (CompletableFuture<?>)
                BamlFfi.callAsync("ai.FunctionSpec.name", SELF_NAMES, new Object[] {this}, null);
    }

    @SuppressWarnings("unchecked")
    public java.util.Map<String, Object> arguments() {
        return (java.util.Map<String, Object>)
                BamlFfi.callSync("ai.FunctionSpec.arguments", SELF_NAMES, new Object[] {this}, null);
    }

    @SuppressWarnings("unchecked")
    public CompletableFuture<java.util.Map<String, Object>> arguments_async() {
        return (CompletableFuture<java.util.Map<String, Object>>) (CompletableFuture<?>)
                BamlFfi.callAsync("ai.FunctionSpec.arguments", SELF_NAMES, new Object[] {this}, null);
    }

    public BamlType output_type() {
        return (BamlType)
                BamlFfi.callSync(
                        "ai.FunctionSpec.output_type", SELF_NAMES, new Object[] {this}, null);
    }

    @SuppressWarnings("unchecked")
    public CompletableFuture<BamlType> output_type_async() {
        return (CompletableFuture<BamlType>) (CompletableFuture<?>)
                BamlFfi.callAsync(
                        "ai.FunctionSpec.output_type", SELF_NAMES, new Object[] {this}, null);
    }

    public Object tools() {
        return BamlFfi.callSync("ai.FunctionSpec.tools", SELF_NAMES, new Object[] {this}, null);
    }

    public CompletableFuture<Object> tools_async() {
        return BamlFfi.callAsync("ai.FunctionSpec.tools", SELF_NAMES, new Object[] {this}, null);
    }

    public String client_id() {
        return (String)
                BamlFfi.callSync("ai.FunctionSpec.client_id", SELF_NAMES, new Object[] {this}, null);
    }

    @SuppressWarnings("unchecked")
    public CompletableFuture<String> client_id_async() {
        return (CompletableFuture<String>) (CompletableFuture<?>)
                BamlFfi.callAsync("ai.FunctionSpec.client_id", SELF_NAMES, new Object[] {this}, null);
    }
}
