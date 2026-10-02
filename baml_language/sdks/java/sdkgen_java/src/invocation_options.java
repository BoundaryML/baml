public final class BamlOptions extends baml_bridge.InvocationOptions {
    private BamlOptions(Builder builder) { super(builder.trace == null ? null : builder.trace.bamlTraceHandle(), builder.cancel, builder.timeout); }
    public static BamlOptions empty() { return new Builder().build(); }
    public static Builder builder() { return new Builder(); }
    public static final class Builder {
        private baml_sdk.vendor.trace.TraceSelection trace;
        private baml_sdk.baml.spawn.CancelToken cancel;
        private java.lang.Long timeout;
        public Builder trace(baml_sdk.vendor.trace.TraceSelection value) { trace = value; return this; }
        public Builder cancel(baml_sdk.baml.spawn.CancelToken value) { cancel = value; return this; }
        public Builder timeoutMs(long value) { timeout = value; return this; }
        public BamlOptions build() { return new BamlOptions(this); }
    }
}
