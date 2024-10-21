export interface Checked<T, K extends BaseChecks> {
    value: T;
    checks: K;
}
export interface Check {
    name: string;
    expr: string;
    status: "succeeded" | "failed";
}
export interface BaseChecks {
    [key: string]: Check;
}
export declare function all_succeeded<K extends BaseChecks>(checks: K): boolean;
export declare function get_checks<K extends BaseChecks>(checks: K): Check[];
//# sourceMappingURL=checked.d.ts.map