package baml_sdk.baml.stream;

/**
 * Runtime-owned stream sentinel (BAML stdlib path {@code baml.stream}).
 *
 * <p>A sentinel value returned by {@link baml_bridge.BamlStream#next()} when the
 * stream is exhausted — the "finished" arm of the {@code TPartial | StreamFinished}
 * result. A sentinel is used instead of {@code null} because {@code null} is a
 * valid partial value.
 *
 * <p>Runtime-owned like the media wrapper classes: its body lives in the
 * {@code baml-bridge} runtime library (never code-generated per fixture, per the
 * conventions doc), and it is registered in the typemap under its BAML FQN
 * {@link #FQN} at bridge init (see {@code TypeRegistry}) so the decoder reifies a
 * {@code class_value(baml.stream.StreamFinished, {})} into an instance of this
 * class. Split-package with generated {@code baml_sdk.*} code on the classpath is
 * expected (the runtime owns this path, exactly as it owns {@code baml.media.*}).
 */
public final class StreamFinished {
    /** BAML stdlib FQN, used to register this class in the typemap. */
    public static final String FQN = "baml.stream.StreamFinished";

    public StreamFinished() {}
}
