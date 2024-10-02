import { BamlSpan, RuntimeContextManager, BamlRuntime, BamlLogEvent } from './native';
export declare class BamlCtxManager {
    private rt;
    private ctx;
    constructor(rt: BamlRuntime);
    allowResets(): boolean;
    reset(): void;
    upsertTags(tags: Record<string, string>): void;
    cloneContext(): RuntimeContextManager;
    startTrace(name: string, args: Record<string, any>): [RuntimeContextManager, BamlSpan];
    endTrace(span: BamlSpan, response: any): void;
    flush(): void;
    onLogEvent(callback: ((event: BamlLogEvent) => void) | undefined): void;
    traceFnSync<ReturnType, F extends (...args: any[]) => ReturnType>(name: string, func: F): F;
    traceFnAsync<ReturnType, F extends (...args: any[]) => Promise<ReturnType>>(name: string, func: F): F;
}
//# sourceMappingURL=async_context_vars.d.ts.map