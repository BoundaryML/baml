import os

if "BAML_LOG" not in os.environ:
    os.environ["BAML_LOG"] = "info"


def invoke_runtime_cli():
    import baml_py

    baml_py.invoke_runtime_cli()
