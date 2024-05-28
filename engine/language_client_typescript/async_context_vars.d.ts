import { BamlSpan, RuntimeContextManager, BamlRuntime } from './native';
export declare class CtxManager {
    private rt;
    private ctx;
    constructor(rt: BamlRuntime);
    upsertTags(tags: Record<string, string>): void;
    get(): RuntimeContextManager;
    startTraceSync(name: string, args: Record<string, any>): BamlSpan;
    startTraceAsync(name: string, args: Record<string, any>): BamlSpan;
    endTrace(span: BamlSpan, response: any): Promise<void>;
    traceFnSync<ReturnType, F extends (...args: any[]) => Promise<ReturnType>>(name: string, func: F): F;
    traceFnAync<ReturnType, F extends (...args: any[]) => Promise<ReturnType>>(func: F): F;
}
//# sourceMappingURL=async_context_vars.d.ts.map