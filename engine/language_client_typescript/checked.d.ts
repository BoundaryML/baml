export interface Checked<T, K extends BaseChecks> {
    value: T;
    checks: K;
}
interface Check {
    name: string;
    expr: string;
    result: "succeeded" | "failed";
}
interface BaseChecks {
    [key: string]: Check;
}
export {};
//# sourceMappingURL=checked.d.ts.map