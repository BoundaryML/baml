// Validate a reference and extract its typed contract without calling it.
// Optional parameters keep their BAML defaults, including AI client/on_event.
function condition_input(signature: reflect.function.Type) -> reflect.function.Parameter {
    let required: reflect.function.Parameter[] = [];
    for (let parameter in signature.params()) {
        if (!parameter.optional) {
            required.push(parameter);
        }
    }
    if (required.length() != 1) {
        throw "Condition must accept exactly one required context parameter";
    }
    required[0]
}

function condition_scope(reference: string, package: reflect.Package) -> string {
    let name = "root." + reference;
    let signature = package.functions().get(name) ?? throw "Unknown condition: " + name;
    let parameter = condition_input(signature);
    if (parameter.type == reflect.Type.of<FileChange>()) {
        let _ = package.get_function<FileCondition>(name) ?? throw "Missing file condition";
        "file"
    } else if (parameter.type == reflect.Type.of<FileChange[]>()) {
        let _ = package.get_function<FilesCondition>(name) ?? throw "Missing multi-file condition";
        "files"
    } else if (parameter.type == reflect.Type.of<PullRequest>()) {
        let _ = package.get_function<PrCondition>(name) ?? throw "Missing PR condition";
        "pr"
    } else {
        throw "Unsupported condition input type"
    }
}
