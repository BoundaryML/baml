"""Check real SDK consumers against source and emitted package declarations."""

from pathlib import Path
import subprocess
import sys


def main() -> None:
    project = Path(sys.argv[1]).resolve()
    workspace = Path(__file__).resolve().parents[2]
    compiler = workspace / "sdks/typescript/bridge_typescript/node_modules/.bin/tsc"
    flags = ["--target", "ES2022", "--module", "NodeNext", "--moduleResolution", "NodeNext", "--strict", "--skipLibCheck"]
    names = [
        "generated_interfaces.ts", "generated_interfaces_negative.ts",
        "generated_interface_inputs.ts", "generated_concrete_methods.ts",
        "generated_interface_projection.ts",
    ]

    def run(*args: str) -> None:
        subprocess.run([str(compiler), *flags, *args], cwd=project, check=True)

    run("--noEmit", *names)
    run(
        "--declaration", "--emitDeclarationOnly", "--rootDir", "generated/baml_sdk",
        "--outDir", "declaration_check/baml_sdk", "generated/baml_sdk/index.ts",
    )
    consumers = project / "declaration_check/consumers"
    consumers.mkdir(parents=True, exist_ok=True)
    for name in names:
        original = (project / name).read_text()
        source_import = "'./generated/baml_sdk/index.js'"
        if source_import not in original:
            raise ValueError(f"{name}: expected generated SDK import")
        (consumers / name).write_text(original.replace(source_import, "'../baml_sdk/index.js'"))
    run("--noEmit", *(str(consumers / name) for name in names))
    print("Source and emitted-declaration consumers passed every positive/negative check")


if __name__ == "__main__":
    main()
