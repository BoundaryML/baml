package baml_bridge.internal;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * Synthesizes {@link StackTraceElement}s from the pre-rendered BAML wire trace
 * lines and prepends them onto a native exception's stack trace — the Java
 * analog of Python's {@code attach_baml_traceback} / {@code _synthesize_traceback}
 * (errors.py, 31g-phase6). After the splice, {@code printStackTrace()} renders
 * the {@code .baml} source frames inline (as ordinary {@code at fn(src:N)}
 * lines) above the real Java frames, rather than leaving the BAML stack as a
 * detached blob only reachable via {@code baml_trace()}.
 *
 * <p>Best-effort, exactly like the Python original's {@code try/except} around
 * the splice: any parse failure (or an empty/absent trace) leaves the
 * exception's stack trace untouched, so error <em>delivery</em> never depends on
 * this cosmetic step. Malformed wire lines are individually skipped.
 */
public final class BamlTraceback {
    /**
     * Wire trace-line shape, e.g. {@code File "resume.baml", line 12, in
     * user.extract}. The same regex as Python's {@code _TRACE_LINE}
     * (errors.py:21): {@code file} and {@code func} are greedy {@code .*}, so a
     * {@code func} bearing dotted namespaces (e.g. {@code user.throws_test.Fn})
     * is captured whole.
     */
    private static final Pattern TRACE_LINE =
            Pattern.compile("File \"(?<file>.*)\", line (?<line>\\d+), in (?<func>.*)");

    /**
     * Declaring-class shown for a BAML frame whose function name carries no
     * namespace (no dot). A dotted function splits into {@code declaringClass =
     * namespace}, {@code methodName = leaf} instead, so a printed frame reads
     * {@code at user.throws_test.ParseJson(types.baml:12)} rather than burying
     * the namespace in the method slot.
     */
    private static final String DECLARING_CLASS = "<baml>";

    private BamlTraceback() {}

    /**
     * Parse {@code trace} into synthetic BAML stack frames and prepend them onto
     * {@code exc}'s stack trace via {@link Throwable#setStackTrace}. Returns
     * {@code exc} (for fluent {@code throw splice(new X(), trace)} use). A null /
     * empty trace, or a trace with no parseable line, leaves the stack untouched.
     *
     * <p>The wire is most-recent-call-last (the throwing function is the last
     * line); a printed Java stack trace shows the most-recent frame first, so the
     * parsed frames are reversed — the throwing BAML function lands on top,
     * followed by its callers, then the real Java frames.
     */
    public static <T extends Throwable> T splice(T exc, List<String> trace) {
        if (exc == null || trace == null || trace.isEmpty()) {
            return exc;
        }
        try {
            List<StackTraceElement> baml = new ArrayList<>();
            for (String line : trace) {
                StackTraceElement frame = parseFrame(line);
                if (frame != null) {
                    baml.add(frame);
                }
            }
            if (baml.isEmpty()) {
                return exc; // nothing parsed → leave the native stack intact
            }
            Collections.reverse(baml); // most-recent (throwing fn) first
            StackTraceElement[] real = exc.getStackTrace();
            StackTraceElement[] merged = new StackTraceElement[baml.size() + real.length];
            for (int i = 0; i < baml.size(); i++) {
                merged[i] = baml.get(i);
            }
            System.arraycopy(real, 0, merged, baml.size(), real.length);
            exc.setStackTrace(merged);
        } catch (RuntimeException ignored) {
            // Best-effort: a cosmetic splice must never break error delivery.
        }
        return exc;
    }

    /**
     * Parse one wire line into a {@link StackTraceElement}, or null if it does
     * not match the {@code File "...", line N, in fn} shape (skipped by the
     * caller). A dotted {@code fn} splits its trailing leaf into the method name
     * and the leading namespace into the declaring class.
     */
    private static StackTraceElement parseFrame(String line) {
        if (line == null) {
            return null;
        }
        Matcher m = TRACE_LINE.matcher(line.trim());
        if (!m.matches()) {
            return null;
        }
        int lineNo;
        try {
            lineNo = Integer.parseInt(m.group("line"));
        } catch (NumberFormatException e) {
            return null;
        }
        if (lineNo < 1) {
            lineNo = 1; // mirror Python's max(int(lineno), 1)
        }
        String file = m.group("file");
        String func = m.group("func");
        String declaringClass;
        String methodName;
        int dot = func.lastIndexOf('.');
        if (dot >= 0) {
            declaringClass = func.substring(0, dot);
            methodName = func.substring(dot + 1);
        } else {
            declaringClass = DECLARING_CLASS;
            methodName = func;
        }
        return new StackTraceElement(declaringClass, methodName, file, lineNo);
    }
}
