export interface Checked<T,K extends BaseChecks> {
    value: T,
    checks: K,
}

interface Check {
    name: string,
    expr: string
    result: "succeeded" | "failed"
}

interface BaseChecks {
    [key: string]: Check
}

function all_succeeded<K extends BaseChecks>(checks: K): boolean {
    return Object.values(checks).every(value => value.result == "succeeded");
}
