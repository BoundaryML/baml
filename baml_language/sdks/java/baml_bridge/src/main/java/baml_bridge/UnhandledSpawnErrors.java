package baml_bridge;

import baml_bridge.internal.ProtoReader;

final class UnhandledSpawnErrors {
    private UnhandledSpawnErrors() {}

    static void report(byte[] errorEnvelope, boolean cancelled) {
        try {
            ProtoReader.decodeOutboundResult(errorEnvelope);
        } catch (Throwable error) {
            if (cancelled) {
                error.printStackTrace(System.err);
                return;
            }
            Thread current = Thread.currentThread();
            current.getUncaughtExceptionHandler().uncaughtException(current, error);
        }
    }
}
