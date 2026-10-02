package baml_bridge.internal;

import baml_bridge.BamlHandle;
import java.util.ArrayList;
import java.util.List;

/** Roll back newly cloned wire ownership if preparation/encoding fails. */
public final class EncodingScope implements AutoCloseable {
    private static final ThreadLocal<EncodingScope> CURRENT = new ThreadLocal<>();
    private final EncodingScope prior;
    private record OwnedKey(long key, int kind) {}
    private final List<OwnedKey> owned = new ArrayList<>();
    private boolean submitted;
    public EncodingScope() { prior = CURRENT.get(); CURRENT.set(this); }
    public static void own(long key, int kind) { EncodingScope active = CURRENT.get(); if (active != null && key != 0) active.owned.add(new OwnedKey(key, kind)); }
    public void submitted() { submitted = true; }
    @Override public void close() {
        if (prior == null) CURRENT.remove(); else CURRENT.set(prior);
        if (!submitted) for (OwnedKey handle : owned) new BamlHandle(handle.key(), handle.kind()).close();
        owned.clear();
    }
}
