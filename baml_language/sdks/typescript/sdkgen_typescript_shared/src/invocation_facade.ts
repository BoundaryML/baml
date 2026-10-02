import type * as Trace from "./vendor/trace/index.js";
import type { CancelToken } from "./baml/spawn/index.js";

export interface BamlOptions {
    readonly trace?: Trace.Options | Trace.ReservedSpan | null;
    readonly cancel?: CancelToken | null;
    readonly timeoutMs?: number | null;
    readonly signal?: AbortSignal | null;
}
