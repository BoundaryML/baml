export interface Checked<T,K extends BaseChecks> {
    value: T,
    checks: K,
}

export interface Check {
    name: string,
    expr: string
    status: "succeeded" | "failed"
}

export interface BaseChecks {
    [key: string]: Check
}

export function all_succeeded<K extends BaseChecks>(checks: K): boolean {
    return Object.values(checks).every(value => value.status == "succeeded");
}

export function get_checks<K extends BaseChecks>(checks: K): Check[] {
    return Object.values(checks)
}
