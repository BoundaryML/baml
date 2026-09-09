"""Check directional typing against a freshly generated shared-fixture SDK.

Run with the SDK's Python environment (which must include Pyright):
    python check_input_roles.py /absolute/path/to/generated

The argument is the directory containing baml_sdk, not baml_sdk itself.
"""

import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def main(cases=None) -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: check_input_roles.py DIRECTORY_CONTAINING_BAML_SDK")
    generated = Path(sys.argv[1]).resolve(strict=True)
    if not (generated / "baml_sdk" / "__init__.pyi").is_file():
        raise SystemExit("generate the shared interfaces Python SDK first")

    probes = Path(__file__).resolve().parent
    with tempfile.TemporaryDirectory(prefix="baml-input-roles-") as directory:
        scratch = Path(directory)
        for filename, negative in cases or [
            ("generated_input_roles.py", False),
            ("generated_input_roles_rejections.py", True),
        ]:
            source = probes / filename
            shutil.copy2(source, scratch / filename)
            config = scratch / "pyrightconfig.json"
            config.write_text(
                json.dumps(
                    {
                        "include": [filename],
                        "extraPaths": [str(generated)],
                        "typeCheckingMode": "strict",
                        "pythonVersion": "3.10",
                    }
                )
            )
            result = subprocess.run(
                [
                    sys.executable,
                    "-m",
                    "pyright",
                    "--pythonpath",
                    sys.executable,
                    "-p",
                    str(config),
                    "--outputjson",
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            if result.returncode not in (0, 1):
                raise SystemExit(result.stderr or result.stdout)
            report = json.loads(result.stdout)
            diagnostics = report["generalDiagnostics"]
            expected = {
                index
                for index, line in enumerate(source.read_text().splitlines())
                if "# expected-error" in line
            }
            actual = {
                item["range"]["start"]["line"]
                for item in diagnostics
                if item["severity"] == "error"
                and Path(item["file"]).resolve() == (scratch / filename).resolve()
            }
            valid = (
                bool(expected)
                and actual == expected
                and len(diagnostics) == len(expected)
                if negative
                else not diagnostics and result.returncode == 0
            )
            if not valid:
                raise SystemExit(f"{filename}: unexpected typing result\n{result.stdout}")
            print(f"{filename}: passed ({len(expected)} expected errors)")


if __name__ == "__main__":
    main()
