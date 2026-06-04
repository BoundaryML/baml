from baml_sdk import optional_args_probe


optional_args_probe()  # pyright: ignore[reportCallIssue]
optional_args_probe(1, 8)  # pyright: ignore[reportCallIssue]
optional_args_probe(1, opt3=1)  # pyright: ignore[reportCallIssue]
optional_args_probe("x")  # pyright: ignore[reportArgumentType]
optional_args_probe(1, opt1="x")  # pyright: ignore[reportArgumentType]
