import { BamlSpanPy, RuntimeContextManagerPy, BamlRuntimePy } from './native';
export declare class CtxManager {
    private rt;
    private ctx;
    constructor(rt: BamlRuntimePy);
    upsertTags(tags: Record<string, string>): void;
    get(): RuntimeContextManagerPy;
    startTraceSync(name: string, args: Record<string, any>): BamlSpanPy;
    startTraceAsync(name: string, args: Record<string, any>): BamlSpanPy;
    endTrace(span: BamlSpanPy, response: any): Promise<void>;
    traceFnSync<ReturnType, F extends (...args: any[]) => Promise<ReturnType>>(name: string, func: F): F;
    traceFnAync<ReturnType, F extends (...args: any[]) => Promise<ReturnType>>(func: F): F;
}
//# sourceMappingURL=async_context_vars.d.ts.map