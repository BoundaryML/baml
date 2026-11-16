from baml_sdk import optional_args_probe_sync


optional_args_probe_sync()  # pyright: ignore[reportCallIssue]
optional_args_probe_sync(1, 8)  # pyright: ignore[reportCallIssue]
optional_args_probe_sync(1, opt3=1)  # pyright: ignore[reportCallIssue]
optional_args_probe_sync("x")  # pyright: ignore[reportArgumentType]
optional_args_probe_sync(1, opt1="x")  # pyright: ignore[reportArgumentType]
