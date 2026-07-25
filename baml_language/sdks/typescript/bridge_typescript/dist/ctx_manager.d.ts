/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import { HostSpanManager } from './native.js';
export declare class CtxManager {
    private rt;
    private ctx;
    constructor(rt: unknown);
    get(): HostSpanManager;
    allowReset(): boolean;
    reset(): void;
    cloneContext(): HostSpanManager;
    upsertTags(tags: Record<string, string>): void;
    traceFn<T>(name: string, fn: (...args: unknown[]) => T): (...args: unknown[]) => T;
    traceFnAsync<T>(name: string, fn: (...args: unknown[]) => Promise<T>): (...args: unknown[]) => Promise<T>;
    flush(): void;
}
//# sourceMappingURL=ctx_manager.d.ts.map