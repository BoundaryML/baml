// The coding tools. Each is an ordinary BAML function passed directly in the
// Task's `tools:` list — the function name becomes the tool name and the
// docstring becomes its description for the model.
//
// Every tool takes a trailing `summary` argument that the function body never
// reads: the model fills it with a one-sentence narration, and the Observer
// extracts it from the raw call args to display progress.
//
// Tools fail by throwing ToolFault — an ai.Failure — and invoke_tool reifies
// the throw into a ToolError the model reads and recovers from. The message
// is model-facing guidance, so it says what to do next.

class ToolFault {
    message: string,
}

implements ai.Failure for ToolFault {
    function effects(self) -> ai.Effects throws never { ai.Effects.None }
}

implements baml.ToString for ToolFault {
    function to_string(self) -> string throws never { self.message }
}

/// Read a UTF-8 text file and return it with one-based line numbers. Set
/// summary to one short sentence explaining why.
function read_file(path: string, summary: string = "") -> string {
    if (path.trim().length() == 0) {
        throw ToolFault { message: "read_file needs a `path`." };
    };
    let exists = baml.fs.exists(path) catch_all (error) {
        _ => {
            throw ToolFault { message: `could not inspect ${path}: ${error}` };
        },
    };
    if (!exists) {
        throw ToolFault { message: "no such file: " + path };
    };
    let body = baml.fs.read(path) catch_all (error) {
        _ => {
            throw ToolFault { message: `could not read ${path}: ${error}` };
        },
    };
    "Contents of " + path + ":\n" + _with_line_numbers(body)
}

/// Create or overwrite a UTF-8 text file. Set summary to one short sentence
/// explaining why.
function write_file(path: string, content: string, summary: string = "") -> string {
    if (path.trim().length() == 0) {
        throw ToolFault { message: "write_file needs a `path`." };
    };
    let bytes = baml.fs.write(path, content) catch_all (error) {
        _ => {
            throw ToolFault { message: `could not write ${path}: ${error}` };
        },
    };
    `Wrote ${bytes} bytes to ${path}.`
}

/// Replace the first exact occurrence of old_str in a UTF-8 text file. Set
/// summary to one short sentence explaining why.
function edit_file(
    path: string,
    old_str: string,
    new_str: string,
    summary: string = "",
) -> string {
    if (path.trim().length() == 0) {
        throw ToolFault { message: "edit_file needs a `path`." };
    };
    if (old_str.length() == 0) {
        throw ToolFault { message: "edit_file needs a non-empty `old_str` — copy the exact text to replace." };
    };
    let exists = baml.fs.exists(path) catch_all (error) {
        _ => {
            throw ToolFault { message: `could not inspect ${path}: ${error}` };
        },
    };
    if (!exists) {
        throw ToolFault { message: "no such file: " + path };
    };
    let original = baml.fs.read(path) catch_all (error) {
        _ => {
            throw ToolFault { message: `could not read ${path}: ${error}` };
        },
    };
    if (!original.includes(old_str)) {
        throw ToolFault { message: "old_str was not found in " + path + ". Read the file again and copy the exact text." };
    };
    let updated = original.replace(old_str, new_str);
    let _ = baml.fs.write(path, updated) catch_all (error) {
        _ => {
            throw ToolFault { message: `could not write ${path}: ${error}` };
        },
    };
    "Edited " + path + " (replaced 1 occurrence)."
}

/// List one directory, using an empty path for the current directory.
/// Directory names end in a slash. Set summary to one short sentence
/// explaining why.
function list_dir(path: string, summary: string = "") -> string {
    let directory = if (path.trim().length() == 0) {
        "."
    } else {
        path
    };
    let exists = baml.fs.exists(directory) catch_all (error) {
        _ => {
            throw ToolFault { message: `could not inspect ${directory}: ${error}` };
        },
    };
    if (!exists) {
        throw ToolFault { message: "no such directory: " + directory };
    };
    let entries = baml.fs.read_dir(directory) catch_all (error) {
        _ => {
            throw ToolFault { message: `could not list ${directory}: ${error}` };
        },
    };
    let lines: string[] = [];
    for (let entry in entries) {
        let marker = if (entry.is_dir) {
            "/"
        } else {
            ""
        };
        lines.push("  " + entry.name + marker);
    }
    if (lines.length() == 0) {
        return directory + " is empty.";
    };
    "Contents of " + directory + ":\n" + lines.join("\n")
}

/// Run a command with /bin/sh and return its exit code, stdout, and stderr.
/// Set summary to one short sentence explaining why.
function run_bash(command: string, summary: string = "") -> string {
    if (command.trim().length() == 0) {
        throw ToolFault { message: "run_bash needs a `command`." };
    };
    let output = baml.sys.shell(command, null) catch_all (error) {
        _ => {
            throw ToolFault { message: `could not start shell command: ${error}` };
        },
    };
    let stdout = output.stdout.to_string();
    let stderr = output.stderr.to_string();
    let rendered = `exit code: ${output.exit_code}`;
    if (stdout.trim().length() > 0) {
        rendered = rendered + "\n--- stdout ---\n" + stdout;
    };
    if (stderr.trim().length() > 0) {
        rendered = rendered + "\n--- stderr ---\n" + stderr;
    };
    rendered
}
