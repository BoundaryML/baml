public final class Invocation implements baml_bridge.InvocationCapture {
    private final baml_bridge.internal.InvocationFrames.Frame frame;
    private Invocation(baml_bridge.internal.InvocationFrames.Frame frame) { this.frame = frame; }
    public static java.util.Optional<Invocation> current() { return java.util.Optional.ofNullable(baml_bridge.internal.InvocationFrames.CURRENT.get()).map(Invocation::new); }
    public baml_sdk.baml.spawn.CancelToken cancel() { return (baml_sdk.baml.spawn.CancelToken) frame.cancel(); }
    public <T,R> java.util.function.Function<T,R> wrapFunction(java.util.function.Function<T,R> body) { return frame.wrapFunction(body); }
    public Runnable wrapRunnable(Runnable body) { return frame.wrapRunnable(body); }
    public java.util.concurrent.Executor executor(java.util.concurrent.Executor delegate) { return frame.executor(delegate); }
    public <T> T run(java.util.function.Supplier<T> body) { return frame.run(body); }
    public baml_bridge.internal.InvocationFrames.Frame bridgeFrame() { return frame; }
}
