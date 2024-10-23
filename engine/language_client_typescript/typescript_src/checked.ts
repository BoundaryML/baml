export interface Checked<T,CheckName extends string = string> {
    value: T,
    checks: Record<CheckName, Check>,
}

export interface Check {
    name: string,
    expr: string
    status: "succeeded" | "failed"
}

export function all_succeeded<CheckName extends string>(checks: Record<CheckName, Check>): boolean {
    return get_checks(checks).every(check => check.status === "succeeded")
}

export function get_checks<CheckName extends string>(checks: Record<CheckName, Check>): Check[] {
    return Object.values(checks)
}
