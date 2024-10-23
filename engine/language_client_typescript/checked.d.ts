export interface Checked<T, CheckName extends string = string> {
    value: T;
    checks: Record<CheckName, Check>;
}
export interface Check {
    name: string;
    expr: string;
    status: "succeeded" | "failed";
}
export declare function all_succeeded<CheckName extends string>(checks: Record<CheckName, Check>): boolean;
export declare function get_checks<CheckName extends string>(checks: Record<CheckName, Check>): Check[];
//# sourceMappingURL=checked.d.ts.map