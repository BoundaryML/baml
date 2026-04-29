// Test shell operations typecheck correctly

function RunCommand() -> string {
    baml.sys.shell("echo hello").stdout
}

function RunCommandWithVar() -> string {
    let cmd = "ls -la";
    baml.sys.shell(cmd).stdout
}

function ChainCommands() -> string {
    let result1 = baml.sys.shell("pwd");
    let result2 = baml.sys.shell("whoami");
    result1.stdout + " - " + result2.stdout
}

function CheckExitCode() -> int {
    baml.sys.shell("exit 42").exit_code
}

function CheckOk() -> bool {
    baml.sys.shell("echo test").ok()
}
