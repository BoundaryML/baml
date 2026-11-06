class CreateSetupLinkRequest {
    project_id: string,
    name: string,
    max_deployments: int?,
}

function getGcpOAuthProvider() -> string { "google" }
function echo(value: string) -> string { value }
function make_adder(n: int) -> (int) -> int throws never { (x) -> { n + x } }
function EchoTimes<T>(x: T, times: int = 1) -> T { x }
function EchoWith<T>(x: T, f: (T) -> T throws never) -> T { f(x) }
function consume(x: string[] | int[]) -> int { match (x) { let s: string[] => s.length(), let i: int[] => i.length() } }
function consume_strings(x: string[]) -> int { consume(x) }
