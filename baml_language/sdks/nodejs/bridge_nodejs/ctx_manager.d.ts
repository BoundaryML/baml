/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_core/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
import { HostSpanManager } from './native';
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